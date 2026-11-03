//! Authenticated per-instance avatar file API; Core owns decoding and atomic storage.
use super::api::{ApiError, AppState};
use anvil_core::client::{ClientInstaller, MAX_AVATAR_INPUT_BYTES};
use axum::{
  Json,
  body::Bytes,
  extract::{Path, State},
  http::{
    HeaderMap, StatusCode,
    header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, IF_NONE_MATCH},
  },
  response::{IntoResponse, Response},
};
use serde::Serialize;

static AVATAR_WRITES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AvatarChange {
  revision: String,
}

fn write_permit() -> Result<tokio::sync::SemaphorePermit<'static>, ApiError> {
  AVATAR_WRITES.try_acquire().map_err(|_| {
    ApiError::new(
      StatusCode::TOO_MANY_REQUESTS,
      "avatar_busy",
      "头像正在保存，请稍后重试。",
    )
  })
}

pub(super) async fn get(
  State(state): State<AppState>,
  Path(name): Path<String>,
  headers: HeaderMap,
) -> Result<Response, ApiError> {
  let avatar = ClientInstaller::new(state.runtime.context.clone())
    .avatar(&name)
    .await
    .map_err(super::api::resource_error)?
    .ok_or_else(|| {
      ApiError::new(
        StatusCode::NOT_FOUND,
        "avatar_not_found",
        "实例还没有自定义头像。",
      )
    })?;
  let etag = format!("\"{}\"", avatar.revision);
  if headers
    .get(IF_NONE_MATCH)
    .and_then(|value| value.to_str().ok())
    .is_some_and(|value| {
      value
        .split(',')
        .any(|candidate| candidate.trim() == etag || candidate.trim() == "*")
    })
  {
    return Ok(StatusCode::NOT_MODIFIED.into_response());
  }
  let mut response = avatar.png.into_response();
  let response_headers = response.headers_mut();
  response_headers.insert(CONTENT_TYPE, "image/png".parse().expect("static header"));
  response_headers.insert(ETAG, etag.parse().expect("hex etag"));
  response_headers.insert(
    CACHE_CONTROL,
    "private, max-age=0, must-revalidate"
      .parse()
      .expect("static header"),
  );
  Ok(response)
}

pub(super) async fn save(
  State(state): State<AppState>,
  Path(name): Path<String>,
  bytes: Bytes,
) -> Result<Json<AvatarChange>, ApiError> {
  let _permit = write_permit()?;
  let avatar = ClientInstaller::new(state.runtime.context.clone())
    .save_avatar(&name, bytes.to_vec())
    .await
    .map_err(super::api::resource_error)?;
  tracing::info!(revision = %avatar.revision, "Instance avatar saved");
  Ok(Json(AvatarChange {
    revision: avatar.revision,
  }))
}

pub(super) async fn clear(
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
  let _permit = write_permit()?;
  ClientInstaller::new(state.runtime.context.clone())
    .clear_avatar(&name)
    .await
    .map_err(super::api::resource_error)?;
  tracing::info!("Instance avatar cleared");
  Ok(StatusCode::NO_CONTENT)
}

pub(super) const MAX_BODY_BYTES: usize = MAX_AVATAR_INPUT_BYTES;

#[cfg(test)]
#[path = "../../tests/support/client_avatar_api.rs"]
mod tests;
