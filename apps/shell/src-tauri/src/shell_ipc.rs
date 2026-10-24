use crate::activation::AppActivation;
use crate::app_storage::ActivationFailureReason;
use crate::app_storage::AppStorage;
use crate::runtime::{RuntimeConnection, RuntimeStatus, RuntimeSupervisor};
#[cfg(test)]
use crate::shell_commands::SHELL_COMMANDS;
use crate::updater::{AppUpdateState, AppUpdateStatus, UpdateCheckOutcome, UpdateService};
use serde::Serialize;
use tauri::{Url, WebviewWindow};

const DEFAULT_APP_ID: &str = env!("ANVIL_DEFAULT_APP_ID");
const BUNDLED_APP_VERSION: &str = env!("ANVIL_BUNDLED_APP_VERSION");
const TARGET_PLATFORM: &str = env!("ANVIL_TARGET_PLATFORM");
const SHELL_API_VERSION: &str = env!("ANVIL_SHELL_API_VERSION");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
  pub app_id: String,
  pub app_version: Option<String>,
  pub bundled_app_version: String,
  pub installed: bool,
  pub target_platform: String,
  pub shell_version: String,
  pub shell_api_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellUpdateState {
  NotConfigured,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
  pub shell: ShellUpdateState,
  pub app: AppUpdateStatus,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
  pub outcome: UpdateCheckOutcome,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingStatus {
  directory: Option<String>,
  file_logging_available: bool,
}

#[tauri::command]
pub fn shell_get_logging_status(
  window: WebviewWindow,
  logging: tauri::State<'_, anvil_logging::LoggingStatus>,
) -> Result<LoggingStatus, String> {
  ensure_authorized_window(&window)?;
  Ok(LoggingStatus {
    directory: logging
      .directory()
      .map(|path| path.to_string_lossy().into_owned()),
    file_logging_available: logging.file_logging_available(),
  })
}

/// Only opens the directory captured at startup; the page cannot supply a path.
#[tauri::command]
pub async fn shell_open_log_directory(
  window: WebviewWindow,
  logging: tauri::State<'_, anvil_logging::LoggingStatus>,
) -> Result<(), String> {
  ensure_authorized_window(&window)?;
  let directory = logging
    .directory()
    .ok_or("log directory unavailable")?
    .to_path_buf();
  tauri::async_runtime::spawn_blocking(move || {
    if !directory.is_dir() {
      return Err("log directory unavailable".to_owned());
    }
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("/usr/bin/open");
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    let status = command
      .arg(directory)
      .stdin(std::process::Stdio::null())
      .stdout(std::process::Stdio::null())
      .stderr(std::process::Stdio::null())
      .status()
      .map_err(|_| "could not open log directory".to_owned())?;
    if status.success() {
      Ok(())
    } else {
      Err("could not open log directory".into())
    }
  })
  .await
  .map_err(|_| "could not open log directory".to_owned())?
}

#[tauri::command]
pub fn shell_get_app_info(window: WebviewWindow) -> Result<AppInfo, String> {
  ensure_authorized_window(&window)?;

  let app_storage = AppStorage::for_current_user().map_err(|error| error.to_string())?;
  let app_store = app_storage
    .app(DEFAULT_APP_ID)
    .map_err(|error| error.to_string())?;
  let app_version = app_store
    .resolve_active()
    .map_err(|error| error.to_string())?
    .map(|version| version.version().to_string());

  Ok(AppInfo {
    app_id: DEFAULT_APP_ID.to_owned(),
    installed: app_version.is_some(),
    app_version,
    bundled_app_version: BUNDLED_APP_VERSION.to_owned(),
    target_platform: TARGET_PLATFORM.to_owned(),
    shell_version: env!("CARGO_PKG_VERSION").to_owned(),
    shell_api_version: SHELL_API_VERSION.to_owned(),
  })
}

#[tauri::command]
pub fn shell_get_update_status(
  window: WebviewWindow,
  updates: tauri::State<'_, UpdateService>,
) -> Result<UpdateStatus, String> {
  ensure_authorized_window(&window)?;
  Ok(UpdateStatus {
    shell: ShellUpdateState::NotConfigured,
    app: updates.status(),
  })
}

#[tauri::command]
pub fn shell_check_for_updates(
  window: WebviewWindow,
  updates: tauri::State<'_, UpdateService>,
) -> Result<UpdateCheckResult, String> {
  ensure_authorized_window(&window)?;
  Ok(UpdateCheckResult {
    outcome: updates.check_for_updates_now(),
  })
}

#[tauri::command]
pub fn shell_restart_for_update(
  window: WebviewWindow,
  app: tauri::AppHandle,
  updates: tauri::State<'_, UpdateService>,
  runtime: tauri::State<'_, RuntimeSupervisor>,
) -> Result<(), String> {
  ensure_authorized_window(&window)?;
  if updates.status().state != AppUpdateState::Staged
    && runtime.status().available_runtime_version.is_none()
  {
    return Err("there is no staged App or Runtime update".into());
  }
  app.restart();
}

#[tauri::command]
pub fn shell_cancel_update(
  window: WebviewWindow,
  updates: tauri::State<'_, UpdateService>,
) -> Result<bool, String> {
  ensure_authorized_window(&window)?;
  Ok(updates.cancel())
}

#[tauri::command]
pub fn shell_get_runtime_status(
  window: WebviewWindow,
  runtime: tauri::State<'_, RuntimeSupervisor>,
) -> Result<RuntimeStatus, String> {
  ensure_authorized_window(&window)?;
  Ok(runtime.status())
}

#[tauri::command]
pub fn shell_get_runtime_connection(
  window: WebviewWindow,
  runtime: tauri::State<'_, RuntimeSupervisor>,
) -> Result<RuntimeConnection, String> {
  ensure_authorized_window(&window)?;
  runtime
    .connection()
    .ok_or_else(|| "App Runtime is not ready".to_owned())
}

#[tauri::command]
pub fn shell_confirm_app_ready(
  window: WebviewWindow,
  runtime: tauri::State<'_, RuntimeSupervisor>,
  activation: tauri::State<'_, AppActivation>,
) -> Result<bool, String> {
  ensure_authorized_window(&window)?;
  let status = runtime.status();
  if status.state != crate::runtime::RuntimeState::Ready {
    return Err("App Runtime is not ready".into());
  }
  let connection = runtime
    .connection()
    .ok_or_else(|| "App Runtime connection is unavailable".to_owned())?;
  if status.api_version.as_deref() != Some(connection.api_version.as_str()) {
    return Err("App Runtime API version is inconsistent".into());
  }
  activation
    .confirm_health()
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn shell_report_app_startup_failure(
  window: WebviewWindow,
  runtime: tauri::State<'_, RuntimeSupervisor>,
  activation: tauri::State<'_, AppActivation>,
) -> Result<bool, String> {
  ensure_authorized_window(&window)?;
  let rolled_back = activation
    .rollback(ActivationFailureReason::PageHealth)
    .map_err(|error| error.to_string())?;
  if rolled_back {
    runtime.shutdown();
    runtime.start_default_app();
    window
      .eval("window.location.reload()")
      .map_err(|error| error.to_string())?;
  }
  Ok(rolled_back)
}

/// Open an HTTPS destination in the system browser without navigating the App WebView.
#[tauri::command]
pub async fn shell_open_external_url(window: WebviewWindow, url: String) -> Result<(), String> {
  ensure_authorized_window(&window)?;
  let parsed = Url::parse(&url).map_err(|_| "invalid external URL")?;
  if parsed.scheme() != "https"
    || parsed.host_str().is_none()
    || !parsed.username().is_empty()
    || parsed.password().is_some()
    || url.len() > 4096
  {
    return Err("external destination must be an HTTPS URL".into());
  }
  tauri::async_runtime::spawn_blocking(move || {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("/usr/bin/open");
    #[cfg(target_os = "windows")]
    let mut command = {
      let mut command = std::process::Command::new("rundll32.exe");
      command.arg("url.dll,FileProtocolHandler");
      command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    let status = command
      .arg(parsed.as_str())
      .stdin(std::process::Stdio::null())
      .stdout(std::process::Stdio::null())
      .stderr(std::process::Stdio::null())
      .status()
      .map_err(|_| "could not open the system browser".to_owned())?;
    if status.success() {
      Ok(())
    } else {
      Err("could not open the system browser".to_owned())
    }
  })
  .await
  .map_err(|_| "could not open the system browser".to_owned())?
}

fn ensure_authorized_window(window: &WebviewWindow) -> Result<(), String> {
  let url = window.url().map_err(|error| error.to_string())?;
  if is_authorized_shell_origin(window.label(), &url, cfg!(debug_assertions)) {
    Ok(())
  } else {
    Err("Shell IPC is only available to the local main App window".into())
  }
}

fn is_authorized_shell_origin(label: &str, url: &Url, allow_dev_server: bool) -> bool {
  if label != "main" || !url.username().is_empty() || url.password().is_some() {
    return false;
  }

  let production_origin = match url.scheme() {
    "anvil-app" => url.host_str() == Some("localhost") && url.port().is_none(),
    "http" | "https" => url.host_str() == Some("anvil-app.localhost") && url.port().is_none(),
    _ => false,
  };

  let development_origin = allow_dev_server
    && url.scheme() == "http"
    && matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
    && url.port() == Some(5173);

  production_origin || development_origin
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::Value;

  #[test]
  fn accepts_only_the_main_window_and_shell_origins() {
    for source in [
      "anvil-app://localhost/",
      "http://anvil-app.localhost/",
      "https://anvil-app.localhost/",
    ] {
      let url = Url::parse(source).unwrap();
      assert!(is_authorized_shell_origin("main", &url, false), "{source}");
      assert!(
        !is_authorized_shell_origin("other", &url, false),
        "{source}"
      );
    }

    let dev_url = Url::parse("http://localhost:5173/").unwrap();
    assert!(is_authorized_shell_origin("main", &dev_url, true));
    assert!(!is_authorized_shell_origin("main", &dev_url, false));

    for source in [
      "https://example.com/",
      "http://localhost:5174/",
      "http://anvil-app.localhost:1234/",
      "anvil-app://example.com/",
      "http://user@anvil-app.localhost/",
    ] {
      let url = Url::parse(source).unwrap();
      assert!(!is_authorized_shell_origin("main", &url, true), "{source}");
    }
  }

  #[test]
  fn capability_grants_shell_commands_only_to_the_main_local_window() {
    let capability: Value =
      serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
    assert_eq!(capability["windows"], serde_json::json!(["main"]));
    assert!(capability.get("remote").is_none());

    let permissions = capability["permissions"].as_array().unwrap();
    let expected: Vec<String> = SHELL_COMMANDS
      .iter()
      .map(|command| format!("allow-{}", command.replace('_', "-")))
      .collect();
    assert_eq!(permissions.len(), expected.len() + 3);
    assert_eq!(permissions[0], "core:default");
    for permission in expected {
      assert!(permissions.iter().any(|entry| entry == &permission));
    }
  }
}
