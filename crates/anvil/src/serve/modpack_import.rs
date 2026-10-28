//! Register a task before streaming its bounded upload; Core owns installation.
use super::api::{ApiError, AppState};
use super::operations::{InstallOperationSnapshot, RuntimeState, StartInstallError};
use axum::{
  Json,
  body::Body,
  extract::{Path as RoutePath, State},
  http::StatusCode,
};
use http_body_util::BodyExt;
use serde::Deserialize;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

static UPLOADS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ImportRequest {
  request_id: String,
  filename: String,
  size: u64,
}
pub(super) async fn reserve(
  State(state): State<AppState>,
  Json(input): Json<ImportRequest>,
) -> Result<Json<InstallOperationSnapshot>, ApiError> {
  state
    .runtime
    .start_import(input.request_id, input.filename, input.size)
    .map(Json)
    .map_err(|error| match error {
      StartInstallError::Invalid(message) => ApiError::new(StatusCode::BAD_REQUEST, message),
      StartInstallError::Conflict(message) => ApiError::new(StatusCode::CONFLICT, message),
    })
}
struct UploadGuard {
  runtime: Arc<RuntimeState>,
  id: u64,
  armed: bool,
}
impl Drop for UploadGuard {
  fn drop(&mut self) {
    if self.armed {
      self.runtime.finish_import_upload(
        self.id,
        Err(anvil_core::Error::Invalid(
          "整合包上传中断，请重新导入".into(),
        )),
      );
    }
  }
}
pub(super) async fn upload(
  State(state): State<AppState>,
  RoutePath(id): RoutePath<u64>,
  mut body: Body,
) -> Result<Json<InstallOperationSnapshot>, ApiError> {
  let _permit = UPLOADS
    .try_acquire()
    .map_err(|_| ApiError::new(StatusCode::TOO_MANY_REQUESTS, "已有整合包正在上传"))?;
  let snapshot = state
    .runtime
    .operation(id)
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "导入任务不存在"))?;
  if snapshot.import_request_id.is_none() {
    return Err(ApiError::new(StatusCode::BAD_REQUEST, "不是整合包任务"));
  }
  let Some((cancel, expected)) = state.runtime.import_upload(id) else {
    return Ok(Json(snapshot));
  };
  let mut guard = UploadGuard {
    runtime: state.runtime.clone(),
    id,
    armed: true,
  };
  let temp = tempfile::Builder::new()
    .prefix("anvil-mrpack-upload-")
    .tempdir()
    .map_err(io_error)?;
  let mut file = tokio::fs::File::create(temp.path().join("source.mrpack"))
    .await
    .map_err(io_error)?;
  let mut bytes = 0u64;
  loop {
    let frame = tokio::select! {
      _=cancel.cancelled()=>return Err(ApiError::new(StatusCode::CONFLICT,"导入已取消")),
      frame=body.frame()=>frame,
    };
    let Some(frame) = frame else {
      break;
    };
    let frame = frame.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "整合包上传中断"))?;
    if let Ok(data) = frame.into_data() {
      bytes += data.len() as u64;
      if bytes > expected {
        return Err(ApiError::new(
          StatusCode::PAYLOAD_TOO_LARGE,
          "上传文件超过声明大小",
        ));
      }
      file.write_all(&data).await.map_err(io_error)?;
    }
  }
  file.sync_all().await.map_err(io_error)?;
  drop(file);
  if bytes != expected {
    return Err(ApiError::new(StatusCode::BAD_REQUEST, "整合包上传不完整"));
  }
  if cancel.is_cancelled() {
    return Err(ApiError::new(StatusCode::CONFLICT, "导入已取消"));
  }
  state.runtime.finish_import_upload(id, Ok(temp));
  guard.armed = false;
  state
    .runtime
    .operation(id)
    .map(Json)
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "导入任务不存在"))
}
fn io_error(_: std::io::Error) -> ApiError {
  ApiError::new(
    StatusCode::INTERNAL_SERVER_ERROR,
    "无法暂存整合包，请检查磁盘空间和权限",
  )
}

#[cfg(test)]
#[path = "../../tests/support/modpack_upload.rs"]
mod tests;
