use crate::{
  app_manifest::AppManifest,
  app_storage::{AppStorage, PublishedAppVersion},
};
use getrandom::fill as fill_random;
use serde::{Deserialize, Serialize};
use std::{
  io::{self, BufRead, BufReader, Read, Write},
  path::{Path, PathBuf},
  process::{Child, ChildStdin, Command, Stdio},
  sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
    mpsc,
  },
  thread,
  time::{Duration, Instant},
};

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_READY_LINE_BYTES: usize = 4096;
const READY_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const MONITOR_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
  NotInstalled,
  Starting,
  Ready,
  Failed,
  Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeFailure {
  StorageUnavailable,
  BundledAppInstallFailed,
  InvalidManifest,
  SidecarMissing,
  SidecarNotExecutable,
  SecureRandomUnavailable,
  SpawnFailed,
  StartupTimeout,
  InvalidReadyMessage,
  IncompatibleApi,
  ProcessExited,
  ProcessMonitorFailed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
  pub state: RuntimeState,
  pub failure: Option<RuntimeFailure>,
  pub api_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConnection {
  pub base_url: String,
  pub token: String,
  pub api_version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadyMessage {
  kind: String,
  port: u16,
  api_version: String,
}

#[derive(Debug)]
struct Inner {
  status: RuntimeStatus,
  connection: Option<RuntimeConnection>,
  child: Option<Child>,
  child_stdin: Option<ChildStdin>,
  stop_monitor: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisor {
  inner: Arc<Mutex<Inner>>,
}

impl RuntimeSupervisor {
  pub fn new() -> Self {
    Self {
      inner: Arc::new(Mutex::new(Inner {
        status: RuntimeStatus {
          state: RuntimeState::NotInstalled,
          failure: None,
          api_version: None,
        },
        connection: None,
        child: None,
        child_stdin: None,
        stop_monitor: None,
      })),
    }
  }

  /// Start the sidecar from the currently active, validated App package.
  pub fn start_default_app(&self) {
    self.set_status(RuntimeStatus {
      state: RuntimeState::Starting,
      failure: None,
      api_version: None,
    });

    match active_sidecar() {
      Ok(None) => self.set_status(RuntimeStatus {
        state: RuntimeState::NotInstalled,
        failure: None,
        api_version: None,
      }),
      Ok(Some((path, api_version))) => {
        if let Err(failure) = self.launch(&path, &api_version) {
          self.set_status(RuntimeStatus {
            state: RuntimeState::Failed,
            failure: Some(failure),
            api_version: None,
          });
        }
      }
      Err(failure) => self.set_status(RuntimeStatus {
        state: RuntimeState::Failed,
        failure: Some(failure),
        api_version: None,
      }),
    }
  }

  pub fn status(&self) -> RuntimeStatus {
    self
      .inner
      .lock()
      .map(|inner| inner.status.clone())
      .unwrap_or(RuntimeStatus {
        state: RuntimeState::Failed,
        failure: Some(RuntimeFailure::ProcessMonitorFailed),
        api_version: None,
      })
  }

  /// Use the freshly built source package during `tauri dev`, without installing it.
  #[cfg(dev)]
  pub fn start_development_app(&self, package_directory: &Path) {
    let result = (|| {
      let root = package_directory
        .canonicalize()
        .map_err(|_| RuntimeFailure::InvalidManifest)?;
      let manifest = crate::bundled_app::validate_package(
        &root,
        env!("ANVIL_DEFAULT_APP_ID"),
        env!("ANVIL_TARGET_PLATFORM"),
        env!("ANVIL_SHELL_API_VERSION"),
        env!("ANVIL_BUNDLED_APP_VERSION"),
      )
      .map_err(|_| RuntimeFailure::InvalidManifest)?;
      let executable = root.join(&manifest.sidecar_entry);
      if !is_executable(&executable) {
        return Err(RuntimeFailure::SidecarNotExecutable);
      }
      self.launch(&executable, &manifest.http_api_version)
    })();
    if let Err(failure) = result {
      self.mark_startup_failure(failure);
    }
  }

  pub fn connection(&self) -> Option<RuntimeConnection> {
    self
      .inner
      .lock()
      .ok()
      .and_then(|inner| inner.connection.clone())
  }

  pub fn mark_startup_failure(&self, failure: RuntimeFailure) {
    self.set_status(RuntimeStatus {
      state: RuntimeState::Failed,
      failure: Some(failure),
      api_version: None,
    });
  }

  /// Ask the sidecar to stop, wait for graceful shutdown, then force-kill on timeout.
  pub fn shutdown(&self) {
    tracing::info!("Runtime shutdown requested");
    let (mut child, mut child_stdin) = match self.inner.lock() {
      Ok(mut inner) => {
        if let Some(stop) = inner.stop_monitor.take() {
          stop.store(true, Ordering::Release);
        }
        inner.connection = None;
        (inner.child.take(), inner.child_stdin.take())
      }
      Err(_) => return,
    };

    if let Some(mut stdin) = child_stdin.take() {
      let _ = stdin.write_all(b"shutdown\n");
      let _ = stdin.flush();
    }
    drop(child_stdin);

    if let Some(child) = child.as_mut() {
      let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
      loop {
        match child.try_wait() {
          Ok(Some(_)) => break,
          Ok(None) if Instant::now() < deadline => {
            thread::sleep(Duration::from_millis(50));
          }
          Ok(None) | Err(_) => {
            tracing::warn!("Runtime did not stop gracefully; terminating process");
            let _ = child.kill();
            let _ = child.wait();
            break;
          }
        }
      }
    }
    drop(child);
    self.set_status(RuntimeStatus {
      state: RuntimeState::Stopped,
      failure: None,
      api_version: None,
    });
  }

  fn launch(&self, executable: &Path, expected_api_version: &str) -> Result<(), RuntimeFailure> {
    self.set_status(RuntimeStatus {
      state: RuntimeState::Starting,
      failure: None,
      api_version: Some(expected_api_version.to_owned()),
    });

    tracing::info!(api_version = expected_api_version, "Runtime launch started");
    let result = self.launch_ready(executable, expected_api_version);
    if let Err(failure) = result {
      self.set_status(RuntimeStatus {
        state: RuntimeState::Failed,
        failure: Some(failure),
        api_version: None,
      });
    }
    result
  }

  fn launch_ready(
    &self,
    executable: &Path,
    expected_api_version: &str,
  ) -> Result<(), RuntimeFailure> {
    let token = create_token()?;
    let mut command = Command::new(executable);
    command
      .env_clear()
      .env("ANVIL_HTTP_API_VERSION", expected_api_version)
      .env("ANVIL_LOG_STDERR", if cfg!(dev) { "1" } else { "0" })
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped());
    #[cfg(windows)]
    for key in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
      if let Some(value) = std::env::var_os(key) {
        command.env(key, value);
      }
    }
    // Keep both processes in the same log directory even when XDG/HOME
    // overrides are absent from the sanitized sidecar environment.
    if let Ok(directory) = anvil_logging::log_directory() {
      command.env("ANVIL_LOG_DIRECTORY", directory);
    }
    if let Some(value) = std::env::var_os("RUST_LOG") {
      command.env("RUST_LOG", value);
    }
    // The public App registration can be overridden for local development.
    if let Some(value) = std::env::var_os("ANVIL_MICROSOFT_CLIENT_ID") {
      command.env("ANVIL_MICROSOFT_CLIENT_ID", value);
    }
    let mut child = command.spawn().map_err(|_| RuntimeFailure::SpawnFailed)?;

    let mut child_stdin = child.stdin.take().ok_or(RuntimeFailure::SpawnFailed)?;
    child_stdin
      .write_all(token.as_bytes())
      .and_then(|_| child_stdin.write_all(b"\n"))
      .and_then(|_| child_stdin.flush())
      .map_err(|_| {
        let _ = child.kill();
        let _ = child.wait();
        RuntimeFailure::SpawnFailed
      })?;

    if let Some(stderr) = child.stderr.take() {
      forward_stderr(stderr);
    }
    let stdout = child.stdout.take().ok_or_else(|| {
      let _ = child.kill();
      let _ = child.wait();
      RuntimeFailure::InvalidReadyMessage
    })?;
    let ready_line = match read_ready_line(stdout) {
      Ok(line) => line,
      Err(failure) => {
        let _ = child.kill();
        let _ = child.wait();
        return Err(failure);
      }
    };
    let ready: ReadyMessage = serde_json::from_str(&ready_line).map_err(|_| {
      let _ = child.kill();
      let _ = child.wait();
      RuntimeFailure::InvalidReadyMessage
    })?;
    if ready.kind != "ready" || ready.port == 0 {
      let _ = child.kill();
      let _ = child.wait();
      return Err(RuntimeFailure::InvalidReadyMessage);
    }
    if ready.api_version != expected_api_version {
      let _ = child.kill();
      let _ = child.wait();
      return Err(RuntimeFailure::IncompatibleApi);
    }
    if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
      let _ = child.kill();
      let _ = child.wait();
      return Err(RuntimeFailure::ProcessExited);
    }

    let connection = RuntimeConnection {
      base_url: format!("http://127.0.0.1:{}", ready.port),
      token,
      api_version: ready.api_version.clone(),
    };
    let stop_monitor = Arc::new(AtomicBool::new(false));
    {
      let mut inner = self
        .inner
        .lock()
        .map_err(|_| RuntimeFailure::ProcessMonitorFailed)?;
      inner.child = Some(child);
      inner.child_stdin = Some(child_stdin);
      inner.connection = Some(connection);
      inner.status = RuntimeStatus {
        state: RuntimeState::Ready,
        failure: None,
        api_version: Some(ready.api_version),
      };
      inner.stop_monitor = Some(stop_monitor.clone());
    }
    tracing::info!("Runtime ready handshake accepted");
    spawn_monitor(Arc::downgrade(&self.inner), stop_monitor);
    Ok(())
  }

  fn set_status(&self, status: RuntimeStatus) {
    if let Ok(mut inner) = self.inner.lock() {
      if status.failure.is_some() {
        tracing::warn!(state = ?status.state, failure = ?status.failure, "Runtime status changed");
      } else {
        tracing::info!(state = ?status.state, "Runtime status changed");
      }
      inner.connection = None;
      inner.status = status;
    }
  }
}

impl Default for RuntimeSupervisor {
  fn default() -> Self {
    Self::new()
  }
}

fn active_sidecar() -> Result<Option<(PathBuf, String)>, RuntimeFailure> {
  let storage = AppStorage::for_current_user().map_err(|_| RuntimeFailure::StorageUnavailable)?;
  let store = storage
    .app(env!("ANVIL_DEFAULT_APP_ID"))
    .map_err(|_| RuntimeFailure::StorageUnavailable)?;
  let Some(version) = store
    .resolve_active()
    .map_err(|_| RuntimeFailure::StorageUnavailable)?
  else {
    return Ok(None);
  };

  let manifest = load_manifest(&version)?;
  let path = version
    .entry_path(&manifest.sidecar_entry)
    .map_err(|_| RuntimeFailure::SidecarMissing)?;
  if !is_executable(&path) {
    return Err(RuntimeFailure::SidecarNotExecutable);
  }
  Ok(Some((path, manifest.http_api_version)))
}

fn load_manifest(version: &PublishedAppVersion) -> Result<AppManifest, RuntimeFailure> {
  let file = version
    .open_entry("manifest.json")
    .map_err(|_| RuntimeFailure::InvalidManifest)?;
  if file
    .metadata()
    .map_err(|_| RuntimeFailure::InvalidManifest)?
    .len()
    > MAX_MANIFEST_BYTES
  {
    return Err(RuntimeFailure::InvalidManifest);
  }
  let mut source = Vec::new();
  file
    .take(MAX_MANIFEST_BYTES + 1)
    .read_to_end(&mut source)
    .map_err(|_| RuntimeFailure::InvalidManifest)?;
  let manifest = AppManifest::parse_and_validate(
    &source,
    env!("ANVIL_DEFAULT_APP_ID"),
    env!("ANVIL_TARGET_PLATFORM"),
    env!("ANVIL_SHELL_API_VERSION"),
  )
  .map_err(|_| RuntimeFailure::InvalidManifest)?;
  let app_version =
    semver::Version::parse(&manifest.version).map_err(|_| RuntimeFailure::InvalidManifest)?;
  if app_version != *version.version() {
    return Err(RuntimeFailure::InvalidManifest);
  }
  Ok(manifest)
}

fn is_executable(path: &Path) -> bool {
  let Ok(metadata) = std::fs::metadata(path) else {
    return false;
  };
  if !metadata.is_file() {
    return false;
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
  }
  #[cfg(not(unix))]
  {
    true
  }
}

fn create_token() -> Result<String, RuntimeFailure> {
  let mut bytes = [0u8; 32];
  fill_random(&mut bytes).map_err(|_| RuntimeFailure::SecureRandomUnavailable)?;
  let mut token = String::with_capacity(64);
  for byte in bytes {
    use std::fmt::Write as _;
    write!(token, "{byte:02x}").expect("writing to a String cannot fail");
  }
  Ok(token)
}

fn read_ready_line(stdout: impl Read + Send + 'static) -> Result<String, RuntimeFailure> {
  let (sender, receiver) = mpsc::sync_channel(1);
  thread::spawn(move || {
    let result = read_bounded_line(BufReader::new(stdout), MAX_READY_LINE_BYTES)
      .map_err(|_| RuntimeFailure::InvalidReadyMessage);
    let _ = sender.send(result);
  });
  match receiver.recv_timeout(READY_TIMEOUT) {
    Ok(Ok(line)) => Ok(line),
    Ok(Err(error)) => Err(error),
    Err(mpsc::RecvTimeoutError::Timeout) => Err(RuntimeFailure::StartupTimeout),
    Err(mpsc::RecvTimeoutError::Disconnected) => Err(RuntimeFailure::InvalidReadyMessage),
  }
}

fn read_bounded_line(mut reader: impl BufRead, limit: usize) -> io::Result<String> {
  let mut bytes = Vec::with_capacity(limit.min(256));
  loop {
    let available = reader.fill_buf()?;
    if available.is_empty() {
      return Err(io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "missing ready message",
      ));
    }
    let take = available.iter().position(|byte| *byte == b'\n');
    let count = take.map_or(available.len(), |index| index + 1);
    if bytes.len() + count > limit {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "ready message too large",
      ));
    }
    bytes.extend_from_slice(&available[..count]);
    reader.consume(count);
    if take.is_some() {
      return String::from_utf8(bytes)
        .map(|line| line.trim_end_matches(['\r', '\n']).to_owned())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "ready message is not UTF-8"));
    }
  }
}

fn forward_stderr(mut stderr: impl Read + Send + 'static) {
  thread::spawn(move || {
    let _ = io::copy(&mut stderr, &mut io::stderr());
  });
}

fn spawn_monitor(inner: Weak<Mutex<Inner>>, stop: Arc<AtomicBool>) {
  thread::spawn(move || {
    while !stop.load(Ordering::Acquire) {
      thread::sleep(MONITOR_INTERVAL);
      if stop.load(Ordering::Acquire) {
        return;
      }
      let Some(inner) = inner.upgrade() else {
        return;
      };
      let Ok(mut inner) = inner.lock() else {
        return;
      };
      let Some(child) = inner.child.as_mut() else {
        return;
      };
      match child.try_wait() {
        Ok(None) => {}
        Ok(Some(status)) => {
          tracing::warn!(exit_code = ?status.code(), "Runtime exited unexpectedly");
          inner.child = None;
          inner.child_stdin = None;
          inner.connection = None;
          inner.status = RuntimeStatus {
            state: RuntimeState::Failed,
            failure: Some(RuntimeFailure::ProcessExited),
            api_version: None,
          };
          return;
        }
        Err(_) => {
          tracing::warn!("Runtime process monitoring failed");
          inner.connection = None;
          inner.status = RuntimeStatus {
            state: RuntimeState::Failed,
            failure: Some(RuntimeFailure::ProcessMonitorFailed),
            api_version: None,
          };
          return;
        }
      }
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;
  #[cfg(unix)]
  use std::os::unix::fs::PermissionsExt;
  use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
  use tempfile::tempdir;

  #[cfg(unix)]
  static NEXT_PORT: AtomicU64 = AtomicU64::new(44000);

  #[cfg(unix)]
  #[test]
  fn starts_ready_sidecar_and_stops_it_gracefully() {
    let directory = tempdir().unwrap();
    let port = NEXT_PORT.fetch_add(1, AtomicOrdering::Relaxed);
    let script = make_stub(
      directory.path(),
      &format!(
        "#!/bin/sh\nIFS= read -r token || exit 2\nprintf '{{\"kind\":\"ready\",\"port\":{port},\"apiVersion\":\"1.0.0\"}}\\n'\nIFS= read -r command\n[ \"$command\" = shutdown ]\n"
      ),
    );
    let supervisor = RuntimeSupervisor::new();
    supervisor.launch(&script, "1.0.0").unwrap();
    assert_eq!(supervisor.status().state, RuntimeState::Ready);
    let connection = supervisor.connection().unwrap();
    assert_eq!(connection.base_url, format!("http://127.0.0.1:{port}"));
    assert_eq!(connection.api_version, "1.0.0");
    assert_eq!(connection.token.len(), 64);

    supervisor.shutdown();
    assert_eq!(supervisor.status().state, RuntimeState::Stopped);
    assert!(supervisor.connection().is_none());
  }

  #[cfg(unix)]
  #[test]
  fn failed_handshake_is_reported_and_child_is_reaped() {
    let directory = tempdir().unwrap();
    let script = make_stub(
      directory.path(),
      "#!/bin/sh\nIFS= read -r token || exit 2\nexit 7\n",
    );
    let supervisor = RuntimeSupervisor::new();
    assert_eq!(
      supervisor.launch(&script, "1.0.0").unwrap_err(),
      RuntimeFailure::InvalidReadyMessage
    );
    assert_eq!(supervisor.status().state, RuntimeState::Failed);
  }

  #[cfg(unix)]
  fn make_stub(directory: &Path, source: &str) -> PathBuf {
    let path = directory.join("sidecar-stub");
    std::fs::write(&path, source).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path
  }
}
