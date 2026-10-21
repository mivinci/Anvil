use std::{
  error::Error as StdError,
  future::Future,
  path::PathBuf,
  process::Stdio,
  sync::Arc,
  time::{Duration, Instant},
};

use anvil_core::{
  CancellationToken, Context, Error, Paths, Result,
  account::{
    AccountService, AccountSession, AuthorizationEvent, MicrosoftAccountProvider,
    MicrosoftClientId, SystemCredentialStore,
  },
  account_store,
  client::{ClientInstallStage, ClientInstaller, ClientListEntry, ClientProgress},
  client_plan::ClientTarget,
  java::JavaProgress,
  launch::{LaunchOptions, build_launch_spec},
};
use tokio::{
  process::Command as TokioCommand,
  sync::{mpsc, watch},
  task::JoinSet,
};

use crate::{AccountRow, AnvilWindow, ClientRow, MICROSOFT_CLIENT_ID};

pub enum GuiCommand {
  Refresh,
  Install(String, String),
  Login,
  Cancel,
  Launch(String, String),
  SelectAccount(String),
  Logout(String),
}

enum UiEvent {
  Status(String),
  Busy(bool),
  Clients(Vec<ClientRow>),
  Accounts(Vec<AccountRow>, String),
  DeviceCode(String, String),
  ClearDeviceCode,
}

#[derive(Clone)]
struct Backend {
  context: Context,
  installer: ClientInstaller,
  window: slint::Weak<AnvilWindow>,
}

pub fn run(
  window: slint::Weak<AnvilWindow>,
  commands: mpsc::Receiver<GuiCommand>,
  shutdown: CancellationToken,
) -> std::result::Result<(), Box<dyn StdError + Send + Sync>> {
  let runtime = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()?;
  runtime.block_on(async move {
    let data_dir = std::env::var_os("ANVIL_DATA_DIR").map(PathBuf::from);
    let context = match Paths::discover(data_dir).and_then(Context::new) {
      Ok(context) => context,
      Err(error) => {
        send_ui(
          &window,
          UiEvent::Status(format!("无法初始化 Anvil 数据目录：{error}")),
        );
        return;
      }
    };
    let backend = Backend {
      installer: ClientInstaller::new(context.clone()),
      context,
      window,
    };
    run_commands(backend, commands, shutdown).await;
  });
  Ok(())
}

async fn run_commands(
  backend: Backend,
  mut commands: mpsc::Receiver<GuiCommand>,
  shutdown: CancellationToken,
) {
  let mut tasks = JoinSet::<Result<()>>::new();
  let mut active_cancel: Option<CancellationToken> = None;
  let mut commands_open = true;

  loop {
    if shutdown.is_cancelled() {
      if let Some(cancel) = active_cancel.take() {
        cancel.cancel();
      }
      while tasks.join_next().await.is_some() {}
      return;
    }

    tokio::select! {
      _ = shutdown.cancelled() => {
        if let Some(cancel) = active_cancel.as_ref() {
          cancel.cancel();
        }
      }
      command = commands.recv(), if commands_open => match command {
        Some(GuiCommand::Cancel) => {
          if let Some(cancel) = active_cancel.as_ref() {
            send_ui(
              &backend.window,
              UiEvent::Status("正在取消操作并清理临时文件…".into()),
            );
            cancel.cancel();
          }
        }
        Some(_) if !tasks.is_empty() => {}
        Some(command) => {
          let cancel = CancellationToken::new();
          active_cancel = Some(cancel.clone());
          send_ui(&backend.window, UiEvent::Busy(true));
          send_ui(&backend.window, UiEvent::ClearDeviceCode);
          let task_backend = backend.clone();
          tasks.spawn(async move { task_backend.handle(command, cancel).await });
        }
        None => commands_open = false,
      },
      result = tasks.join_next(), if !tasks.is_empty() => {
        active_cancel = None;
        send_ui(&backend.window, UiEvent::Busy(false));
        send_ui(&backend.window, UiEvent::ClearDeviceCode);
        match result {
          Some(Ok(Ok(()))) => {}
          Some(Ok(Err(Error::Cancelled))) => {
            send_ui(&backend.window, UiEvent::Status("操作已取消。".into()));
          }
          Some(Ok(Err(error))) => {
            send_ui(&backend.window, UiEvent::Status(format!("操作失败：{error}")));
          }
          Some(Err(_)) => {
            send_ui(&backend.window, UiEvent::Status("后台操作异常退出。".into()));
          }
          None => {}
        }
      }
      else => break,
    }
  }
}

impl Backend {
  async fn handle(&self, command: GuiCommand, cancel: CancellationToken) -> Result<()> {
    match command {
      GuiCommand::Refresh => self.refresh().await,
      GuiCommand::Install(name, version) => self.install(&name, &version, &cancel).await,
      GuiCommand::Login => self.login(&cancel).await,
      GuiCommand::Cancel => Ok(()),
      GuiCommand::Launch(name, account_id) => self.launch(&name, &account_id, &cancel).await,
      GuiCommand::SelectAccount(id) => {
        account_store::select(&self.context.paths.data, &id)?;
        self.refresh().await?;
        send_ui(
          &self.window,
          UiEvent::Status("已切换当前 Minecraft 账号。".into()),
        );
        Ok(())
      }
      GuiCommand::Logout(id) => self.logout(&id).await,
    }
  }

  async fn refresh(&self) -> Result<()> {
    let clients = self.installer.list_with_versions().await?;
    let rows = clients.into_iter().map(client_row).collect();
    send_ui(&self.window, UiEvent::Clients(rows));

    let profiles = account_store::load(&self.context.paths.data)?;
    let selected_id = profiles
      .selected()
      .map(|profile| profile.id().to_owned())
      .unwrap_or_default();
    let rows = profiles
      .entries()
      .into_iter()
      .map(|entry| AccountRow {
        id: entry.profile.id().into(),
        username: entry.profile.username().into(),
        selected: entry.selected,
      })
      .collect();
    send_ui(&self.window, UiEvent::Accounts(rows, selected_id));
    send_ui(
      &self.window,
      UiEvent::Status("客户端和账号列表已刷新。".into()),
    );
    Ok(())
  }

  async fn install(&self, name: &str, version: &str, cancel: &CancellationToken) -> Result<()> {
    ClientInstaller::validate_name(name)?;
    if version.trim().is_empty() {
      return Err(Error::Invalid("Minecraft version must not be empty".into()));
    }
    send_ui(
      &self.window,
      UiEvent::Status(format!("正在解析 Minecraft {version} 的安装计划…")),
    );
    let (progress_tx, mut progress_rx) = watch::channel(ClientProgress::default());
    let version = version.trim().to_owned();
    let target = ClientTarget::current()?;
    let plan = self
      .installer
      .plan(&version, target, cancel, &progress_tx)
      .await?;
    let operation = self.installer.install(name, &plan, cancel, &progress_tx);
    tokio::pin!(operation);
    let mut progress_open = true;
    let mut last_stage = None;
    let mut last_update = Instant::now();
    let result = loop {
      tokio::select! {
        result = &mut operation => break result?,
        changed = progress_rx.changed(), if progress_open => {
          if changed.is_err() {
            progress_open = false;
            continue;
          }
          let progress = progress_rx.borrow_and_update().clone();
          if last_stage != Some(progress.stage)
            || last_update.elapsed() >= Duration::from_millis(250)
          {
            send_ui(&self.window, UiEvent::Status(progress_message(&progress)));
            last_stage = Some(progress.stage);
            last_update = Instant::now();
          }
        }
      }
    };
    self.refresh().await?;
    send_ui(
      &self.window,
      UiEvent::Status(format!(
        "Minecraft {} 已{}：下载 {} 个文件，复用 {} 个缓存文件。",
        result.installed.version_id,
        if result.already_installed {
          "就绪"
        } else {
          "安装完成"
        },
        result.downloaded_files,
        result.reused_files,
      )),
    );
    Ok(())
  }

  async fn login(&self, cancel: &CancellationToken) -> Result<()> {
    let service = account_service()?;
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let session = await_authorization(
      service.authorize(cancel.clone(), event_tx),
      event_rx,
      &self.window,
    )
    .await?;
    let profile = session.profile().clone();
    if let Err(error) = account_store::upsert(&self.context.paths.data, profile.clone()) {
      let _ = service.logout(profile.id()).await;
      return Err(error);
    }
    self.refresh().await?;
    send_ui(
      &self.window,
      UiEvent::Status(format!("已登录 Minecraft 账号 {}。", profile.username())),
    );
    Ok(())
  }

  async fn logout(&self, id: &str) -> Result<()> {
    let profiles = account_store::load(&self.context.paths.data)?;
    let profile = profiles
      .find(id)
      .cloned()
      .ok_or_else(|| Error::NotFound("Minecraft account profile".into()))?;
    account_service()?.logout(profile.id()).await?;
    account_store::remove(&self.context.paths.data, profile.id())?;
    self.refresh().await?;
    send_ui(
      &self.window,
      UiEvent::Status(format!("已从 Anvil 移除账号 {}。", profile.username())),
    );
    Ok(())
  }

  async fn launch(&self, name: &str, account_id: &str, cancel: &CancellationToken) -> Result<()> {
    let instance = self
      .installer
      .list()
      .await?
      .into_iter()
      .find(|instance| instance.name.eq_ignore_ascii_case(name))
      .ok_or_else(|| Error::NotFound(format!("Minecraft client instance {name}")))?;
    let installed = self.installer.load_install_record(&instance).await?;
    let profiles = account_store::load(&self.context.paths.data)?;
    let profile = profiles
      .find(account_id)
      .cloned()
      .ok_or(Error::AccountRequired)?;

    send_ui(
      &self.window,
      UiEvent::Status(format!(
        "正在刷新 {} 的 Microsoft 登录…",
        profile.username()
      )),
    );
    let service = account_service()?;
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let session = await_authorization(
      service.resume(&profile, cancel.clone(), event_tx),
      event_rx,
      &self.window,
    )
    .await?;

    let target = ClientTarget::current()?;
    let (client_progress_tx, client_progress_rx) = watch::channel(ClientProgress::default());
    let plan = self
      .installer
      .plan(&installed.version_id, target, cancel, &client_progress_tx)
      .await?;

    send_ui(
      &self.window,
      UiEvent::Status("正在准备 Java、游戏资源和 natives…".into()),
    );
    let (java_progress_tx, mut java_progress_rx) = watch::channel(JavaProgress::default());
    let preparation = self
      .installer
      .prepare_runtime(&instance, &plan, cancel, java_progress_tx);
    tokio::pin!(preparation);
    let mut progress_open = true;
    let mut last_progress = Instant::now();
    let runtime = loop {
      tokio::select! {
        result = &mut preparation => break result?,
        changed = java_progress_rx.changed(), if progress_open => {
          if changed.is_err() {
            progress_open = false;
            continue;
          }
          if last_progress.elapsed() >= Duration::from_millis(250) {
            let progress = java_progress_rx.borrow_and_update().clone();
            send_ui(&self.window, UiEvent::Status(format!(
              "Java {:?}：已下载 {} / {} 字节。",
              progress.stage,
              progress.downloaded_bytes,
              progress.total_bytes.map_or_else(|| "未知".into(), |total| total.to_string()),
            )));
            last_progress = Instant::now();
          }
        }
      }
    };
    drop(client_progress_rx);

    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    let game_directory = self
      .context
      .paths
      .data
      .join("instances")
      .join(&instance.id)
      .join("game");
    let client_id = MicrosoftClientId::new(MICROSOFT_CLIENT_ID.to_owned())?;
    let options = LaunchOptions {
      microsoft_client_id: Some(client_id),
      ..LaunchOptions::default()
    };
    let launch = build_launch_spec(&plan, runtime, &session, game_directory, &options)?;
    let mut process = TokioCommand::new(launch.java_executable());
    process
      .args(launch.argv())
      .current_dir(launch.working_directory())
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .kill_on_drop(true);
    let mut child = process
      .spawn()
      .map_err(|error| Error::ClientLaunch(error.to_string()))?;
    send_ui(
      &self.window,
      UiEvent::Status(format!("Minecraft {} 正在运行。", installed.version_id)),
    );
    let status = tokio::select! {
      result = child.wait() => result?,
      _ = cancel.cancelled() => {
        let _ = child.start_kill();
        let _ = child.wait().await;
        return Err(Error::Cancelled);
      }
    };
    if !status.success() {
      return Err(Error::ClientExit(format!(
        "exit code {}",
        status.code().unwrap_or(-1)
      )));
    }
    send_ui(
      &self.window,
      UiEvent::Status("Minecraft 已正常退出。".into()),
    );
    Ok(())
  }
}

async fn await_authorization<F>(
  operation: F,
  mut events: mpsc::UnboundedReceiver<AuthorizationEvent>,
  window: &slint::Weak<AnvilWindow>,
) -> Result<AccountSession>
where
  F: Future<Output = Result<AccountSession>>,
{
  tokio::pin!(operation);
  let mut events_open = true;
  loop {
    tokio::select! {
      result = &mut operation => return result,
      event = events.recv(), if events_open => match event {
        Some(AuthorizationEvent::DeviceCode {
          verification_uri,
          user_code,
          expires_in_seconds,
          ..
        }) => {
          send_ui(window, UiEvent::DeviceCode(verification_uri.clone(), user_code));
          send_ui(window, UiEvent::Status(format!(
            "请在 {} 秒内打开 {} 并输入设备码。",
            expires_in_seconds,
            verification_uri,
          )));
        }
        Some(AuthorizationEvent::WaitingForApproval) => {
          send_ui(window, UiEvent::Status("等待 Microsoft 登录确认…".into()));
        }
        None => events_open = false,
      }
    }
  }
}

fn account_service() -> Result<AccountService<MicrosoftAccountProvider, SystemCredentialStore>> {
  Ok(AccountService::new(
    MicrosoftClientId::new(MICROSOFT_CLIENT_ID.to_owned())?,
    Arc::new(MicrosoftAccountProvider::new()?),
    Arc::new(SystemCredentialStore),
  ))
}

fn client_row(entry: ClientListEntry) -> ClientRow {
  ClientRow {
    name: entry.instance.name.into(),
    version: entry.version_id.into(),
  }
}

fn progress_message(progress: &ClientProgress) -> String {
  let stage = match progress.stage {
    ClientInstallStage::Resolving => "查询版本",
    ClientInstallStage::Planning => "生成安装计划",
    ClientInstallStage::Downloading => "下载文件",
    ClientInstallStage::Verifying => "校验文件",
    ClientInstallStage::Publishing => "写入安装记录",
    ClientInstallStage::Completed => "完成",
  };
  format!(
    "{stage}：{} / {} 个文件。",
    progress.completed_files,
    progress
      .total_files
      .map_or_else(|| "未知".into(), |n| n.to_string()),
  )
}

fn send_ui(window: &slint::Weak<AnvilWindow>, event: UiEvent) {
  let window = window.clone();
  let _ = slint::invoke_from_event_loop(move || {
    let Some(window) = window.upgrade() else {
      return;
    };
    match event {
      UiEvent::Status(status) => window.set_status_text(status.into()),
      UiEvent::Busy(busy) => window.set_busy(busy),
      UiEvent::Clients(clients) => {
        let model = slint::VecModel::from(clients);
        window.set_clients(slint::ModelRc::new(model));
      }
      UiEvent::Accounts(accounts, selected_id) => {
        let model = slint::VecModel::from(accounts);
        window.set_accounts(slint::ModelRc::new(model));
        window.set_selected_account_id(selected_id.into());
      }
      UiEvent::DeviceCode(uri, code) => {
        window.set_verification_uri(uri.into());
        window.set_device_code(code.into());
      }
      UiEvent::ClearDeviceCode => {
        window.set_verification_uri("".into());
        window.set_device_code("".into());
      }
    }
  });
}
