//! Local mod and resource-pack APIs; storage and layout rules remain in Core.
use super::api::{ApiError, AppState};
use anvil_core::resources::{ResourceManager, ResourceManifest, ResourcePackEntry};
use axum::{
  Json,
  body::{Body, Bytes},
  extract::{Path as RoutePath, Query, State},
  http::{Request, StatusCode},
  middleware::Next,
  response::{IntoResponse, Response},
};

// Limit uploads before body extraction: at most one 64 MiB resource request at a time.
static RESOURCE_REQUESTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
pub(super) async fn limit_resource_requests(request: Request<Body>, next: Next) -> Response {
  let Ok(_permit) = RESOURCE_REQUESTS.try_acquire() else {
    return ApiError::new(
      StatusCode::TOO_MANY_REQUESTS,
      "resource manager is busy; retry after refreshing",
    )
    .into_response();
  };
  next.run(request).await
}
pub(super) fn resource_error(error: anvil_core::Error) -> ApiError {
  let status = match &error {
    anvil_core::Error::Invalid(_) => StatusCode::BAD_REQUEST,
    anvil_core::Error::NotFound(_) => StatusCode::NOT_FOUND,
    anvil_core::Error::Busy(_) | anvil_core::Error::Conflict(_) => StatusCode::CONFLICT,
    _ => StatusCode::INTERNAL_SERVER_ERROR,
  };
  tracing::warn!(
    error_code = error.code(),
    "Client resource operation failed"
  );
  ApiError::new(status, error.to_string())
}
pub(super) async fn client_resources(
  State(state): State<AppState>,
  RoutePath(name): RoutePath<String>,
) -> Result<Json<ResourceManifest>, ApiError> {
  ResourceManager::new(state.runtime.context.clone())
    .list(&name)
    .await
    .map(Json)
    .map_err(resource_error)
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImportResourceQuery {
  filename: String,
  revision: u64,
}
pub(super) async fn import_client_resource(
  State(state): State<AppState>,
  RoutePath(name): RoutePath<String>,
  Query(query): Query<ImportResourceQuery>,
  bytes: axum::body::Bytes,
) -> Result<Json<ResourceManifest>, ApiError> {
  let result = ResourceManager::new(state.runtime.context.clone())
    .import(&name, query.filename, bytes.to_vec(), query.revision)
    .await
    .map_err(resource_error)?;
  tracing::info!(
    revision = result.revision,
    mod_count = result.mods.len(),
    "Local mod reference imported"
  );
  Ok(Json(result))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceRevision {
  revision: u64,
}
pub(super) async fn sync_client_resources(
  State(state): State<AppState>,
  RoutePath(name): RoutePath<String>,
  Json(request): Json<ResourceRevision>,
) -> Result<Json<ResourceManifest>, ApiError> {
  let result = ResourceManager::new(state.runtime.context.clone())
    .sync(&name, request.revision)
    .await
    .map_err(resource_error)?;
  tracing::info!(
    revision = result.revision,
    "Client resource layout synchronized"
  );
  Ok(Json(result))
}

pub(super) async fn remove_client_resource(
  State(state): State<AppState>,
  RoutePath((name, sha256)): RoutePath<(String, String)>,
  Query(query): Query<ResourceRevision>,
) -> Result<Json<ResourceManifest>, ApiError> {
  let result = ResourceManager::new(state.runtime.context.clone())
    .change(&name, sha256, query.revision, None)
    .await
    .map_err(resource_error)?;
  tracing::info!(revision = result.revision, "Local mod reference removed");
  Ok(Json(result))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceEnable {
  revision: u64,
  enabled: bool,
}
pub(super) async fn enable_client_resource(
  State(state): State<AppState>,
  RoutePath((name, sha256)): RoutePath<(String, String)>,
  Json(request): Json<ResourceEnable>,
) -> Result<Json<ResourceManifest>, ApiError> {
  let result = ResourceManager::new(state.runtime.context.clone())
    .change(&name, sha256, request.revision, Some(request.enabled))
    .await
    .map_err(resource_error)?;
  tracing::info!(
    revision = result.revision,
    enabled = request.enabled,
    "Local mod reference preference changed"
  );
  Ok(Json(result))
}

/// The resource revision is shared with mods; a stale view cannot overwrite another collection.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PackCollection {
  revision: u64,
  applied_revision: Option<u64>,
  packs: Vec<ResourcePackEntry>,
}
impl From<ResourceManifest> for PackCollection {
  fn from(manifest: ResourceManifest) -> Self {
    Self {
      revision: manifest.revision,
      applied_revision: manifest.applied_revision,
      packs: manifest.resource_packs,
    }
  }
}
pub(super) async fn packs(
  State(state): State<AppState>,
  RoutePath(name): RoutePath<String>,
) -> Result<Json<PackCollection>, ApiError> {
  ResourceManager::new(state.runtime.context.clone())
    .list(&name)
    .await
    .map(|manifest| Json(manifest.into()))
    .map_err(resource_error)
}
pub(super) async fn import_pack(
  State(state): State<AppState>,
  RoutePath(name): RoutePath<String>,
  Query(query): Query<ImportResourceQuery>,
  bytes: Bytes,
) -> Result<Json<PackCollection>, ApiError> {
  let manifest = ResourceManager::new(state.runtime.context.clone())
    .import_pack(&name, query.filename, bytes.to_vec(), query.revision)
    .await
    .map_err(resource_error)?;
  tracing::info!(
    revision = manifest.revision,
    pack_count = manifest.resource_packs.len(),
    "Local resource pack imported and applied"
  );
  Ok(Json(manifest.into()))
}
pub(super) async fn remove_pack(
  State(state): State<AppState>,
  RoutePath((name, sha256)): RoutePath<(String, String)>,
  Query(query): Query<ResourceRevision>,
) -> Result<Json<PackCollection>, ApiError> {
  let manifest = ResourceManager::new(state.runtime.context.clone())
    .remove_pack(&name, sha256, query.revision)
    .await
    .map_err(resource_error)?;
  tracing::info!(
    revision = manifest.revision,
    pack_count = manifest.resource_packs.len(),
    "Local resource pack removed"
  );
  Ok(Json(manifest.into()))
}
