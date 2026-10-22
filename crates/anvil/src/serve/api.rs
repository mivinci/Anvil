use super::{
  accounts::{
    AccountState, AccountSummary, LoginConfiguration, LoginOperationSnapshot, StartLoginError,
  },
  operations::{InstallOperationSnapshot, InstallRequest, RuntimeState, StartInstallError},
  protocol::{constant_time_eq, is_allowed_origin, is_loopback_address, is_token},
};
use anvil_core::{
  client::{ClientInstaller, ClientListEntry},
  metadata::GameVersion,
};
use axum::{
  Json, Router,
  body::Body,
  extract::{DefaultBodyLimit, Path as RoutePath, Query, State},
  http::{
    HeaderValue, Method, Request, StatusCode,
    header::{
      ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
      ACCESS_CONTROL_REQUEST_HEADERS, ACCESS_CONTROL_REQUEST_METHOD, AUTHORIZATION, ORIGIN, VARY,
    },
  },
  middleware::{self, Next},
  response::{IntoResponse, Response},
  routing::{get, post},
};
use std::{io, sync::Arc};
use tokio::{net::TcpListener, sync::oneshot, time::Duration};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub(super) struct AppState {
  versions: Arc<super::versions::VersionCatalog>,
  runtime: Arc<RuntimeState>,
  accounts: Arc<AccountState>,
  api_version: Arc<str>,
  token: Arc<str>,
  pub(super) logging: Option<anvil_logging::LoggingStatus>,
}

impl AppState {
  pub(super) fn new(
    runtime: Arc<RuntimeState>,
    accounts: Arc<AccountState>,
    api_version: String,
    token: String,
  ) -> Self {
    Self {
      versions: Arc::new(super::versions::VersionCatalog::default()),
      runtime,
      accounts,
      api_version: Arc::from(api_version),
      token: Arc::from(token),
      logging: None,
    }
  }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ClientSummary {
  id: String,
  name: String,
  version_id: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionSummary {
  id: String,
  #[serde(rename = "type")]
  version_type: String,
  release_time: String,
}

#[derive(Debug)]
struct ApiError {
  status: StatusCode,
  message: String,
}

pub(super) async fn serve(
  listener: TcpListener,
  state: AppState,
  shutdown: oneshot::Receiver<()>,
) -> io::Result<()> {
  if !is_loopback_address(listener.local_addr()?) {
    return Err(io::Error::new(
      io::ErrorKind::AddrNotAvailable,
      "Runtime listener must be loopback",
    ));
  }
  if !is_token(&state.token) {
    return Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      "invalid Runtime credential",
    ));
  }

  let runtime = state.runtime.clone();
  let account_state = state.accounts.clone();
  let app = Router::new()
    .route("/health", get(health))
    .route("/client/versions", get(client_versions))
    .route("/client/version-catalog", get(client_version_catalog))
    .route("/clients", get(clients))
    .route("/clients/install", post(install_client))
    .route("/operations", get(operations))
    .route("/operations/{id}", get(operation).delete(cancel_operation))
    .route("/operations/{id}/pause", post(pause_operation))
    .route("/operations/{id}/resume", post(resume_operation))
    .route("/accounts", get(accounts))
    .route(
      "/accounts/login",
      get(login_configuration).post(login_account),
    )
    .route("/accounts/{id}", axum::routing::delete(logout_account))
    .route("/accounts/{id}/select", post(select_account))
    .route("/account-operations/active", get(active_account_operation))
    .route(
      "/account-operations/{id}",
      get(account_operation).delete(cancel_account_operation),
    )
    .layer(DefaultBodyLimit::max(64 * 1024))
    .layer(middleware::from_fn_with_state(
      state.clone(),
      authorize_request,
    ))
    .with_state(state);

  axum::serve(listener, app)
    .with_graceful_shutdown(async move {
      let _ = shutdown.await;
      tracing::info!("Runtime shutdown requested; draining operations");
      tokio::join!(
        runtime.shutdown_operations(),
        account_state.shutdown_operations()
      );
    })
    .await
}

async fn authorize_request(
  State(state): State<AppState>,
  request: Request<Body>,
  next: Next,
) -> Response {
  let Some(origin) = request
    .headers()
    .get(ORIGIN)
    .and_then(|value| value.to_str().ok())
    .filter(|origin| is_allowed_origin(origin))
    .map(str::to_owned)
  else {
    return StatusCode::FORBIDDEN.into_response();
  };

  if request.method() == Method::OPTIONS {
    let valid_method = request
      .headers()
      .get(ACCESS_CONTROL_REQUEST_METHOD)
      .and_then(|value| value.to_str().ok())
      .is_some_and(|method| matches!(method, "GET" | "POST" | "DELETE"));
    let valid_headers = request
      .headers()
      .get(ACCESS_CONTROL_REQUEST_HEADERS)
      .and_then(|value| value.to_str().ok())
      .is_none_or(|headers| {
        headers.split(',').all(|header| {
          header.trim().eq_ignore_ascii_case("authorization")
            || header.trim().eq_ignore_ascii_case("content-type")
        })
      });
    let response = if valid_method && valid_headers {
      StatusCode::NO_CONTENT.into_response()
    } else {
      StatusCode::FORBIDDEN.into_response()
    };
    return with_cors(response, &origin);
  }

  let authorized = request
    .headers()
    .get(AUTHORIZATION)
    .and_then(|value| value.to_str().ok())
    .and_then(|header| header.strip_prefix("Bearer "))
    .is_some_and(|provided| constant_time_eq(provided.as_bytes(), state.token.as_bytes()));
  if !authorized {
    let response = ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    return with_cors(response, &origin);
  }

  // Only route templates are logged: never URL/query, headers, body or account IDs.
  let route = request
    .extensions()
    .get::<axum::extract::MatchedPath>()
    .map(|path| path.as_str().to_owned())
    .unwrap_or_else(|| "unmatched".into());
  let method = request.method().clone();
  let started = std::time::Instant::now();
  let response = match tokio::time::timeout(REQUEST_TIMEOUT, next.run(request)).await {
    Ok(response) => response,
    Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
  };
  if response.status().is_server_error() || response.status() == StatusCode::REQUEST_TIMEOUT {
    tracing::warn!(route = %route, method = %method, status = response.status().as_u16(),
      elapsed_ms = started.elapsed().as_millis() as u64, "Runtime API request failed");
  } else {
    tracing::debug!(route = %route, method = %method, status = response.status().as_u16(),
      elapsed_ms = started.elapsed().as_millis() as u64, "Runtime API request completed");
  }
  with_cors(response, &origin)
}

fn with_cors(mut response: Response, origin: &str) -> Response {
  let headers = response.headers_mut();
  if let Ok(origin) = HeaderValue::from_str(origin) {
    headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, origin);
  }
  headers.insert(VARY, HeaderValue::from_static("Origin"));
  headers.insert(
    ACCESS_CONTROL_ALLOW_METHODS,
    HeaderValue::from_static("GET, POST, DELETE, OPTIONS"),
  );
  headers.insert(
    ACCESS_CONTROL_ALLOW_HEADERS,
    HeaderValue::from_static("authorization, content-type"),
  );
  response
}

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "status": "ok",
    "apiVersion": state.api_version.as_ref(),
    "fileLoggingAvailable": state.logging.as_ref().map(|logging| logging.file_logging_available()),
  }))
}

async fn client_versions(
  State(state): State<AppState>,
) -> Result<Json<Vec<VersionSummary>>, ApiError> {
  let catalog = state
    .versions
    .get(state.runtime.context.clone(), false, None)
    .await
    .map_err(|error| ApiError::new(StatusCode::BAD_GATEWAY, error))?;
  Ok(Json(
    catalog.versions.into_iter().map(version_summary).collect(),
  ))
}

#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct VersionQuery {
  #[serde(default)]
  refresh: bool,
  after: Option<u64>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionCatalogResponse {
  versions: Vec<VersionSummary>,
  stale: bool,
  fresh_for_ms: u64,
  revision: u64,
  refresh_error: Option<String>,
}

async fn client_version_catalog(
  State(state): State<AppState>,
  Query(query): Query<VersionQuery>,
) -> Result<Json<VersionCatalogResponse>, ApiError> {
  let catalog = state
    .versions
    .get(state.runtime.context.clone(), query.refresh, query.after)
    .await
    .map_err(|error| ApiError::new(StatusCode::BAD_GATEWAY, error))?;
  Ok(Json(VersionCatalogResponse {
    versions: catalog.versions.into_iter().map(version_summary).collect(),
    stale: catalog.stale,
    fresh_for_ms: catalog.fresh_for_ms,
    revision: catalog.revision,
    refresh_error: catalog.refresh_error,
  }))
}

async fn clients(State(state): State<AppState>) -> Result<Json<Vec<ClientSummary>>, ApiError> {
  ClientInstaller::new(state.runtime.context.clone())
    .list_with_versions()
    .await
    .map(|clients| clients.into_iter().map(client_summary).collect())
    .map(Json)
    .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
}

fn client_summary(entry: ClientListEntry) -> ClientSummary {
  ClientSummary {
    id: entry.instance.id,
    name: entry.instance.name,
    version_id: entry.version_id,
  }
}

fn version_summary(version: GameVersion) -> VersionSummary {
  VersionSummary {
    id: version.id,
    version_type: version.version_type,
    release_time: version.release_time,
  }
}

async fn install_client(
  State(state): State<AppState>,
  Json(request): Json<InstallRequest>,
) -> Result<(StatusCode, Json<InstallOperationSnapshot>), ApiError> {
  state
    .runtime
    .start_install(request)
    .map(|operation| (StatusCode::ACCEPTED, Json(operation)))
    .map_err(|error| match error {
      StartInstallError::Invalid(message) => ApiError::new(StatusCode::BAD_REQUEST, message),
      StartInstallError::Conflict(message) => ApiError::new(StatusCode::CONFLICT, message),
    })
}

async fn operations(State(state): State<AppState>) -> Json<Vec<InstallOperationSnapshot>> {
  Json(state.runtime.operations())
}

async fn operation(
  State(state): State<AppState>,
  RoutePath(id): RoutePath<u64>,
) -> Result<Json<InstallOperationSnapshot>, ApiError> {
  state
    .runtime
    .operation(id)
    .map(Json)
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "operation not found"))
}

async fn cancel_operation(
  State(state): State<AppState>,
  RoutePath(id): RoutePath<u64>,
) -> Result<(StatusCode, Json<InstallOperationSnapshot>), ApiError> {
  state
    .runtime
    .cancel_operation(id)
    .map(|operation| (StatusCode::ACCEPTED, Json(operation)))
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "operation not found"))
}

async fn pause_operation(
  State(state): State<AppState>,
  RoutePath(id): RoutePath<u64>,
) -> Result<(StatusCode, Json<InstallOperationSnapshot>), ApiError> {
  state
    .runtime
    .pause_operation(id)
    .map_err(|error| ApiError::new(StatusCode::CONFLICT, error))?
    .map(|snapshot| (StatusCode::ACCEPTED, Json(snapshot)))
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "operation not found"))
}

async fn resume_operation(
  State(state): State<AppState>,
  RoutePath(id): RoutePath<u64>,
) -> Result<(StatusCode, Json<InstallOperationSnapshot>), ApiError> {
  state
    .runtime
    .resume_operation(id)
    .map_err(|error| ApiError::new(StatusCode::CONFLICT, error))?
    .map(|snapshot| (StatusCode::ACCEPTED, Json(snapshot)))
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "operation not found"))
}

async fn accounts(State(state): State<AppState>) -> Result<Json<Vec<AccountSummary>>, ApiError> {
  state
    .accounts
    .list()
    .await
    .map(Json)
    .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error))
}

async fn login_configuration(State(state): State<AppState>) -> Json<LoginConfiguration> {
  Json(state.accounts.login_configuration())
}

async fn login_account(
  State(state): State<AppState>,
) -> Result<(StatusCode, Json<LoginOperationSnapshot>), ApiError> {
  state
    .accounts
    .start_login()
    .map(|operation| (StatusCode::ACCEPTED, Json(operation)))
    .map_err(|error| match error {
      StartLoginError::Invalid(message) => ApiError::new(StatusCode::BAD_REQUEST, message),
      StartLoginError::Conflict(message) => ApiError::new(StatusCode::CONFLICT, message),
      StartLoginError::Internal(message) => {
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, message)
      }
    })
}

async fn account_operation(
  State(state): State<AppState>,
  RoutePath(id): RoutePath<u64>,
) -> Result<Json<LoginOperationSnapshot>, ApiError> {
  state
    .accounts
    .operation(id)
    .map(Json)
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "account operation not found"))
}

async fn active_account_operation(State(state): State<AppState>) -> Json<serde_json::Value> {
  Json(serde_json::json!({ "operation": state.accounts.active_operation() }))
}

async fn cancel_account_operation(
  State(state): State<AppState>,
  RoutePath(id): RoutePath<u64>,
) -> Result<(StatusCode, Json<LoginOperationSnapshot>), ApiError> {
  state
    .accounts
    .cancel_operation(id)
    .map(|operation| (StatusCode::ACCEPTED, Json(operation)))
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "account operation not found"))
}

async fn select_account(
  State(state): State<AppState>,
  RoutePath(id): RoutePath<String>,
) -> Result<Json<Vec<AccountSummary>>, ApiError> {
  state
    .accounts
    .select(id)
    .await
    .map(Json)
    .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error))
}

async fn logout_account(
  State(state): State<AppState>,
  RoutePath(id): RoutePath<String>,
) -> Result<Json<Vec<AccountSummary>>, ApiError> {
  state
    .accounts
    .logout(id)
    .await
    .map(Json)
    .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error))
}

impl ApiError {
  fn new(status: StatusCode, message: impl Into<String>) -> Self {
    Self {
      status,
      message: message.into(),
    }
  }
}

impl IntoResponse for ApiError {
  fn into_response(self) -> Response {
    (
      self.status,
      Json(serde_json::json!({ "error": self.message })),
    )
      .into_response()
  }
}
