use std::{
  future::Future,
  io::{BufRead, Write},
  path::PathBuf,
  process::{ExitCode, Stdio},
  sync::Arc,
  time::Duration,
};

use anvil_core::{
  Context, Error, Paths,
  account::{
    AccountProfile, AccountService, AccountSession, AuthorizationEvent, CredentialStore,
    MicrosoftAccountProvider, MicrosoftClientId, SystemCredentialStore,
  },
  account_store::{self, AccountEntry},
  client::{ClientInstallResult, ClientInstaller, ClientInstance, ClientListEntry, ClientProgress},
  client_plan::ClientTarget,
  java::{InstallResult, JavaManager, JavaProgress},
  launch::{LaunchOptions, build_launch_spec},
  metadata::{self, GameVersion},
  server::{
    ServerInstallStage, ServerInstaller, ServerInstance, ServerInstanceManager, ServerProgress,
    ServerSessionRecord, ServerStartPreparation, server_instance_id,
  },
};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::Value;
use tokio::{
  io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
  process::Command as TokioCommand,
  sync::{Mutex, mpsc, watch},
  time::Instant,
};
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(
  name = "anvil",
  version,
  about = "Lightweight Minecraft launcher tools"
)]
struct Cli {
  /// Output one schema-versioned JSON result; suppress progress.
  #[arg(long, global = true)]
  json: bool,
  /// Stream schema-versioned JSON Lines lifecycle events for `server start`.
  #[arg(long, global = true, conflicts_with = "json")]
  json_events: bool,
  /// Override application data root (cache is stored in ROOT/cache).
  #[arg(long, global = true, env = "ANVIL_DATA_DIR")]
  data_dir: Option<PathBuf>,
  #[command(subcommand)]
  command: Command,
}

impl Cli {
  fn quiet(&self) -> bool {
    self.json || self.json_events
  }
}

#[derive(Subcommand)]
enum Command {
  /// Inspect Mojang's official Minecraft version catalog.
  Version {
    #[command(subcommand)]
    command: VersionCommand,
  },
  /// Manage Anvil-owned Java runtimes.
  Java {
    #[command(subcommand)]
    command: JavaCommand,
  },
  /// Create and inspect original Minecraft server instances.
  Server {
    #[command(subcommand)]
    command: ServerCommand,
  },
  /// Install and run original Minecraft client instances.
  Client {
    #[command(subcommand)]
    command: ClientCommand,
  },
  /// Manage Microsoft-authenticated Minecraft accounts.
  Account {
    #[command(subcommand)]
    command: AccountCommand,
  },
}
#[derive(Subcommand)]
enum VersionCommand {
  /// List released, snapshot, and historical versions.
  List,
}
#[derive(Subcommand)]
enum JavaCommand {
  /// Install the latest GA of a major version or an exact release/build.
  Install {
    #[arg(long = "version", value_name = "VERSION")]
    java_version: String,
    #[arg(long, default_value = "temurin", value_parser = ["temurin"])]
    vendor: String,
  },
  /// List managed Java runtimes (no system scan).
  List,
  /// Remove a managed runtime; keep downloaded archives.
  Uninstall { id: String },
}

#[derive(Subcommand)]
enum ServerCommand {
  /// Create an original server instance and cache its official server JAR.
  Create {
    name: String,
    #[arg(long = "version", value_name = "ID")]
    version_id: String,
  },
  /// List managed server instances.
  List,
  /// Run an original server in the foreground.
  Start {
    name: String,
    /// Record an explicit acceptance of the Minecraft EULA.
    #[arg(long)]
    accept_eula: bool,
  },
}

#[derive(Subcommand)]
enum ClientCommand {
  /// Install a released Minecraft client and its verified files.
  Install {
    name: String,
    #[arg(long = "version", value_name = "ID")]
    version_id: String,
  },
  /// List locally installed client instances.
  List,
  /// Refresh an account session and run the client in the foreground.
  Launch {
    name: String,
    #[arg(long, value_name = "UUID")]
    account: Option<String>,
    #[arg(long, env = "ANVIL_MICROSOFT_CLIENT_ID", value_name = "GUID")]
    client_id: String,
  },
}

#[derive(Subcommand)]
enum AccountCommand {
  /// Sign in through Microsoft's device-code flow.
  Login {
    #[arg(long, env = "ANVIL_MICROSOFT_CLIENT_ID", value_name = "GUID")]
    client_id: String,
  },
  /// List saved public Minecraft profile identities.
  List,
  /// Remove a saved profile and its refresh token from the system vault.
  Logout { account_id: String },
}

#[derive(Serialize)]
#[serde(untagged)]
enum Output {
  VersionList {
    versions: Vec<GameVersion>,
  },
  Install(InstallResult),
  List {
    runtimes: Vec<anvil_core::java::InstalledJava>,
  },
  Uninstall {
    removed: anvil_core::java::InstalledJava,
  },
  ServerCreated(ServerInstance),
  ServerList {
    servers: Vec<ServerInstance>,
  },
  ServerRun {
    session: ServerSessionRecord,
  },
  ClientInstalled {
    instance: ClientInstance,
    version_id: String,
    already_installed: bool,
    downloaded_files: usize,
    reused_files: usize,
  },
  ClientList {
    clients: Vec<ClientListEntry>,
  },
  ClientRun {
    instance: ClientInstance,
    version_id: String,
    exit_code: i32,
  },
  AccountLogin {
    account: AccountProfile,
  },
  AccountList {
    accounts: Vec<AccountEntry>,
  },
  AccountLogout {
    account_id: String,
  },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
  // Preserve JSON errors even when parsing fails; clap still owns help/version output.
  let json = std::env::args_os().any(|arg| arg == "--json");
  let json_events = std::env::args_os().any(|arg| arg == "--json-events");
  let cli = match Cli::try_parse() {
    Ok(cli) => cli,
    Err(error) => {
      if error.use_stderr() && json_events {
        emit_event_error("invalid_argument", &error.to_string(), 2, false);
        return ExitCode::from(2);
      }
      if json && error.use_stderr() {
        emit_error("invalid_argument", &error.to_string(), 2);
        return ExitCode::from(2);
      }
      error.exit();
    }
  };
  init_tracing();
  match run(&cli).await {
    Ok(output) => {
      if cli.json_events {
        // `server start` emits its terminal state before returning.
      } else if cli.json {
        #[derive(Serialize)]
        struct Success {
          schema_version: u32,
          ok: bool,
          data: Output,
        }
        match serde_json::to_string(&Success {
          schema_version: 1,
          ok: true,
          data: output,
        }) {
          Ok(json) => println!("{json}"),
          Err(error) => {
            emit_error("invalid_json", &error.to_string(), 6);
            return ExitCode::from(6);
          }
        }
      } else {
        print_human(&output);
      }
      ExitCode::SUCCESS
    }
    Err(error) => {
      if cli.json_events {
        emit_runtime_event_error(&error);
      } else if cli.json {
        emit_error(error.code(), &error.to_string(), error.exit_code());
      } else {
        eprintln!("error [{}]: {error}", error.code());
      }
      ExitCode::from(error.exit_code())
    }
  }
}

fn init_tracing() {
  if let Err(error) = tracing_subscriber::fmt()
    .with_max_level(tracing::Level::WARN)
    .with_ansi(false)
    .with_writer(std::io::stderr)
    .try_init()
  {
    eprintln!("could not initialize diagnostic logging: {error}");
  }
}

fn emit_error(code: &str, message: &str, exit_code: u8) {
  println!(
    "{}",
    serde_json::json!({ "schema_version": 1, "ok": false, "error": { "code": code, "message": message, "exit_code": exit_code } })
  );
}

fn emit_event_error(code: &str, message: &str, exit_code: u8, cancelled: bool) {
  let state = if cancelled { "cancelled" } else { "failed" };
  emit_state_event(
    true,
    state,
    serde_json::json!({
      "error": { "code": code, "message": message, "exit_code": exit_code }
    }),
  );
}

fn emit_runtime_event_error(error: &Error) {
  let mut error_data = serde_json::json!({
    "code": error.code(),
    "message": error.to_string(),
    "exit_code": error.exit_code(),
  });
  match error {
    Error::ServerExit {
      process_exit_code, ..
    } => error_data["process_exit_code"] = serde_json::json!(process_exit_code),
    Error::ServerNotReady { process_exit_code } => {
      error_data["process_exit_code"] = serde_json::json!(process_exit_code)
    }
    _ => {}
  }
  let cancelled = error.code() == "cancelled";
  let state = if cancelled { "cancelled" } else { "failed" };
  emit_state_event(true, state, serde_json::json!({ "error": error_data }));
}

fn emit_state_event(events: bool, state: &str, details: Value) {
  if !events {
    return;
  }
  let mut data = serde_json::Map::new();
  data.insert("state".into(), Value::String(state.into()));
  if let Value::Object(details) = details {
    data.extend(details);
  }
  let event = serde_json::json!({
    "schema_version": 1,
    "event": "state",
    "data": data,
  });
  let mut stdout = std::io::stdout().lock();
  let _ = writeln!(stdout, "{event}");
  let _ = stdout.flush();
}

async fn run(cli: &Cli) -> Result<Output, Error> {
  if cli.json_events
    && !matches!(
      &cli.command,
      Command::Server {
        command: ServerCommand::Start { .. }
      }
    )
  {
    return Err(Error::Invalid(
      "--json-events is only supported by `server start`".into(),
    ));
  }
  let context = Context::new(Paths::discover(cli.data_dir.clone())?)?;
  let manager = JavaManager::new(context.clone());
  let server_installer = ServerInstaller::new(context.clone());
  let server_instances = ServerInstanceManager::new(context.clone());
  let client_installer = ClientInstaller::new(context.clone());
  match &cli.command {
    Command::Version {
      command: VersionCommand::List,
    } => Ok(Output::VersionList {
      versions: metadata::list_versions(&context).await?,
    }),
    Command::Java {
      command: JavaCommand::List,
    } => Ok(Output::List {
      runtimes: manager.list().await?,
    }),
    Command::Java {
      command: JavaCommand::Uninstall { id },
    } => Ok(Output::Uninstall {
      removed: manager.uninstall(id).await?,
    }),
    Command::Java {
      command: JavaCommand::Install { java_version, .. },
    } => {
      let cancel = CancellationToken::new();
      let (sender, mut updates) = watch::channel(JavaProgress::default());
      let install = manager.install(java_version, &cancel, sender);
      tokio::pin!(install);
      let interrupt = tokio::signal::ctrl_c();
      tokio::pin!(interrupt);
      let mut interrupted = false;
      let mut progress_open = true;
      let mut last_progress = std::time::Instant::now();
      let mut last_stage = None;
      loop {
        tokio::select! {
            result = &mut install => return result.map(Output::Install),
            signal = &mut interrupt, if !interrupted => {
                interrupted = true;
                cancel.cancel();
                if let Err(error) = signal {
                    tracing::warn!(%error, "cannot listen for Ctrl-C");
                }
                if !cli.quiet() {
                    eprintln!("Cancelling; waiting for cleanup…");
                }
            }
            result = updates.changed(), if progress_open => {
                if result.is_err() { progress_open = false; continue; }
                if !cli.quiet() {
                    let progress = updates.borrow_and_update().clone();
                    if last_stage != Some(progress.stage) || last_progress.elapsed() >= std::time::Duration::from_millis(250) {
                        eprintln!("{:?}: {} / {} bytes", progress.stage, progress.downloaded_bytes,
                            progress.total_bytes.map_or_else(|| "?".into(), |n| n.to_string()));
                        last_stage = Some(progress.stage);
                        last_progress = std::time::Instant::now();
                    }
                }
            }
        }
      }
    }
    Command::Server {
      command: ServerCommand::List,
    } => Ok(Output::ServerList {
      servers: server_instances.list().await?,
    }),
    Command::Server {
      command: ServerCommand::Start { name, accept_eula },
    } => {
      server_instance_id(name)?;
      emit_state_event(
        cli.json_events,
        "preparing",
        serde_json::json!({ "name": name }),
      );
      let mut interrupt = InterruptListener::new()?;
      let preparation = prepare_server_start(
        &server_instances,
        name,
        *accept_eula,
        cli.quiet(),
        &mut interrupt,
      )
      .await?;
      if !cli.quiet() {
        eprintln!(
          "Starting server {} in the foreground…",
          preparation.instance.name
        );
      }
      emit_state_event(
        cli.json_events,
        "starting",
        serde_json::json!({ "session_name": preparation.instance.name }),
      );
      run_server(preparation, cli.quiet(), cli.json_events, &mut interrupt)
        .await
        .map(|session| Output::ServerRun { session })
    }
    Command::Server {
      command: ServerCommand::Create { name, version_id },
    } => {
      server_instance_id(name)?;
      let cancel = CancellationToken::new();
      let (sender, mut updates) = watch::channel(ServerProgress::default());
      let operation = async {
        let plan = server_installer.plan(version_id, &cancel, &sender).await?;
        let download = server_installer.download(&plan, &cancel, &sender).await?;
        server_instances
          .create(name, &plan, &download, &cancel)
          .await
      };
      tokio::pin!(operation);
      let interrupt = tokio::signal::ctrl_c();
      tokio::pin!(interrupt);
      let mut interrupted = false;
      let mut progress_open = true;
      let mut last_progress = std::time::Instant::now();
      let mut last_stage = None;
      loop {
        tokio::select! {
          result = &mut operation => return result.map(Output::ServerCreated),
          signal = &mut interrupt, if !interrupted => {
            interrupted = true;
            cancel.cancel();
            if let Err(error) = signal {
              tracing::warn!(%error, "cannot listen for Ctrl-C");
            }
            if !cli.quiet() {
              eprintln!("Cancelling; waiting for cleanup…");
            }
          }
          result = updates.changed(), if progress_open => {
            if result.is_err() { progress_open = false; continue; }
            if !cli.quiet() {
              let progress = updates.borrow_and_update().clone();
              if last_stage != Some(progress.stage) || last_progress.elapsed() >= std::time::Duration::from_millis(250) {
                match progress.stage {
                  ServerInstallStage::Resolving | ServerInstallStage::Completed => {
                    eprintln!("{:?}", progress.stage);
                  }
                  _ => eprintln!("{:?}: {} / {} bytes", progress.stage, progress.downloaded_bytes,
                    progress.total_bytes.map_or_else(|| "?".into(), |n| n.to_string())),
                }
                last_stage = Some(progress.stage);
                last_progress = std::time::Instant::now();
              }
            }
          }
        }
      }
    }
    Command::Client {
      command: ClientCommand::List,
    } => Ok(Output::ClientList {
      clients: client_installer.list_with_versions().await?,
    }),
    Command::Client {
      command: ClientCommand::Install { name, version_id },
    } => {
      let result = install_client(cli, &client_installer, name, version_id).await?;
      Ok(Output::ClientInstalled {
        instance: result.instance,
        version_id: result.installed.version_id,
        already_installed: result.already_installed,
        downloaded_files: result.downloaded_files,
        reused_files: result.reused_files,
      })
    }
    Command::Client {
      command: ClientCommand::Launch {
        name,
        account,
        client_id,
      },
    } => {
      let (instance, version_id, exit_code) = launch_client(
        cli,
        &context,
        &client_installer,
        name,
        account.as_deref(),
        client_id,
      )
      .await?;
      Ok(Output::ClientRun {
        instance,
        version_id,
        exit_code,
      })
    }
    Command::Account {
      command: AccountCommand::List,
    } => {
      let accounts = account_store::load(&context.paths.data)?.entries();
      Ok(Output::AccountList { accounts })
    }
    Command::Account {
      command: AccountCommand::Login { client_id },
    } => {
      let profile = login_account(&context, client_id).await?;
      Ok(Output::AccountLogin { account: profile })
    }
    Command::Account {
      command: AccountCommand::Logout { account_id },
    } => {
      let accounts = account_store::load(&context.paths.data)?;
      let profile = accounts
        .find(account_id)
        .ok_or_else(|| Error::NotFound("Minecraft account profile".into()))?;
      let canonical_id = profile.id().to_owned();
      let credentials = Arc::new(SystemCredentialStore);
      let credential_id = canonical_id.clone();
      tokio::task::spawn_blocking(move || credentials.delete_refresh_token(&credential_id))
        .await??;
      account_store::remove(&context.paths.data, &canonical_id)?;
      Ok(Output::AccountLogout {
        account_id: canonical_id,
      })
    }
  }
}

fn cancellation_watcher(cancel: CancellationToken) -> tokio::task::JoinHandle<()> {
  tokio::spawn(async move {
    if let Err(error) = tokio::signal::ctrl_c().await {
      tracing::warn!(%error, "cannot listen for Ctrl-C");
    }
    eprintln!("Cancelling; waiting for cleanup…");
    cancel.cancel();
  })
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
  fn drop(&mut self) {
    self.0.abort();
  }
}

async fn install_client(
  cli: &Cli,
  installer: &ClientInstaller,
  name: &str,
  version_id: &str,
) -> Result<ClientInstallResult, Error> {
  ClientInstaller::validate_name(name)?;
  let cancel = CancellationToken::new();
  let _watcher = AbortOnDrop(cancellation_watcher(cancel.clone()));
  let (sender, mut updates) = watch::channel(ClientProgress::default());
  let operation = async {
    let target = ClientTarget::current()?;
    let plan = installer.plan(version_id, target, &cancel, &sender).await?;
    installer.install(name, &plan, &cancel, &sender).await
  };
  tokio::pin!(operation);
  let mut progress_open = true;
  let mut last_progress = std::time::Instant::now();
  let mut last_stage = None;
  loop {
    tokio::select! {
      result = &mut operation => break result,
      changed = updates.changed(), if progress_open => {
        if changed.is_err() { progress_open = false; continue; }
        if !cli.quiet() {
          let progress = updates.borrow_and_update().clone();
          if last_stage != Some(progress.stage)
            || last_progress.elapsed() >= Duration::from_millis(250)
          {
            eprintln!("{:?}: {} / {} files", progress.stage, progress.completed_files,
              progress.total_files.map_or_else(|| "?".into(), |total| total.to_string()));
            last_stage = Some(progress.stage);
            last_progress = std::time::Instant::now();
          }
        }
      }
    }
  }
}

async fn login_account(context: &Context, client_id: &str) -> Result<AccountProfile, Error> {
  let client_id = MicrosoftClientId::new(client_id.to_owned())?;
  let service = AccountService::new(
    client_id,
    Arc::new(MicrosoftAccountProvider::new()?),
    Arc::new(SystemCredentialStore),
  );
  let cancel = CancellationToken::new();
  let _watcher = AbortOnDrop(cancellation_watcher(cancel.clone()));
  let (event_sender, event_receiver) = mpsc::unbounded_channel();
  let session = await_account_operation(
    service.authorize(cancel.clone(), event_sender),
    event_receiver,
    cancel,
  )
  .await;
  let session = session?;
  let profile = session.profile().clone();
  if let Err(error) = account_store::upsert(&context.paths.data, profile.clone()) {
    if let Err(cleanup_error) = service.logout(profile.id()).await {
      tracing::warn!(%cleanup_error, "could not remove refresh token after profile storage failed");
    }
    return Err(error);
  }
  Ok(profile)
}

async fn await_account_operation<F>(
  operation: F,
  mut events: mpsc::UnboundedReceiver<AuthorizationEvent>,
  cancel: CancellationToken,
) -> Result<AccountSession, Error>
where
  F: Future<Output = Result<AccountSession, Error>>,
{
  tokio::pin!(operation);
  let mut events_open = true;
  let mut cancellation_reported = false;
  loop {
    tokio::select! {
      result = &mut operation => return result,
      event = events.recv(), if events_open => match event {
        Some(event) => show_authorization_event(event),
        None => events_open = false,
      },
      _ = cancel.cancelled(), if !cancellation_reported => {
        cancellation_reported = true;
        eprintln!("Microsoft authorization cancelled; waiting for cleanup…");
      }
    }
  }
}

fn show_authorization_event(event: AuthorizationEvent) {
  match event {
    AuthorizationEvent::DeviceCode {
      verification_uri,
      user_code,
      expires_in_seconds,
      ..
    } => {
      eprintln!("Open {verification_uri} and enter code {user_code}.");
      eprintln!("The code expires in {expires_in_seconds} seconds.");
    }
    AuthorizationEvent::CheckingProfile => {
      eprintln!("Microsoft authorized; checking Minecraft profile…");
    }
    AuthorizationEvent::WaitingForApproval => {
      eprintln!("Waiting for Microsoft account approval…");
    }
  }
}

async fn launch_client(
  cli: &Cli,
  context: &Context,
  installer: &ClientInstaller,
  name: &str,
  account_id: Option<&str>,
  client_id: &str,
) -> Result<(ClientInstance, String, i32), Error> {
  let client_id = MicrosoftClientId::new(client_id.to_owned())?;
  let instance = installer
    .list()
    .await?
    .into_iter()
    .find(|instance| instance.name.eq_ignore_ascii_case(name))
    .ok_or_else(|| Error::NotFound(format!("Minecraft client instance {name}")))?;
  let installed = installer.load_install_record(&instance).await?;
  let account_file = account_store::load(&context.paths.data)?;
  let profile = match account_id {
    Some(id) => account_file.find(id),
    None => account_file.selected(),
  }
  .cloned()
  .ok_or(Error::AccountRequired)?;

  let cancel = CancellationToken::new();
  let _watcher = AbortOnDrop(cancellation_watcher(cancel.clone()));
  let service = AccountService::new(
    client_id.clone(),
    Arc::new(MicrosoftAccountProvider::new()?),
    Arc::new(SystemCredentialStore),
  );
  let (event_sender, event_receiver) = mpsc::unbounded_channel();
  let session = await_account_operation(
    service.resume(&profile, cancel.clone(), event_sender),
    event_receiver,
    cancel.clone(),
  )
  .await?;

  let (client_sender, mut client_updates) = watch::channel(ClientProgress::default());
  let target = ClientTarget::current()?;
  let planning = installer.plan(&installed.version_id, target, &cancel, &client_sender);
  tokio::pin!(planning);
  let mut client_progress_open = true;
  let mut last_client_stage = None;
  let mut last_client_progress = std::time::Instant::now();
  let plan = loop {
    tokio::select! {
      result = &mut planning => break result?,
      changed = client_updates.changed(), if client_progress_open => {
        if changed.is_err() { client_progress_open = false; continue; }
        if !cli.quiet() {
          let progress = client_updates.borrow_and_update().clone();
          if last_client_stage != Some(progress.stage)
            || last_client_progress.elapsed() >= Duration::from_millis(250)
          {
            eprintln!("{:?}: {} / {} files", progress.stage, progress.completed_files,
              progress.total_files.map_or_else(|| "?".into(), |total| total.to_string()));
            last_client_stage = Some(progress.stage);
            last_client_progress = std::time::Instant::now();
          }
        }
      }
    }
  };

  let (java_sender, mut java_updates) = watch::channel(JavaProgress::default());
  let preparation = installer.prepare_runtime(&instance, &plan, &cancel, java_sender);
  tokio::pin!(preparation);
  let mut java_progress_open = true;
  let mut last_java_stage = None;
  let mut last_java_progress = std::time::Instant::now();
  let runtime = loop {
    tokio::select! {
      result = &mut preparation => break result?,
      changed = java_updates.changed(), if java_progress_open => {
        if changed.is_err() { java_progress_open = false; continue; }
        if !cli.quiet() {
          let progress = java_updates.borrow_and_update().clone();
          if last_java_stage != Some(progress.stage)
            || last_java_progress.elapsed() >= Duration::from_millis(250)
          {
            eprintln!("Java {:?}: {} / {} bytes", progress.stage, progress.downloaded_bytes,
              progress.total_bytes.map_or_else(|| "?".into(), |total| total.to_string()));
            last_java_stage = Some(progress.stage);
            last_java_progress = std::time::Instant::now();
          }
        }
      }
    }
  };
  if cancel.is_cancelled() {
    return Err(Error::Cancelled);
  }
  let game_directory = context
    .paths
    .data
    .join("instances")
    .join(&instance.id)
    .join("game");
  let options = LaunchOptions {
    microsoft_client_id: Some(client_id),
    ..LaunchOptions::default()
  };
  let launch = build_launch_spec(&plan, runtime, &session, game_directory, &options)?;
  let mut command = TokioCommand::new(launch.java_executable());
  command
    .args(launch.argv())
    .current_dir(launch.working_directory())
    .stdin(Stdio::inherit())
    // Keep game logs off stdout so `--json` remains one parseable document.
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .kill_on_drop(true);
  let mut child = command
    .spawn()
    .map_err(|error| Error::ClientLaunch(error.to_string()))?;
  let mut child_stdout = child
    .stdout
    .take()
    .ok_or_else(|| Error::ClientLaunch("client stdout pipe is unavailable".into()))?;
  let stdout_task = tokio::spawn(async move {
    let mut stderr = tokio::io::stderr();
    tokio::io::copy(&mut child_stdout, &mut stderr).await
  });
  let status = tokio::select! {
    result = child.wait() => result?,
    _ = cancel.cancelled() => {
      let _ = child.start_kill();
      let _ = child.wait().await;
      let _ = stdout_task.await;
      return Err(Error::Cancelled);
    }
  };
  stdout_task.await??;
  let Some(exit_code) = status.code() else {
    return Err(Error::ClientExit("terminated by a signal".into()));
  };
  if !status.success() {
    return Err(Error::ClientExit(format!("exit code {exit_code}")));
  }
  Ok((instance.clone(), installed.version_id.clone(), exit_code))
}

struct InterruptListener {
  receiver: mpsc::Receiver<std::io::Result<()>>,
}

impl InterruptListener {
  #[cfg(unix)]
  fn new() -> std::io::Result<Self> {
    let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let (sender, receiver) = mpsc::channel(1);
    tokio::spawn(async move {
      loop {
        if signal.recv().await.is_none() {
          let _ = sender
            .send(Err(std::io::Error::other("Ctrl-C signal stream closed")))
            .await;
          return;
        }
        if sender.send(Ok(())).await.is_err() {
          return;
        }
      }
    });
    Ok(Self { receiver })
  }

  #[cfg(windows)]
  fn new() -> std::io::Result<Self> {
    let mut signal = tokio::signal::windows::ctrl_c()?;
    let (sender, receiver) = mpsc::channel(1);
    tokio::spawn(async move {
      loop {
        if signal.recv().await.is_none() {
          let _ = sender
            .send(Err(std::io::Error::other("Ctrl-C signal stream closed")))
            .await;
          return;
        }
        if sender.send(Ok(())).await.is_err() {
          return;
        }
      }
    });
    Ok(Self { receiver })
  }

  async fn recv(&mut self) -> std::io::Result<()> {
    return self
      .receiver
      .recv()
      .await
      .unwrap_or_else(|| Err(std::io::Error::other("Ctrl-C listener stopped")));
  }
}

async fn prepare_server_start(
  manager: &ServerInstanceManager,
  name: &str,
  accept_eula: bool,
  json: bool,
  interrupt: &mut InterruptListener,
) -> Result<ServerStartPreparation, Error> {
  let cancel = CancellationToken::new();
  let (sender, mut updates) = watch::channel(JavaProgress::default());
  let preparation = manager.prepare_start(name, accept_eula, &cancel, sender);
  tokio::pin!(preparation);
  let mut interrupted = false;
  let mut progress_open = true;
  let mut last_progress = std::time::Instant::now();
  let mut last_stage = None;
  loop {
    tokio::select! {
      result = &mut preparation => return result,
      signal = interrupt.recv(), if !interrupted => {
        interrupted = true;
        cancel.cancel();
        if let Err(error) = signal {
          tracing::warn!(%error, "cannot listen for Ctrl-C");
        }
        if !json {
          eprintln!("Cancelling server preparation; waiting for cleanup…");
        }
      }
      result = updates.changed(), if progress_open => {
        if result.is_err() { progress_open = false; continue; }
        if !json {
          let progress = updates.borrow_and_update().clone();
          if last_stage != Some(progress.stage) || last_progress.elapsed() >= Duration::from_millis(250) {
            eprintln!("Java {:?}: {} / {} bytes", progress.stage, progress.downloaded_bytes,
              progress.total_bytes.map_or_else(|| "?".into(), |n| n.to_string()));
            last_stage = Some(progress.stage);
            last_progress = std::time::Instant::now();
          }
        }
      }
    }
  }
}

async fn run_server(
  mut preparation: ServerStartPreparation,
  quiet: bool,
  json_events: bool,
  interrupt: &mut InterruptListener,
) -> Result<ServerSessionRecord, Error> {
  let starting_session = preparation.begin_session()?;
  let mut command = TokioCommand::new(&preparation.java.executable);
  command
    .arg("-jar")
    .arg(&preparation.server_jar)
    .arg("nogui")
    .current_dir(&preparation.game_directory)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
  #[cfg(unix)]
  command.process_group(0);
  #[cfg(windows)]
  command.creation_flags(0x00000200); // CREATE_NEW_PROCESS_GROUP

  let (input_tx, mut input_rx) = mpsc::channel(8);
  let _input_thread = std::thread::Builder::new()
    .name("anvil-server-console".into())
    .spawn(move || {
      let stdin = std::io::stdin();
      let mut input = stdin.lock();
      loop {
        let mut line = Vec::new();
        match input.read_until(b'\n', &mut line) {
          Ok(0) => break,
          Ok(_) => {
            if input_tx.blocking_send(Ok(line)).is_err() {
              break;
            }
          }
          Err(error) => {
            let _ = input_tx.blocking_send(Err(error));
            break;
          }
        }
      }
    })?;

  let mut child = match command.spawn() {
    Ok(child) => child,
    Err(error) => {
      let detail = error.to_string();
      preparation.mark_spawn_failed(&detail)?;
      return Err(Error::ServerLaunch(detail));
    }
  };
  let mut child_stdin = child.stdin.take();
  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| Error::ServerLaunch("server stdout pipe is unavailable".into()))?;
  let stderr = child
    .stderr
    .take()
    .ok_or_else(|| Error::ServerLaunch("server stderr pipe is unavailable".into()))?;
  let output = Arc::new(Mutex::new(tokio::io::stderr()));
  let (ready_tx, mut ready_rx) = mpsc::channel(1);
  let stdout_task = tokio::spawn(forward_server_output(
    stdout,
    output.clone(),
    ready_tx.clone(),
  ));
  let stderr_task = tokio::spawn(forward_server_output(stderr, output, ready_tx.clone()));
  drop(ready_tx);
  let mut wait = Box::pin(child.wait());

  let mut record_error = match preparation.mark_running() {
    Ok(_) => None,
    Err(error) => {
      tracing::warn!(%error, "could not update server session record");
      Some(error.to_string())
    }
  };
  emit_state_event(
    json_events,
    "running",
    serde_json::json!({ "session_id": starting_session.session_id }),
  );
  let mut ready = false;
  let mut ready_open = true;
  let mut stop_requested = false;
  let mut stop_deadline = None;
  let mut stop_timeout_reported = false;
  let mut interrupt_open = true;
  let status = loop {
    let timeout_at =
      stop_deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(365 * 24 * 60 * 60));
    tokio::select! {
      result = &mut wait => break result?,
      signal = interrupt.recv(), if interrupt_open => {
        interrupt_open = false;
        if !stop_requested {
          stop_requested = true;
          stop_deadline = Some(Instant::now() + Duration::from_secs(30));
          emit_state_event(json_events, "stopping", serde_json::json!({ "reason": "ctrl_c" }));
        }
        match signal {
          Ok(()) => {
            if let Some(stdin) = child_stdin.as_mut()
            {
              match stdin.write_all(b"stop\n").await {
                Ok(()) => match stdin.flush().await {
                  Ok(()) => {
                    if !quiet { eprintln!("Sent stop; waiting for world save and shutdown…"); }
                  }
                  Err(error) => tracing::warn!(%error, "could not flush stop command to server"),
                },
                Err(error) => tracing::warn!(%error, "could not send stop command to server"),
              }
            }
          }
          Err(error) => tracing::warn!(%error, "cannot listen for Ctrl-C"),
        }
      }
      signal = ready_rx.recv(), if ready_open && !ready => {
        match signal {
          Some(()) => {
            ready = true;
            emit_state_event(json_events, "ready", serde_json::json!({}));
          }
          None => ready_open = false,
        }
      }
      input = input_rx.recv(), if child_stdin.is_some() => {
        match input {
          Some(Ok(bytes)) => {
            if is_stop_command(&bytes) && !stop_requested {
              stop_requested = true;
              stop_deadline = Some(Instant::now() + Duration::from_secs(30));
            }
            if let Some(stdin) = child_stdin.as_mut()
              && let Err(error) = stdin.write_all(&bytes).await
            {
              tracing::warn!(%error, "server console input closed");
              child_stdin.take();
            }
          }
          Some(Err(error)) => {
            tracing::warn!(%error, "cannot read server console input");
            child_stdin.take();
          }
          None => { child_stdin.take(); }
        }
      }
      _ = tokio::time::sleep_until(timeout_at), if stop_deadline.is_some() && !stop_timeout_reported => {
        stop_timeout_reported = true;
        if !quiet {
          eprintln!("Server is still running after 30 seconds; keeping it in the foreground and retaining the instance lock.");
        }
      }
    }
  };

  let stdout_result = stdout_task.await?;
  let stderr_result = stderr_task.await?;
  if !ready && matches!(ready_rx.try_recv(), Ok(())) {
    ready = true;
    emit_state_event(json_events, "ready", serde_json::json!({}));
  }
  if let Err(error) = stdout_result.and(stderr_result) {
    tracing::warn!(%error, "could not forward server output");
    record_error.get_or_insert_with(|| error.to_string());
  }
  let code = status.code();
  let session = preparation.mark_exited(code, stop_requested)?;
  if let Some(error) = record_error {
    return Err(Error::Io(std::io::Error::other(error)));
  }
  if !status.success() {
    return Err(Error::ServerExit {
      details: code.map_or_else(
        || "terminated by a signal".into(),
        |code| format!("exit code {code}"),
      ),
      process_exit_code: code,
    });
  }
  if !ready {
    return Err(Error::ServerNotReady {
      process_exit_code: code.unwrap_or_default(),
    });
  }
  emit_state_event(
    json_events,
    "exited",
    serde_json::json!({ "session": &session }),
  );
  Ok(session)
}

fn is_stop_command(line: &[u8]) -> bool {
  line.trim_ascii().eq_ignore_ascii_case(b"stop")
}

async fn forward_server_output<R>(
  mut input: R,
  output: Arc<Mutex<tokio::io::Stderr>>,
  ready: mpsc::Sender<()>,
) -> std::io::Result<()>
where
  R: AsyncRead + Unpin,
{
  const MAX_READY_LINE_BYTES: usize = 16 * 1024;
  let mut buffer = [0; 16 * 1024];
  let mut output_error = None;
  let mut line = Vec::with_capacity(512);
  let mut line_too_long = false;
  let mut ready_reported = false;
  loop {
    let count = input.read(&mut buffer).await?;
    if count == 0 {
      if !ready_reported && !line_too_long && is_ready_line(&line) {
        let _ = ready.try_send(());
      }
      break;
    }
    for byte in &buffer[..count] {
      if *byte == b'\n' {
        if !ready_reported && !line_too_long && is_ready_line(&line) {
          let _ = ready.try_send(());
          ready_reported = true;
        }
        line.clear();
        line_too_long = false;
      } else if !line_too_long {
        if line.len() < MAX_READY_LINE_BYTES {
          line.push(*byte);
        } else {
          line.clear();
          line_too_long = true;
        }
      }
    }
    if output_error.is_none() {
      let mut output = output.lock().await;
      if let Err(error) = output.write_all(&buffer[..count]).await {
        output_error = Some(error);
      } else if let Err(error) = output.flush().await {
        output_error = Some(error);
      }
    }
  }
  output_error.map_or(Ok(()), Err)
}

fn is_ready_line(line: &[u8]) -> bool {
  line
    .windows(b"Done (".len())
    .any(|window| window == b"Done (")
    && line
      .windows(b"For help".len())
      .any(|window| window == b"For help")
}

fn print_human(output: &Output) {
  match output {
    Output::VersionList { versions } => {
      if versions.is_empty() {
        println!("No Minecraft versions found.");
      }
      for version in versions {
        println!(
          "{}\t{}\t{}",
          version.id, version.version_type, version.release_time
        );
      }
    }
    Output::Install(result) => {
      println!(
        "{} {}",
        if result.already_installed {
          "Already installed:"
        } else {
          "Installed:"
        },
        result.runtime.id
      );
      println!("{}", result.runtime.executable.display());
    }
    Output::List { runtimes } => {
      if runtimes.is_empty() {
        println!("No managed Java runtimes installed.");
      }
      for runtime in runtimes {
        println!("{}\t{}", runtime.id, runtime.executable.display());
      }
    }
    Output::Uninstall { removed } => println!("Removed: {}", removed.id),
    Output::ServerCreated(instance) => {
      println!(
        "Created server: {} ({})",
        instance.name, instance.version_id
      );
      println!("Instance ID: {}", instance.id);
    }
    Output::ServerList { servers } => {
      if servers.is_empty() {
        println!("No managed server instances found.");
      }
      for server in servers {
        println!("{}\t{}\t{}", server.name, server.version_id, server.id);
      }
    }
    Output::ServerRun { session } => {
      println!(
        "Server session {} exited with {}",
        session.session_id,
        session.exit_code.map_or_else(
          || "signal termination".into(),
          |code| format!("code {code}")
        )
      );
    }
    Output::ClientInstalled {
      instance,
      version_id,
      already_installed,
      downloaded_files,
      reused_files,
    } => {
      println!(
        "{} client {} ({version_id}).",
        if *already_installed {
          "Already installed"
        } else {
          "Installed"
        },
        instance.name
      );
      println!("Downloaded {downloaded_files} files; reused {reused_files} cached files.");
    }
    Output::ClientList { clients } => {
      if clients.is_empty() {
        println!("No Minecraft client instances found.");
      }
      for client in clients {
        println!(
          "{}\t{}\t{}",
          client.instance.name, client.version_id, client.instance.id
        );
      }
    }
    Output::ClientRun {
      instance,
      version_id,
      exit_code,
    } => println!(
      "Minecraft client {} ({version_id}) exited with code {exit_code}.",
      instance.name
    ),
    Output::AccountLogin { account } => println!(
      "Signed in to Minecraft profile {} ({})",
      account.username(),
      account.id()
    ),
    Output::AccountList { accounts } => {
      if accounts.is_empty() {
        println!("No saved Minecraft accounts.");
      }
      for entry in accounts {
        println!(
          "{}\t{}{}",
          entry.profile.username(),
          entry.profile.id(),
          if entry.selected { "\tselected" } else { "" }
        );
      }
    }
    Output::AccountLogout { account_id } => println!("Logged out: {account_id}"),
  }
}
