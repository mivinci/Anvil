//! Instance-independent read-only discovery. Installation remains instance-scoped.
use super::api::{ApiError, AppState};
use axum::{
  Json,
  extract::{Path, Query, State},
  http::StatusCode,
};
use serde::Deserialize;
#[cfg(test)]
#[path = "../../tests/support/resource_discovery.rs"]
mod tests;

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Source {
  Modrinth,
  Curseforge,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Search {
  #[serde(default)]
  q: String,
  game_version: Option<String>,
  loader: Option<String>,
  #[serde(default)]
  index: u32,
  #[serde(default)]
  refresh: bool,
  after: Option<u64>,
}
impl Search {
  fn validate(&self) -> Result<(), ApiError> {
    if self.loader.as_deref().is_some_and(|l| l != "fabric")
      || self
        .game_version
        .as_deref()
        .is_some_and(|g| !anvil_core::metadata::is_valid_version_id(g))
      || self.q.len() > 128
      || self.q.chars().any(char::is_control)
      || self.index > 9980
    {
      return Err(ApiError::new(
        StatusCode::BAD_REQUEST,
        "模组查询无效；当前仅支持 Fabric。",
      ));
    }
    Ok(())
  }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Refresh {
  #[serde(default)]
  refresh: bool,
  after: Option<u64>,
}
pub(super) async fn search(
  State(state): State<AppState>,
  Path(source): Path<Source>,
  Query(q): Query<Search>,
) -> Result<Json<serde_json::Value>, ApiError> {
  q.validate()?;
  let context = state.runtime.context.clone();
  let result = match source {
    Source::Modrinth => {
      let _slot = state.modrinth.slot()?;
      let provider = state.modrinth.provider()?;
      let query = anvil_core::modrinth::DiscoveryQuery::Search {
        game: q.game_version,
        text: q.q.trim().into(),
        index: q.index,
      };
      state
        .modrinth
        .discovery
        .get(context, provider, query, q.refresh, q.after)
        .await
    }
    Source::Curseforge => {
      let _slot = state.curseforge.slot()?;
      let provider = state.curseforge.provider()?.clone();
      let query = anvil_core::curseforge::DiscoveryQuery::Search {
        game: q.game_version,
        text: q.q.trim().into(),
        index: q.index,
      };
      state
        .curseforge
        .discovery
        .get(context, provider, query, q.refresh, q.after)
        .await
    }
  };
  result
    .map(Json)
    .map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, e))
}
pub(super) async fn project(
  State(state): State<AppState>,
  Path((source, project)): Path<(Source, String)>,
  Query(q): Query<Refresh>,
) -> Result<Json<serde_json::Value>, ApiError> {
  let context = state.runtime.context.clone();
  let result = match source {
    Source::Modrinth => {
      let query = anvil_core::modrinth::DiscoveryQuery::Project { project };
      query.identity().map_err(super::api::resource_error)?;
      let _slot = state.modrinth.slot()?;
      let provider = state.modrinth.provider()?;
      state
        .modrinth
        .discovery
        .get(context, provider, query, q.refresh, q.after)
        .await
    }
    Source::Curseforge => {
      let project = project
        .parse::<u32>()
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "CurseForge 项目标识无效。"))?;
      let _slot = state.curseforge.slot()?;
      let provider = state.curseforge.provider()?.clone();
      let query = anvil_core::curseforge::DiscoveryQuery::Project { project };
      state
        .curseforge
        .discovery
        .get(context, provider, query, q.refresh, q.after)
        .await
    }
  };
  result
    .map(Json)
    .map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, e))
}
