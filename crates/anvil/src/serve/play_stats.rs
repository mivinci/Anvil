//! Read-only play statistics; sessions are recorded by the launch lifecycle.
use super::api::{ApiError, AppState};
use anvil_core::play_stats::PlayStatsSnapshot;
use axum::{Json, extract::State, http::StatusCode};

pub(super) async fn snapshot(State(state): State<AppState>) -> Json<PlayStatsSnapshot> {
  Json(state.play_stats.snapshot())
}

pub(super) async fn clear(
  State(state): State<AppState>,
) -> std::result::Result<Json<PlayStatsSnapshot>, ApiError> {
  let stats = state.play_stats.clone();
  tokio::task::spawn_blocking(move || stats.clear())
    .await
    .map_err(|error| {
      ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("无法清除游玩统计：{error}"),
      )
    })?
    .map_err(|error| {
      ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("无法清除游玩统计：{error}"),
      )
    })?;
  Ok(Json(state.play_stats.snapshot()))
}

#[cfg(test)]
#[path = "../../tests/support/play_stats_api.rs"]
mod play_stats_tests;
