pub(super) use super::client_resources::resource_error;
use super::client_resources::{
  client_resources, enable_client_resource, import_client_resource, limit_resource_requests,
  remove_client_resource, sync_client_resources,
};
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
  pub(super) launches: Arc<super::launches::Launches>,
  versions: Arc<super::versions::VersionCatalog>,
  fabric_versions: Arc<super::fabric_catalog::FabricCatalog>,
  pub(super) runtime: Arc<RuntimeState>,
  pub(super) curseforge: Arc<super::curseforge::Catalog>,
  pub(super) modrinth: Arc<super::modrinth::Catalog>,
  news: Arc<super::news::NewsCatalog>,
  pub(super) resources: Arc<super::resource_operations::ResourceOperations>,
  pub(super) accounts: Arc<AccountState>,
  pub(super) play_stats: Arc<anvil_core::play_stats::PlayStatsStore>,
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
    let play_stats = Arc::new(anvil_core::play_stats::PlayStatsStore::open(
      &runtime.context.paths.data,
    ));
    Self {
      launches: Arc::new(super::launches::Launches::new(play_stats.clone())),
      play_stats,
      curseforge: Arc::new(super::curseforge::Catalog::new()),
      modrinth: Arc::new(super::modrinth::Catalog::new()),
      news: Arc::new(super::news::NewsCatalog::default()),
      resources: Arc::new(super::resource_operations::ResourceOperations::new()),
      versions: Arc::new(super::versions::VersionCatalog::default()),
      fabric_versions: Arc::new(super::fabric_catalog::FabricCatalog::default()),
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
  loader: Option<anvil_core::fabric::ClientLoader>,
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
pub(super) struct ApiError {
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

  let launches = state.launches.clone();
  let resources = state.resources.clone();
  let runtime = state.runtime.clone();
  let account_state = state.accounts.clone();
  let app = Router::new()
    .route(
      "/resources/{source}/projects",
      get(super::resource_discovery::search),
    )
    .route(
      "/resources/{source}/projects/{project}",
      get(super::resource_discovery::project),
    )
    .route(
      "/resources/modrinth/shader-projects",
      get(super::resource_discovery::shader_search),
    )
    .route(
      "/resources/modrinth/shader-projects/{project}",
      get(super::resource_discovery::shader_project),
    )
    .route(
      "/resources/modrinth/shader-projects/{project}/versions",
      get(super::resource_discovery::shader_versions),
    )
    .route(
      "/clients/{name}/modrinth/projects",
      get(super::modrinth::search),
    )
    .route(
      "/clients/{name}/modrinth/projects/{project}/files",
      get(super::modrinth::files),
    )
    .route(
      "/clients/{name}/modrinth/projects/{project}/files/{file}/plan",
      post(super::modrinth::create_plan),
    )
    .route(
      "/play-stats",
      get(super::play_stats::snapshot).delete(super::play_stats::clear),
    )
    .route(
      "/play-stats/accounts/{account_id}",
      axum::routing::delete(super::play_stats::clear_account),
    )
    .route("/health", get(health))
    .route("/news", get(news_feed))
    .route("/client-launches", get(super::launches::list))
    .route(
      "/client-launches/{id}",
      axum::routing::delete(super::launches::stop),
    )
    .route("/clients/{name}/launch", post(super::launches::start))
    .route(
      "/resource-operations",
      get(super::resource_operations::list),
    )
    .route(
      "/resource-operations/{id}",
      axum::routing::delete(super::resource_operations::cancel),
    )
    .route(
      "/clients/{name}/resource-plans/{id}/install",
      post(super::resource_operations::submit),
    )
    .route(
      "/clients/{name}/curseforge/projects/{project}/files/{file}/plan",
      post(super::curseforge::create_plan),
    )
    .route(
      "/resources/curseforge/configuration",
      get(super::curseforge::configuration),
    )
    .route(
      "/clients/{name}/curseforge/projects",
      get(super::curseforge::search),
    )
    .route(
      "/clients/{name}/curseforge/projects/{project}/files",
      get(super::curseforge::files),
    )
    .route(
      "/clients/{name}/curseforge/projects/{project}/files/{file}/preview",
      get(super::curseforge::preview),
    )
    .route("/client/versions", get(client_versions))
    .route("/client/fabric/versions/{game}", get(fabric_versions))
    .route("/client/version-catalog", get(client_version_catalog))
    .route("/clients", get(clients))
    .route("/clients/{name}", get(client_details).delete(delete_client))
    .route(
      "/clients/{name}/launch-settings",
      get(super::client_settings::get).put(super::client_settings::save),
    )
    .route("/system/memory", get(super::client_settings::memory))
    .route("/clients/{name}/logs/latest", get(client_game_log))
    .route("/clients/{name}/crash-reports", get(client_crash_reports))
    .route(
      "/clients/{name}/crash-reports/{filename}",
      get(client_crash_report),
    )
    .route(
      "/clients/{name}/logs/console",
      get(super::launches::console_log),
    )
    .route(
      "/clients/{name}/resources",
      get(client_resources)
        .post(import_client_resource)
        .layer(DefaultBodyLimit::max(
          anvil_core::resources::MAX_IMPORT_BYTES,
        ))
        .layer(middleware::from_fn(limit_resource_requests)),
    )
    .route(
      "/clients/{name}/resources/sync",
      post(sync_client_resources).layer(middleware::from_fn(limit_resource_requests)),
    )
    .route(
      "/clients/{name}/resources/{sha256}",
      axum::routing::delete(remove_client_resource)
        .layer(middleware::from_fn(limit_resource_requests)),
    )
    .route(
      "/clients/{name}/resources/{sha256}/enabled",
      post(enable_client_resource).layer(middleware::from_fn(limit_resource_requests)),
    )
    .route(
      "/clients/{name}/resource-packs",
      get(super::client_resources::packs)
        .post(super::client_resources::import_pack)
        .layer(DefaultBodyLimit::max(
          anvil_core::resources::MAX_IMPORT_BYTES,
        ))
        .layer(middleware::from_fn(limit_resource_requests)),
    )
    .route(
      "/clients/{name}/resource-packs/{sha256}",
      axum::routing::delete(super::client_resources::remove_pack)
        .layer(middleware::from_fn(limit_resource_requests)),
    )
    .route(
      "/clients/{name}/shader-packs",
      get(super::client_resources::shader_packs)
        .post(super::client_resources::import_shader_pack)
        .layer(DefaultBodyLimit::max(
          anvil_core::resources::MAX_IMPORT_BYTES,
        ))
        .layer(middleware::from_fn(limit_resource_requests)),
    )
    .route(
      "/clients/{name}/shader-packs/{sha256}",
      axum::routing::delete(super::client_resources::remove_shader_pack)
        .layer(middleware::from_fn(limit_resource_requests)),
    )
    .route(
      "/clients/{name}/shader-plans",
      post(super::client_resources::plan_shader_pack)
        .layer(middleware::from_fn(limit_resource_requests)),
    )
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
    .route("/accounts/{id}/skin", get(account_skin))
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
        launches.shutdown(),
        resources.shutdown(),
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

async fn fabric_versions(
  State(state): State<AppState>,
  RoutePath(game): RoutePath<String>,
  Query(query): Query<VersionQuery>,
) -> Result<Json<super::fabric_catalog::Response>, ApiError> {
  state
    .fabric_versions
    .get(
      state.runtime.context.clone(),
      game,
      query.refresh,
      query.after,
    )
    .await
    .map(Json)
    .map_err(|error| ApiError::new(StatusCode::BAD_GATEWAY, error))
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
    catalog.value.into_iter().map(version_summary).collect(),
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
    versions: catalog.value.into_iter().map(version_summary).collect(),
    stale: catalog.stale,
    fresh_for_ms: catalog.fresh_for_ms,
    revision: catalog.revision,
    refresh_error: catalog.refresh_error,
  }))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct NewsResponse {
  items: Vec<anvil_core::news::NewsItem>,
  stale: bool,
  fresh_for_ms: u64,
  revision: u64,
  refresh_error: Option<String>,
}

async fn news_feed(
  State(state): State<AppState>,
  Query(query): Query<VersionQuery>,
) -> Result<Json<NewsResponse>, ApiError> {
  let catalog = state
    .news
    .get(state.runtime.context.clone(), query.refresh, query.after)
    .await
    .map_err(|error| ApiError::new(StatusCode::BAD_GATEWAY, error))?;
  Ok(Json(NewsResponse {
    items: catalog.value,
    stale: catalog.stale,
    fresh_for_ms: catalog.fresh_for_ms,
    revision: catalog.revision,
    refresh_error: catalog.refresh_error,
  }))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ClientDetails {
  loader: Option<anvil_core::fabric::ClientLoader>,
  id: String,
  name: String,
  version_id: String,
  version_type: String,
  java_major: u32,
  os: String,
  architecture: String,
  file_count: usize,
}

static LOG_READS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
fn crash_error(error: anvil_core::Error) -> ApiError {
  tracing::warn!(error_code = error.code(), "Crash report read failed");
  let (status, message) = match error {
    anvil_core::Error::Invalid(message) => (StatusCode::BAD_REQUEST, message),
    anvil_core::Error::NotFound(_) => (StatusCode::NOT_FOUND, "报告不存在，请刷新列表。".into()),
    _ => (
      StatusCode::INTERNAL_SERVER_ERROR,
      "无法读取崩溃报告，请刷新后重试。".into(),
    ),
  };
  ApiError::new(status, message)
}
async fn client_crash_reports(
  State(state): State<AppState>,
  RoutePath(name): RoutePath<String>,
) -> Result<Json<anvil_core::client::ClientCrashReports>, ApiError> {
  let _permit = LOG_READS
    .try_acquire()
    .map_err(|_| ApiError::new(StatusCode::TOO_MANY_REQUESTS, "日志正在读取，请稍后重试。"))?;
  ClientInstaller::new(state.runtime.context.clone())
    .list_crash_reports(&name)
    .await
    .map(Json)
    .map_err(crash_error)
}
async fn client_crash_report(
  State(state): State<AppState>,
  RoutePath((name, filename)): RoutePath<(String, String)>,
) -> Result<Json<anvil_core::client::ClientCrashReport>, ApiError> {
  let _permit = LOG_READS
    .try_acquire()
    .map_err(|_| ApiError::new(StatusCode::TOO_MANY_REQUESTS, "日志正在读取，请稍后重试。"))?;
  ClientInstaller::new(state.runtime.context.clone())
    .read_crash_report(&name, &filename)
    .await
    .map(Json)
    .map_err(crash_error)
}

async fn client_game_log(
  State(state): State<AppState>,
  RoutePath(name): RoutePath<String>,
) -> Result<Json<anvil_core::client::ClientGameLog>, ApiError> {
  let _permit = LOG_READS
    .try_acquire()
    .map_err(|_| ApiError::new(StatusCode::TOO_MANY_REQUESTS, "日志正在读取，请稍后重试。"))?;
  ClientInstaller::new(state.runtime.context.clone())
    .read_game_log(&name)
    .await
    .map(Json)
    .map_err(|error| {
      tracing::warn!(error_code = error.code(), "Game log read failed");
      ApiError::new(StatusCode::BAD_REQUEST, "无法读取此实例的游戏日志。")
    })
}

async fn client_details(
  State(state): State<AppState>,
  RoutePath(name): RoutePath<String>,
) -> Result<Json<ClientDetails>, ApiError> {
  let (instance, record) = ClientInstaller::new(state.runtime.context.clone())
    .details(&name)
    .await
    .map_err(|error| {
      let status = match &error {
        anvil_core::Error::Invalid(_) => StatusCode::BAD_REQUEST,
        anvil_core::Error::NotFound(_) => StatusCode::NOT_FOUND,
        anvil_core::Error::Busy(_) | anvil_core::Error::Conflict(_) => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
      };
      ApiError::new(status, error.to_string())
    })?;
  Ok(Json(ClientDetails {
    loader: record.loader,
    id: instance.id,
    name: instance.name,
    version_id: record.version_id,
    version_type: record.version_type,
    java_major: record.java_major,
    os: record.os,
    architecture: record.architecture,
    file_count: record.files.len(),
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
    loader: entry.loader,
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

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteClientRequest {
  confirmation: String,
}
static CLIENT_DELETIONS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
async fn delete_client(
  State(state): State<AppState>,
  RoutePath(name): RoutePath<String>,
  Json(request): Json<DeleteClientRequest>,
) -> Result<Json<anvil_core::client::ClientRemoval>, ApiError> {
  ClientInstaller::validate_name(&name).map_err(resource_error)?;
  if !request.confirmation.eq_ignore_ascii_case(&name) {
    return Err(ApiError::new(
      StatusCode::BAD_REQUEST,
      "删除确认与实例不匹配。",
    ));
  }
  let _permit = CLIENT_DELETIONS
    .try_acquire()
    .map_err(|_| ApiError::new(StatusCode::CONFLICT, "已有实例正在删除，请稍后刷新。"))?;
  if state.runtime.busy_for_client(&name)
    || state.launches.busy_for_client(&name)
    || state.resources.busy_for_client(&name)
  {
    return Err(ApiError::new(
      StatusCode::CONFLICT,
      "实例正在运行、准备启动或安装，请等待完成后再删除。",
    ));
  }
  let result = ClientInstaller::new(state.runtime.context.clone())
    .remove(&name)
    .await
    .map_err(resource_error)?;
  tracing::info!(
    cleanup_pending = result.cleanup_pending,
    "Client instance removed"
  );
  Ok(Json(result))
}

async fn account_skin(
  State(state): State<AppState>,
  RoutePath(id): RoutePath<String>,
) -> Result<Json<Option<anvil_core::account_skin::AccountSkin>>, ApiError> {
  let accounts = state
    .accounts
    .list()
    .await
    .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "无法读取账户"))?;
  let account = accounts
    .iter()
    .find(|account| account.id == id)
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "账户不存在"))?;
  anvil_core::account_skin::read(&state.runtime.context, &account.id)
    .await
    .map(Json)
    .map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, "暂时无法读取角色皮肤"))
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
  pub(super) fn new(status: StatusCode, message: impl Into<String>) -> Self {
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
