use super::*;
fn runtime() -> (tempfile::TempDir, Arc<RuntimeState>) {
  let root = tempfile::tempdir().unwrap();
  let context = Context::new(anvil_core::Paths {
    data: root.path().join("data"),
    cache: root.path().join("cache"),
  })
  .unwrap();
  (root, Arc::new(RuntimeState::new(context)))
}
async fn terminal(runtime: &RuntimeState, id: u64) -> InstallOperationSnapshot {
  tokio::time::timeout(std::time::Duration::from_secs(5), async {
    loop {
      let snapshot = runtime.operation(id).unwrap();
      if !snapshot.state.is_active() {
        return snapshot;
      }
      tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
  })
  .await
  .unwrap()
}
#[tokio::test]
async fn import_exists_before_upload_and_is_idempotent_and_cancellable() {
  let (_root, runtime) = runtime();
  let key = "11111111-1111-1111-1111-111111111111".to_string();
  let first = runtime
    .start_import(key.clone(), "Pack.mrpack".into(), 10)
    .unwrap();
  assert!(matches!(
    first.progress.stage,
    ClientInstallStage::Uploading
  ));
  assert_eq!(
    runtime
      .start_import(key.clone(), "Pack.mrpack".into(), 10)
      .unwrap()
      .id,
    first.id
  );
  assert!(
    runtime
      .start_import(key, "Other.mrpack".into(), 10)
      .is_err()
  );
  assert!(
    runtime
      .start_import(
        "22222222-2222-2222-2222-222222222222".into(),
        "Pack.mrpack".into(),
        10
      )
      .is_err()
  );
  runtime.cancel_operation(first.id).unwrap();
  assert!(matches!(
    terminal(&runtime, first.id).await.state,
    OperationState::Cancelled
  ));
  assert!(runtime.import_upload(first.id).is_none());
}
#[tokio::test]
async fn bad_archive_fails_retained_task_and_deletes_upload_without_network() {
  let (_root, runtime) = runtime();
  let upload = tempfile::tempdir().unwrap();
  let original = upload.path().to_owned();
  std::fs::write(upload.path().join("source.mrpack"), b"not a zip").unwrap();
  let op = runtime
    .start_import(
      "11111111-1111-1111-1111-111111111111".into(),
      "Pack.mrpack".into(),
      9,
    )
    .unwrap();
  runtime.finish_import_upload(op.id, Ok(upload));
  let ended = terminal(&runtime, op.id).await;
  assert!(matches!(ended.state, OperationState::Failed));
  assert!(ended.error.is_some());
  assert!(!original.exists());
  assert!(
    ClientInstaller::new(runtime.context.clone())
      .list()
      .await
      .unwrap()
      .is_empty()
  );
  runtime.shutdown_operations().await;
}
#[tokio::test]
async fn interrupted_upload_fails_and_can_be_retried_with_new_identity() {
  let (_root, runtime) = runtime();
  let op = runtime
    .start_import(
      "11111111-1111-1111-1111-111111111111".into(),
      "Pack.mrpack".into(),
      1,
    )
    .unwrap();
  runtime.finish_import_upload(
    op.id,
    Err(anvil_core::Error::Invalid("upload interrupted".into())),
  );
  assert!(matches!(
    terminal(&runtime, op.id).await.state,
    OperationState::Failed
  ));
  let retry = runtime
    .start_import(
      "22222222-2222-2222-2222-222222222222".into(),
      "Pack.mrpack".into(),
      1,
    )
    .unwrap();
  runtime.cancel_operation(retry.id);
  terminal(&runtime, retry.id).await;
}
