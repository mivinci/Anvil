use super::*;
use crate::serve::{accounts::AccountState, operations::OperationState};
use axum::response::IntoResponse;
fn fixture() -> (tempfile::TempDir, AppState) {
  let root = tempfile::tempdir().unwrap();
  let context = anvil_core::Context::new(anvil_core::Paths {
    data: root.path().join("data"),
    cache: root.path().join("cache"),
  })
  .unwrap();
  let state = AppState::new(
    Arc::new(RuntimeState::new(context.clone())),
    Arc::new(AccountState::new(context)),
    "1.0.0".into(),
    "a".repeat(64),
  );
  (root, state)
}
#[tokio::test]
async fn streaming_upload_checks_declared_size_and_reports_failure_in_global_task() {
  for (size, bytes, expected) in [
    (1, b"too long".as_slice(), StatusCode::PAYLOAD_TOO_LARGE),
    (9, b"short".as_slice(), StatusCode::BAD_REQUEST),
    (9, b"not a zip".as_slice(), StatusCode::OK),
  ] {
    let (_root, state) = fixture();
    let Json(op) = reserve(
      State(state.clone()),
      Json(ImportRequest {
        request_id: "11111111-1111-1111-1111-111111111111".into(),
        filename: "Pack.mrpack".into(),
        size,
      }),
    )
    .await
    .unwrap();
    assert!(matches!(
      op.progress.stage,
      anvil_core::client::ClientInstallStage::Uploading
    ));
    let result = upload(State(state.clone()), RoutePath(op.id), Body::from(bytes)).await;
    let status = match result {
      Ok(_) => StatusCode::OK,
      Err(error) => error.into_response().status(),
    };
    assert_eq!(status, expected);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
      loop {
        let snapshot = state.runtime.operation(op.id).unwrap();
        if matches!(snapshot.state, OperationState::Failed) {
          break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
      }
    })
    .await
    .unwrap();
    assert!(
      anvil_core::client::ClientInstaller::new(state.runtime.context.clone())
        .list()
        .await
        .unwrap()
        .is_empty()
    );
    state.runtime.shutdown_operations().await;
  }
}
#[tokio::test]
async fn reservation_rejects_unsupported_file_and_oversized_declared_upload() {
  let (_root, state) = fixture();
  for (filename, size) in [
    ("Pack.zip", 10),
    ("Pack.mrpack", 0),
    ("Pack.mrpack", 512 * 1024 * 1024 + 1),
  ] {
    assert!(
      reserve(
        State(state.clone()),
        Json(ImportRequest {
          request_id: "11111111-1111-1111-1111-111111111111".into(),
          filename: filename.into(),
          size
        })
      )
      .await
      .is_err()
    );
  }
  assert!(state.runtime.operations().is_empty());
}
