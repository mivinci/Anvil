use super::*;

#[tokio::test]
async fn api_errors_preserve_status_and_original_detail_with_additive_code() {
  let response =
    ApiError::new(StatusCode::CONFLICT, "plan_expired", "fixture 原始说明").into_response();
  assert_eq!(response.status(), StatusCode::CONFLICT);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(body["error"], "fixture 原始说明");
  assert_eq!(body["code"], "plan_expired");
}

#[tokio::test]
async fn core_error_code_is_carried_without_inferring_from_its_description() {
  let error = anvil_core::Error::Conflict("wording changed entirely".into());
  let response = resource_error(error).into_response();
  assert_eq!(response.status(), StatusCode::CONFLICT);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(body["code"], "conflict");
  assert!(
    body["error"]
      .as_str()
      .unwrap()
      .contains("wording changed entirely")
  );
}
