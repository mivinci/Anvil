//! Desktop orchestration for read-only CurseForge discovery. Credentials stay in Runtime.
use super::{
  api::{ApiError, AppState},
  client_resources::fabric_game as game,
};
use anvil_core::curseforge::{CurseForge, DependencyPreview};
use axum::{
  Json,
  extract::{Path, Query, State},
  http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;

pub(super) struct Catalog {
  provider: Result<Arc<CurseForge>, ()>,
  pub(super) discovery: super::discovery_catalog::DiscoveryCatalog,
  slots: Semaphore,
}
impl Catalog {
  pub fn new() -> Self {
    let key = credential(
      std::env::var("ANVIL_CURSEFORGE_API_KEY").ok(),
      bundled_key(),
    );
    Self::with_key(key)
  }
  pub(super) fn with_key(key: Option<String>) -> Self {
    Self {
      provider: CurseForge::new(key).map(Arc::new).map_err(|_| ()),
      discovery: super::discovery_catalog::DiscoveryCatalog::default(),
      slots: Semaphore::new(2),
    }
  }
  pub(super) fn provider(&self) -> Result<&Arc<CurseForge>, ApiError> {
    let provider = self.provider.as_ref().map_err(|_| {
      ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "provider_not_configured",
        "CurseForge 配置无效，请检查 Runtime 环境变量。",
      )
    })?;
    if !provider.configured() {
      return Err(ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "provider_not_configured",
        "CurseForge API Key 尚未配置，正在等待申请。",
      ));
    }
    Ok(provider)
  }
  pub(super) fn slot(&self) -> Result<tokio::sync::SemaphorePermit<'_>, ApiError> {
    self.slots.try_acquire().map_err(|_| {
      ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "busy",
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
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Preview {
  game_version: String,
  revision: u64,
  #[serde(flatten)]
  preview: DependencyPreview,
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
  ApiError::new(status, error.code(), error.to_string())
}
async fn bounded<T>(
  future: impl std::future::Future<Output = Result<T, ApiError>>,
) -> Result<T, ApiError> {
  tokio::time::timeout(std::time::Duration::from_secs(50), future)
    .await
    .map_err(|_| {
      ApiError::new(
        StatusCode::GATEWAY_TIMEOUT,
        "request_timeout",
        "CurseForge 查询超时，请重试。",
      )
    })?
}
pub(super) async fn search(
  State(state): State<AppState>,
  Path(name): Path<String>,
  Query(query): Query<Search>,
) -> Result<Json<serde_json::Value>, ApiError> {
  let _slot = state.curseforge.slot()?;
  let provider = state.curseforge.provider()?.clone();
  let version = game(&state.runtime, &name).await?;
  let identity = anvil_core::curseforge::DiscoveryQuery::Search {
    game: Some(version),
    text: query.q.trim().into(),
    index: query.index,
  };
  state
    .curseforge
    .discovery
    .get(
      state.runtime.context.clone(),
      provider,
      identity,
      query.refresh,
      query.after,
    )
    .await
    .map(Json)
    .map_err(|error| ApiError::new(StatusCode::BAD_GATEWAY, "upstream_unavailable", error))
}
pub(super) async fn files(
  State(state): State<AppState>,
  Path((name, project)): Path<(String, u32)>,
  Query(query): Query<Paging>,
) -> Result<Json<serde_json::Value>, ApiError> {
  let _slot = state.curseforge.slot()?;
  let provider = state.curseforge.provider()?.clone();
  let version = game(&state.runtime, &name).await?;
  let identity = anvil_core::curseforge::DiscoveryQuery::Files {
    game: version,
    project,
    index: query.index,
  };
  state
    .curseforge
    .discovery
    .get(
      state.runtime.context.clone(),
      provider,
      identity,
      query.refresh,
      query.after,
    )
    .await
    .map(Json)
    .map_err(|error| ApiError::new(StatusCode::BAD_GATEWAY, "upstream_unavailable", error))
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
  selected_optional_projects: Vec<u32>,
  replacement: Option<super::resource_operations::ReplacementPreview<u32>>,
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
  Query(selection): Query<super::resource_operations::PlanSelection>,
) -> Result<Json<InstallPlanResponse>, ApiError> {
  let optional = selection.ids::<u32>()?;
  let replace_from = selection
    .replace_from
    .as_deref()
    .map(|id| {
      id.parse::<u32>()
        .ok()
        .filter(|id| *id != 0 && *id != file)
        .ok_or_else(|| {
          ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_argument",
            "请选择与当前版本不同的有效 CurseForge 版本。",
          )
        })
    })
    .transpose()?;

  let _slot = state.curseforge.slot()?;
  let provider = state.curseforge.provider()?;
  bounded(async {
    let game_version = game(&state.runtime, &name).await?;
    let manager = anvil_core::resources::ResourceManager::new(state.runtime.context.clone());
    let manifest = manager
      .list(&name)
      .await
      .map_err(super::api::resource_error)?;
    if let Some(previous) = &replace_from
      && !manifest.mods.iter().any(|entry| {
        entry
          .origin
          .as_ref()
          .is_some_and(|origin| origin.project_id == project && origin.file_id == *previous)
      })
    {
      return Err(super::resource_operations::replacement_changed());
    }
    let existing = manifest
      .mods
      .iter()
      .filter_map(|entry| entry.origin.as_ref().map(|o| (o.project_id, o.file_id)))
      .collect();
    let preview = provider
      .preview_with_optional(&game_version, project, file, &existing, &optional)
      .await
      .map_err(upstream)?;
    // Clone the bounded metadata once; URLs remain in Rust-only plan fields.
    let plan = manager
      .plan_curseforge_with_replacement(&name, preview.clone(), manifest.revision, replace_from)
      .await
      .map_err(super::api::resource_error)?;
    let replacement = plan.replaced_entry().map(|entry| {
      let origin = entry
        .origin
        .as_ref()
        .expect("replacement has a matching origin");
      let target = plan
        .files()
        .iter()
        .find(|file| file.direct)
        .expect("replacement has one selected project");
      super::resource_operations::ReplacementPreview {
        project_id: project,
        from_file_id: origin.file_id,
        to_file_id: file,
        from_filename: entry.filename.clone(),
        to_filename: target.filename.clone(),
        enabled: entry.enabled,
        dependents: manifest
          .mods
          .iter()
          .filter_map(|entry| entry.origin.as_ref())
          .filter(|entry| entry.required_projects.contains(&project))
          .map(|entry| entry.project_name.clone())
          .collect(),
      }
    });
    let response = InstallPlanResponse {
      replacement,
      selected_optional_projects: optional,
      revision: plan.revision(),
      download_bytes: plan.download_bytes(),
      files: plan.files().to_vec(),
      plan_id: state
        .resources
        .save(super::resource_operations::InstallPlan::CurseForge(plan)),
      expires_in_ms: 600_000,
      game_version,
      preview,
    };
    Ok(Json(response))
  })
  .await
}

// An explicit empty runtime value disables the bundled credential.
fn credential(runtime: Option<String>, bundled: Option<&str>) -> Option<String> {
  runtime.or_else(|| bundled.map(str::to_owned))
}

#[cfg(feature = "bundled-credentials")]
fn bundled_key() -> Option<&'static str> {
  option_env!("ANVIL_CURSEFORGE_API_KEY")
}

#[cfg(not(feature = "bundled-credentials"))]
fn bundled_key() -> Option<&'static str> {
  None
}

#[cfg(test)]
mod credential_tests {
  use super::credential;

  #[test]
  fn runtime_override_and_explicit_disable_take_precedence() {
    assert_eq!(
      credential(Some("runtime".into()), Some("bundled")),
      Some("runtime".into())
    );
    assert_eq!(
      credential(Some(String::new()), Some("bundled")),
      Some(String::new())
    );
    assert_eq!(credential(None, Some("bundled")), Some("bundled".into()));
    assert_eq!(credential(None, None), None);
  }

  #[cfg(feature = "bundled-credentials")]
  #[test]
  fn release_feature_uses_compile_time_configuration() {
    assert_eq!(
      super::bundled_key(),
      option_env!("ANVIL_CURSEFORGE_API_KEY")
    );
    assert_eq!(
      credential(None, super::bundled_key()),
      option_env!("ANVIL_CURSEFORGE_API_KEY").map(str::to_owned)
    );
  }

  #[cfg(not(feature = "bundled-credentials"))]
  #[test]
  fn ordinary_build_has_no_bundled_credential() {
    assert_eq!(super::bundled_key(), None);
  }
}
