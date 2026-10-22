//! One bounded, process-local catalogue; concurrent refreshes share one worker.
use anvil_core::{
  Context,
  metadata::{self, GameVersion},
};
use std::{
  sync::{Arc, Mutex},
  time::{Duration, Instant},
};
use tokio::sync::watch;

const TTL: Duration = Duration::from_secs(30 * 60);
type Refresh = watch::Receiver<Option<Result<(), String>>>;

#[derive(Default)]
struct State {
  cached: Option<(Vec<GameVersion>, Instant)>,
  refreshing: Option<Refresh>,
  revision: u64,
  last_error: Option<String>,
}

#[derive(Default)]
pub(super) struct VersionCatalog(Mutex<State>);

pub(super) struct CatalogResult {
  pub versions: Vec<GameVersion>,
  pub stale: bool,
  pub fresh_for_ms: u64,
  pub revision: u64,
  pub refresh_error: Option<String>,
}

impl VersionCatalog {
  pub async fn get(
    self: &Arc<Self>,
    context: Context,
    force: bool,
    after: Option<u64>,
  ) -> Result<CatalogResult, String> {
    let mut pending = {
      let mut state = self
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      let fresh = state
        .cached
        .as_ref()
        .is_some_and(|(_, at)| at.elapsed() < TTL);
      // A follow-up joins the stale read's refresh, including a completed failure.
      // Only an explicit manual refresh (without `after`) starts another attempt.
      if force
        && after.is_some_and(|revision| revision <= state.revision)
        && state.refreshing.is_none()
        && state.cached.is_some()
      {
        return Ok(snapshot(&state, state.last_error.clone()));
      }
      if fresh && !force {
        return Ok(snapshot(&state, None));
      }
      if state.refreshing.is_none() {
        state.revision = state.revision.saturating_add(1);
        let (tx, rx) = watch::channel(None);
        state.refreshing = Some(rx);
        let catalog = self.clone();
        tokio::spawn(async move {
          let result =
            match tokio::time::timeout(Duration::from_secs(30), metadata::list_versions(&context))
              .await
            {
              Ok(Ok(versions)) => Ok(versions),
              Ok(Err(error)) => {
                tracing::warn!(
                  error_code = error.code(),
                  "Version catalogue refresh failed"
                );
                Err("无法更新版本目录，请稍后重试。".to_owned())
              }
              Err(_) => {
                tracing::warn!(error_code = "timeout", "Version catalogue refresh failed");
                Err("更新版本目录超时，请稍后重试。".to_owned())
              }
            };
          let mut state = catalog
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
          let outcome = match result {
            Ok(versions) => {
              tracing::info!(
                version_count = versions.len(),
                "Version catalogue refreshed"
              );
              state.cached = Some((versions, Instant::now()));
              Ok(())
            }
            Err(error) => Err(error),
          };
          state.last_error = outcome.as_ref().err().cloned();
          state.refreshing = None;
          tx.send_replace(Some(outcome));
        });
      }
      if state.cached.is_some() && !force {
        return Ok(snapshot(&state, None));
      }
      state.refreshing.as_ref().expect("refresh started").clone()
    };
    // The worker belongs to the cache, so closing one dialog cannot cancel it.
    let outcome = loop {
      if let Some(result) = pending.borrow().clone() {
        break result;
      }
      pending
        .changed()
        .await
        .map_err(|_| "版本目录更新中断。".to_owned())?;
    };
    let state = self
      .0
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    match outcome {
      Ok(()) => Ok(snapshot(&state, None)),
      Err(error) if state.cached.is_some() => Ok(snapshot(&state, Some(error))),
      Err(error) => Err(error),
    }
  }
}

fn snapshot(state: &State, refresh_error: Option<String>) -> CatalogResult {
  let (versions, at) = state.cached.as_ref().expect("successful catalogue exists");
  CatalogResult {
    versions: versions.clone(),
    stale: at.elapsed() >= TTL,
    fresh_for_ms: TTL.saturating_sub(at.elapsed()).as_millis() as u64,
    revision: state.revision,
    refresh_error,
  }
}
