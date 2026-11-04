//! Bounded, idempotently submitted resource plans and drainable background installs.
use super::api::{ApiError, AppState};
use anvil_core::{
  CancellationToken, Context,
  resources::{
    ModrinthInstallPlan, ResourceInstallPlan, ResourceInstallProgress, ResourceInstallStage,
    ResourceManager, ShaderInstallPlan,
  },
};
use axum::{
  Json,
  extract::{Path, State},
  http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::{
  collections::{BTreeMap, VecDeque},
  sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
  },
  time::{Duration, Instant},
};
use tokio::sync::{Notify, watch};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PlanSelection {
  include_optional: Option<String>,
  pub(super) replace_from: Option<String>,
}
impl PlanSelection {
  pub(super) fn ids<T: std::str::FromStr>(&self) -> Result<Vec<T>, ApiError> {
    let invalid = || {
      ApiError::new(
        StatusCode::BAD_REQUEST,
        "invalid_argument",
        "请选择至多八个不同的可选依赖。",
      )
    };
    let Some(value) = self.include_optional.as_deref() else {
      return Ok(Vec::new());
    };
    if value.len() > 512 {
      return Err(invalid());
    }
    let ids = value
      .split(',')
      .map(str::parse)
      .collect::<Result<Vec<_>, _>>()
      .map_err(|_| invalid())?;
    if ids.len() > 8 || value.split(',').any(str::is_empty) {
      return Err(invalid());
    }
    Ok(ids)
  }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReplacementPreview<Id> {
  pub project_id: Id,
  pub from_file_id: Id,
  pub to_file_id: Id,
  pub from_filename: String,
  pub to_filename: String,
  pub enabled: bool,
  pub dependents: Vec<String>,
}

pub(super) fn replacement_changed() -> ApiError {
  ApiError::new(
    StatusCode::CONFLICT,
    "replacement_changed",
    "已安装版本已变化，请刷新实例模组后重新选择。",
  )
}

const LIMIT: usize = 8;
const MAX_ACTIVE: usize = 2;
const TTL: Duration = Duration::from_secs(600);
#[derive(Clone)]
pub(super) enum InstallPlan {
  CurseForge(ResourceInstallPlan),
  Modrinth(ModrinthInstallPlan),
  Shader(ShaderInstallPlan),
}
impl InstallPlan {
  fn name(&self) -> &str {
    match self {
      Self::CurseForge(p) => p.name(),
      Self::Modrinth(p) => p.name(),
      Self::Shader(p) => &p.name,
    }
  }
  fn kind(&self) -> Kind {
    match self {
      Self::Shader(_) => Kind::Shader,
      _ => Kind::Mod,
    }
  }
  fn progress(&self) -> ResourceInstallProgress {
    match self {
      Self::CurseForge(p) => ResourceInstallProgress::for_plan(p),
      Self::Modrinth(p) => ResourceInstallProgress {
        stage: ResourceInstallStage::Preparing,
        completed_files: 0,
        total_files: p.files().len(),
        downloaded_bytes: 0,
        total_bytes: p.total_bytes(),
        received_bytes: 0,
      },
      Self::Shader(p) => ResourceInstallProgress {
        stage: ResourceInstallStage::Preparing,
        completed_files: 0,
        total_files: 1,
        downloaded_bytes: 0,
        total_bytes: p.file_size,
        received_bytes: 0,
      },
    }
  }
}
struct SavedPlan {
  id: u64,
  at: Instant,
  plan: InstallPlan,
}
enum Transfer {
  Active(CancellationToken),
  Pausing,
  Paused,
  Resuming,
  Finished,
}
struct TransferControl {
  state: Mutex<Transfer>,
  wake: Notify,
}
struct Operation {
  transfer: Arc<TransferControl>,
  progress: watch::Receiver<ResourceInstallProgress>,
  cancel: CancellationToken,
  snapshot: watch::Receiver<Snapshot>,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
  Mod,
  Shader,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Status {
  Running,
  Completed,
  Failed,
  Cancelled,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Snapshot {
  pub id: u64,
  pub name: String,
  pub kind: Kind,
  pub state: Status,
  pub cancel_requested: bool,
  pub pause_requested: bool,
  pub paused: bool,
  pub progress: ResourceInstallProgress,
  pub revision: Option<u64>,
  pub error: Option<String>,
  pub error_code: Option<String>,
}
// Core derives ASCII and Unicode instance IDs in separate namespaces.
fn same_instance(left: &str, right: &str) -> bool {
  match (left.is_ascii(), right.is_ascii()) {
    (true, true) => left.eq_ignore_ascii_case(right),
    (false, false) => left.to_lowercase() == right.to_lowercase(),
    _ => false,
  }
}

struct Inner {
  plans: VecDeque<SavedPlan>,
  operations: BTreeMap<u64, Operation>,
  stopping: bool,
}
pub(super) struct ResourceOperations {
  inner: Mutex<Inner>,
  next: AtomicU64,
}
impl ResourceOperations {
  pub(super) fn busy_for_client(&self, name: &str) -> bool {
    self
      .inner
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .operations
      .values()
      .any(|operation| {
        let snapshot = operation.snapshot.borrow();
        same_instance(&snapshot.name, name) && snapshot.state == Status::Running
      })
  }
  pub fn new() -> Self {
    Self {
      inner: Mutex::new(Inner {
        plans: VecDeque::new(),
        operations: BTreeMap::new(),
        stopping: false,
      }),
      next: AtomicU64::new(1),
    }
  }
  pub fn save(&self, plan: InstallPlan) -> u64 {
    let id = self.next.fetch_add(1, Ordering::Relaxed);
    let mut inner = self
      .inner
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    inner.plans.retain(|p| p.at.elapsed() < TTL);
    while inner.plans.len() >= LIMIT {
      inner.plans.pop_front();
    }
    inner.plans.push_back(SavedPlan {
      id,
      at: Instant::now(),
      plan,
    });
    id
  }
  fn start(&self, context: Context, name: &str, id: u64) -> Result<Snapshot, ApiError> {
    let mut inner = self
      .inner
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    if inner.stopping {
      return Err(ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "runtime_stopping",
        "Runtime 正在停止。",
      ));
    }
    if let Some(operation) = inner.operations.get(&id) {
      let snapshot = snapshot(operation);
      if !same_instance(&snapshot.name, name) {
        return Err(ApiError::new(
          StatusCode::NOT_FOUND,
          "plan_not_found",
          "安装计划不存在。",
        ));
      }
      return Ok(snapshot);
    }
    let saved = inner
      .plans
      .iter()
      .find(|p| p.id == id && same_instance(p.plan.name(), name) && p.at.elapsed() < TTL)
      .ok_or_else(|| {
        ApiError::new(
          StatusCode::CONFLICT,
          "plan_expired",
          "安装计划已过期，请重新预览。",
        )
      })?;
    let mut active = 0;
    for operation in inner.operations.values() {
      let snapshot = operation.snapshot.borrow();
      if snapshot.state != Status::Running {
        continue;
      }
      if same_instance(&snapshot.name, name) {
        return Err(ApiError::new(
          StatusCode::CONFLICT,
          "instance_busy",
          "此实例已有资源安装任务。",
        ));
      }
      active += 1;
    }
    if active >= MAX_ACTIVE {
      return Err(ApiError::new(
        StatusCode::CONFLICT,
        "resource_slots_full",
        "已有两个资源安装任务，请等待完成或取消一个任务。",
      ));
    }
    let plan = saved.plan.clone();
    // Accepted plans are represented by their operation. Evicting a terminal
    // snapshot must not make a previously submitted plan executable again.
    inner.plans.retain(|saved| saved.id != id);
    let progress = plan.progress();
    let initial = Snapshot {
      id,
      name: plan.name().into(),
      kind: plan.kind(),
      state: Status::Running,
      cancel_requested: false,
      pause_requested: false,
      paused: false,
      progress: progress.clone(),
      revision: None,
      error: None,
      error_code: None,
    };
    let (updates, receiver) = watch::channel(initial.clone());
    let (progress_tx, progress_rx) = watch::channel(progress);
    let cancel = CancellationToken::new();
    let first_attempt = cancel.child_token();
    let transfer = Arc::new(TransferControl {
      state: Mutex::new(Transfer::Active(first_attempt.clone())),
      wake: Notify::new(),
    });
    while inner.operations.len() >= LIMIT {
      // An old paused task may outlive multiple newer operations.
      let id = inner
        .operations
        .iter()
        .find_map(|(&id, operation)| {
          (operation.snapshot.borrow().state != Status::Running).then_some(id)
        })
        .expect("the history limit exceeds the active limit");
      inner.operations.remove(&id);
    }
    inner.operations.insert(
      id,
      Operation {
        cancel: cancel.clone(),
        transfer: transfer.clone(),
        progress: progress_rx,
        snapshot: receiver,
      },
    );
    tokio::spawn(async move {
      tracing::info!(operation_id = id, "Resource installation started");
      let mut attempt = first_attempt;
      let result = loop {
        let manager = ResourceManager::new(context.clone());
        let plan_for_attempt = plan.clone();
        let token = attempt.clone();
        let progress = progress_tx.clone();
        let result = tokio::spawn(async move {
          match plan_for_attempt {
            InstallPlan::CurseForge(p) => manager.install_curseforge(p, token, progress).await,
            InstallPlan::Modrinth(p) => manager.install_modrinth(p, token, progress).await,
            InstallPlan::Shader(p) => manager.install_shader(p, token, progress).await,
          }
        })
        .await
        .map_err(anvil_core::Error::from)
        .and_then(|result| result);
        {
          let mut state = transfer
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
          if !matches!(*state, Transfer::Pausing)
            || !result
              .as_ref()
              .is_err_and(|error| error.code() == "cancelled")
            || cancel.is_cancelled()
          {
            *state = Transfer::Finished;
            break result;
          }
          *state = Transfer::Paused;
        }
        tokio::select! {
          biased;
          _ = cancel.cancelled() => {
            *transfer.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Transfer::Finished;
            break Err(anvil_core::Error::Cancelled);
          }
          _ = transfer.wake.notified() => {}
        }
        let mut state = transfer
          .state
          .lock()
          .unwrap_or_else(std::sync::PoisonError::into_inner);
        attempt = cancel.child_token();
        progress_tx.send_replace(plan.progress());
        *state = Transfer::Active(attempt.clone());
      };
      updates.send_modify(|s| {
        s.progress = progress_tx.borrow().clone();
        match &result {
          Ok(manifest) => {
            s.state = Status::Completed;
            s.progress.stage = ResourceInstallStage::Completed;
            s.revision = Some(manifest.revision);
          }
          Err(error) if error.code() == "cancelled" => {
            s.state = Status::Cancelled;
            s.error_code = Some("cancelled".into());
          }
          Err(error) => {
            s.state = Status::Failed;
            s.error_code = Some(error.code().into());
            s.error = Some(format!(
              "安装未能完整结束，请刷新实例资源清单确认结果。{error}"
            ));
          }
        }
      });
      match result {
        Ok(_) => tracing::info!(operation_id = id, "Resource installation completed"),
        Err(error) => tracing::warn!(
          operation_id = id,
          error_code = error.code(),
          "Resource installation stopped"
        ),
      }
    });
    Ok(initial)
  }
  pub async fn shutdown(&self) {
    let receivers = {
      let mut inner = self
        .inner
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      inner.stopping = true;
      inner
        .operations
        .values()
        .map(|op| {
          op.cancel.cancel();
          op.snapshot.clone()
        })
        .collect::<Vec<_>>()
    };
    for mut receiver in receivers {
      while receiver.borrow().state == Status::Running {
        if receiver.changed().await.is_err() {
          break;
        }
      }
    }
  }
}
fn snapshot(op: &Operation) -> Snapshot {
  let mut value = op.snapshot.borrow().clone();
  let state = op
    .transfer
    .state
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner);
  if value.state == Status::Running {
    value.progress = op.progress.borrow().clone();
    if matches!(*state, Transfer::Resuming) {
      value.progress.stage = ResourceInstallStage::Preparing;
    }
  }
  value.pause_requested = matches!(*state, Transfer::Pausing);
  value.paused = matches!(*state, Transfer::Paused);
  value.cancel_requested = value.state == Status::Running && op.cancel.is_cancelled();
  value
}
pub(super) async fn submit(
  State(state): State<AppState>,
  Path((name, id)): Path<(String, u64)>,
) -> Result<(StatusCode, Json<Snapshot>), ApiError> {
  let op = state
    .resources
    .start(state.runtime.context.clone(), &name, id)?;
  Ok((StatusCode::ACCEPTED, Json(op)))
}
pub(super) async fn list(State(state): State<AppState>) -> Json<Vec<Snapshot>> {
  let inner = state
    .resources
    .inner
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner);
  Json(inner.operations.values().map(snapshot).collect())
}
pub(super) async fn cancel(
  State(state): State<AppState>,
  Path(id): Path<u64>,
) -> Result<Json<Snapshot>, ApiError> {
  cancel_operation(&state.resources, id).map(Json)
}

fn cancel_operation(resources: &ResourceOperations, id: u64) -> Result<Snapshot, ApiError> {
  let inner = resources
    .inner
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner);
  let op = inner
    .operations
    .get(&id)
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not_found", "资源任务不存在。"))?;
  if op.snapshot.borrow().state == Status::Running {
    op.cancel.cancel();
  }
  Ok(snapshot(op))
}

pub(super) async fn pause(
  State(state): State<AppState>,
  Path(id): Path<u64>,
) -> Result<Json<Snapshot>, ApiError> {
  change_transfer(&state.resources, id, true).map(Json)
}

pub(super) async fn resume(
  State(state): State<AppState>,
  Path(id): Path<u64>,
) -> Result<Json<Snapshot>, ApiError> {
  change_transfer(&state.resources, id, false).map(Json)
}

fn change_transfer(
  resources: &ResourceOperations,
  id: u64,
  pause: bool,
) -> Result<Snapshot, ApiError> {
  let inner = resources
    .inner
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner);
  let op = inner.operations.get(&id).ok_or_else(|| {
    ApiError::new(
      StatusCode::NOT_FOUND,
      "not_found",
      "Resource operation not found",
    )
  })?;
  {
    let mut state = op
      .transfer
      .state
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    let conflict = || {
      ApiError::new(
        StatusCode::CONFLICT,
        "conflict",
        "Resource transfer cannot change state now",
      )
    };
    if op.cancel.is_cancelled() {
      return Err(conflict());
    }
    if pause {
      match &*state {
        Transfer::Pausing | Transfer::Paused => {}
        Transfer::Active(token) => {
          // Hold the live progress read lock through cancellation so the
          // worker cannot enter Publishing between the check and the request.
          let progress = op.progress.borrow();
          if !matches!(progress.stage, ResourceInstallStage::Downloading) {
            return Err(conflict());
          }
          token.cancel();
          *state = Transfer::Pausing;
        }
        _ => return Err(conflict()),
      }
    } else {
      match *state {
        Transfer::Paused => {
          *state = Transfer::Resuming;
          op.transfer.wake.notify_one();
        }
        Transfer::Resuming => {}
        _ => return Err(conflict()),
      }
    }
  }
  Ok(snapshot(op))
}

#[cfg(test)]
#[path = "../../tests/support/resource_pause.rs"]
mod tests;
