//! Modrinth browse and plan orchestration; installs share the resource task lifecycle.
use super::api::{ApiError, AppState};
use anvil_core::{
  modrinth::{DependencyPreview, DiscoveryQuery, Modrinth},
  resources::{ModrinthPlanFile, ResourceManager},
};
use axum::{
  Json,
  extract::{Path, Query, State},
  http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;

pub(super) struct Catalog {
  provider: Result<Arc<Modrinth>, ()>,
  pub(super) discovery: super::modrinth_catalog::DiscoveryCatalog,
  pub(super) shader_discovery: super::shader_catalog::DiscoveryCatalog,
  slots: Semaphore,
}
impl Catalog {
  pub fn new() -> Self {
    Self {
      provider: Modrinth::new().map(Arc::new).map_err(|_| ()),
      discovery: Default::default(),
      shader_discovery: Default::default(),
      slots: Semaphore::new(2),
    }
  }
  pub(super) fn provider(&self) -> Result<Arc<Modrinth>, ApiError> {
    self.provider.as_ref().cloned().map_err(|_| {
      ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "Modrinth HTTP 初始化失败。",
      )
    })
  }
  pub(super) fn slot(&self) -> Result<tokio::sync::SemaphorePermit<'_>, ApiError> {
    self.slots.try_acquire().map_err(|_| {
      ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "Modrinth 正在处理其他请求，请稍后重试。",
      )
    })
  }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Search {
  #[serde(default)]
  q: String,
  #[serde(default)]
  index: u32,
  #[serde(default)]
  refresh: bool,
  after: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Paging {
  #[serde(default)]
  index: u32,
  #[serde(default)]
  refresh: bool,
  after: Option<u64>,
}
pub(super) async fn search(
  State(state): State<AppState>,
  Path(name): Path<String>,
  Query(q): Query<Search>,
) -> Result<Json<serde_json::Value>, ApiError> {
  let _slot = state.modrinth.slot()?;
  let provider = state.modrinth.provider()?;
  let game = super::client_resources::fabric_game(&state.runtime, &name).await?;
  let query = DiscoveryQuery::Search {
    game: Some(game),
    text: q.q.trim().into(),
    index: q.index,
  };
  query.identity().map_err(super::api::resource_error)?;
  state
    .modrinth
    .discovery
    .get(
      state.runtime.context.clone(),
      provider,
      query,
      q.refresh,
      q.after,
    )
    .await
    .map(Json)
    .map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, e))
}
pub(super) async fn files(
  State(state): State<AppState>,
  Path((name, project)): Path<(String, String)>,
  Query(q): Query<Paging>,
) -> Result<Json<serde_json::Value>, ApiError> {
  let _slot = state.modrinth.slot()?;
  let provider = state.modrinth.provider()?;
  let game = super::client_resources::fabric_game(&state.runtime, &name).await?;
  let query = DiscoveryQuery::Files {
    game,
    project,
    index: q.index,
  };
  query.identity().map_err(super::api::resource_error)?;
  state
    .modrinth
    .discovery
    .get(
      state.runtime.context.clone(),
      provider,
      query,
      q.refresh,
      q.after,
    )
    .await
    .map(Json)
    .map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, e))
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InstallPlanResponse {
  plan_id: u64,
  expires_in_ms: u64,
  revision: u64,
  game_version: String,
  files: Vec<ModrinthPlanFile>,
  download_bytes: u64,
  #[serde(flatten)]
  preview: DependencyPreview,
}
pub(super) async fn create_plan(
  State(state): State<AppState>,
  Path((name, project, file)): Path<(String, String, String)>,
) -> Result<Json<InstallPlanResponse>, ApiError> {
  let _slot = state.modrinth.slot()?;
  let provider = state.modrinth.provider()?;
  tokio::time::timeout(std::time::Duration::from_secs(50), async {
    let game_version = super::client_resources::fabric_game(&state.runtime, &name).await?;
    let manager = ResourceManager::new(state.runtime.context.clone());
    let manifest = manager
      .list(&name)
      .await
      .map_err(super::api::resource_error)?;
    let existing = manifest
      .mods
      .iter()
      .filter_map(|e| {
        e.modrinth_origin
          .as_ref()
          .map(|o| (o.project_id.clone(), o.version_id.clone()))
      })
      .collect();
    let preview = provider
      .preview(&game_version, &project, &file, &existing)
      .await
      .map_err(super::api::resource_error)?;
    let plan = manager
      .plan_modrinth(&name, preview.clone(), manifest.revision)
      .await
      .map_err(super::api::resource_error)?;
    Ok(Json(InstallPlanResponse {
      revision: plan.revision(),
      files: plan.files().to_vec(),
      download_bytes: plan.download_bytes(),
      plan_id: state
        .resources
        .save(super::resource_operations::InstallPlan::Modrinth(plan)),
      expires_in_ms: 600_000,
      game_version,
      preview,
    }))
  })
  .await
  .map_err(|_| {
    ApiError::new(
      StatusCode::GATEWAY_TIMEOUT,
      "Modrinth 依赖查询超时，请稍后重试。",
    )
  })?
}
