//! Window-independent, bounded Minecraft process ownership. Never expose launch argv or tokens.
mod console;
use super::{
  accounts::AccountState,
  api::{ApiError, AppState},
};
use anvil_core::{
  CancellationToken, Context, Error, Result,
  account::{AccountService, AccountSession, MicrosoftAccountProvider, SystemCredentialStore},
  account_store,
  client::{ClientInstaller, ClientProgress},
  client_plan::ClientTarget,
  java::{InstallStage as JavaStage, JavaProgress},
  launch::{LaunchOptions, build_launch_spec_with_settings},
};
use axum::{
  Json,
  extract::{Path, State},
  http::StatusCode,
};
use console::{Console, ConsoleSnapshot};
use serde::{Deserialize, Serialize};
use std::{
  collections::BTreeMap,
  process::Stdio,
  sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
  },
};
use tokio::{
  process::Command,
  sync::{Semaphore, mpsc, watch},
};

const MAX_HISTORY: usize = 16;
const MAX_ACTIVE: usize = 4;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Phase {
  Preparing,
  Authenticating,
  Planning,
  Java,
  Running,
  Stopping,
  Exited,
  Failed,
  Cancelled,
}
impl Phase {
  fn active(self) -> bool {
    !matches!(self, Self::Exited | Self::Failed | Self::Cancelled)
  }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Snapshot {
  id: u64,
  name: String,
  offline_test: bool,
  phase: Phase,
  cancel_requested: bool,
  java_major: Option<u32>,
  java_stage: Option<JavaStage>,
  java_download_started: bool,
  java_downloaded_bytes: u64,
  java_total_bytes: Option<u64>,
  exit_code: Option<i32>,
  error_code: Option<String>,
  error: Option<String>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LaunchRequest {
  #[serde(default)]
  account_id: Option<String>,
  #[serde(default)]
  offline_test: bool,
  #[serde(default)]
  install_java: bool,
}
struct Operation {
  console: Arc<Mutex<Console>>,
  cancel: CancellationToken,
  updates: watch::Sender<Snapshot>,
}
struct Inner {
  operations: BTreeMap<u64, Operation>,
  stopping: bool,
}
pub(super) struct Launches {
  inner: Mutex<Inner>,
  next: AtomicU64,
  account_gate: Arc<Semaphore>,
}
impl Launches {
  pub(super) fn busy_for_client(&self, name: &str) -> bool {
    self
      .inner
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .operations
      .values()
      .any(|operation| {
        let snapshot = operation.updates.borrow();
        snapshot.name.eq_ignore_ascii_case(name) && snapshot.phase.active()
      })
  }
  pub fn new() -> Self {
    Self {
      inner: Mutex::new(Inner {
        operations: BTreeMap::new(),
        stopping: false,
      }),
      next: AtomicU64::new(1),
      account_gate: Arc::new(Semaphore::new(1)),
    }
  }
  fn start(
    &self,
    context: Context,
    accounts: Arc<AccountState>,
    name: String,
    request: LaunchRequest,
  ) -> std::result::Result<Snapshot, ApiError> {
    ClientInstaller::validate_name(&name).map_err(super::api::resource_error)?;
    let valid = match (&request.account_id, request.offline_test) {
      (None, true) => true,
      (Some(id), false) => {
        !id.is_empty() && id.len() <= 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
      }
      _ => false,
    };
    if !valid {
      return Err(ApiError::new(
        StatusCode::BAD_REQUEST,
        "choose a saved account or explicit offline test mode",
      ));
    }
    let name = name.to_ascii_lowercase();
    let mut inner = self
      .inner
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    if inner.stopping {
      return Err(ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "Runtime 正在停止。",
      ));
    }
    if let Some(operation) = inner
      .operations
      .values()
      .find(|op| op.updates.borrow().name == name && op.updates.borrow().phase.active())
    {
      return Ok(snapshot(operation));
    }
    if inner
      .operations
      .values()
      .filter(|op| op.updates.borrow().phase.active())
      .count()
      >= MAX_ACTIVE
    {
      return Err(ApiError::new(
        StatusCode::CONFLICT,
        "最多同时启动四个游戏实例。",
      ));
    }
    while inner.operations.len() >= MAX_HISTORY {
      let Some(id) = inner
        .operations
        .iter()
        .find(|(_, op)| !op.updates.borrow().phase.active())
        .map(|(id, _)| *id)
      else {
        break;
      };
      inner.operations.remove(&id);
    }
    let id = self.next.fetch_add(1, Ordering::Relaxed);
    let initial = Snapshot {
      id,
      name: name.clone(),
      offline_test: request.offline_test,
      phase: Phase::Preparing,
      cancel_requested: false,
      java_major: None,
      java_stage: None,
      java_download_started: false,
      java_downloaded_bytes: 0,
      java_total_bytes: None,
      exit_code: None,
      error_code: None,
      error: None,
    };
    let (updates, _) = watch::channel(initial.clone());
    let cancel = CancellationToken::new();
    let console = Arc::new(Mutex::new(Console::default()));
    inner.operations.insert(
      id,
      Operation {
        console: console.clone(),
        cancel: cancel.clone(),
        updates: updates.clone(),
      },
    );
    let gate = self.account_gate.clone();
    tokio::spawn(async move {
      tracing::info!(launch_id = id, "Client launch accepted");
      let worker_updates = updates.clone();
      let result = tokio::spawn(async move {
        run(
          context,
          accounts,
          name,
          request,
          cancel,
          gate,
          (worker_updates, console),
        )
        .await
      })
      .await
      .map_err(Error::from)
      .and_then(|r| r);
      if let Err(error) = &result {
        if let Error::Unsupported(reason) = error {
          tracing::warn!(launch_id = id, phase = ?updates.borrow().phase, error_code = error.code(), reason = %reason, "Client launch preparation failed");
        } else if error.code() != "cancelled" {
          tracing::warn!(launch_id = id, phase = ?updates.borrow().phase, error_code = error.code(), "Client launch failed");
        }
      }
      updates.send_modify(|s| match &result {
        Ok(code) => {
          s.exit_code = Some(*code);
          s.phase = if *code == 0 {
            Phase::Exited
          } else {
            Phase::Failed
          };
          if *code != 0 {
            s.error_code = Some("game_exit".into());
            s.error = Some(format!(
              "游戏异常退出（代码 {code}），请检查此实例的游戏日志。"
            ));
          }
        }
        Err(error) => {
          s.phase = if error.code() == "cancelled" {
            Phase::Cancelled
          } else {
            Phase::Failed
          };
          let (code, message) = failure(error);
          s.error_code = Some(code.into());
          s.error = Some(message.into());
        }
      });
      tracing::info!(launch_id = id, phase = ?updates.borrow().phase, "Client launch finished");
    });
    Ok(initial)
  }
  pub async fn shutdown(&self) {
    let snapshots = {
      let mut inner = self
        .inner
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      inner.stopping = true;
      inner
        .operations
        .values()
        .map(|op| {
          if op.updates.borrow().phase.active() {
            op.cancel.cancel();
          }
          op.updates.subscribe()
        })
        .collect::<Vec<_>>()
    };
    for mut value in snapshots {
      loop {
        if !value.borrow().phase.active() {
          break;
        }
        if value.changed().await.is_err() {
          break;
        }
      }
    }
  }
}
fn snapshot(op: &Operation) -> Snapshot {
  let mut value = op.updates.borrow().clone();
  value.cancel_requested = value.phase.active() && op.cancel.is_cancelled();
  value
}
fn phase(updates: &watch::Sender<Snapshot>, value: Phase) {
  updates.send_modify(|s| s.phase = value);
  tracing::info!(launch_id = updates.borrow().id, phase = ?value, "Client launch phase changed");
}
fn failure(error: &Error) -> (&'static str, &'static str) {
  match error {
    Error::AccountRequired => ("account_required", "请先登录 Microsoft 账户，再点击启动。"),
    Error::Authentication(_) => (
      "authentication_failed",
      "账户验证未完成，请重新登录后再试。",
    ),
    Error::Java(message) if message.starts_with("managed Java ") => {
      ("java_required", "缺少匹配的托管 Java，可安装后启动。")
    }
    Error::Java(_) => (
      "java_failed",
      "Java 准备失败，请检查网络或托管 Java 后重试。",
    ),
    Error::Busy(_) => ("busy", "实例或资源正被其他操作使用，请稍后重试。"),
    Error::Conflict(_) => (
      "installation_conflict",
      "实例文件或模组状态存在冲突，请检查详情或重新安装游戏文件。",
    ),
    Error::NotFound(_) => ("not_found", "实例不存在，请刷新列表。"),
    Error::Unsupported(reason) if reason.starts_with("missing Minecraft launch value ") => (
      "missing_launch_argument",
      "启动参数不完整，请查看启动器日志。",
    ),
    Error::Unsupported(_) => ("unsupported", "此版本不支持当前系统或架构。"),
    Error::ClientLaunch(_) => ("spawn_failed", "无法启动 Java 进程，请检查运行环境。"),
    _ if error.code() == "cancelled" => ("cancelled", "启动已取消或游戏已结束。"),
    _ => (
      "launch_failed",
      "启动准备失败，请检查网络、Java 和游戏文件后重试。",
    ),
  }
}
async fn run(
  context: Context,
  accounts: Arc<AccountState>,
  name: String,
  request: LaunchRequest,
  cancel: CancellationToken,
  gate: Arc<Semaphore>,
  output: (watch::Sender<Snapshot>, Arc<Mutex<Console>>),
) -> Result<i32> {
  let (updates, console) = output;
  let installer = ClientInstaller::new(context.clone());
  let (instance, installed) = installer.details(&name).await?;
  let launch_settings = installer.launch_settings(&instance.name).await?;
  updates.send_modify(|s| s.java_major = Some(installed.java_major));
  let (session, client_id) = if request.offline_test {
    (Arc::new(AccountSession::offline_test()), None)
  } else {
    let data = context.paths.data.clone();
    let account_id = request.account_id.ok_or(Error::AccountRequired)?;
    let profile = tokio::task::spawn_blocking(move || {
      account_store::load(&data)?
        .find(&account_id)
        .cloned()
        .ok_or(Error::AccountRequired)
    })
    .await??;
    phase(&updates, Phase::Authenticating);
    let client_id = accounts.client_id()?;
    let permit = tokio::select! { _ = cancel.cancelled() => return Err(Error::Cancelled), permit = gate.acquire_owned() => permit.map_err(|_| Error::Cancelled)? };
    let service = AccountService::new(
      client_id.clone(),
      Arc::new(MicrosoftAccountProvider::new()?),
      Arc::new(SystemCredentialStore),
    );
    // Refresh emits a bounded sequence; discard public auth progress instead of collecting it.
    let (events, mut receiver) = mpsc::unbounded_channel();
    let drain = tokio::spawn(async move { while receiver.recv().await.is_some() {} });
    let session = service.resume(&profile, cancel.clone(), events).await;
    let _ = drain.await;
    drop(permit);
    (Arc::new(session?), Some(client_id))
  };
  phase(&updates, Phase::Planning);
  let (progress, _) = watch::channel(ClientProgress::default());
  let plan = installer
    .plan_with_loader(
      &installed.version_id,
      ClientTarget::current()?,
      installed.loader.as_ref(),
      &cancel,
      &progress,
    )
    .await?;
  phase(&updates, Phase::Java);
  let (java_tx, mut java_rx) = watch::channel(JavaProgress::default());
  let forwarded = updates.clone();
  let observer = tokio::spawn(async move {
    while java_rx.changed().await.is_ok() {
      let value = java_rx.borrow_and_update().clone();
      forwarded.send_modify(|s| {
        s.java_stage = Some(value.stage);
        s.java_download_started |= matches!(
          value.stage,
          JavaStage::Downloading | JavaStage::Extracting | JavaStage::Publishing
        );
        s.java_downloaded_bytes = value.downloaded_bytes;
        s.java_total_bytes = value.total_bytes;
      });
    }
  });
  // The Core helper holds the instance lock through Java preparation and the game session.
  let prepared = installer
    .prepare_runtime_with_java_policy(&instance, &plan, &cancel, java_tx, request.install_java)
    .await;
  let _ = observer.await;
  let prepared = prepared?;
  if cancel.is_cancelled() {
    return Err(Error::Cancelled);
  }
  let options = LaunchOptions {
    microsoft_client_id: client_id,
    ..LaunchOptions::default()
  };
  let game = context
    .paths
    .data
    .join("instances")
    .join(&instance.id)
    .join("game");
  let spec =
    build_launch_spec_with_settings(&plan, prepared, &session, game, &options, &launch_settings)?;
  let mut command = Command::new(spec.java_executable());
  // Capture pipes separately from Runtime stdout and the shared launcher log.
  command
    .args(spec.argv())
    .current_dir(spec.working_directory())
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
  #[cfg(windows)]
  command.creation_flags(0x08000000);
  let mut child = command
    .spawn()
    .map_err(|e| Error::ClientLaunch(e.to_string()))?;
  let stdout = child.stdout.take().expect("piped stdout");
  let stderr = child.stderr.take().expect("piped stderr");
  let mut readers = tokio::task::JoinSet::new();
  readers.spawn(console::capture(
    stdout,
    "stdout",
    console.clone(),
    session.clone(),
  ));
  readers.spawn(console::capture(stderr, "stderr", console.clone(), session));
  phase(&updates, Phase::Running);
  let result = tokio::select! {
    result = child.wait() => result.map_err(Error::from),
    _ = cancel.cancelled() => { phase(&updates, Phase::Stopping); let _ = child.start_kill(); match child.wait().await { Ok(_) => Err(Error::Cancelled), Err(error) => Err(error.into()) } }
  };
  // On wait errors, reap the process before releasing the instance lock/natives.
  if result.is_err() {
    let _ = child.start_kill();
    let _ = child.wait().await;
  }
  drop(child);
  console::finish(readers, &console).await;
  drop(spec);
  let status = result?;
  Ok(status.code().unwrap_or(-1))
}
pub(super) async fn list(State(state): State<AppState>) -> Json<Vec<Snapshot>> {
  let inner = state
    .launches
    .inner
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner);
  Json(inner.operations.values().map(snapshot).collect())
}
pub(super) async fn start(
  State(state): State<AppState>,
  Path(name): Path<String>,
  Json(request): Json<LaunchRequest>,
) -> std::result::Result<(StatusCode, Json<Snapshot>), ApiError> {
  state
    .launches
    .start(
      state.runtime.context.clone(),
      state.accounts.clone(),
      name,
      request,
    )
    .map(|value| (StatusCode::ACCEPTED, Json(value)))
}
pub(super) async fn stop(
  State(state): State<AppState>,
  Path(id): Path<u64>,
) -> std::result::Result<Json<Snapshot>, ApiError> {
  let inner = state
    .launches
    .inner
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner);
  let op = inner
    .operations
    .get(&id)
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "启动任务不存在。"))?;
  if op.updates.borrow().phase.active() {
    op.cancel.cancel();
  }
  Ok(Json(snapshot(op)))
}

pub(super) async fn console_log(
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> std::result::Result<Json<ConsoleSnapshot>, ApiError> {
  ClientInstaller::validate_name(&name).map_err(super::api::resource_error)?;
  let inner = state
    .launches
    .inner
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner);
  let operation = inner
    .operations
    .iter()
    .rev()
    .find(|(_, op)| op.updates.borrow().name == name.to_ascii_lowercase());
  Ok(Json(match operation {
    Some((id, operation)) => operation
      .console
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .snapshot(*id),
    None => ConsoleSnapshot {
      exists: false,
      launch_id: None,
      text: String::new(),
      total_bytes: 0,
      truncated: false,
      read_failed: false,
    },
  }))
}
