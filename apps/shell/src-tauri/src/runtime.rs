use crate::{
  app_manifest::AppManifest,
  app_storage::{AppStorage, PublishedAppVersion},
};
use getrandom::fill as fill_random;
use serde::{Deserialize, Serialize};
use std::{
  fs,
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

/// Directory (inside the App's data directory) where the sidecar may publish
/// newer versions of itself and the pointer the Shell prefers at startup.
const SIDECAR_STORE_DIR: &str = "sidecar-store";
const OVERRIDE_POINTER_ENTRY: &str = "current.json";
const BLOCKED_LIST_ENTRY: &str = "blocked.json";
const MAX_BLOCKED_VERSIONS: usize = 64;

/// Sidecar self-update pointer, written by the running sidecar and preferred
/// over the packaged seed by the Shell when it validates.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SidecarOverride {
  schema_version: u32,
  /// Sidecar package version, opaque to the Shell; used for failure blocking.
  version: String,
  sidecar_entry: String,
  http_api_version: String,
}

/// Environment contract the Shell offers the sidecar so it can update itself:
/// a private writable store plus the API range the packaged UI accepts.
#[derive(Debug, Clone)]
struct SidecarEnvironment {
  store_dir: PathBuf,
  http_api_compatibility: String,
}

#[derive(Debug)]
struct SidecarCandidate {
  path: PathBuf,
  api_version: String,
  /// Present only for overrides; the sidecar package version for blocking.
  version: Option<String>,
}

#[derive(Debug)]
struct ActiveSidecars {
  /// Preferred first; the packaged seed is always last.
  candidates: Vec<SidecarCandidate>,
  environment: Option<SidecarEnvironment>,
}

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

    let Ok(storage) = AppStorage::for_current_user() else {
      self.set_status(RuntimeStatus {
        state: RuntimeState::Failed,
        failure: Some(RuntimeFailure::StorageUnavailable),
        api_version: None,
      });
      return;
    };
    self.start_from_storage(&storage);
  }

  /// Resolve sidecar candidates from `storage` and launch the first one that
  /// reaches the ready handshake, falling back through the list on failure.
  fn start_from_storage(&self, storage: &AppStorage) {
    let active = match active_sidecars(storage) {
      Ok(active) => active,
      Err(failure) => {
        self.set_status(RuntimeStatus {
          state: RuntimeState::Failed,
          failure: Some(failure),
          api_version: None,
        });
        return;
      }
    };
    if active.candidates.is_empty() {
      self.set_status(RuntimeStatus {
        state: RuntimeState::NotInstalled,
        failure: None,
        api_version: None,
      });
      return;
    }

    let mut last_failure = None;
    for candidate in active.candidates {
      let origin = candidate
        .version
        .as_deref()
        .map(|version| format!("override {version}"))
        .unwrap_or_else(|| "packaged seed".to_owned());
      match self.launch(
        &candidate.path,
        &candidate.api_version,
        active.environment.as_ref(),
      ) {
        Ok(()) => {
          tracing::info!(origin, "Runtime startup succeeded");
          return;
        }
        Err(failure) => {
          if let (Some(version), Some(environment)) =
            (&candidate.version, active.environment.as_ref())
          {
            block_override_version(environment, version);
          }
          tracing::warn!(origin, failure = ?failure, "Runtime candidate failed; falling back");
          last_failure = Some(failure);
        }
      }
    }
    self.set_status(RuntimeStatus {
      state: RuntimeState::Failed,
      failure: last_failure,
      api_version: None,
    });
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
      self.launch(&executable, &manifest.http_api_version, None)
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

  fn launch(
    &self,
    executable: &Path,
    expected_api_version: &str,
    environment: Option<&SidecarEnvironment>,
  ) -> Result<(), RuntimeFailure> {
    self.set_status(RuntimeStatus {
      state: RuntimeState::Starting,
      failure: None,
      api_version: Some(expected_api_version.to_owned()),
    });

    tracing::info!(api_version = expected_api_version, "Runtime launch started");
    let result = self.launch_ready(executable, expected_api_version, environment);
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
    environment: Option<&SidecarEnvironment>,
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
    // Self-update contract: a private writable store plus the API range the
    // packaged UI accepts, so the sidecar can publish compatible versions.
    if let Some(environment) = environment {
      command
        .env("ANVIL_SIDECAR_STORE_DIR", &environment.store_dir)
        .env("ANVIL_HTTP_API_COMPAT", &environment.http_api_compatibility);
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

fn active_sidecars(storage: &AppStorage) -> Result<ActiveSidecars, RuntimeFailure> {
  let store = storage
    .app(env!("ANVIL_DEFAULT_APP_ID"))
    .map_err(|_| RuntimeFailure::StorageUnavailable)?;
  let Some(version) = store
    .resolve_active()
    .map_err(|_| RuntimeFailure::StorageUnavailable)?
  else {
    return Ok(ActiveSidecars {
      candidates: Vec::new(),
      environment: None,
    });
  };

  let manifest = load_manifest(&version)?;
  let environment = SidecarEnvironment {
    store_dir: store.app_dir().join(SIDECAR_STORE_DIR),
    http_api_compatibility: manifest.http_api_compatibility.clone(),
  };
  let packaged = SidecarCandidate {
    path: version
      .entry_path(&manifest.sidecar_entry)
      .map_err(|_| RuntimeFailure::SidecarMissing)?,
    api_version: manifest.http_api_version.clone(),
    version: None,
  };
  if !is_executable(&packaged.path) {
    return Err(RuntimeFailure::SidecarNotExecutable);
  }

  let mut candidates = Vec::new();
  match resolve_override(&environment, &manifest) {
    Ok(Some(candidate)) => candidates.push(candidate),
    Ok(None) => {}
    Err(failure) => {
      tracing::warn!(failure = ?failure, "Ignoring an invalid sidecar override pointer");
    }
  }
  candidates.push(packaged);
  Ok(ActiveSidecars {
    candidates,
    environment: Some(environment),
  })
}

/// Read and validate the sidecar override pointer. `Ok(None)` means no valid
/// override is present; malformed pointers never fail startup outright.
fn resolve_override(
  environment: &SidecarEnvironment,
  manifest: &AppManifest,
) -> Result<Option<SidecarCandidate>, RuntimeFailure> {
  let Ok(source) = crate::bundled_app::read_entry(
    &environment.store_dir,
    OVERRIDE_POINTER_ENTRY,
    MAX_MANIFEST_BYTES,
  ) else {
    return Ok(None);
  };
  let pointer: SidecarOverride =
    serde_json::from_slice(&source).map_err(|_| RuntimeFailure::InvalidManifest)?;
  if pointer.schema_version != 1 {
    return Err(RuntimeFailure::InvalidManifest);
  }
  if semver::Version::parse(&pointer.version).is_err() {
    return Err(RuntimeFailure::InvalidManifest);
  }
  let api_version = semver::Version::parse(&pointer.http_api_version)
    .map_err(|_| RuntimeFailure::IncompatibleApi)?;
  let compatibility = semver::VersionReq::parse(&manifest.http_api_compatibility)
    .map_err(|_| RuntimeFailure::InvalidManifest)?;
  if !compatibility.matches(&api_version) {
    return Err(RuntimeFailure::IncompatibleApi);
  }
  if blocked_versions(environment).contains(&pointer.version) {
    tracing::warn!(
      version = %pointer.version,
      "Skipping a blocked sidecar override version"
    );
    return Ok(None);
  }
  let path = crate::bundled_app::resolve_entry(&environment.store_dir, &pointer.sidecar_entry)
    .map_err(|_| RuntimeFailure::SidecarMissing)?;
  if !is_executable(&path) {
    return Err(RuntimeFailure::SidecarNotExecutable);
  }
  Ok(Some(SidecarCandidate {
    path,
    api_version: pointer.http_api_version,
    version: Some(pointer.version),
  }))
}

fn blocked_versions(environment: &SidecarEnvironment) -> Vec<String> {
  crate::bundled_app::read_entry(
    &environment.store_dir,
    BLOCKED_LIST_ENTRY,
    MAX_MANIFEST_BYTES,
  )
  .ok()
  .and_then(|source| {
    serde_json::from_slice::<BlockedListWithSchema>(&source)
      .ok()
      .filter(|blocked| blocked.schema_version == 1)
      .map(|blocked| blocked.versions)
  })
  .unwrap_or_default()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockedListWithSchema {
  schema_version: u32,
  #[serde(default)]
  versions: Vec<String>,
}

/// Record a failed override version and drop the pointer, so the next startup
/// goes straight to the packaged seed and the sidecar stops re-offering it.
fn block_override_version(environment: &SidecarEnvironment, version: &str) {
  let mut versions = blocked_versions(environment);
  if !versions.iter().any(|entry| entry == version) {
    versions.push(version.to_owned());
  }
  versions.truncate(MAX_BLOCKED_VERSIONS);
  let document = BlockedListWithSchema {
    schema_version: 1,
    versions,
  };
  let result = (|| -> std::io::Result<()> {
    fs::create_dir_all(&environment.store_dir)?;
    let serialized = serde_json::to_vec(&document).map_err(std::io::Error::other)?;
    let mut temporary = tempfile::NamedTempFile::new_in(&environment.store_dir)?;
    use std::io::Write;
    temporary.write_all(&serialized)?;
    temporary.as_file().sync_all()?;
    temporary.persist(environment.store_dir.join(BLOCKED_LIST_ENTRY))?;
    let _ = fs::remove_file(environment.store_dir.join(OVERRIDE_POINTER_ENTRY));
    Ok(())
  })();
  if let Err(error) = result {
    tracing::warn!(%error, "Could not record the failed sidecar override version");
  }
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
    supervisor.launch(&script, "1.0.0", None).unwrap();
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
      supervisor.launch(&script, "1.0.0", None).unwrap_err(),
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

  // --- sidecar override (Phase 3) ---

  use crate::app_storage::AppVersionState;

  #[cfg(unix)]
  struct OverrideFixture {
    root: tempfile::TempDir,
    storage: AppStorage,
    seed_port: u16,
    store_dir: PathBuf,
  }

  /// Publish and activate a minimal App package with a working seed stub.
  #[cfg(unix)]
  fn override_fixture() -> OverrideFixture {
    let root = tempdir().unwrap();
    let storage = AppStorage::open(root.path().join("data")).unwrap();
    let store = storage.app(env!("ANVIL_DEFAULT_APP_ID")).unwrap();
    let staging = store.stage_version().unwrap();
    let directory = staging.directory();
    fs::create_dir_all(directory.join("web")).unwrap();
    fs::create_dir_all(directory.join("runtime")).unwrap();
    let manifest = serde_json::json!({
      "schema_version": 2,
      "app_id": env!("ANVIL_DEFAULT_APP_ID"),
      "version": "1.2.3",
      "target": env!("ANVIL_TARGET_PLATFORM"),
      "page_entry": "web/index.html",
      "sidecar_entry": "runtime/anvil",
      "http_api_version": "1.0.0",
      "http_api_compatibility": "^1.0.0",
      "shell_api_compatibility": env!("ANVIL_SHELL_API_VERSION"),
    });
    fs::write(directory.join("manifest.json"), manifest.to_string()).unwrap();
    fs::write(directory.join("web/index.html"), "page").unwrap();
    let seed_port = NEXT_PORT.fetch_add(1, AtomicOrdering::Relaxed);
    let seed = make_stub(
      directory,
      &format!(
        "#!/bin/sh\nIFS= read -r token || exit 2\nprintf '{{\"kind\":\"ready\",\"port\":{seed_port},\"apiVersion\":\"1.0.0\"}}\\n'\nIFS= read -r command\n[ \"$command\" = shutdown ]\n"
      ),
    );
    fs::copy(seed, directory.join("runtime/anvil")).unwrap();
    let version = semver::Version::parse("1.2.3").unwrap();
    staging.publish(&version).unwrap();
    store
      .write_state(&AppVersionState {
        active_version: Some(version),
        ..AppVersionState::default()
      })
      .unwrap();
    OverrideFixture {
      store_dir: store.app_dir().join(SIDECAR_STORE_DIR),
      root,
      storage,
      seed_port: seed_port as u16,
    }
  }

  /// Publish an override binary plus the pointer the Shell should prefer.
  #[cfg(unix)]
  fn write_override(
    fixture: &OverrideFixture,
    version: &str,
    http_api_version: &str,
    source: &str,
  ) {
    let binary_dir = fixture.store_dir.join("versions").join(version);
    fs::create_dir_all(&binary_dir).unwrap();
    let path = make_stub(&binary_dir, source);
    fs::rename(path, binary_dir.join("anvil")).unwrap();
    let pointer = serde_json::json!({
      "schema_version": 1,
      "version": version,
      "sidecar_entry": format!("versions/{version}/anvil"),
      "http_api_version": http_api_version,
    });
    fs::create_dir_all(&fixture.store_dir).unwrap();
    fs::write(
      fixture.store_dir.join(OVERRIDE_POINTER_ENTRY),
      pointer.to_string(),
    )
    .unwrap();
  }

  #[cfg(unix)]
  fn blocked_path(fixture: &OverrideFixture) -> PathBuf {
    fixture.store_dir.join(BLOCKED_LIST_ENTRY)
  }

  #[cfg(unix)]
  fn pointer_path(fixture: &OverrideFixture) -> PathBuf {
    fixture.store_dir.join(OVERRIDE_POINTER_ENTRY)
  }

  #[cfg(unix)]
  #[test]
  fn prefers_a_valid_override_and_passes_the_update_environment() {
    let fixture = override_fixture();
    let override_port = NEXT_PORT.fetch_add(1, AtomicOrdering::Relaxed);
    let marker = fixture.root.path().join("override-env.txt");
    let source = format!(
      "#!/bin/sh\nIFS= read -r token || exit 2\nprintf '{{\"kind\":\"ready\",\"port\":{override_port},\"apiVersion\":\"1.0.0\"}}\\n'\nenv | grep -E '^ANVIL_SIDECAR_STORE_DIR=|^ANVIL_HTTP_API_COMPAT=' > \"{marker}\"\nIFS= read -r command\n[ \"$command\" = shutdown ]\n",
      marker = marker.display(),
    );
    write_override(&fixture, "2.0.0", "1.0.0", &source);

    let supervisor = RuntimeSupervisor::new();
    supervisor.start_from_storage(&fixture.storage);
    let status = supervisor.status();
    assert_eq!(status.state, RuntimeState::Ready);
    let connection = supervisor.connection().unwrap();
    assert_eq!(
      connection.base_url,
      format!("http://127.0.0.1:{override_port}")
    );

    // The stub writes the marker after printing ready; poll until it lands.
    let mut environment = String::new();
    for _ in 0..40 {
      if let Ok(contents) = fs::read_to_string(&marker) {
        environment = contents;
        break;
      }
      thread::sleep(Duration::from_millis(50));
    }
    assert!(environment.contains(&format!(
      "ANVIL_SIDECAR_STORE_DIR={}",
      fixture.store_dir.display()
    )));
    assert!(environment.contains("ANVIL_HTTP_API_COMPAT=^1.0.0"));
    supervisor.shutdown();
  }

  #[cfg(unix)]
  #[test]
  fn falls_back_to_the_packaged_seed_and_blocks_the_failed_override() {
    let fixture = override_fixture();
    write_override(
      &fixture,
      "2.0.0",
      "1.0.0",
      "#!/bin/sh\nIFS= read -r token || exit 2\nexit 7\n",
    );

    let supervisor = RuntimeSupervisor::new();
    supervisor.start_from_storage(&fixture.storage);
    let status = supervisor.status();
    assert_eq!(status.state, RuntimeState::Ready);
    let connection = supervisor.connection().unwrap();
    assert_eq!(
      connection.base_url,
      format!("http://127.0.0.1:{}", fixture.seed_port)
    );

    let blocked: serde_json::Value =
      serde_json::from_slice(&fs::read(blocked_path(&fixture)).unwrap()).unwrap();
    assert!(
      blocked["versions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "2.0.0")
    );
    assert!(!pointer_path(&fixture).exists());
    supervisor.shutdown();
  }

  #[cfg(unix)]
  #[test]
  fn skips_an_override_outside_the_api_compatibility_range() {
    let fixture = override_fixture();
    let override_port = NEXT_PORT.fetch_add(1, AtomicOrdering::Relaxed);
    write_override(
      &fixture,
      "2.0.0",
      "2.0.0",
      &format!(
        "#!/bin/sh\nIFS= read -r token || exit 2\nprintf '{{\"kind\":\"ready\",\"port\":{override_port},\"apiVersion\":\"2.0.0\"}}\\n'\nIFS= read -r command\n[ \"$command\" = shutdown ]\n"
      ),
    );

    let supervisor = RuntimeSupervisor::new();
    supervisor.start_from_storage(&fixture.storage);
    assert_eq!(supervisor.status().state, RuntimeState::Ready);
    let connection = supervisor.connection().unwrap();
    assert_eq!(
      connection.base_url,
      format!("http://127.0.0.1:{}", fixture.seed_port)
    );
    // The pointer stays: the runtime may replace it with a compatible one.
    assert!(pointer_path(&fixture).exists());
    supervisor.shutdown();
  }

  #[cfg(unix)]
  #[test]
  fn never_launches_a_blocked_override_version() {
    let fixture = override_fixture();
    let override_port = NEXT_PORT.fetch_add(1, AtomicOrdering::Relaxed);
    write_override(
      &fixture,
      "2.0.0",
      "1.0.0",
      &format!(
        "#!/bin/sh\nIFS= read -r token || exit 2\nprintf '{{\"kind\":\"ready\",\"port\":{override_port},\"apiVersion\":\"1.0.0\"}}\\n'\nIFS= read -r command\n[ \"$command\" = shutdown ]\n"
      ),
    );
    let blocked = serde_json::json!({"schema_version": 1, "versions": ["2.0.0"]});
    fs::write(blocked_path(&fixture), blocked.to_string()).unwrap();

    let supervisor = RuntimeSupervisor::new();
    supervisor.start_from_storage(&fixture.storage);
    assert_eq!(supervisor.status().state, RuntimeState::Ready);
    let connection = supervisor.connection().unwrap();
    assert_eq!(
      connection.base_url,
      format!("http://127.0.0.1:{}", fixture.seed_port)
    );
    supervisor.shutdown();
  }
}
