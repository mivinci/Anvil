use anvil_core::{
  Context,
  client::{ClientInstallResult, ClientInstallStage, ClientInstaller, ClientProgress},
  client_plan::ClientTarget,
};
use serde::{Deserialize, Serialize};
use std::{
  collections::HashMap,
  sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
  },
};
use tokio::sync::{Notify, watch};

const MAX_RETAINED_OPERATIONS: usize = 32;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum OperationState {
  Running,
  Pausing,
  Paused,
  Resuming,
  Completed,
  Failed,
  Cancelled,
}

impl OperationState {
  fn is_active(self) -> bool {
    matches!(
      self,
      Self::Running | Self::Pausing | Self::Paused | Self::Resuming
    )
  }
}

struct TransferControl {
  pause_requested: bool,
  attempt_cancel: anvil_core::CancellationToken,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InstallReceipt {
  pub loader: Option<anvil_core::fabric::ClientLoader>,
  pub id: String,
  pub name: String,
  pub version_id: String,
  pub already_installed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InstallProgress {
  pub stage: ClientInstallStage,
  pub completed_files: usize,
  pub total_files: Option<usize>,
  pub downloaded_bytes: u64,
  pub total_bytes: Option<u64>,
  pub received_bytes: u64,
}

impl From<ClientProgress> for InstallProgress {
  fn from(progress: ClientProgress) -> Self {
    Self {
      stage: progress.stage,
      completed_files: progress.completed_files,
      total_files: progress.total_files,
      downloaded_bytes: progress.downloaded_bytes,
      total_bytes: progress.total_bytes,
      received_bytes: progress.received_bytes,
    }
  }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InstallOperationSnapshot {
  pub loader: Option<anvil_core::fabric::ClientLoader>,
  pub id: u64,
  pub name: String,
  pub version_id: String,
  pub cancel_requested: bool,
  pub state: OperationState,
  pub progress: InstallProgress,
  pub result: Option<InstallReceipt>,
  pub error: Option<String>,
}

pub(super) struct InstallOperation {
  cancel: anvil_core::CancellationToken,
  snapshot: watch::Receiver<InstallOperationSnapshot>,
  updates: watch::Sender<InstallOperationSnapshot>,
  transfer: Arc<Mutex<TransferControl>>,
  resume: Arc<Notify>,
}

pub(super) struct RuntimeState {
  pub context: Context,
  operations: Mutex<HashMap<u64, InstallOperation>>,
  next_operation_id: AtomicU64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct InstallRequest {
  #[serde(default)]
  pub loader: Option<anvil_core::fabric::ClientLoader>,
  pub name: String,
  pub version_id: String,
}

#[derive(Debug)]
pub(super) enum StartInstallError {
  Invalid(String),
  Conflict(String),
}

impl RuntimeState {
  pub(super) fn new(context: Context) -> Self {
    Self {
      context,
      operations: Mutex::new(HashMap::new()),
      next_operation_id: AtomicU64::new(1),
    }
  }

  pub(super) fn start_install(
    self: &Arc<Self>,
    request: InstallRequest,
  ) -> Result<InstallOperationSnapshot, StartInstallError> {
    ClientInstaller::validate_name(&request.name)
      .map_err(|error| StartInstallError::Invalid(error.to_string()))?;
    if !anvil_core::metadata::is_valid_version_id(&request.version_id) {
      return Err(StartInstallError::Invalid(
        "invalid Minecraft client version ID".into(),
      ));
    }

    if let Some(loader) = &request.loader {
      loader
        .validate()
        .map_err(|error| StartInstallError::Invalid(error.to_string()))?;
    }
    let id = self.next_operation_id.fetch_add(1, Ordering::Relaxed);
    let initial = InstallOperationSnapshot {
      loader: request.loader.clone(),
      id,
      name: request.name.clone(),
      version_id: request.version_id.clone(),
      cancel_requested: false,
      state: OperationState::Running,
      progress: ClientProgress::default().into(),
      result: None,
      error: None,
    };
    let (snapshot_tx, snapshot_rx) = watch::channel(initial.clone());
    let (progress_tx, mut progress_rx) = watch::channel(ClientProgress::default());
    let cancel = anvil_core::CancellationToken::new();
    let transfer = Arc::new(Mutex::new(TransferControl {
      pause_requested: false,
      attempt_cancel: cancel.child_token(),
    }));
    let resume = Arc::new(Notify::new());
    {
      let mut operations = self
        .operations
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      if operations
        .values()
        .any(|operation| operation.snapshot.borrow().state.is_active())
      {
        return Err(StartInstallError::Conflict(
          "another Minecraft installation is already running".into(),
        ));
      }
      while operations.len() >= MAX_RETAINED_OPERATIONS {
        let oldest_completed = operations
          .iter()
          .filter(|(_, operation)| !operation.snapshot.borrow().state.is_active())
          .map(|(operation_id, _)| *operation_id)
          .min();
        let Some(oldest_completed) = oldest_completed else {
          return Err(StartInstallError::Conflict(
            "too many recent operations; restart the desktop app to clear them".into(),
          ));
        };
        operations.remove(&oldest_completed);
      }
      operations.insert(
        id,
        InstallOperation {
          cancel: cancel.clone(),
          snapshot: snapshot_rx,
          updates: snapshot_tx.clone(),
          transfer: transfer.clone(),
          resume: resume.clone(),
        },
      );
    }

    tracing::info!(operation_id = id, "Client installation accepted");
    let started = std::time::Instant::now();
    let state_tx = snapshot_tx.clone();
    let context = self.context.clone();
    tokio::spawn(async move {
      let progress_tx_for_monitor = state_tx.clone();
      let monitor = tokio::spawn(async move {
        let mut last_stage = None;
        while progress_rx.changed().await.is_ok() {
          let progress = progress_rx.borrow().clone();
          if last_stage != Some(progress.stage) {
            tracing::info!(operation_id = id, stage = ?progress.stage,
              completed_files = progress.completed_files, total_files = ?progress.total_files,
              downloaded_bytes = progress.downloaded_bytes, total_bytes = ?progress.total_bytes,
              received_bytes = progress.received_bytes, "Client installation stage changed");
            last_stage = Some(progress.stage);
          }
          progress_tx_for_monitor.send_modify(|snapshot| {
            if matches!(
              snapshot.state,
              OperationState::Running | OperationState::Pausing
            ) {
              snapshot.progress = progress.into();
            }
          });
        }
      });

      let installer = ClientInstaller::new(context);
      let outcome = async {
        let target = ClientTarget::current()?;
        let plan = installer
          .plan_with_loader(
            &request.version_id,
            target,
            request.loader.as_ref(),
            &cancel,
            &progress_tx,
          )
          .await?;
        loop {
          let attempt = {
            let mut control = transfer
              .lock()
              .unwrap_or_else(std::sync::PoisonError::into_inner);
            control.attempt_cancel = cancel.child_token();
            state_tx.send_modify(|snapshot| snapshot.state = OperationState::Running);
            tracing::info!(operation_id = id, "Client installation attempt started");
            control.attempt_cancel.clone()
          };
          let result = installer
            .install(&request.name, &plan, &attempt, &progress_tx)
            .await;
          let pause_requested = transfer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pause_requested;
          if pause_requested
            && !cancel.is_cancelled()
            && matches!(result, Err(anvil_core::Error::Cancelled))
          {
            // Core has joined all downloads and q2q-http has stopped its workers.
            // Keep the logical task and verified plan alive, with no network work.
            state_tx.send_modify(|snapshot| {
              snapshot.state = OperationState::Paused;
              snapshot.progress = progress_tx.borrow().clone().into();
            });
            tracing::info!(
              operation_id = id,
              "Client installation paused; workers stopped"
            );
            tokio::select! {
              _ = cancel.cancelled() => return Err(anvil_core::Error::Cancelled),
              _ = resume.notified() => {},
            }
            if cancel.is_cancelled() {
              return Err(anvil_core::Error::Cancelled);
            }
            continue;
          }
          break result;
        }
      }
      .await;
      monitor.abort();

      state_tx.send_modify(|snapshot| match outcome {
        Ok(result) => {
          tracing::info!(operation_id = id, elapsed_ms = started.elapsed().as_millis() as u64, already_installed = result.already_installed, received_bytes = progress_tx.borrow().received_bytes, completed_files = progress_tx.borrow().completed_files, "Client installation completed");
          snapshot.state = OperationState::Completed;
          snapshot.progress = progress_tx.borrow().clone().into();
          snapshot.result = Some(install_receipt(result));
        }
        Err(error) if cancel.is_cancelled() => {
          tracing::info!(operation_id = id, elapsed_ms = started.elapsed().as_millis() as u64, "Client installation cancelled; cleanup complete");
          snapshot.state = OperationState::Cancelled;
          snapshot.error = Some(error.to_string());
          snapshot.progress = progress_tx.borrow().clone().into();
        }
        Err(error) => {
          tracing::warn!(operation_id = id, error_code = error.code(), stage = ?progress_tx.borrow().stage, elapsed_ms = started.elapsed().as_millis() as u64, "Client installation failed");
          snapshot.state = OperationState::Failed;
          snapshot.error = Some(error.to_string());
          snapshot.progress = progress_tx.borrow().clone().into();
        }
      });
    });

    Ok(initial)
  }

  /// Bounded in-memory history lets the page recover a lost start response or reload.
  pub(super) fn operations(&self) -> Vec<InstallOperationSnapshot> {
    let operations = self
      .operations
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut snapshots = operations
      .values()
      .map(operation_snapshot)
      .collect::<Vec<_>>();
    snapshots.sort_by_key(|snapshot| snapshot.id);
    snapshots
  }

  pub(super) fn operation(&self, id: u64) -> Option<InstallOperationSnapshot> {
    let operations = self
      .operations
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    operations.get(&id).map(operation_snapshot)
  }

  pub(super) fn cancel_operation(&self, id: u64) -> Option<InstallOperationSnapshot> {
    let operations = self
      .operations
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    let operation = operations.get(&id)?;
    if operation.snapshot.borrow().state.is_active() && !operation.cancel.is_cancelled() {
      tracing::info!(
        operation_id = id,
        "Client installation cancellation requested"
      );
      operation.cancel.cancel();
    }
    Some(operation_snapshot(operation))
  }

  pub(super) fn pause_operation(
    &self,
    id: u64,
  ) -> Result<Option<InstallOperationSnapshot>, String> {
    let operations = self
      .operations
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(operation) = operations.get(&id) else {
      return Ok(None);
    };
    let snapshot = operation_snapshot(operation);
    if matches!(
      snapshot.state,
      OperationState::Paused | OperationState::Pausing
    ) {
      return Ok(Some(snapshot));
    }
    if !matches!(snapshot.state, OperationState::Running)
      || !matches!(snapshot.progress.stage, ClientInstallStage::Downloading)
      || snapshot.cancel_requested
    {
      return Err("installation can only be paused while downloading".into());
    }
    let mut control = operation
      .transfer
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    tracing::info!(operation_id = id, "Client installation pause requested");
    control.pause_requested = true;
    operation
      .updates
      .send_modify(|snapshot| snapshot.state = OperationState::Pausing);
    control.attempt_cancel.cancel();
    Ok(Some(operation_snapshot(operation)))
  }

  pub(super) fn resume_operation(
    &self,
    id: u64,
  ) -> Result<Option<InstallOperationSnapshot>, String> {
    let operations = self
      .operations
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(operation) = operations.get(&id) else {
      return Ok(None);
    };
    let snapshot = operation_snapshot(operation);
    if matches!(snapshot.state, OperationState::Resuming) {
      return Ok(Some(snapshot));
    }
    if !matches!(snapshot.state, OperationState::Paused) || snapshot.cancel_requested {
      return Err("installation is not paused".into());
    }
    let mut control = operation
      .transfer
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    tracing::info!(operation_id = id, "Client installation resume requested");
    control.pause_requested = false;
    operation
      .updates
      .send_modify(|snapshot| snapshot.state = OperationState::Resuming);
    operation.resume.notify_one();
    Ok(Some(operation_snapshot(operation)))
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
          if operation.snapshot.borrow().state.is_active() && !operation.cancel.is_cancelled() {
            tracing::info!(
              operation_id = operation.snapshot.borrow().id,
              "Client installation cancelled for Runtime shutdown"
            );
          }
          operation.cancel.cancel();
          operation.snapshot.clone()
        })
        .collect::<Vec<_>>()
    };

    for snapshot in &mut snapshots {
      loop {
        if !snapshot.borrow().state.is_active() {
          break;
        }
        if snapshot.changed().await.is_err() {
          break;
        }
      }
    }
  }
}

fn install_receipt(result: ClientInstallResult) -> InstallReceipt {
  InstallReceipt {
    loader: result.installed.loader,
    id: result.instance.id,
    name: result.instance.name,
    version_id: result.installed.version_id,
    already_installed: result.already_installed,
  }
}

fn operation_snapshot(operation: &InstallOperation) -> InstallOperationSnapshot {
  let mut snapshot = operation.snapshot.borrow().clone();
  snapshot.cancel_requested = snapshot.state.is_active() && operation.cancel.is_cancelled();
  snapshot
}
