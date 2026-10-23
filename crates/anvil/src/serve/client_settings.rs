//! Desktop API for the Core's per-instance launch settings.
use super::api::{ApiError, AppState};
use anvil_core::client::{ClientInstaller, ClientLaunchSettings};
use axum::{
  Json,
  extract::{Path, State},
  http::StatusCode,
};
use serde::Serialize;
fn error(error: anvil_core::Error) -> ApiError {
  tracing::warn!(
    error_code = error.code(),
    "Client launch settings operation failed"
  );
  let (status, message) = match error {
    anvil_core::Error::Invalid(message) => (StatusCode::BAD_REQUEST, message),
    anvil_core::Error::Conflict(message) => (StatusCode::CONFLICT, message),
    anvil_core::Error::Busy(_) => (StatusCode::CONFLICT, "设置正在保存，请稍后重试。".into()),
    _ => (
      StatusCode::INTERNAL_SERVER_ERROR,
      "无法读取或保存实例设置，请重试。".into(),
    ),
  };
  ApiError::new(status, message)
}
pub(super) async fn get(
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> Result<Json<ClientLaunchSettings>, ApiError> {
  ClientInstaller::new(state.runtime.context.clone())
    .launch_settings(&name)
    .await
    .map(Json)
    .map_err(error)
}
pub(super) async fn save(
  State(state): State<AppState>,
  Path(name): Path<String>,
  Json(settings): Json<ClientLaunchSettings>,
) -> Result<Json<ClientLaunchSettings>, ApiError> {
  ClientInstaller::new(state.runtime.context.clone())
    .save_launch_settings(&name, settings)
    .await
    .map(Json)
    .map_err(error)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Memory {
  total_mib: Option<u64>,
}
pub(super) async fn memory() -> Json<Memory> {
  static MEMORY: tokio::sync::OnceCell<Option<u64>> = tokio::sync::OnceCell::const_new();
  Json(Memory {
    total_mib: *MEMORY.get_or_init(total_memory).await,
  })
}
async fn total_memory() -> Option<u64> {
  #[cfg(target_os = "linux")]
  {
    tokio::task::spawn_blocking(|| {
      use std::io::Read;
      let mut content = String::new();
      std::fs::File::open("/proc/meminfo")
        .ok()?
        .take(16384)
        .read_to_string(&mut content)
        .ok()?;
      let kib = content
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
      Some(kib / 1024)
    })
    .await
    .ok()
    .flatten()
  }
  #[cfg(any(target_os = "macos", target_os = "windows"))]
  {
    #[cfg(target_os = "macos")]
    let mut command = {
      let mut cmd = tokio::process::Command::new("/usr/sbin/sysctl");
      cmd.args(["-n", "hw.memsize"]);
      cmd
    };
    #[cfg(target_os = "windows")]
    let mut command = {
      let root = std::env::var_os("SystemRoot").or_else(|| std::env::var_os("WINDIR"))?;
      let executable =
        std::path::PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
      let mut cmd = tokio::process::Command::new(executable);
      cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "[Console]::Write([uint64](Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory)",
      ]);
      cmd.creation_flags(0x08000000);
      cmd
    };
    command
      .kill_on_drop(true)
      .stdin(std::process::Stdio::null())
      .stderr(std::process::Stdio::null());
    let output = tokio::time::timeout(std::time::Duration::from_secs(2), command.output())
      .await
      .ok()?
      .ok()?;
    if !output.status.success() || output.stdout.len() > 64 {
      return None;
    }
    let bytes = String::from_utf8(output.stdout)
      .ok()?
      .trim()
      .parse::<u64>()
      .ok()?;
    Some(bytes / (1024 * 1024))
  }
  #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
  {
    None
  }
}
