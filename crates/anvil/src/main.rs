use std::{
  io::{BufRead, Write},
  path::PathBuf,
  process::{ExitCode, Stdio},
  sync::Arc,
  time::Duration,
};

use anvil_core::{
  Context, Error, Paths,
  java::{InstallResult, JavaManager, JavaProgress},
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
  }
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
  }
}
