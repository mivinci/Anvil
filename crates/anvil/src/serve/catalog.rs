//! Shared stale-while-revalidate coordination. Provider adapters own keys and validation.
use anvil_core::{Context, Error};
use std::{
  sync::{Arc, Mutex},
  time::{Duration, Instant, SystemTime},
};
use tokio::sync::watch;

pub(super) struct CatalogData<T> {
  pub value: T,
  pub fetched_at: SystemTime,
  pub persistence_error: Option<Error>,
}
pub(super) trait Source: Send + Sync + 'static {
  type Value: Clone + Send + Sync + 'static;
  const TTL: Duration;
  const NAME: &'static str;
  fn needs_refresh(&self, _: &Self::Value) -> bool {
    false
  }
  fn load(
    &self,
    context: Context,
  ) -> impl std::future::Future<Output = Result<Option<CatalogData<Self::Value>>, Error>> + Send;
  fn fetch(
    &self,
    context: Context,
  ) -> impl std::future::Future<Output = Result<CatalogData<Self::Value>, Error>> + Send;
  fn error(error: &Error) -> String;
}
type Refresh = watch::Receiver<Option<Result<(), String>>>;

struct State<T> {
  cached: Option<CachedCatalog<T>>,
  refreshing: Option<Refresh>,
  revision: u64,
  last_error: Option<String>,
}

pub(super) struct Catalog<S: Source> {
  source: S,
  state: Mutex<State<S::Value>>,
  restored: tokio::sync::OnceCell<()>,
}
struct CachedCatalog<T> {
  value: T,
  age_at_load: Duration,
  loaded_at: Instant,
  needs_refresh: bool,
}
impl<T> CachedCatalog<T> {
  fn age(&self) -> Duration {
    self.age_at_load.saturating_add(self.loaded_at.elapsed())
  }
  fn new(value: CatalogData<T>, ttl: Duration) -> Self {
    Self {
      value: value.value,
      age_at_load: SystemTime::now()
        .duration_since(value.fetched_at)
        .unwrap_or(ttl),
      loaded_at: Instant::now(),
      needs_refresh: false,
    }
  }
}

pub(super) struct CatalogResult<T> {
  pub value: T,
  pub stale: bool,
  pub fresh_for_ms: u64,
  pub revision: u64,
  pub refresh_error: Option<String>,
}

impl<S: Source + Default> Default for Catalog<S> {
  fn default() -> Self {
    Self::new(S::default())
  }
}
impl<S: Source> Catalog<S> {
  pub fn new(source: S) -> Self {
    Self {
      source,
      state: Mutex::new(State {
        cached: None,
        refreshing: None,
        revision: 0,
        last_error: None,
      }),
      restored: tokio::sync::OnceCell::new(),
    }
  }
  pub async fn get(
    self: &Arc<Self>,
    context: Context,
    force: bool,
    after: Option<u64>,
  ) -> Result<CatalogResult<S::Value>, String> {
    // Restore once per Runtime, outside the state mutex. Concurrent opens share disk I/O.
    self
      .restored
      .get_or_init(|| async {
        match self.source.load(context.clone()).await {
          Ok(Some(value)) => {
            let mut cached = CachedCatalog::new(value, S::TTL);
            cached.needs_refresh = self.source.needs_refresh(&cached.value);
            tracing::info!(
              source = S::NAME,
              stale = cached.needs_refresh || cached.age() >= S::TTL,
              "Catalogue restored from disk"
            );
            self
              .state
              .lock()
              .unwrap_or_else(std::sync::PoisonError::into_inner)
              .cached = Some(cached);
          }
          Ok(None) => {}
          Err(error) => tracing::warn!(
            source = S::NAME,
            error_code = error.code(),
            "Catalogue disk cache unavailable; falling back to network"
          ),
        }
      })
      .await;
    let mut pending = {
      let mut state = self
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      let fresh = state
        .cached
        .as_ref()
        .is_some_and(|cached| !cached.needs_refresh && cached.age() < S::TTL);
      // A follow-up joins the stale read's refresh, including a completed failure.
      // Only an explicit manual refresh (without `after`) starts another attempt.
      if force
        && after.is_some_and(|revision| revision <= state.revision)
        && state.refreshing.is_none()
        && state.cached.is_some()
      {
        return Ok(snapshot(&state, S::TTL, state.last_error.clone()));
      }
      if fresh && !force {
        return Ok(snapshot(&state, S::TTL, None));
      }
      if state.refreshing.is_none() {
        state.revision = state.revision.saturating_add(1);
        let (tx, rx) = watch::channel(None);
        state.refreshing = Some(rx);
        let catalog = self.clone();
        tokio::spawn(async move {
          let result = match tokio::time::timeout(
            Duration::from_secs(30),
            catalog.source.fetch(context.clone()),
          )
          .await
          {
            Ok(Ok(versions)) => Ok(versions),
            Ok(Err(error)) => {
              let detail = S::error(&error);
              tracing::warn!(source = S::NAME, error_code = error.code(), reason = %detail, "Catalogue refresh failed");
              Err(detail)
            }
            Err(_) => {
              tracing::warn!(
                source = S::NAME,
                error_code = "timeout",
                "Catalogue refresh failed"
              );
              Err("更新版本目录超时，请稍后重试。".to_owned())
            }
          };
          let mut state = catalog
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
          let outcome = match result {
            Ok(value) => {
              if let Some(error) = &value.persistence_error {
                tracing::warn!(
                  source = S::NAME,
                  error_code = error.code(),
                  "Catalogue updated but disk cache could not be saved"
                );
              }
              tracing::info!(source = S::NAME, "Catalogue refreshed");
              state.cached = Some(CachedCatalog::new(value, S::TTL));
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
        return Ok(snapshot(&state, S::TTL, None));
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
      .state
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    match outcome {
      Ok(()) => Ok(snapshot(&state, S::TTL, None)),
      Err(error) if state.cached.is_some() => Ok(snapshot(&state, S::TTL, Some(error))),
      Err(error) => Err(error),
    }
  }
}

fn snapshot<T: Clone>(
  state: &State<T>,
  ttl: Duration,
  refresh_error: Option<String>,
) -> CatalogResult<T> {
  let cached = state.cached.as_ref().expect("successful catalogue exists");
  let age = cached.age();
  CatalogResult {
    value: cached.value.clone(),
    stale: cached.needs_refresh || age >= ttl,
    fresh_for_ms: if cached.needs_refresh {
      0
    } else {
      ttl.saturating_sub(age).as_millis() as u64
    },
    revision: state.revision,
    refresh_error,
  }
}
