use super::*;

#[test]
fn login_failure_and_cleanup_have_codes_independent_of_raw_wording() {
  let failure = LoginFailure::from(anvil_core::Error::Authentication(
    "expired arbitrary wording".into(),
  ));
  assert_eq!(failure.code, "authentication_failed");
  assert!(failure.message.contains("expired arbitrary wording"));
  let failure = failure.cleanup("arbitrary cleanup detail");
  assert_eq!(failure.code, "account_cleanup_failed");
  assert!(failure.message.contains("expired arbitrary wording"));
  assert!(failure.message.contains("arbitrary cleanup detail"));
}

#[test]
fn login_snapshot_keeps_original_fields_and_adds_error_code() {
  let failure = LoginFailure::from(anvil_core::Error::Authentication("fixture detail".into()));
  let snapshot = LoginOperationSnapshot {
    id: 7,
    state: LoginState::Failed,
    stage: LoginStage::CheckingProfile,
    cancel_requested: false,
    authorization: None,
    account: None,
    error: Some(failure.message),
    error_code: Some(failure.code.into()),
  };
  let body = serde_json::to_value(snapshot).unwrap();
  assert_eq!(body["state"], "failed");
  assert_eq!(body["errorCode"], "authentication_failed");
  assert!(body["error"].as_str().unwrap().contains("fixture detail"));
  assert!(body["authorization"].is_null());
}
