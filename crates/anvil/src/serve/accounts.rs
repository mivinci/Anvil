use anvil_core::{
  Context,
  account::{
    AccountProfile, AccountService, AuthorizationEvent, MicrosoftAccountProvider,
    MicrosoftClientId, SystemCredentialStore,
  },
  account_store,
};
use serde::Serialize;
use std::{
  collections::HashMap,
  sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
  },
};
use tokio::sync::{mpsc, watch};

const MAX_RETAINED_OPERATIONS: usize = 16;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AccountSummary {
  pub id: String,
  pub username: String,
  pub selected: bool,
}

// Public application registration already used by the CLI documentation.
const DEFAULT_MICROSOFT_CLIENT_ID: &str = "b374e556-655b-41b1-aee9-45821ad631fe";

fn configured_client_id() -> Result<MicrosoftClientId, String> {
  let value = std::env::var("ANVIL_MICROSOFT_CLIENT_ID").unwrap_or_else(|_| {
    option_env!("ANVIL_MICROSOFT_CLIENT_ID")
      .unwrap_or(DEFAULT_MICROSOFT_CLIENT_ID)
      .to_owned()
  });
  MicrosoftClientId::new(value).map_err(|error| error.to_string())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LoginConfiguration {
  pub enabled: bool,
  pub unavailable_reason: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LoginStage {
  RequestingCode,
  WaitingForApproval,
  CheckingProfile,
  SavingAccount,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LoginState {
  Running,
  Completed,
  Failed,
  Cancelled,
}

#[derive(Clone, Debug, Serialize)]
#[serde(
  tag = "kind",
  rename_all = "snake_case",
  rename_all_fields = "camelCase"
)]
pub(super) enum AuthorizationUpdate {
  DeviceCode {
    verification_uri: String,
    user_code: String,
    expires_in_seconds: u64,
    poll_interval_seconds: u64,
    expires_at: u64,
  },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LoginOperationSnapshot {
  pub id: u64,
  pub state: LoginState,
  pub stage: LoginStage,
  pub cancel_requested: bool,
  pub authorization: Option<AuthorizationUpdate>,
  pub account: Option<AccountSummary>,
  pub error: Option<String>,
}

struct LoginOperation {
  cancel: anvil_core::CancellationToken,
  snapshot: watch::Receiver<LoginOperationSnapshot>,
  snapshot_tx: watch::Sender<LoginOperationSnapshot>,
}

pub(super) struct AccountState {
  context: Context,
  client_id: Result<MicrosoftClientId, String>,
  operations: Mutex<HashMap<u64, LoginOperation>>,
  next_operation_id: AtomicU64,
}

#[derive(Debug)]
pub(super) enum StartLoginError {
  Invalid(String),
  Conflict(String),
  Internal(String),
}

impl AccountState {
  pub(super) fn new(context: Context) -> Self {
    Self {
      context,
      client_id: configured_client_id(),
      operations: Mutex::new(HashMap::new()),
      next_operation_id: AtomicU64::new(1),
    }
  }

  pub(super) async fn list(&self) -> Result<Vec<AccountSummary>, String> {
    let data = self.context.paths.data.clone();
    tokio::task::spawn_blocking(move || {
      account_store::load(&data).map(|file| summarize(file.entries()))
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
  }

  pub(super) fn login_configuration(&self) -> LoginConfiguration {
    LoginConfiguration {
      enabled: self.client_id.is_ok(),
      unavailable_reason: self.client_id.is_err().then_some("invalid_client_id"),
    }
  }

  pub(super) fn start_login(self: &Arc<Self>) -> Result<LoginOperationSnapshot, StartLoginError> {
    let client_id = self.client_id.clone().map_err(StartLoginError::Invalid)?;
    let provider = MicrosoftAccountProvider::new()
      .map_err(|error| StartLoginError::Internal(error.to_string()))?;
    let credentials = Arc::new(SystemCredentialStore);
    let service = Arc::new(AccountService::new(
      client_id,
      Arc::new(provider),
      credentials,
    ));

    let id = self.next_operation_id.fetch_add(1, Ordering::Relaxed);
    let initial = LoginOperationSnapshot {
      id,
      state: LoginState::Running,
      stage: LoginStage::RequestingCode,
      cancel_requested: false,
      authorization: None,
      account: None,
      error: None,
    };
    let (snapshot_tx, snapshot_rx) = watch::channel(initial.clone());
    let cancel = anvil_core::CancellationToken::new();
    {
      let mut operations = self
        .operations
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      if operations
        .values()
        .any(|operation| matches!(operation.snapshot.borrow().state, LoginState::Running))
      {
        return Err(StartLoginError::Conflict(
          "another Microsoft sign-in is already running".into(),
        ));
      }
      while operations.len() >= MAX_RETAINED_OPERATIONS {
        let oldest_finished = operations
          .iter()
          .filter(|(_, operation)| {
            !matches!(operation.snapshot.borrow().state, LoginState::Running)
          })
          .map(|(operation_id, _)| *operation_id)
          .min();
        let Some(oldest_finished) = oldest_finished else {
          return Err(StartLoginError::Conflict(
            "too many recent sign-in operations".into(),
          ));
        };
        operations.remove(&oldest_finished);
      }
      operations.insert(
        id,
        LoginOperation {
          cancel: cancel.clone(),
          snapshot: snapshot_rx,
          snapshot_tx: snapshot_tx.clone(),
        },
      );
    }

    tracing::info!(login_operation_id = id, stage = ?LoginStage::RequestingCode, "Account login started");
    let started = std::time::Instant::now();
    let context = self.context.clone();
    tokio::spawn(async move {
      let (events_tx, mut events_rx) = mpsc::unbounded_channel();
      let service_for_auth = service.clone();
      let operation = service_for_auth.authorize(cancel.clone(), events_tx);
      tokio::pin!(operation);
      let mut events_open = true;
      let session = loop {
        tokio::select! {
          result = &mut operation => break result.map_err(|error| {
            if !cancel.is_cancelled() {
              tracing::warn!(login_operation_id = id, error_code = error.code(), "Account authorization failed");
            }
            error.to_string()
          }),
          event = events_rx.recv(), if events_open => match event {
            Some(event) => {
              snapshot_tx.send_modify(|snapshot| {
                if snapshot.cancel_requested {
                  return;
                }
                let previous_stage = snapshot.stage;
                match event {
                AuthorizationEvent::DeviceCode {
                  verification_uri, user_code, expires_in_seconds, poll_interval_seconds,
                } => {
                  let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default().as_secs();
                  snapshot.stage = LoginStage::WaitingForApproval;
                  snapshot.authorization = Some(AuthorizationUpdate::DeviceCode {
                    verification_uri, user_code, expires_in_seconds, poll_interval_seconds,
                    expires_at: now.saturating_add(expires_in_seconds),
                  });
                }
                AuthorizationEvent::WaitingForApproval => {
                  snapshot.stage = LoginStage::WaitingForApproval;
                }
                AuthorizationEvent::CheckingProfile => {
                  snapshot.stage = LoginStage::CheckingProfile;
                  snapshot.authorization = None;
                }
                }
                if snapshot.stage != previous_stage {
                  tracing::info!(login_operation_id = id, stage = ?snapshot.stage, "Account login stage changed");
                }
              });
            }
            None => events_open = false,
          }
        }
      };

      let outcome = match session {
        Ok(session) => {
          // Serialize cancellation with the commit boundary using the snapshot lock.
          // Once SavingAccount is visible, cancellation no longer interrupts persistence.
          let committing = snapshot_tx.send_if_modified(|snapshot| {
            if snapshot.cancel_requested || cancel.is_cancelled() {
              return false;
            }
            tracing::info!(login_operation_id = id, stage = ?LoginStage::SavingAccount, "Account login stage changed");
            snapshot.stage = LoginStage::SavingAccount;
            snapshot.authorization = None;
            true
          });
          let profile = session.profile().clone();
          if !committing {
            Err(
              cleanup_failed_login(
                &context,
                &service,
                &profile,
                "Microsoft sign-in cancelled".into(),
              )
              .await,
            )
          } else {
            let data = context.paths.data.clone();
            let profile_to_store = profile.clone();
            match tokio::task::spawn_blocking(move || {
              account_store::upsert(&data, profile_to_store)
            })
            .await
            {
              Ok(Ok(_)) => Ok(profile),
              Ok(Err(error)) => {
                Err(cleanup_failed_login(&context, &service, &profile, error.to_string()).await)
              }
              Err(error) => {
                Err(cleanup_failed_login(&context, &service, &profile, error.to_string()).await)
              }
            }
          }
        }
        Err(error) => Err(error),
      };

      snapshot_tx.send_modify(|snapshot| {
        snapshot.authorization = None;
        match outcome {
          Ok(profile) => {
            tracing::info!(login_operation_id = id, elapsed_ms = started.elapsed().as_millis() as u64, "Account login completed");
            snapshot.state = LoginState::Completed;
            snapshot.account = Some(AccountSummary {
              id: profile.id().to_owned(),
              username: profile.username().to_owned(),
              selected: true,
            });
          }
          Err(error) if cancel.is_cancelled() => {
            tracing::info!(login_operation_id = id, "Account login cancelled; cleanup complete");
            snapshot.state = LoginState::Cancelled;
            snapshot.error = Some(error);
          }
          Err(error) => {
            tracing::warn!(login_operation_id = id, stage = ?snapshot.stage, "Account login failed");
            snapshot.state = LoginState::Failed;
            snapshot.error = Some(error);
          }
        }
      });
    });

    Ok(initial)
  }

  pub(super) fn operation(&self, id: u64) -> Option<LoginOperationSnapshot> {
    let operations = self
      .operations
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    operations
      .get(&id)
      .map(|operation| operation.snapshot.borrow().clone())
  }

  pub(super) fn active_operation(&self) -> Option<LoginOperationSnapshot> {
    let operations = self
      .operations
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    operations.values().find_map(|operation| {
      let snapshot = operation.snapshot.borrow();
      matches!(snapshot.state, LoginState::Running).then(|| snapshot.clone())
    })
  }

  pub(super) fn cancel_operation(&self, id: u64) -> Option<LoginOperationSnapshot> {
    let operations = self
      .operations
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    let operation = operations.get(&id)?;
    operation.snapshot_tx.send_modify(|snapshot| {
      if matches!(snapshot.state, LoginState::Running)
        && !matches!(snapshot.stage, LoginStage::SavingAccount)
      {
        if !snapshot.cancel_requested {
          tracing::info!(
            login_operation_id = id,
            "Account login cancellation requested"
          );
        }
        snapshot.cancel_requested = true;
        snapshot.authorization = None;
        operation.cancel.cancel();
      }
    });
    Some(operation.snapshot.borrow().clone())
  }

  pub(super) async fn select(&self, id: String) -> Result<Vec<AccountSummary>, String> {
    let data = self.context.paths.data.clone();
    let result = tokio::task::spawn_blocking(move || {
      account_store::select(&data, &id).map(|file| summarize(file.entries()))
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string());
    match &result {
      Ok(_) => tracing::info!(action = "select", "Account change completed"),
      Err(_) => tracing::warn!(
        action = "select",
        error_code = "account_store",
        "Account change failed"
      ),
    }
    result
  }

  pub(super) async fn logout(&self, id: String) -> Result<Vec<AccountSummary>, String> {
    let data = self.context.paths.data.clone();
    let result = tokio::task::spawn_blocking(move || {
      let file = account_store::load(&data)?;
      let profile = file
        .find(&id)
        .ok_or_else(|| anvil_core::Error::NotFound("Minecraft account profile".into()))?;
      let canonical_id = profile.id().to_owned();
      let credentials = SystemCredentialStore;
      anvil_core::account::CredentialStore::delete_refresh_token(&credentials, &canonical_id)?;
      account_store::remove(&data, &canonical_id).map(|file| summarize(file.entries()))
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string());
    match &result {
      Ok(_) => tracing::info!(action = "logout", "Account change completed"),
      Err(_) => tracing::warn!(
        action = "logout",
        error_code = "account_store",
        "Account change failed"
      ),
    }
    result
  }

  pub(super) async fn shutdown_operations(&self) {
    let mut snapshots = {
      let operations = self
        .operations
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      operations
        .values()
        .map(|operation| {
          operation.cancel.cancel();
          operation.snapshot.clone()
        })
        .collect::<Vec<_>>()
    };

    for snapshot in &mut snapshots {
      loop {
        if !matches!(snapshot.borrow().state, LoginState::Running) {
          break;
        }
        if snapshot.changed().await.is_err() {
          break;
        }
      }
    }
  }
}

// Authorize has already stored the refreshed credential. Never remove it when
// the identity belongs to an existing saved account, even if this login is cancelled.
async fn cleanup_failed_login(
  context: &Context,
  service: &AccountService<MicrosoftAccountProvider, SystemCredentialStore>,
  profile: &AccountProfile,
  reason: String,
) -> String {
  let data = context.paths.data.clone();
  let account_id = profile.id().to_owned();
  let existing = tokio::task::spawn_blocking(move || {
    account_store::load(&data).map(|file| file.find(&account_id).is_some())
  })
  .await;
  match existing {
    Ok(Ok(true)) => reason,
    Ok(Ok(false)) => match service.logout(profile.id()).await {
      Ok(()) => reason,
      Err(error) => format!("{reason}; credential cleanup failed: {error}"),
    },
    Ok(Err(error)) => format!("{reason}; could not check saved account for cleanup: {error}"),
    Err(error) => format!("{reason}; could not check saved account for cleanup: {error}"),
  }
}

fn summarize(entries: Vec<account_store::AccountEntry>) -> Vec<AccountSummary> {
  entries
    .into_iter()
    .map(|entry| AccountSummary {
      id: entry.profile.id().to_owned(),
      username: entry.profile.username().to_owned(),
      selected: entry.selected,
    })
    .collect()
}
