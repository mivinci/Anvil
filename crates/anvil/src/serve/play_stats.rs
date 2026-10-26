//! Play statistics are recorded by the launch lifecycle and explicitly cleared here.
use super::api::{ApiError, AppState};
use anvil_core::play_stats::PlayStatsSnapshot;
use axum::{
  Json,
  extract::{Path, State},
  http::StatusCode,
};

pub(super) async fn snapshot(State(state): State<AppState>) -> Json<PlayStatsSnapshot> {
  Json(state.play_stats.snapshot())
}

pub(super) async fn clear(
  State(state): State<AppState>,
) -> std::result::Result<Json<PlayStatsSnapshot>, ApiError> {
  clear_target(state, None).await
}

pub(super) async fn clear_account(
  State(state): State<AppState>,
  Path(account_id): Path<String>,
) -> std::result::Result<Json<PlayStatsSnapshot>, ApiError> {
  clear_target(state, Some(account_id)).await
}

async fn clear_target(
  state: AppState,
  account_id: Option<String>,
) -> std::result::Result<Json<PlayStatsSnapshot>, ApiError> {
  let launches = state.launches.clone();
  tokio::task::spawn_blocking(move || launches.clear_play_stats(account_id.as_deref()))
    .await
    .map_err(|error| {
      ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("无法清除游玩统计：{error}"),
      )
    })?
    .map(Json)
}

#[cfg(test)]
#[path = "../../tests/support/play_stats_api.rs"]
mod play_stats_tests;
