use crate::app_storage::{ActivationFailureReason, AppStorage, AppStorageError, AppVersionStore};
use semver::Version;
use std::{
  sync::{Arc, Condvar, Mutex},
  time::{Duration, Instant},
};
use thiserror::Error;

const PAGE_READY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub enum AppActivationError {
  #[error("App activation storage is unavailable: {0}")]
  StorageUnavailable(String),
  #[error(transparent)]
  Storage(#[from] AppStorageError),
  #[error("App activation state changed while a version was starting")]
  StateChanged,
  #[error("App activation lock is unavailable")]
  LockUnavailable,
}

#[derive(Debug)]
struct Attempt {
  version: Version,
  started_at: Instant,
}

#[derive(Debug, Default)]
struct Inner {
  attempt: Option<Attempt>,
}

/// Coordinates a pending App version's one-time startup and health confirmation.
#[derive(Debug, Clone)]
pub struct AppActivation {
  store: Option<AppVersionStore>,
  storage_error: Option<String>,
  timeout: Duration,
  inner: Arc<(Mutex<Inner>, Condvar)>,
}

impl AppActivation {
  pub fn new(store: AppVersionStore) -> Self {
    Self::with_timeout(store, PAGE_READY_TIMEOUT)
  }

  pub fn with_timeout(store: AppVersionStore, timeout: Duration) -> Self {
    Self {
      store: Some(store),
      storage_error: None,
      timeout,
      inner: Arc::new((Mutex::new(Inner::default()), Condvar::new())),
    }
  }

  pub fn for_current_user() -> Self {
    match AppStorage::for_current_user()
      .and_then(|storage| storage.app(env!("ANVIL_DEFAULT_APP_ID")))
    {
      Ok(store) => Self::new(store),
      Err(error) => Self {
        store: None,
        storage_error: Some(error.to_string()),
        timeout: PAGE_READY_TIMEOUT,
        inner: Arc::new((Mutex::new(Inner::default()), Condvar::new())),
      },
    }
  }

  /// Recover an interrupted attempt, then make one pending version the active startup candidate.
  pub fn prepare_startup(&self) -> Result<bool, AppActivationError> {
    let store = self.store()?;
    let (lock, _) = &*self.inner;
    let mut inner = lock
      .lock()
      .map_err(|_| AppActivationError::LockUnavailable)?;
    let mut state = store.state()?;

    if state.activation_in_progress {
      let failed_version = state.active_version.clone();
      state.active_version = state.previous_healthy_version.clone();
      state.previous_healthy_version = state.active_version.clone();
      if state.pending_version == failed_version {
        state.pending_version = None;
      }
      state.activation_in_progress = false;
      state.failed_version = failed_version;
      state.activation_failure = Some(ActivationFailureReason::Interrupted);
      store.write_state(&state)?;
    }

    let Some(candidate) = state.pending_version.clone() else {
      inner.attempt = None;
      return Ok(false);
    };
    if state.failed_version.as_ref() == Some(&candidate) {
      state.pending_version = None;
      store.write_state(&state)?;
      inner.attempt = None;
      return Ok(false);
    }

    let fallback = state.active_version.clone();
    state.active_version = Some(candidate.clone());
    state.pending_version = None;
    state.previous_healthy_version = fallback;
    state.activation_in_progress = true;
    state.failed_version = None;
    state.activation_failure = None;
    store.write_state(&state)?;

    inner.attempt = Some(Attempt {
      version: candidate,
      started_at: Instant::now(),
    });
    Ok(true)
  }

  /// Record that the page loaded and verified the Runtime API version.
  pub fn confirm_health(&self) -> Result<bool, AppActivationError> {
    let store = self.store()?;
    let (lock, condition) = &*self.inner;
    let mut inner = lock
      .lock()
      .map_err(|_| AppActivationError::LockUnavailable)?;
    let Some(attempt) = inner.attempt.as_ref() else {
      return Ok(false);
    };
    if attempt.started_at.elapsed() >= self.timeout {
      return Ok(false);
    }

    let mut state = store.state()?;
    if !state.activation_in_progress || state.active_version.as_ref() != Some(&attempt.version) {
      return Err(AppActivationError::StateChanged);
    }
    state.activation_in_progress = false;
    state.failed_version = None;
    state.activation_failure = None;
    store.write_state(&state)?;
    inner.attempt = None;
    condition.notify_all();
    Ok(true)
  }

  /// Roll back the unconfirmed startup candidate, recording why it failed.
  pub fn rollback(&self, reason: ActivationFailureReason) -> Result<bool, AppActivationError> {
    let store = self.store()?;
    let (lock, condition) = &*self.inner;
    let mut inner = lock
      .lock()
      .map_err(|_| AppActivationError::LockUnavailable)?;
    let Some(attempt) = inner.attempt.as_ref() else {
      return Ok(false);
    };
    let mut state = store.state()?;
    if !state.activation_in_progress || state.active_version.as_ref() != Some(&attempt.version) {
      return Err(AppActivationError::StateChanged);
    }
    let failed_version = attempt.version.clone();
    state.active_version = state.previous_healthy_version.clone();
    state.previous_healthy_version = state.active_version.clone();
    if state.pending_version.as_ref() == Some(&failed_version) {
      state.pending_version = None;
    }
    state.activation_in_progress = false;
    state.failed_version = Some(failed_version);
    state.activation_failure = Some(reason);
    store.write_state(&state)?;
    inner.attempt = None;
    condition.notify_all();
    Ok(true)
  }

  /// Wait until health is confirmed, the attempt is rolled back, or its 30-second budget expires.
  pub fn wait_for_timeout(&self) -> bool {
    let (lock, condition) = &*self.inner;
    let Ok(mut inner) = lock.lock() else {
      return false;
    };
    loop {
      let Some(attempt) = inner.attempt.as_ref() else {
        return false;
      };
      let remaining = self.timeout.saturating_sub(attempt.started_at.elapsed());
      if remaining.is_zero() {
        return true;
      }
      match condition.wait_timeout(inner, remaining) {
        Ok((next, result)) => {
          inner = next;
          if result.timed_out() {
            return inner.attempt.is_some();
          }
        }
        Err(_) => return false,
      }
    }
  }

  pub fn is_pending(&self) -> bool {
    self
      .inner
      .0
      .lock()
      .map(|inner| inner.attempt.is_some())
      .unwrap_or(false)
  }

  fn store(&self) -> Result<&AppVersionStore, AppActivationError> {
    self.store.as_ref().ok_or_else(|| {
      AppActivationError::StorageUnavailable(
        self
          .storage_error
          .clone()
          .unwrap_or_else(|| "unknown storage error".into()),
      )
    })
  }
}

impl Default for AppActivation {
  fn default() -> Self {
    Self::for_current_user()
  }
}
