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
use serde::Serialize;
use std::{
  collections::{BTreeMap, VecDeque},
  sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
  },
  time::{Duration, Instant},
};
use tokio::sync::watch;

const LIMIT: usize = 8;
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
struct Operation {
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
  pub progress: ResourceInstallProgress,
  pub revision: Option<u64>,
  pub error: Option<String>,
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
        snapshot.name.eq_ignore_ascii_case(name) && snapshot.state == Status::Running
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
        "Runtime 正在停止。",
      ));
    }
    if let Some(operation) = inner.operations.get(&id) {
      let snapshot = snapshot(operation);
      if snapshot.name != name.to_ascii_lowercase() {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "安装计划不存在。"));
      }
      return Ok(snapshot);
    }
    if inner
      .operations
      .values()
      .any(|op| op.snapshot.borrow().state == Status::Running)
    {
      return Err(ApiError::new(
        StatusCode::CONFLICT,
        "已有资源安装任务，请先等待完成或取消。",
      ));
    }
    let saved = inner
      .plans
      .iter()
      .find(|p| p.id == id && p.plan.name() == name.to_ascii_lowercase() && p.at.elapsed() < TTL)
      .ok_or_else(|| ApiError::new(StatusCode::CONFLICT, "安装计划已过期，请重新预览。"))?;
    let plan = saved.plan.clone();
    let progress = plan.progress();
    let initial = Snapshot {
      id,
      name: plan.name().into(),
      kind: plan.kind(),
      state: Status::Running,
      cancel_requested: false,
      progress: progress.clone(),
      revision: None,
      error: None,
    };
    let (updates, receiver) = watch::channel(initial.clone());
    let (progress_tx, mut progress_rx) = watch::channel(progress);
    let cancel = CancellationToken::new();
    while inner.operations.len() >= LIMIT {
      let id = *inner.operations.keys().next().expect("nonempty");
      inner.operations.remove(&id);
    }
    inner.operations.insert(
      id,
      Operation {
        cancel: cancel.clone(),
        snapshot: receiver,
      },
    );
    tokio::spawn(async move {
      tracing::info!(operation_id = id, "Resource installation started");
      let forward = updates.clone();
      let observer = tokio::spawn(async move {
        while progress_rx.changed().await.is_ok() {
          let value = progress_rx.borrow_and_update().clone();
          forward.send_modify(|s| s.progress = value.clone());
        }
      });
      let result = tokio::spawn(async move {
        let manager = ResourceManager::new(context);
        match plan {
          InstallPlan::CurseForge(p) => manager.install_curseforge(p, cancel, progress_tx).await,
          InstallPlan::Modrinth(p) => manager.install_modrinth(p, cancel, progress_tx).await,
          InstallPlan::Shader(p) => manager.install_shader(p, cancel, progress_tx).await,
        }
      })
      .await
      .map_err(anvil_core::Error::from)
      .and_then(|result| result);
      let _ = observer.await;
      updates.send_modify(|s| match &result {
        Ok(manifest) => {
          s.state = Status::Completed;
          s.progress.stage = ResourceInstallStage::Completed;
          s.revision = Some(manifest.revision);
        }
        Err(error) if error.code() == "cancelled" => s.state = Status::Cancelled,
        Err(error) => {
          s.state = Status::Failed;
          s.error = Some(format!(
            "安装未能完整结束，请刷新实例资源清单确认结果。{error}"
          ));
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
  let inner = state
    .resources
    .inner
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner);
  let op = inner
    .operations
    .get(&id)
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "资源任务不存在。"))?;
  if op.snapshot.borrow().state == Status::Running {
    op.cancel.cancel();
  }
  Ok(Json(snapshot(op)))
}
