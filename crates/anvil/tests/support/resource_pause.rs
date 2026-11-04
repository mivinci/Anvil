use super::*;

#[test]
fn pause_uses_live_download_stage_and_keeps_operation_cancellation_separate() {
  let resources = ResourceOperations::new();
  let cancel = CancellationToken::new();
  let attempt = cancel.child_token();
  let control = Arc::new(TransferControl {
    state: Mutex::new(Transfer::Active(attempt.clone())),
    wake: Notify::new(),
  });
  let progress = ResourceInstallProgress {
    stage: ResourceInstallStage::Downloading,
    completed_files: 0,
    total_files: 1,
    downloaded_bytes: 0,
    total_bytes: 10,
    received_bytes: 0,
  };
  let (_updates, snapshot_rx) = watch::channel(Snapshot {
    id: 1,
    name: "test".into(),
    kind: Kind::Mod,
    state: Status::Running,
    cancel_requested: false,
    pause_requested: false,
    paused: false,
    progress: progress.clone(),
    revision: None,
    error: None,
    error_code: None,
  });
  let (progress_tx, progress_rx) = watch::channel(ResourceInstallProgress {
    stage: ResourceInstallStage::Publishing,
    ..progress
  });
  resources.inner.lock().unwrap().operations.insert(
    1,
    Operation {
      cancel: cancel.clone(),
      transfer: control.clone(),
      progress: progress_rx,
      snapshot: snapshot_rx,
    },
  );

  // The UI snapshot may lag behind the actual publication boundary.
  assert!(change_transfer(&resources, 1, true).is_err());
  assert!(!attempt.is_cancelled());

  progress_tx.send_modify(|progress| progress.stage = ResourceInstallStage::Downloading);
  let snapshot = change_transfer(&resources, 1, true).ok().unwrap();
  assert!(snapshot.pause_requested && !snapshot.paused);
  assert!(attempt.is_cancelled());
  assert!(!cancel.is_cancelled());
  assert!(resources.busy_for_client("test"));
  assert!(change_transfer(&resources, 1, false).is_err());

  // Only a drained attempt can be marked paused by the worker.
  *control.state.lock().unwrap() = Transfer::Paused;
  assert!(change_transfer(&resources, 1, true).ok().unwrap().paused);
  assert!(change_transfer(&resources, 1, false).is_ok());
  assert!(change_transfer(&resources, 1, false).is_ok());
  cancel.cancel();
  assert!(change_transfer(&resources, 1, false).is_err());
}

#[tokio::test]
async fn resource_admission_is_per_instance_and_preserves_active_history() {
  fn plan(name: &str) -> InstallPlan {
    InstallPlan::Shader(ShaderInstallPlan {
      name: name.into(),
      revision: 0,
      project_id: "fixture".into(),
      project_name: "Fixture".into(),
      version_id: "fixture".into(),
      version_name: "Fixture".into(),
      file_name: "fixture.zip".into(),
      file_size: 10,
      sha1: "a".repeat(40),
      // Admission is tested synchronously before the worker runs.
      url: "invalid".into(),
    })
  }

  let root = tempfile::tempdir().unwrap();
  let context = Context::new(anvil_core::Paths {
    data: root.path().join("data"),
    cache: root.path().join("cache"),
  })
  .unwrap();
  let resources = ResourceOperations::new();
  let first_id = resources.save(plan("Älpha"));
  let duplicate_instance = resources.save(plan("älpha"));
  let second_id = resources.save(plan("beta"));
  let third_id = resources.save(plan("gamma"));

  let first = resources.start(context.clone(), "älpha", first_id).unwrap();
  assert_eq!(first.id, first_id);
  {
    let mut inner = resources.inner.lock().unwrap();
    *inner.operations[&first_id].transfer.state.lock().unwrap() = Transfer::Paused;
    // Fill the history with newer terminal entries. The oldest entry is active.
    for id in 100..100 + (LIMIT as u64 - 1) {
      let (_, snapshot) = watch::channel(Snapshot {
        id,
        state: Status::Completed,
        ..first.clone()
      });
      let (_, progress) = watch::channel(first.progress.clone());
      inner.operations.insert(
        id,
        Operation {
          cancel: CancellationToken::new(),
          transfer: Arc::new(TransferControl {
            state: Mutex::new(Transfer::Finished),
            wake: Notify::new(),
          }),
          progress,
          snapshot,
        },
      );
    }
  }
  assert!(
    resources
      .start(context.clone(), "ÄLPHA", duplicate_instance)
      .is_err()
  );
  let second = resources.start(context.clone(), "BETA", second_id).unwrap();
  assert_eq!(second.id, second_id);
  assert_eq!(resources.inner.lock().unwrap().operations.len(), LIMIT);
  assert!(
    !resources
      .inner
      .lock()
      .unwrap()
      .operations
      .contains_key(&100)
  );
  assert!(resources.start(context.clone(), "gamma", third_id).is_err());

  let original = resources.start(context.clone(), "ÄLPHA", first_id).unwrap();
  assert_eq!(original.id, first_id);
  assert!(original.paused);
  assert!(
    cancel_operation(&resources, first_id)
      .unwrap()
      .cancel_requested
  );
  assert!(resources.busy_for_client("älpha"));
  let second = resources.start(context, "beta", second_id).unwrap();
  assert!(second.state == Status::Running && !second.cancel_requested);
  assert!(resources.busy_for_client("beta"));
}
