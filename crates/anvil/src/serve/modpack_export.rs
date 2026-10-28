//! Window-independent modpack export tasks; Core owns the bounded snapshot and ZIP.
use super::api::{ApiError, AppState};
use anvil_core::{
  CancellationToken,
  modpack::{ExportOptions, ExportProgress, ExportStage, ModpackExporter},
};
use axum::{
  Json,
  extract::{Path, State},
  http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::{
  collections::BTreeMap,
  path::{Path as FsPath, PathBuf},
  sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
  },
};
use tokio::sync::watch;

const LIMIT: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ExportState {
  Running,
  Completed,
  Failed,
  Cancelled,
}
impl ExportState {
  fn active(self) -> bool {
    self == Self::Running
  }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ResultView {
  path: String,
  size: u64,
  sha256: String,
  files: usize,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Snapshot {
  pub id: u64,
  pub request_id: String,
  pub name: String,
  pub state: ExportState,
  pub cancel_requested: bool,
  pub progress: ExportProgress,
  pub result: Option<ResultView>,
  pub error: Option<String>,
}
struct Operation {
  cancel: CancellationToken,
  snapshot: watch::Receiver<Snapshot>,
  identity: String,
}
struct Inner {
  operations: BTreeMap<u64, Operation>,
  requests: BTreeMap<String, u64>,
  stopping: bool,
}
pub(super) struct ModpackExports {
  context: anvil_core::Context,
  destination: Option<PathBuf>,
  inner: Mutex<Inner>,
  next: AtomicU64,
}
impl ModpackExports {
  pub(super) fn new(context: anvil_core::Context) -> Self {
    let destination = downloads_directory();
    Self::with_destination(context, destination)
  }
  fn with_destination(context: anvil_core::Context, destination: Option<PathBuf>) -> Self {
    Self {
      context,
      destination,
      inner: Mutex::new(Inner {
        operations: BTreeMap::new(),
        requests: BTreeMap::new(),
        stopping: false,
      }),
      next: AtomicU64::new(1),
    }
  }
  pub(super) fn create(
    self: &std::sync::Arc<Self>,
    name: &str,
    request_id: String,
    options: ExportOptions,
  ) -> Result<Snapshot, ApiError> {
    anvil_core::client::ClientInstaller::validate_name(name).map_err(super::api::resource_error)?;
    options.validate().map_err(super::api::resource_error)?;
    if request_id.len() != 36
      || !request_id
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
      return Err(ApiError::new(StatusCode::BAD_REQUEST, "导出请求标识无效。"));
    }
    let identity = format!(
      "{}\0{}",
      name.to_ascii_lowercase(),
      serde_json::to_string(&options)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "导出选项无效。"))?
    );
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
    if let Some(id) = inner.requests.get(&request_id).copied() {
      let operation = inner
        .operations
        .get(&id)
        .expect("request points at an operation");
      if operation.identity != identity {
        return Err(ApiError::new(
          StatusCode::CONFLICT,
          "导出请求标识已绑定其他实例或选项。",
        ));
      }
      return Ok(snapshot(operation));
    }
    if inner
      .operations
      .values()
      .any(|operation| operation.snapshot.borrow().state.active())
    {
      return Err(ApiError::new(
        StatusCode::CONFLICT,
        "已有整合包导出任务，请先等待完成或取消。",
      ));
    }
    let destination = self.destination.clone().ok_or_else(|| {
      ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "无法找到系统下载目录，暂不能导出整合包。",
      )
    })?;
    if !destination.is_dir() {
      return Err(ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "系统下载目录不可用，暂不能导出整合包。",
      ));
    }
    let id = self.next.fetch_add(1, Ordering::Relaxed);
    while inner.operations.len() >= LIMIT {
      let Some(oldest) = inner
        .operations
        .iter()
        .find(|(_, operation)| !operation.snapshot.borrow().state.active())
        .map(|(id, _)| *id)
      else {
        break;
      };
      inner.operations.remove(&oldest);
      inner.requests.retain(|_, value| *value != oldest);
    }
    let progress = ExportProgress::default();
    let initial = Snapshot {
      id,
      request_id: request_id.clone(),
      name: name.to_ascii_lowercase(),
      state: ExportState::Running,
      cancel_requested: false,
      progress,
      result: None,
      error: None,
    };
    let (updates, receiver) = watch::channel(initial.clone());
    let cancel = CancellationToken::new();
    inner.operations.insert(
      id,
      Operation {
        cancel: cancel.clone(),
        snapshot: receiver,
        identity,
      },
    );
    inner.requests.insert(request_id, id);
    drop(inner);
    let context = self.context.clone();
    let instance = name.to_owned();
    tokio::spawn(async move {
      let exporter = ModpackExporter::new(context);
      let path = export_filename(&destination, &options);
      let (progress_tx, mut progress_rx) = watch::channel(ExportProgress::default());
      let forward = updates.clone();
      let observer = tokio::spawn(async move {
        while progress_rx.changed().await.is_ok() {
          let value = progress_rx.borrow_and_update().clone();
          forward.send_modify(|snapshot| snapshot.progress = value);
        }
      });
      let result = exporter
        .export(&instance, options, path, &cancel, &progress_tx)
        .await;
      drop(progress_tx);
      let _ = observer.await;
      updates.send_modify(|snapshot| match result {
        Ok(result) => {
          snapshot.state = ExportState::Completed;
          snapshot.progress.stage = ExportStage::Completed;
          snapshot.result = Some(ResultView {
            path: result.path.to_string_lossy().into_owned(),
            size: result.size,
            sha256: result.sha256,
            files: result.files,
          });
        }
        Err(error) if error.code() == "cancelled" => {
          snapshot.state = ExportState::Cancelled;
          snapshot.error = Some(error.to_string());
        }
        Err(error) => {
          snapshot.state = ExportState::Failed;
          snapshot.error = Some(error.to_string());
        }
      });
    });
    Ok(initial)
  }
  pub(super) fn busy_for_client(&self, name: &str) -> bool {
    self
      .inner
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .operations
      .values()
      .any(|operation| {
        let snapshot = operation.snapshot.borrow();
        snapshot.state.active() && snapshot.name.eq_ignore_ascii_case(name)
      })
  }
  pub(super) fn list(&self) -> Vec<Snapshot> {
    self
      .inner
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .operations
      .values()
      .map(snapshot)
      .collect()
  }
  pub(super) fn get(&self, id: u64) -> Option<Snapshot> {
    self
      .inner
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .operations
      .get(&id)
      .map(snapshot)
  }
  pub(super) fn cancel(&self, id: u64) -> Option<Snapshot> {
    let inner = self
      .inner
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    let operation = inner.operations.get(&id)?;
    if operation.snapshot.borrow().state.active() {
      operation.cancel.cancel();
    }
    Some(snapshot(operation))
  }
  pub(super) async fn shutdown(&self) {
    let receivers = {
      let mut inner = self
        .inner
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      inner.stopping = true;
      inner
        .operations
        .values()
        .map(|operation| {
          operation.cancel.cancel();
          operation.snapshot.clone()
        })
        .collect::<Vec<_>>()
    };
    for mut receiver in receivers {
      while receiver.borrow().state.active() {
        if receiver.changed().await.is_err() {
          break;
        }
      }
    }
  }
}
fn snapshot(operation: &Operation) -> Snapshot {
  let mut value = operation.snapshot.borrow().clone();
  value.cancel_requested = value.state.active() && operation.cancel.is_cancelled();
  value
}
fn downloads_directory() -> Option<PathBuf> {
  directories::UserDirs::new()
    .and_then(|dirs| dirs.download_dir().map(FsPath::to_path_buf))
    .filter(|path| path.is_dir())
}
fn safe_part(value: &str) -> String {
  value
    .chars()
    .map(|character| {
      if character.is_control() || "<>:\"/\\|?*".contains(character) {
        '_'
      } else {
        character
      }
    })
    .collect::<String>()
    .trim_matches([' ', '.'])
    .to_owned()
}
fn export_filename(destination: &FsPath, options: &ExportOptions) -> PathBuf {
  let base = format!(
    "{}-{}",
    safe_part(&options.name),
    safe_part(&options.version_id)
  );
  let base = base.trim_matches([' ', '.']);
  let base = if base.is_empty() { "modpack" } else { base };
  let reserved = base
    .to_uppercase()
    .split('.')
    .next()
    .is_some_and(|stem| ["CON", "PRN", "AUX", "NUL"].contains(&stem));
  let stem = if reserved {
    format!("modpack-{base}")
  } else {
    base.to_owned()
  };
  let mut candidate = stem.clone();
  let mut index = 0;
  loop {
    let path = destination.join(format!("{candidate}.mrpack"));
    if !path.exists() || index >= 99 {
      return path;
    }
    index += 1;
    candidate = format!("{stem} ({index})");
  }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CreateRequest {
  request_id: String,
  options: ExportOptions,
}
pub(super) async fn create(
  State(state): State<AppState>,
  Path(name): Path<String>,
  Json(request): Json<CreateRequest>,
) -> Result<Json<Snapshot>, ApiError> {
  if state.runtime.busy_for_client(&name)
    || state.launches.busy_for_client(&name)
    || state.resources.busy_for_client(&name)
  {
    return Err(ApiError::new(
      StatusCode::CONFLICT,
      "实例正在运行、准备启动或修改资源，请等待完成后再导出。",
    ));
  }
  state
    .exports
    .create(&name, request.request_id, request.options)
    .map(Json)
}
pub(super) async fn list(State(state): State<AppState>) -> Json<Vec<Snapshot>> {
  Json(state.exports.list())
}
pub(super) async fn get(
  State(state): State<AppState>,
  Path(id): Path<u64>,
) -> Result<Json<Snapshot>, ApiError> {
  state
    .exports
    .get(id)
    .map(Json)
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "整合包导出任务不存在。"))
}
pub(super) async fn cancel(
  State(state): State<AppState>,
  Path(id): Path<u64>,
) -> Result<Json<Snapshot>, ApiError> {
  state
    .exports
    .cancel(id)
    .map(Json)
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "整合包导出任务不存在。"))
}
#[cfg(test)]
mod tests {
  use super::*;
  use anvil_core::{Context, Paths};
  use serde_json::json;

  fn instance(root: &FsPath) -> (Context, String) {
    let context = Context::new(Paths {
      data: root.join("data"),
      cache: root.join("cache"),
    })
    .unwrap();
    let name = "test";
    let directory = context.paths.data.join("instances/client-test");
    std::fs::create_dir_all(directory.join("game")).unwrap();
    std::fs::write(
      directory.join("instance.json"),
      serde_json::to_vec(&json!({
        "schema_version": 1, "instance_type": "client", "id": "client-test", "name": name
      }))
      .unwrap(),
    )
    .unwrap();
    std::fs::write(
      directory.join("installed.json"),
      serde_json::to_vec(&json!({
        "schema_version": 1, "version_id": "1.21.1", "version_type": "release",
        "metadata_url": "https://piston-meta.mojang.com/v1/packages/example/1.21.1.json",
        "metadata_sha1": "1".repeat(40), "java_component": "java-runtime-gamma",
        "java_major": 21, "os": "linux", "architecture": "x86_64",
        "asset_index_id": "1.21.1", "main_class": "net.minecraft.client.main.Main",
        "arguments": {"game": [], "jvm": []},
        "loader": {"kind": "fabric", "version": "0.16.0"},
        "files": [
          {"path": "versions/1.21.1/1.21.1.jar", "sha1": "1".repeat(40), "size": 1,
           "cache_path": format!("clients/sha1/{}", "1".repeat(40))},
          {"path": "assets/indexes/1.21.1.json", "sha1": "1".repeat(40), "size": 1,
           "cache_path": format!("clients/sha1/{}", "1".repeat(40))}
        ]
      }))
      .unwrap(),
    )
    .unwrap();
    std::fs::write(directory.join("game/options.txt"), b"fov:0.5\n").unwrap();
    (context, name.into())
  }

  #[tokio::test]
  async fn export_task_writes_a_verified_pack_without_replacing_files() {
    let root = tempfile::tempdir().unwrap();
    let downloads = root.path().join("downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    let (context, name) = instance(root.path());
    let exports = std::sync::Arc::new(ModpackExports::with_destination(
      context,
      Some(downloads.clone()),
    ));
    let options = ExportOptions {
      name: "Pack".into(),
      version_id: "1.0.0".into(),
      selection: Default::default(),
    };
    let created = exports
      .create(&name, "a".repeat(36), options.clone())
      .unwrap();
    assert_eq!(created.state, ExportState::Running);
    assert!(exports.busy_for_client(&name));
    let mut final_state = None;
    for _ in 0..500 {
      if let Some(value) = exports.get(created.id)
        && value.state != ExportState::Running
      {
        final_state = Some(value);
        break;
      }
      tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let value = final_state
      .unwrap_or_else(|| panic!("export task did not finish: {:?}", exports.get(created.id)));
    assert_eq!(value.state, ExportState::Completed, "{value:?}");
    let result = value.result.expect("completed export result");
    assert!(std::path::Path::new(&result.path).is_file());
    assert!(result.files >= 1);
    let second = exports.create(&name, "b".repeat(36), options).unwrap();
    let mut duplicate = None;
    for _ in 0..500 {
      if let Some(value) = exports.get(second.id)
        && value.state != ExportState::Running
      {
        duplicate = Some(value);
        break;
      }
      tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let duplicate = duplicate.expect("second export did not finish");
    assert_eq!(duplicate.state, ExportState::Completed, "{duplicate:?}");
    assert_ne!(
      duplicate.result.unwrap().path,
      result.path,
      "a second export must not replace the first"
    );
  }

  #[test]
  fn export_filename_sanitizes_and_avoids_collisions() {
    let options = |name: &str, version: &str| ExportOptions {
      name: name.into(),
      version_id: version.into(),
      selection: Default::default(),
    };
    let root = tempfile::tempdir().unwrap();
    let name = export_filename(root.path(), &options("海岛生存", "1.0.0"));
    assert_eq!(name.file_name().unwrap(), "海岛生存-1.0.0.mrpack");
    std::fs::write(name, b"existing").unwrap();
    let next = export_filename(root.path(), &options("海岛生存", "1.0.0"));
    assert_eq!(next.file_name().unwrap(), "海岛生存-1.0.0 (1).mrpack");
    let sanitized = export_filename(root.path(), &options("a/b:c", "1.0"));
    assert_eq!(
      sanitized.file_name().unwrap().to_string_lossy(),
      "a_b_c-1.0.mrpack"
    );
  }
}
