use crate::{
  app_manifest::AppManifest,
  app_storage::{AppStorage, AppStorageError, AppVersionStore, PublishedAppVersion},
};
use flate2::read::GzDecoder;
use q2q_http::{Checksum, DownloadRequest, Downloader, Task};
use ring::{digest, signature};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
  collections::{HashMap, HashSet},
  fs::{self, File, OpenOptions},
  io::{self, Read, Write},
  path::{Path, PathBuf},
  sync::{Arc, Mutex},
  time::{Duration, SystemTime},
};
use tar::EntryType;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

const UPDATE_MANIFEST_URL: Option<&str> = option_env!("ANVIL_UPDATE_MANIFEST_URL");
const UPDATE_PUBLIC_KEY: Option<&str> = option_env!("ANVIL_UPDATE_PUBLIC_KEY");
const MAX_UPDATE_MANIFEST_BYTES: usize = 128 * 1024;
const MAX_PACKAGE_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_PACKAGE_FILES: usize = 100_000;
const MAX_PACKAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = MAX_PACKAGE_FILES + 100_000;
const UPDATE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);
type FileFingerprint = (u64, [u8; 32], bool);
type PackageFingerprints = HashMap<String, FileFingerprint>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppUpdateState {
  NotConfigured,
  Idle,
  Checking,
  Downloading,
  Verifying,
  Staged,
  UpToDate,
  Failed,
  Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppUpdateFailure {
  StorageUnavailable,
  Network,
  InvalidSignature,
  InvalidRelease,
  Download,
  InvalidPackage,
  VersionConflict,
  Cancelled,
  ActivationFailed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateStatus {
  pub state: AppUpdateState,
  pub failure: Option<AppUpdateFailure>,
  pub current_version: Option<String>,
  pub available_version: Option<String>,
  pub downloaded_bytes: u64,
  pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateCheckOutcome {
  Started,
  AlreadyRunning,
  RecentlyChecked,
  NotConfigured,
  Failed,
}

#[derive(Debug, Clone)]
struct UpdateConfig {
  manifest_url: String,
  public_key: [u8; 32],
  app_id: String,
  target: String,
  shell_api_version: String,
}

#[derive(Debug)]
struct UpdateInner {
  config: Option<UpdateConfig>,
  status: Mutex<AppUpdateStatus>,
  cancellation: Mutex<Option<CancellationToken>>,
}

#[derive(Debug, Clone)]
pub struct UpdateService {
  inner: Arc<UpdateInner>,
}

impl UpdateService {
  pub fn new() -> Self {
    let config = build_update_config();
    let state = if config.is_some() {
      AppUpdateState::Idle
    } else {
      AppUpdateState::NotConfigured
    };
    Self {
      inner: Arc::new(UpdateInner {
        config,
        status: Mutex::new(AppUpdateStatus {
          state,
          failure: None,
          current_version: None,
          available_version: None,
          downloaded_bytes: 0,
          total_bytes: None,
        }),
        cancellation: Mutex::new(None),
      }),
    }
  }

  pub fn status(&self) -> AppUpdateStatus {
    self
      .inner
      .status
      .lock()
      .map(|status| status.clone())
      .unwrap_or(AppUpdateStatus {
        state: AppUpdateState::Failed,
        failure: Some(AppUpdateFailure::StorageUnavailable),
        current_version: None,
        available_version: None,
        downloaded_bytes: 0,
        total_bytes: None,
      })
  }

  /// Start an update check if the configured source has not been checked in the last day.
  pub fn check_for_updates(&self) -> UpdateCheckOutcome {
    let Some(config) = self.inner.config.clone() else {
      return UpdateCheckOutcome::NotConfigured;
    };

    let mut status = match self.inner.status.lock() {
      Ok(status) => status,
      Err(_) => return UpdateCheckOutcome::Failed,
    };
    if matches!(
      status.state,
      AppUpdateState::Checking | AppUpdateState::Downloading | AppUpdateState::Verifying
    ) {
      return UpdateCheckOutcome::AlreadyRunning;
    }

    let store = match AppStorage::for_current_user().and_then(|storage| storage.app(&config.app_id))
    {
      Ok(store) => store,
      Err(_) => {
        status.state = AppUpdateState::Failed;
        status.failure = Some(AppUpdateFailure::StorageUnavailable);
        tracing::warn!(failure = ?status.failure, "App update check could not access storage");
        return UpdateCheckOutcome::Failed;
      }
    };
    let now = SystemTime::now();
    match store.last_update_check() {
      Ok(Some(last_check))
        if now.duration_since(last_check).unwrap_or_default() < UPDATE_INTERVAL =>
      {
        return UpdateCheckOutcome::RecentlyChecked;
      }
      Ok(_) => {}
      Err(_) => {
        status.state = AppUpdateState::Failed;
        status.failure = Some(AppUpdateFailure::StorageUnavailable);
        tracing::warn!(failure = ?status.failure, "App update check could not access storage");
        return UpdateCheckOutcome::Failed;
      }
    }
    let current_version = match store.state() {
      Ok(state) => state.active_version.map(|version| version.to_string()),
      Err(_) => {
        status.state = AppUpdateState::Failed;
        status.failure = Some(AppUpdateFailure::StorageUnavailable);
        tracing::warn!(failure = ?status.failure, "App update check could not access storage");
        return UpdateCheckOutcome::Failed;
      }
    };
    if store.record_update_check(now).is_err() {
      status.state = AppUpdateState::Failed;
      status.failure = Some(AppUpdateFailure::StorageUnavailable);
      tracing::warn!(failure = ?status.failure, "App update check could not access storage");
      return UpdateCheckOutcome::Failed;
    }
    *status = AppUpdateStatus {
      state: AppUpdateState::Checking,
      failure: None,
      current_version,
      available_version: None,
      downloaded_bytes: 0,
      total_bytes: None,
    };
    let cancellation = CancellationToken::new();
    if let Ok(mut slot) = self.inner.cancellation.lock() {
      *slot = Some(cancellation.clone());
    }
    tracing::info!(state = ?status.state, "App update state changed");
    drop(status);

    let inner = self.inner.clone();
    tauri::async_runtime::spawn(async move {
      let result = run_update_check(inner.clone(), config, store, cancellation.clone()).await;
      if let Err(error) = result {
        set_failure(&inner, error);
      }
      if let Ok(mut slot) = inner.cancellation.lock() {
        *slot = None;
      }
    });
    UpdateCheckOutcome::Started
  }

  /// Check at startup, then poll hourly; the persisted timestamp enforces the daily limit.
  pub fn start_background_checks(&self) {
    if self.inner.config.is_none() {
      return;
    }
    let _ = self.check_for_updates();
    let service = self.clone();
    tauri::async_runtime::spawn(async move {
      loop {
        tokio::time::sleep(CHECK_INTERVAL).await;
        let _ = service.check_for_updates();
      }
    });
  }

  pub fn cancel(&self) -> bool {
    self
      .inner
      .cancellation
      .lock()
      .ok()
      .and_then(|slot| slot.as_ref().map(CancellationToken::cancel))
      .is_some()
  }

  pub fn shutdown(&self) {
    let _ = self.cancel();
  }
}

impl Default for UpdateService {
  fn default() -> Self {
    Self::new()
  }
}

fn build_update_config() -> Option<UpdateConfig> {
  let manifest_url = UPDATE_MANIFEST_URL?;
  let public_key = UPDATE_PUBLIC_KEY?;
  let public_key = decode_hex::<32>(public_key).ok()?;
  Some(UpdateConfig {
    manifest_url: manifest_url.to_owned(),
    public_key,
    app_id: env!("ANVIL_DEFAULT_APP_ID").to_owned(),
    target: env!("ANVIL_TARGET_PLATFORM").to_owned(),
    shell_api_version: env!("ANVIL_SHELL_API_VERSION").to_owned(),
  })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedReleaseEnvelope {
  payload: String,
  signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedRelease {
  schema_version: u32,
  channel: String,
  app_id: String,
  version: String,
  target: String,
  package_url: String,
  package_size: u64,
  package_sha256: String,
}

#[derive(Debug, Clone)]
struct ReleaseMetadata {
  version: Version,
  url: String,
  size: u64,
  sha256: String,
}

fn parse_signed_release(
  source: &[u8],
  public_key: &[u8; 32],
  expected_app_id: &str,
  expected_target: &str,
) -> Result<ReleaseMetadata, UpdateError> {
  if source.len() > MAX_UPDATE_MANIFEST_BYTES {
    return Err(UpdateError::InvalidRelease);
  }
  let envelope: SignedReleaseEnvelope =
    serde_json::from_slice(source).map_err(|_| UpdateError::InvalidRelease)?;
  if envelope.payload.len() > MAX_UPDATE_MANIFEST_BYTES * 2 {
    return Err(UpdateError::InvalidRelease);
  }
  let payload = decode_variable_hex(&envelope.payload)?;
  let signature_bytes = decode_hex::<64>(&envelope.signature)?;
  signature::UnparsedPublicKey::new(&signature::ED25519, public_key)
    .verify(&payload, &signature_bytes)
    .map_err(|_| UpdateError::InvalidSignature)?;

  let release: SignedRelease =
    serde_json::from_slice(&payload).map_err(|_| UpdateError::InvalidRelease)?;
  if release.schema_version != 1
    || release.channel != "stable"
    || release.app_id != expected_app_id
    || release.target != expected_target
    || release.package_size == 0
    || release.package_size > MAX_PACKAGE_ARCHIVE_BYTES
    || release.package_sha256.len() != 64
    || !release
      .package_sha256
      .bytes()
      .all(|byte| byte.is_ascii_hexdigit())
  {
    return Err(UpdateError::InvalidRelease);
  }
  let version = Version::parse(&release.version).map_err(|_| UpdateError::InvalidRelease)?;
  if version.to_string() != release.version {
    return Err(UpdateError::InvalidRelease);
  }
  let url = url::Url::parse(&release.package_url).map_err(|_| UpdateError::InvalidRelease)?;
  let trusted_transport = url.scheme() == "https";
  #[cfg(test)]
  let trusted_transport = trusted_transport
    || (url.scheme() == "http"
      && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")));
  if !trusted_transport
    || url.host_str().is_none()
    || !url.username().is_empty()
    || url.password().is_some()
    || url.fragment().is_some()
  {
    return Err(UpdateError::InvalidRelease);
  }
  Ok(ReleaseMetadata {
    version,
    url: release.package_url,
    size: release.package_size,
    sha256: release.package_sha256.to_ascii_lowercase(),
  })
}

async fn run_update_check(
  inner: Arc<UpdateInner>,
  config: UpdateConfig,
  store: AppVersionStore,
  cancellation: CancellationToken,
) -> Result<(), UpdateError> {
  let downloader = Downloader::builder()
    .max_active_files(1)
    .max_requests(4)
    .max_requests_per_host(2)
    .max_disk_tasks(1)
    .build()
    .map_err(|_| UpdateError::Network)?;
  let response = tokio::select! {
    _ = cancellation.cancelled() => return Err(UpdateError::Cancelled),
    response = downloader.executor().get(&config.manifest_url).max_body_size(MAX_UPDATE_MANIFEST_BYTES as u64).send() => {
      response.map_err(|_| UpdateError::Network)?
    }
  };
  let manifest_bytes = tokio::select! {
    _ = cancellation.cancelled() => return Err(UpdateError::Cancelled),
    response = response.collect_bounded(MAX_UPDATE_MANIFEST_BYTES) => response.map_err(|_| UpdateError::Network)?,
  };
  let release = parse_signed_release(
    &manifest_bytes,
    &config.public_key,
    &config.app_id,
    &config.target,
  )?;
  let initial_state = store.state().map_err(|_| UpdateError::Storage)?;
  let current_version = initial_state.active_version.clone();
  update_status(&inner, |status| {
    status.available_version = Some(release.version.to_string());
    status.current_version = current_version.as_ref().map(ToString::to_string);
  });

  if initial_state.failed_version.as_ref() == Some(&release.version) {
    update_status(&inner, |status| {
      status.state = AppUpdateState::Failed;
      status.failure = Some(AppUpdateFailure::ActivationFailed);
    });
    return Ok(());
  }

  if initial_state
    .pending_version
    .as_ref()
    .is_some_and(|pending| pending > &release.version)
  {
    update_status(&inner, |status| {
      status.state = AppUpdateState::Staged;
      status.available_version = initial_state
        .pending_version
        .as_ref()
        .map(ToString::to_string);
      status.failure = None;
    });
    return Ok(());
  }
  if current_version
    .as_ref()
    .is_some_and(|current| release.version <= *current)
    && initial_state.pending_version.is_none()
  {
    update_status(&inner, |status| {
      status.state = AppUpdateState::UpToDate;
      status.failure = None;
    });
    return Ok(());
  }
  update_status(&inner, |status| {
    status.state = AppUpdateState::Downloading;
    status.failure = None;
    status.downloaded_bytes = 0;
    status.total_bytes = Some(release.size);
  });
  let download_directory = store
    .update_staging_dir(&release.version)
    .map_err(|_| UpdateError::Storage)?;
  let archive_path = download_directory.join("package.tar.gz");
  validate_download_paths(&archive_path)?;
  let checksum = Checksum::sha256(&release.sha256).map_err(|_| UpdateError::InvalidRelease)?;
  download_package(
    &inner,
    &downloader,
    &release,
    &archive_path,
    checksum,
    &cancellation,
  )
  .await?;

  if cancellation.is_cancelled() {
    return Err(UpdateError::Cancelled);
  }
  update_status(&inner, |status| status.state = AppUpdateState::Verifying);
  let staging = store.stage_version().map_err(|_| UpdateError::Storage)?;
  let manifest = extract_package(
    &archive_path,
    staging.directory(),
    &config,
    &release,
    &cancellation,
  )?;
  validate_extracted_package(staging.directory(), &manifest)?;

  let published = match store.published_version(&release.version) {
    Ok(published) => {
      validate_published_manifest(&published, &config, &release)?;
      if !same_package_contents(
        staging.directory(),
        &published
          .directory_path()
          .map_err(|_| UpdateError::Storage)?,
      )? {
        return Err(UpdateError::VersionConflict);
      }
      published
    }
    Err(AppStorageError::MissingVersion(_)) => {
      staging
        .publish(&release.version)
        .map_err(|error| match error {
          AppStorageError::VersionAlreadyExists(_) => UpdateError::VersionConflict,
          _ => UpdateError::Storage,
        })?
    }
    Err(_) => return Err(UpdateError::Storage),
  };
  validate_published_manifest(&published, &config, &release)?;

  let mut state = store.state().map_err(|_| UpdateError::Storage)?;
  if state.active_version != current_version {
    return Err(UpdateError::VersionConflict);
  }
  if state
    .active_version
    .as_ref()
    .is_some_and(|active| release.version <= *active)
  {
    update_status(&inner, |status| status.state = AppUpdateState::UpToDate);
    return Ok(());
  }
  state.pending_version = Some(release.version.clone());
  store
    .write_state(&state)
    .map_err(|_| UpdateError::Storage)?;
  update_status(&inner, |status| {
    status.state = AppUpdateState::Staged;
    status.failure = None;
  });
  Ok(())
}

async fn download_package(
  inner: &Arc<UpdateInner>,
  downloader: &Downloader,
  release: &ReleaseMetadata,
  archive_path: &Path,
  checksum: Checksum,
  cancellation: &CancellationToken,
) -> Result<(), UpdateError> {
  let request = DownloadRequest::new(archive_path)
    .mirrors([release.url.as_str()])
    .expected_size(release.size)
    .checksum(checksum);
  let part_path = sidecar_path(archive_path, ".q2q-http.part");
  let journal_path = sidecar_path(archive_path, ".q2q-http.resume");
  let resume = archive_path.exists() || part_path.exists() || journal_path.exists();
  let task = tokio::select! {
    _ = cancellation.cancelled() => return Err(UpdateError::Cancelled),
    task = async {
      if resume { downloader.resume(request).await } else { downloader.download(request).await }
    } => task.map_err(|_| UpdateError::Download)?,
  };
  await_download(inner, task, cancellation).await
}

async fn await_download(
  inner: &Arc<UpdateInner>,
  task: Task,
  cancellation: &CancellationToken,
) -> Result<(), UpdateError> {
  let mut progress = task.subscribe_progress();
  loop {
    let current = *progress.borrow_and_update();
    update_status(inner, |status| {
      status.downloaded_bytes = current.staged_bytes;
      status.total_bytes = Some(current.total_bytes);
    });
    if current.state.is_terminal() {
      break;
    }
    tokio::select! {
      _ = cancellation.cancelled() => {
        task.cancel();
        let _ = task.wait().await;
        return Err(UpdateError::Cancelled);
      }
      changed = progress.changed() => {
        if changed.is_err() { break; }
      }
    }
  }
  task.wait().await.map_err(|_| UpdateError::Download)?;
  Ok(())
}

fn validate_download_paths(archive_path: &Path) -> Result<(), UpdateError> {
  for path in [
    archive_path.to_path_buf(),
    sidecar_path(archive_path, ".q2q-http.part"),
    sidecar_path(archive_path, ".q2q-http.resume"),
  ] {
    match fs::symlink_metadata(&path) {
      Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
        return Err(UpdateError::Storage);
      }
      Ok(_) => {}
      Err(error) if error.kind() == io::ErrorKind::NotFound => {}
      Err(_) => return Err(UpdateError::Storage),
    }
  }
  Ok(())
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
  let mut value = path.as_os_str().to_owned();
  value.push(suffix);
  PathBuf::from(value)
}

fn extract_package(
  archive_path: &Path,
  package_root: &Path,
  config: &UpdateConfig,
  release: &ReleaseMetadata,
  cancellation: &CancellationToken,
) -> Result<AppManifest, UpdateError> {
  let compressed_size = fs::metadata(archive_path)
    .map_err(|_| UpdateError::Download)?
    .len();
  if compressed_size == 0 || compressed_size > MAX_PACKAGE_ARCHIVE_BYTES {
    return Err(UpdateError::InvalidPackage);
  }

  let decoder = GzDecoder::new(File::open(archive_path).map_err(|_| UpdateError::Download)?);
  let limited_decoder = decoder.take(MAX_PACKAGE_BYTES + 64 * 1024 * 1024);
  let mut archive = tar::Archive::new(limited_decoder);
  let entries = archive.entries().map_err(|_| UpdateError::InvalidPackage)?;
  let mut seen = HashSet::new();
  let mut file_count = 0usize;
  let mut entry_count = 0usize;
  let mut total_bytes = 0u64;

  for entry in entries {
    if cancellation.is_cancelled() {
      return Err(UpdateError::Cancelled);
    }
    entry_count += 1;
    if entry_count > MAX_ARCHIVE_ENTRIES {
      return Err(UpdateError::InvalidPackage);
    }
    let mut entry = entry.map_err(|_| UpdateError::InvalidPackage)?;
    let entry_type = entry.header().entry_type();
    let archive_path = entry.path().map_err(|_| UpdateError::InvalidPackage)?;
    let (relative, key) =
      normalize_archive_path(&archive_path, entry_type == EntryType::Directory)?;
    if !seen.insert(key) {
      return Err(UpdateError::InvalidPackage);
    }
    let destination = package_root.join(relative);
    let declared_size = entry
      .header()
      .size()
      .map_err(|_| UpdateError::InvalidPackage)?;

    if entry_type == EntryType::Directory {
      if declared_size != 0 {
        return Err(UpdateError::InvalidPackage);
      }
      fs::create_dir_all(&destination).map_err(|_| UpdateError::InvalidPackage)?;
      continue;
    }
    if !entry_type.is_file() {
      return Err(UpdateError::InvalidPackage);
    }
    file_count += 1;
    total_bytes = total_bytes
      .checked_add(declared_size)
      .ok_or(UpdateError::InvalidPackage)?;
    if file_count > MAX_PACKAGE_FILES || total_bytes > MAX_PACKAGE_BYTES {
      return Err(UpdateError::InvalidPackage);
    }
    let parent = destination.parent().ok_or(UpdateError::InvalidPackage)?;
    fs::create_dir_all(parent).map_err(|_| UpdateError::InvalidPackage)?;
    let mut output = OpenOptions::new()
      .write(true)
      .create_new(true)
      .open(&destination)
      .map_err(|_| UpdateError::InvalidPackage)?;
    let mut limited = (&mut entry).take(declared_size.saturating_add(1));
    let copied = io::copy(&mut limited, &mut output).map_err(|_| UpdateError::InvalidPackage)?;
    if copied != declared_size {
      return Err(UpdateError::InvalidPackage);
    }
    output.flush().map_err(|_| UpdateError::InvalidPackage)?;
    set_archive_file_permissions(&destination, entry.header().mode().unwrap_or(0))
      .map_err(|_| UpdateError::InvalidPackage)?;
  }

  let source = read_bounded_file(&package_root.join("manifest.json"), 64 * 1024)?;
  let manifest = AppManifest::parse_and_validate(
    &source,
    &config.app_id,
    &config.target,
    &config.shell_api_version,
  )
  .map_err(|_| UpdateError::InvalidPackage)?;
  if manifest.version != release.version.to_string() {
    return Err(UpdateError::InvalidPackage);
  }
  Ok(manifest)
}

fn normalize_archive_path(
  path: &Path,
  is_directory: bool,
) -> Result<(PathBuf, String), UpdateError> {
  let raw = path.to_str().ok_or(UpdateError::InvalidPackage)?;
  let raw = if is_directory {
    raw.strip_suffix('/').unwrap_or(raw)
  } else {
    raw
  };
  if raw.is_empty()
    || raw.starts_with('/')
    || raw.contains('\\')
    || raw.contains(':')
    || raw.contains('\0')
  {
    return Err(UpdateError::InvalidPackage);
  }
  let mut relative = PathBuf::new();
  let mut key = String::new();
  for component in raw.split('/') {
    if component.is_empty() || component == "." || component == ".." {
      return Err(UpdateError::InvalidPackage);
    }
    if component.ends_with(['.', ' '])
      || component
        .chars()
        .any(|character| matches!(character, '<' | '>' | '"' | '|' | '?' | '*'))
      || is_windows_reserved_name(component)
    {
      return Err(UpdateError::InvalidPackage);
    }
    relative.push(component);
    if !key.is_empty() {
      key.push('/');
    }
    key.push_str(&component.to_ascii_lowercase());
  }
  Ok((relative, key))
}

fn is_windows_reserved_name(component: &str) -> bool {
  let stem = component
    .split('.')
    .next()
    .unwrap_or_default()
    .to_ascii_uppercase();
  matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
    || ["COM", "LPT"].iter().any(|prefix| {
      stem
        .strip_prefix(prefix)
        .is_some_and(|suffix| matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
    })
}

fn validate_extracted_package(root: &Path, manifest: &AppManifest) -> Result<(), UpdateError> {
  let page = resolve_regular_file(root, &manifest.page_entry)?;
  let sidecar = resolve_regular_file(root, &manifest.sidecar_entry)?;
  if fs::metadata(page)
    .map_err(|_| UpdateError::InvalidPackage)?
    .len()
    > MAX_PACKAGE_BYTES
  {
    return Err(UpdateError::InvalidPackage);
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    if fs::metadata(sidecar)
      .map_err(|_| UpdateError::InvalidPackage)?
      .permissions()
      .mode()
      & 0o111
      == 0
    {
      return Err(UpdateError::InvalidPackage);
    }
  }
  Ok(())
}

fn resolve_regular_file(root: &Path, relative: &str) -> Result<PathBuf, UpdateError> {
  let mut candidate = root.to_path_buf();
  for component in relative.split('/') {
    if component.is_empty() || component == "." || component == ".." || component.contains('\\') {
      return Err(UpdateError::InvalidPackage);
    }
    candidate.push(component);
    let metadata = fs::symlink_metadata(&candidate).map_err(|_| UpdateError::InvalidPackage)?;
    if metadata.file_type().is_symlink() {
      return Err(UpdateError::InvalidPackage);
    }
  }
  let metadata = fs::symlink_metadata(&candidate).map_err(|_| UpdateError::InvalidPackage)?;
  if !metadata.is_file() || !candidate.starts_with(root) {
    return Err(UpdateError::InvalidPackage);
  }
  Ok(candidate)
}

fn validate_published_manifest(
  package: &PublishedAppVersion,
  config: &UpdateConfig,
  release: &ReleaseMetadata,
) -> Result<(), UpdateError> {
  let mut source = Vec::new();
  package
    .open_entry("manifest.json")
    .map_err(|_| UpdateError::InvalidPackage)?
    .take(64 * 1024 + 1)
    .read_to_end(&mut source)
    .map_err(|_| UpdateError::InvalidPackage)?;
  if source.len() > 64 * 1024 {
    return Err(UpdateError::InvalidPackage);
  }
  let manifest = AppManifest::parse_and_validate(
    &source,
    &config.app_id,
    &config.target,
    &config.shell_api_version,
  )
  .map_err(|_| UpdateError::InvalidPackage)?;
  if manifest.version != release.version.to_string() || package.version() != &release.version {
    return Err(UpdateError::InvalidPackage);
  }
  package
    .open_entry(&manifest.page_entry)
    .map_err(|_| UpdateError::InvalidPackage)?;
  package
    .open_entry(&manifest.sidecar_entry)
    .map_err(|_| UpdateError::InvalidPackage)?;
  validate_extracted_package(
    &package
      .directory_path()
      .map_err(|_| UpdateError::InvalidPackage)?,
    &manifest,
  )?;
  Ok(())
}

fn same_package_contents(first: &Path, second: &Path) -> Result<bool, UpdateError> {
  Ok(package_fingerprints(first)? == package_fingerprints(second)?)
}

fn package_fingerprints(root: &Path) -> Result<PackageFingerprints, UpdateError> {
  let mut pending = vec![root.to_path_buf()];
  let mut files = HashMap::new();
  let mut total_bytes = 0u64;
  while let Some(directory) = pending.pop() {
    for entry in fs::read_dir(&directory).map_err(|_| UpdateError::InvalidPackage)? {
      let entry = entry.map_err(|_| UpdateError::InvalidPackage)?;
      let path = entry.path();
      let metadata = fs::symlink_metadata(&path).map_err(|_| UpdateError::InvalidPackage)?;
      if metadata.file_type().is_symlink() {
        return Err(UpdateError::InvalidPackage);
      }
      if metadata.is_dir() {
        pending.push(path);
      } else if metadata.is_file() {
        total_bytes = total_bytes
          .checked_add(metadata.len())
          .ok_or(UpdateError::InvalidPackage)?;
        if files.len() >= MAX_PACKAGE_FILES || total_bytes > MAX_PACKAGE_BYTES {
          return Err(UpdateError::InvalidPackage);
        }
        let relative = path
          .strip_prefix(root)
          .map_err(|_| UpdateError::InvalidPackage)?
          .to_str()
          .ok_or(UpdateError::InvalidPackage)?
          .replace('\\', "/");
        if files
          .insert(relative, fingerprint(&path, &metadata)?)
          .is_some()
        {
          return Err(UpdateError::InvalidPackage);
        }
      } else {
        return Err(UpdateError::InvalidPackage);
      }
    }
  }
  Ok(files)
}

fn fingerprint(path: &Path, metadata: &fs::Metadata) -> Result<(u64, [u8; 32], bool), UpdateError> {
  let mut file = File::open(path).map_err(|_| UpdateError::InvalidPackage)?;
  let mut context = digest::Context::new(&digest::SHA256);
  let mut buffer = [0; 64 * 1024];
  loop {
    let count = file
      .read(&mut buffer)
      .map_err(|_| UpdateError::InvalidPackage)?;
    if count == 0 {
      break;
    }
    context.update(&buffer[..count]);
  }
  let hash = context.finish();
  let mut digest_bytes = [0; 32];
  digest_bytes.copy_from_slice(hash.as_ref());
  #[cfg(unix)]
  let executable = {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
  };
  #[cfg(not(unix))]
  let executable = false;
  Ok((metadata.len(), digest_bytes, executable))
}

fn read_bounded_file(path: &Path, maximum: u64) -> Result<Vec<u8>, UpdateError> {
  let metadata = fs::symlink_metadata(path).map_err(|_| UpdateError::InvalidPackage)?;
  if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > maximum {
    return Err(UpdateError::InvalidPackage);
  }
  fs::read(path).map_err(|_| UpdateError::InvalidPackage)
}

#[cfg(unix)]
fn set_archive_file_permissions(path: &Path, mode: u32) -> io::Result<()> {
  use std::os::unix::fs::PermissionsExt;
  let sanitized = if mode & 0o111 != 0 { 0o755 } else { 0o644 };
  fs::set_permissions(path, fs::Permissions::from_mode(sanitized))
}

#[cfg(not(unix))]
fn set_archive_file_permissions(_path: &Path, _mode: u32) -> io::Result<()> {
  Ok(())
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], UpdateError> {
  if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    return Err(UpdateError::InvalidRelease);
  }
  let mut result = [0; N];
  for (index, byte) in result.iter_mut().enumerate() {
    *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
      .map_err(|_| UpdateError::InvalidRelease)?;
  }
  Ok(result)
}

fn decode_variable_hex(value: &str) -> Result<Vec<u8>, UpdateError> {
  if value.is_empty()
    || !value.len().is_multiple_of(2)
    || value.len() > MAX_UPDATE_MANIFEST_BYTES * 2
    || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
  {
    return Err(UpdateError::InvalidRelease);
  }
  (0..value.len())
    .step_by(2)
    .map(|index| {
      u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| UpdateError::InvalidRelease)
    })
    .collect()
}

fn update_status(inner: &UpdateInner, update: impl FnOnce(&mut AppUpdateStatus)) {
  if let Ok(mut status) = inner.status.lock() {
    let previous_state = status.state;
    let previous_failure = status.failure;
    update(&mut status);
    if status.state != previous_state || status.failure != previous_failure {
      if status.failure.is_some() && status.state != AppUpdateState::Cancelled {
        tracing::warn!(state = ?status.state, failure = ?status.failure, "App update state changed");
      } else {
        tracing::info!(state = ?status.state, "App update state changed");
      }
    }
  }
}

fn set_failure(inner: &UpdateInner, error: UpdateError) {
  update_status(inner, |status| {
    status.state = if matches!(error, UpdateError::Cancelled) {
      AppUpdateState::Cancelled
    } else {
      AppUpdateState::Failed
    };
    status.failure = Some(match error {
      UpdateError::Storage => AppUpdateFailure::StorageUnavailable,
      UpdateError::Network => AppUpdateFailure::Network,
      UpdateError::InvalidSignature => AppUpdateFailure::InvalidSignature,
      UpdateError::InvalidRelease => AppUpdateFailure::InvalidRelease,
      UpdateError::Download => AppUpdateFailure::Download,
      UpdateError::InvalidPackage => AppUpdateFailure::InvalidPackage,
      UpdateError::VersionConflict => AppUpdateFailure::VersionConflict,
      UpdateError::Cancelled => AppUpdateFailure::Cancelled,
    });
  });
}

#[derive(Debug, Error)]
enum UpdateError {
  #[error("update storage failed")]
  Storage,
  #[error("update source could not be reached")]
  Network,
  #[error("update signature is invalid")]
  InvalidSignature,
  #[error("update manifest is invalid")]
  InvalidRelease,
  #[error("update package download failed")]
  Download,
  #[error("update package is invalid")]
  InvalidPackage,
  #[error("published version conflicts with downloaded package")]
  VersionConflict,
  #[error("update was cancelled")]
  Cancelled,
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app_storage::AppVersionState;
  use flate2::{Compression, write::GzEncoder};
  use ring::signature::KeyPair;
  use std::{io::Cursor, net::TcpListener, thread, time::Instant};
  use tar::{Builder, EntryType, Header};

  const TEST_SEED: [u8; 32] = [7; 32];

  fn signed_envelope(payload: &[u8]) -> (Vec<u8>, [u8; 32]) {
    let key_pair = signature::Ed25519KeyPair::from_seed_unchecked(&TEST_SEED).unwrap();
    let signature = key_pair.sign(payload);
    let envelope = serde_json::json!({
      "payload": payload.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
      "signature": signature.as_ref().iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    });
    let mut public_key = [0; 32];
    public_key.copy_from_slice(key_pair.public_key().as_ref());
    (serde_json::to_vec(&envelope).unwrap(), public_key)
  }

  fn release_payload() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
      "schema_version": 1,
      "channel": "stable",
      "app_id": "anvil.desktop",
      "version": "0.2.0",
      "target": env!("ANVIL_TARGET_PLATFORM"),
      "package_url": "https://updates.example.test/anvil-desktop-0.2.0.tar.gz",
      "package_size": 100,
      "package_sha256": "ab".repeat(32)
    }))
    .unwrap()
  }

  fn test_config() -> UpdateConfig {
    UpdateConfig {
      manifest_url: "https://updates.example.test/stable.json".into(),
      public_key: [0; 32],
      app_id: "anvil.desktop".into(),
      target: env!("ANVIL_TARGET_PLATFORM").into(),
      shell_api_version: env!("ANVIL_SHELL_API_VERSION").into(),
    }
  }

  fn sidecar_entry() -> &'static str {
    if env!("ANVIL_TARGET_PLATFORM").contains("windows") {
      "runtime/anvil-desktop.exe"
    } else {
      "runtime/anvil-desktop"
    }
  }

  fn append_file(builder: &mut Builder<GzEncoder<File>>, path: &str, body: &[u8], mode: u32) {
    let mut header = Header::new_gnu();
    header.set_size(body.len() as u64);
    header.set_mode(mode);
    header.set_cksum();
    builder
      .append_data(&mut header, path, Cursor::new(body))
      .unwrap();
  }

  fn append_directory(builder: &mut Builder<GzEncoder<File>>, path: &str) {
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Directory);
    header.set_size(0);
    header.set_mode(0o755);
    header.set_cksum();
    builder
      .append_data(&mut header, path, Cursor::new([]))
      .unwrap();
  }

  fn package_archive(path: &Path, version: &str) {
    let encoder = GzEncoder::new(File::create(path).unwrap(), Compression::default());
    let mut builder = Builder::new(encoder);
    let manifest = serde_json::json!({
      "schema_version": 2,
      "app_id": "anvil.desktop",
      "version": version,
      "target": env!("ANVIL_TARGET_PLATFORM"),
      "page_entry": "web/index.html",
      "sidecar_entry": sidecar_entry(),
      "http_api_version": "1.0.0",
      "http_api_compatibility": "^1.0.0",
      "shell_api_compatibility": "^1.0.0"
    });
    append_directory(&mut builder, "web/");
    append_directory(&mut builder, "runtime/");
    append_file(
      &mut builder,
      "manifest.json",
      serde_json::to_string(&manifest).unwrap().as_bytes(),
      0o644,
    );
    append_file(&mut builder, "web/index.html", b"updated page", 0o644);
    append_file(&mut builder, sidecar_entry(), b"updated sidecar", 0o755);
    builder.into_inner().unwrap().finish().unwrap();
  }

  fn active_store(root: &Path) -> AppVersionStore {
    let storage = AppStorage::open(root.join("data")).unwrap();
    let store = storage.app("anvil.desktop").unwrap();
    let staging = store.stage_version().unwrap();
    fs::write(staging.directory().join("baseline"), b"active baseline").unwrap();
    staging.publish(&Version::parse("0.1.0").unwrap()).unwrap();
    store
      .write_state(&AppVersionState {
        active_version: Some(Version::parse("0.1.0").unwrap()),
        pending_version: None,
        previous_healthy_version: None,
        activation_in_progress: false,
        failed_version: None,
        activation_failure: None,
      })
      .unwrap();
    store
  }

  fn serve_signed_package(
    package: Vec<u8>,
    advertised_digest: Option<String>,
  ) -> (String, [u8; 32], thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let digest = digest::digest(&digest::SHA256, &package);
    let correct_hash = digest
      .as_ref()
      .iter()
      .map(|byte| format!("{byte:02x}"))
      .collect::<String>();
    let key_pair = signature::Ed25519KeyPair::from_seed_unchecked(&TEST_SEED).unwrap();
    let payload = serde_json::to_vec(&serde_json::json!({
      "schema_version": 1,
      "channel": "stable",
      "app_id": "anvil.desktop",
      "version": "0.2.0",
      "target": env!("ANVIL_TARGET_PLATFORM"),
      "package_url": format!("http://{address}/package"),
      "package_size": package.len(),
      "package_sha256": advertised_digest.unwrap_or(correct_hash)
    }))
    .unwrap();
    let signature = key_pair.sign(&payload);
    let envelope = serde_json::to_vec(&serde_json::json!({
      "payload": payload.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
      "signature": signature.as_ref().iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    }))
    .unwrap();
    let mut public_key = [0; 32];
    public_key.copy_from_slice(key_pair.public_key().as_ref());

    let server = thread::spawn(move || {
      let mut last_request = None;
      loop {
        match listener.accept() {
          Ok((mut stream, _)) => {
            last_request = Some(Instant::now());
            stream
              .set_read_timeout(Some(Duration::from_secs(2)))
              .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
              let count = stream.read(&mut buffer).unwrap_or(0);
              if count == 0 {
                break;
              }
              request.extend_from_slice(&buffer[..count]);
              if request.len() > 16 * 1024 {
                break;
              }
            }
            let request_line = String::from_utf8_lossy(&request);
            let (status, body) = if request_line.starts_with("GET /latest ") {
              ("200 OK", envelope.as_slice())
            } else if request_line.starts_with("GET /package ") {
              ("200 OK", package.as_slice())
            } else {
              ("404 Not Found", &[][..])
            };
            let header = format!(
              "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
              body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(body);
            let _ = stream.flush();
          }
          Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
            if last_request.is_some_and(|last| last.elapsed() > Duration::from_millis(300)) {
              break;
            }
            thread::sleep(Duration::from_millis(10));
          }
          Err(_) => break,
        }
      }
    });
    (format!("http://{address}/latest"), public_key, server)
  }

  fn run_async<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
      .unwrap()
      .block_on(future)
  }

  #[test]
  fn accepts_a_signed_release_for_the_configured_app_and_target() {
    let payload = release_payload();
    let (envelope, key) = signed_envelope(&payload);
    let release = parse_signed_release(
      &envelope,
      &key,
      "anvil.desktop",
      env!("ANVIL_TARGET_PLATFORM"),
    )
    .unwrap();
    assert_eq!(release.version, Version::parse("0.2.0").unwrap());
    assert_eq!(release.size, 100);
  }

  #[test]
  fn rejects_tampered_or_wrong_key_release_signatures() {
    let payload = release_payload();
    let (mut envelope, key) = signed_envelope(&payload);
    envelope[10] ^= 1;
    assert!(
      parse_signed_release(
        &envelope,
        &key,
        "anvil.desktop",
        env!("ANVIL_TARGET_PLATFORM")
      )
      .is_err()
    );

    let (envelope, _) = signed_envelope(&payload);
    assert!(
      parse_signed_release(
        &envelope,
        &[3; 32],
        "anvil.desktop",
        env!("ANVIL_TARGET_PLATFORM")
      )
      .is_err()
    );
  }

  #[test]
  fn rejects_wrong_app_target_channel_and_untrusted_urls() {
    for (needle, replacement) in [
      ("anvil.desktop", "anvil.other"),
      (env!("ANVIL_TARGET_PLATFORM"), "x86_64-unknown-linux-gnu"),
      ("stable", "beta"),
      (
        "https://updates.example.test",
        "http://updates.example.test",
      ),
    ] {
      let payload = String::from_utf8(release_payload())
        .unwrap()
        .replace(needle, replacement);
      let (envelope, key) = signed_envelope(payload.as_bytes());
      assert!(
        parse_signed_release(
          &envelope,
          &key,
          "anvil.desktop",
          env!("ANVIL_TARGET_PLATFORM")
        )
        .is_err()
      );
    }
  }

  #[test]
  fn rejects_archive_paths_that_escape_or_collide_across_platforms() {
    for path in [
      "../outside",
      "/absolute",
      "folder\\escape",
      "C:/outside",
      "NUL.txt",
      "a/../b",
    ] {
      assert!(
        normalize_archive_path(Path::new(path), false).is_err(),
        "{path}"
      );
    }
    let (_, first) = normalize_archive_path(Path::new("Web/index.html"), false).unwrap();
    let (_, second) = normalize_archive_path(Path::new("web/index.html"), false).unwrap();
    assert_eq!(first, second);
  }

  #[test]
  fn rejects_signed_release_with_unsupported_size_or_noncanonical_version() {
    for (needle, replacement) in [
      ("\"package_size\":100", "\"package_size\":0"),
      ("\"version\":\"0.2.0\"", "\"version\":\"v0.2.0\""),
    ] {
      let payload = String::from_utf8(release_payload())
        .unwrap()
        .replace(needle, replacement);
      let (envelope, key) = signed_envelope(payload.as_bytes());
      assert!(
        parse_signed_release(
          &envelope,
          &key,
          "anvil.desktop",
          env!("ANVIL_TARGET_PLATFORM")
        )
        .is_err()
      );
    }
  }

  #[test]
  fn extracts_a_compatible_signed_package_and_preserves_sidecar_execution() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("package.tar.gz");
    let package = root.path().join("extracted");
    fs::create_dir(&package).unwrap();
    package_archive(&archive, "0.2.0");
    let release = ReleaseMetadata {
      version: Version::parse("0.2.0").unwrap(),
      url: "https://updates.example.test/package.tar.gz".into(),
      size: fs::metadata(&archive).unwrap().len(),
      sha256: "ab".repeat(32),
    };

    let manifest = extract_package(
      &archive,
      &package,
      &test_config(),
      &release,
      &CancellationToken::new(),
    )
    .unwrap();
    validate_extracted_package(&package, &manifest).unwrap();
    assert_eq!(
      fs::read(package.join("web/index.html")).unwrap(),
      b"updated page"
    );
  }

  #[test]
  fn rejects_an_archive_whose_manifest_version_differs_from_the_signed_release() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("package.tar.gz");
    let package = root.path().join("extracted");
    fs::create_dir(&package).unwrap();
    package_archive(&archive, "0.3.0");
    let release = ReleaseMetadata {
      version: Version::parse("0.2.0").unwrap(),
      url: "https://updates.example.test/package.tar.gz".into(),
      size: fs::metadata(&archive).unwrap().len(),
      sha256: "ab".repeat(32),
    };

    assert!(matches!(
      extract_package(
        &archive,
        &package,
        &test_config(),
        &release,
        &CancellationToken::new(),
      ),
      Err(UpdateError::InvalidPackage)
    ));
  }

  #[test]
  fn downloads_verifies_publishes_and_marks_a_new_version_pending() {
    let root = tempfile::tempdir().unwrap();
    let store = active_store(root.path());
    let archive_path = root.path().join("package.tar.gz");
    package_archive(&archive_path, "0.2.0");
    let (manifest_url, public_key, server) =
      serve_signed_package(fs::read(archive_path).unwrap(), None);
    let inner = Arc::new(UpdateInner {
      config: None,
      status: Mutex::new(AppUpdateStatus {
        state: AppUpdateState::Checking,
        failure: None,
        current_version: Some("0.1.0".into()),
        available_version: None,
        downloaded_bytes: 0,
        total_bytes: None,
      }),
      cancellation: Mutex::new(None),
    });
    let config = UpdateConfig {
      manifest_url,
      public_key,
      app_id: "anvil.desktop".into(),
      target: env!("ANVIL_TARGET_PLATFORM").into(),
      shell_api_version: env!("ANVIL_SHELL_API_VERSION").into(),
    };

    run_async(run_update_check(
      inner.clone(),
      config,
      store.clone(),
      CancellationToken::new(),
    ))
    .unwrap();
    server.join().unwrap();

    assert_eq!(
      store.state().unwrap().active_version,
      Some(Version::parse("0.1.0").unwrap())
    );
    assert_eq!(
      store.state().unwrap().pending_version,
      Some(Version::parse("0.2.0").unwrap())
    );
    assert!(
      store
        .published_version(&Version::parse("0.2.0").unwrap())
        .is_ok()
    );
    assert_eq!(inner.status.lock().unwrap().state, AppUpdateState::Staged);
  }

  #[test]
  fn does_not_redownload_a_version_that_failed_startup_activation() {
    let root = tempfile::tempdir().unwrap();
    let store = active_store(root.path());
    let failed_staging = store.stage_version().unwrap();
    fs::write(failed_staging.directory().join("failed-update"), b"failed").unwrap();
    failed_staging
      .publish(&Version::parse("0.2.0").unwrap())
      .unwrap();
    let mut state = store.state().unwrap();
    state.failed_version = Some(Version::parse("0.2.0").unwrap());
    state.activation_failure = Some(crate::app_storage::ActivationFailureReason::RuntimeStartup);
    store.write_state(&state).unwrap();

    let archive_path = root.path().join("package.tar.gz");
    package_archive(&archive_path, "0.2.0");
    let (manifest_url, public_key, server) =
      serve_signed_package(fs::read(archive_path).unwrap(), None);
    let inner = Arc::new(UpdateInner {
      config: None,
      status: Mutex::new(AppUpdateStatus {
        state: AppUpdateState::Checking,
        failure: None,
        current_version: Some("0.1.0".into()),
        available_version: None,
        downloaded_bytes: 0,
        total_bytes: None,
      }),
      cancellation: Mutex::new(None),
    });
    let config = UpdateConfig {
      manifest_url,
      public_key,
      app_id: "anvil.desktop".into(),
      target: env!("ANVIL_TARGET_PLATFORM").into(),
      shell_api_version: env!("ANVIL_SHELL_API_VERSION").into(),
    };

    run_async(run_update_check(
      inner.clone(),
      config,
      store.clone(),
      CancellationToken::new(),
    ))
    .unwrap();
    server.join().unwrap();

    assert_eq!(store.state().unwrap().pending_version, None);
    assert!(!store.app_dir().join("staging/update-0.2.0").exists());
    let status = inner.status.lock().unwrap();
    assert_eq!(status.state, AppUpdateState::Failed);
    assert_eq!(status.failure, Some(AppUpdateFailure::ActivationFailed));
  }

  #[test]
  fn rejects_a_package_hash_mismatch_without_changing_the_active_version() {
    let root = tempfile::tempdir().unwrap();
    let store = active_store(root.path());
    let archive_path = root.path().join("package.tar.gz");
    package_archive(&archive_path, "0.2.0");
    let (manifest_url, public_key, server) =
      serve_signed_package(fs::read(archive_path).unwrap(), Some("00".repeat(32)));
    let inner = Arc::new(UpdateInner {
      config: None,
      status: Mutex::new(AppUpdateStatus {
        state: AppUpdateState::Checking,
        failure: None,
        current_version: Some("0.1.0".into()),
        available_version: None,
        downloaded_bytes: 0,
        total_bytes: None,
      }),
      cancellation: Mutex::new(None),
    });
    let config = UpdateConfig {
      manifest_url,
      public_key,
      app_id: "anvil.desktop".into(),
      target: env!("ANVIL_TARGET_PLATFORM").into(),
      shell_api_version: env!("ANVIL_SHELL_API_VERSION").into(),
    };

    let result = run_async(run_update_check(
      inner,
      config,
      store.clone(),
      CancellationToken::new(),
    ));
    server.join().unwrap();

    assert!(result.is_err());
    assert_eq!(
      store.state().unwrap().active_version,
      Some(Version::parse("0.1.0").unwrap())
    );
    assert_eq!(store.state().unwrap().pending_version, None);
    assert!(
      store
        .published_version(&Version::parse("0.2.0").unwrap())
        .is_err()
    );
  }

  #[test]
  fn cancellation_before_the_manifest_request_keeps_the_active_version() {
    let root = tempfile::tempdir().unwrap();
    let store = active_store(root.path());
    let inner = Arc::new(UpdateInner {
      config: None,
      status: Mutex::new(AppUpdateStatus {
        state: AppUpdateState::Checking,
        failure: None,
        current_version: Some("0.1.0".into()),
        available_version: None,
        downloaded_bytes: 0,
        total_bytes: None,
      }),
      cancellation: Mutex::new(None),
    });
    let config = UpdateConfig {
      manifest_url: "https://updates.example.test/stable.json".into(),
      public_key: [0; 32],
      app_id: "anvil.desktop".into(),
      target: env!("ANVIL_TARGET_PLATFORM").into(),
      shell_api_version: env!("ANVIL_SHELL_API_VERSION").into(),
    };
    let cancellation = CancellationToken::new();
    cancellation.cancel();

    let result = run_async(run_update_check(inner, config, store.clone(), cancellation));

    assert!(matches!(result, Err(UpdateError::Cancelled)));
    assert_eq!(
      store.state().unwrap().active_version,
      Some(Version::parse("0.1.0").unwrap())
    );
    assert_eq!(store.state().unwrap().pending_version, None);
  }
}
