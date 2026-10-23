//! Desktop orchestration for read-only CurseForge discovery. Credentials stay in Runtime.
use super::{
  api::{ApiError, AppState},
  operations::RuntimeState,
};
use anvil_core::{
  client::ClientInstaller,
  curseforge::{CatalogPage, CurseForge, DependencyPreview, ModFile, Project},
  fabric::ClientLoader,
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
  pub(super) installs: super::resource_operations::ResourceOperations,
  provider: Result<CurseForge, ()>,
  slots: Semaphore,
}
impl Catalog {
  pub fn new() -> Self {
    let key = std::env::var("ANVIL_CURSEFORGE_API_KEY").ok();
    Self {
      installs: super::resource_operations::ResourceOperations::new(),
      provider: CurseForge::new(key).map_err(|_| ()),
      slots: Semaphore::new(2),
    }
  }
  fn provider(&self) -> Result<&CurseForge, ApiError> {
    let provider = self.provider.as_ref().map_err(|_| {
      ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "CurseForge 配置无效，请检查 Runtime 环境变量。",
      )
    })?;
    if !provider.configured() {
      return Err(ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "CurseForge API Key 尚未配置，正在等待申请。",
      ));
    }
    Ok(provider)
  }
  fn slot(&self) -> Result<tokio::sync::SemaphorePermit<'_>, ApiError> {
    self.slots.try_acquire().map_err(|_| {
      ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "资源目录正在处理其他请求，请稍后重试。",
      )
    })
  }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Configuration {
  configured: bool,
  issue: Option<&'static str>,
}
pub(super) async fn configuration(State(state): State<AppState>) -> Json<Configuration> {
  let (configured, issue) = match state.curseforge.provider.as_ref() {
    Ok(provider) if provider.configured() => (true, None),
    Ok(_) => (false, Some("CurseForge API Key 尚未配置，正在等待申请。")),
    Err(_) => (
      false,
      Some("CurseForge 配置无效，请检查 Runtime 环境变量。"),
    ),
  };
  Json(Configuration { configured, issue })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Search {
  #[serde(default)]
  q: String,
  #[serde(default)]
  index: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Paging {
  #[serde(default)]
  index: u32,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Preview {
  game_version: String,
  revision: u64,
  #[serde(flatten)]
  preview: DependencyPreview,
}
async fn game(runtime: &Arc<RuntimeState>, name: &str) -> Result<String, ApiError> {
  let (_, record) = ClientInstaller::new(runtime.context.clone())
    .details(name)
    .await
    .map_err(super::api::resource_error)?;
  if !matches!(record.loader, Some(ClientLoader::Fabric { .. })) {
    return Err(ApiError::new(
      StatusCode::CONFLICT,
      "当前仅支持为 Fabric 实例浏览模组。",
    ));
  }
  Ok(record.version_id)
}
fn upstream(error: anvil_core::Error) -> ApiError {
  tracing::warn!(
    error_code = error.code(),
    "CurseForge metadata request failed"
  );
  let status = match error {
    anvil_core::Error::Invalid(_) => StatusCode::BAD_REQUEST,
    anvil_core::Error::Conflict(_) => StatusCode::CONFLICT,
    _ => StatusCode::BAD_GATEWAY,
  };
  // Core sanitizes transport errors and never forwards the upstream body or credential.
  ApiError::new(status, error.to_string())
}
async fn bounded<T>(
  future: impl std::future::Future<Output = Result<T, ApiError>>,
) -> Result<T, ApiError> {
  tokio::time::timeout(std::time::Duration::from_secs(50), future)
    .await
    .map_err(|_| ApiError::new(StatusCode::GATEWAY_TIMEOUT, "CurseForge 查询超时，请重试。"))?
}
pub(super) async fn search(
  State(state): State<AppState>,
  Path(name): Path<String>,
  Query(query): Query<Search>,
) -> Result<Json<CatalogPage<Project>>, ApiError> {
  let _slot = state.curseforge.slot()?;
  let provider = state.curseforge.provider()?;
  bounded(async {
    let game = game(&state.runtime, &name).await?;
    provider
      .search(&game, query.q.trim(), query.index)
      .await
      .map(Json)
      .map_err(upstream)
  })
  .await
}
pub(super) async fn files(
  State(state): State<AppState>,
  Path((name, project)): Path<(String, u32)>,
  Query(query): Query<Paging>,
) -> Result<Json<CatalogPage<ModFile>>, ApiError> {
  let _slot = state.curseforge.slot()?;
  let provider = state.curseforge.provider()?;
  bounded(async {
    let game = game(&state.runtime, &name).await?;
    provider
      .files(&game, project, query.index)
      .await
      .map(Json)
      .map_err(upstream)
  })
  .await
}
pub(super) async fn preview(
  State(state): State<AppState>,
  Path((name, project, file)): Path<(String, u32, u32)>,
) -> Result<Json<Preview>, ApiError> {
  let _slot = state.curseforge.slot()?;
  let provider = state.curseforge.provider()?;
  bounded(async {
    let game_version = game(&state.runtime, &name).await?;
    let manifest = anvil_core::resources::ResourceManager::new(state.runtime.context.clone())
      .list(&name)
      .await
      .map_err(super::api::resource_error)?;
    let preview = provider
      .preview(&game_version, project, file)
      .await
      .map_err(upstream)?;
    Ok(Json(Preview {
      game_version,
      revision: manifest.revision,
      preview,
    }))
  })
  .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InstallPlanResponse {
  plan_id: u64,
  expires_in_ms: u64,
  revision: u64,
  game_version: String,
  files: Vec<anvil_core::resources::ResourcePlanFile>,
  download_bytes: u64,
  #[serde(flatten)]
  preview: DependencyPreview,
}
pub(super) async fn create_plan(
  State(state): State<AppState>,
  Path((name, project, file)): Path<(String, u32, u32)>,
) -> Result<Json<InstallPlanResponse>, ApiError> {
  let _slot = state.curseforge.slot()?;
  let provider = state.curseforge.provider()?;
  bounded(async {
    let game_version = game(&state.runtime, &name).await?;
    let manager = anvil_core::resources::ResourceManager::new(state.runtime.context.clone());
    let manifest = manager
      .list(&name)
      .await
      .map_err(super::api::resource_error)?;
    let existing = manifest
      .mods
      .iter()
      .filter_map(|entry| entry.origin.as_ref().map(|o| (o.project_id, o.file_id)))
      .collect();
    let preview = provider
      .preview_with_existing(&game_version, project, file, &existing)
      .await
      .map_err(upstream)?;
    // Clone the bounded metadata once; URLs remain in Rust-only plan fields.
    let plan = manager
      .plan_curseforge(&name, preview.clone(), manifest.revision)
      .await
      .map_err(super::api::resource_error)?;
    let response = InstallPlanResponse {
      revision: plan.revision(),
      download_bytes: plan.download_bytes(),
      files: plan.files().to_vec(),
      plan_id: state.curseforge.installs.save(plan),
      expires_in_ms: 600_000,
      game_version,
      preview,
    };
    Ok(Json(response))
  })
  .await
}
