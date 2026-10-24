use crate::app_manifest::validate_app_id;
use directories::ProjectDirs;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::{
  fs,
  path::{Component, Path, PathBuf},
  time::{Duration, SystemTime, UNIX_EPOCH},
};
use tempfile::{Builder, NamedTempFile, TempDir};
use thiserror::Error;

const STATE_SCHEMA_VERSION: u32 = 2;
const LEGACY_STATE_SCHEMA_VERSION: u32 = 1;
const UPDATE_CHECK_SCHEMA_VERSION: u32 = 1;
const MAX_STATE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Error)]
pub enum AppStorageError {
  #[error("invalid App ID: {0}")]
  InvalidAppId(String),
  #[error("invalid App version {0:?}")]
  InvalidVersion(String),
  #[error("invalid package entry path: {0:?}")]
  InvalidEntry(String),
  #[error("App storage path is not a directory: {0}")]
  NotDirectory(PathBuf),
  #[error("App storage path contains a symbolic link: {0}")]
  SymbolicLink(PathBuf),
  #[error("App state is invalid: {0}")]
  InvalidState(String),
  #[error("App state references missing version {0}")]
  MissingVersion(Version),
  #[error("App version already exists and is immutable: {0}")]
  VersionAlreadyExists(Version),
  #[error("App package entry does not exist: {0}")]
  EntryNotFound(PathBuf),
  #[error("App package entry is not a regular file: {0}")]
  NotRegularFile(PathBuf),
  #[error("App package contains an unsupported filesystem entry: {0}")]
  UnsupportedEntry(PathBuf),
  #[error("App data directory is unavailable")]
  DataDirectoryUnavailable,
  #[error("ANVIL_SHELL_DATA_DIR must be an absolute path")]
  InvalidDataDirectory,
  #[error(transparent)]
  Io(#[from] std::io::Error),
  #[error(transparent)]
  Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct AppStorage {
  data_dir: PathBuf,
}

impl AppStorage {
  /// Open an explicit App data directory. The root is created if it does not exist.
  pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, AppStorageError> {
    let data_dir = data_dir.as_ref();
    fs::create_dir_all(data_dir)?;
    let metadata = fs::symlink_metadata(data_dir)?;
    if metadata.file_type().is_symlink() {
      return Err(AppStorageError::SymbolicLink(data_dir.to_path_buf()));
    }
    if !metadata.is_dir() {
      return Err(AppStorageError::NotDirectory(data_dir.to_path_buf()));
    }

    Ok(Self {
      data_dir: fs::canonicalize(data_dir)?,
    })
  }

  /// Open the platform-specific Anvil data directory.
  pub fn for_current_user() -> Result<Self, AppStorageError> {
    if let Some(path) = std::env::var_os("ANVIL_SHELL_DATA_DIR") {
      let path = PathBuf::from(path);
      if !path.is_absolute() {
        return Err(AppStorageError::InvalidDataDirectory);
      }
      return Self::open(path);
    }

    let project_dirs = ProjectDirs::from("com", "mivinci", "Anvil")
      .ok_or(AppStorageError::DataDirectoryUnavailable)?;
    Self::open(project_dirs.data_local_dir())
  }

  pub fn data_dir(&self) -> &Path {
    &self.data_dir
  }

  /// Open storage for a stable, validated App ID.
  pub fn app(&self, app_id: &str) -> Result<AppVersionStore, AppStorageError> {
    validate_app_id("app_id", app_id).map_err(AppStorageError::InvalidAppId)?;

    let apps_dir = self.data_dir.join("apps");
    ensure_directory(&self.data_dir, &apps_dir)?;
    let app_dir = apps_dir.join(app_id);
    ensure_directory(&self.data_dir, &app_dir)?;
    let app_dir = fs::canonicalize(app_dir)?;
    ensure_within(&self.data_dir, &app_dir)?;

    Ok(AppVersionStore {
      data_dir: self.data_dir.clone(),
      app_id: app_id.to_owned(),
      app_dir,
    })
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppVersionState {
  pub active_version: Option<Version>,
  pub pending_version: Option<Version>,
  pub previous_healthy_version: Option<Version>,
  /// True while the active version is waiting for its first successful page handshake.
  pub activation_in_progress: bool,
  /// A version that failed startup validation and must not be installed again automatically.
  pub failed_version: Option<Version>,
  pub activation_failure: Option<ActivationFailureReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationFailureReason {
  Interrupted,
  RuntimeStartup,
  PageHealth,
  PageTimeout,
}

#[derive(Debug, Clone)]
pub struct AppVersionStore {
  data_dir: PathBuf,
  app_id: String,
  app_dir: PathBuf,
}

impl AppVersionStore {
  pub fn app_id(&self) -> &str {
    &self.app_id
  }

  pub fn app_dir(&self) -> &Path {
    &self.app_dir
  }

  pub fn versions_dir(&self) -> PathBuf {
    self.app_dir.join("versions")
  }

  /// Return the timestamp of the most recent update check, if one was recorded.
  pub fn last_update_check(&self) -> Result<Option<SystemTime>, AppStorageError> {
    ensure_directory(&self.data_dir, &self.app_dir)?;
    let path = self.app_dir.join("update-check.json");
    let bytes = match read_regular_file(&path) {
      Ok(bytes) => bytes,
      Err(AppStorageError::EntryNotFound(_)) => return Ok(None),
      Err(error) => return Err(error),
    };
    let document: UpdateCheckDocument = serde_json::from_slice(&bytes)
      .map_err(|error| AppStorageError::InvalidState(error.to_string()))?;
    if document.schema_version != UPDATE_CHECK_SCHEMA_VERSION {
      return Err(AppStorageError::InvalidState(format!(
        "unsupported update-check schema version {}; expected {UPDATE_CHECK_SCHEMA_VERSION}",
        document.schema_version
      )));
    }
    UNIX_EPOCH
      .checked_add(Duration::from_secs(document.last_check_unix_seconds))
      .map(Some)
      .ok_or_else(|| AppStorageError::InvalidState("update-check timestamp is out of range".into()))
  }

  /// Persist the start of a check so restarts cannot bypass the 24-hour limit.
  pub fn record_update_check(&self, checked_at: SystemTime) -> Result<(), AppStorageError> {
    let last_check_unix_seconds = checked_at
      .duration_since(UNIX_EPOCH)
      .map_err(|_| AppStorageError::InvalidState("update-check time predates Unix epoch".into()))?
      .as_secs();
    let document = UpdateCheckDocument {
      schema_version: UPDATE_CHECK_SCHEMA_VERSION,
      last_check_unix_seconds,
    };
    let bytes = serde_json::to_vec(&document)?;
    atomic_replace(
      &self.data_dir,
      &self.app_dir.join("update-check.json"),
      &bytes,
    )?;
    Ok(())
  }

  /// Return a stable private directory for q2q-http's resumable package download.
  pub fn update_staging_dir(&self, version: &Version) -> Result<PathBuf, AppStorageError> {
    validate_version(version)?;
    ensure_directory(&self.data_dir, &self.app_dir)?;
    let staging_root = self.app_dir.join("staging");
    ensure_directory(&self.data_dir, &staging_root)?;
    let path = staging_root.join(format!("update-{version}"));
    ensure_directory(&self.data_dir, &path)?;
    Ok(path)
  }

  /// Remove a completed update download and q2q-http resume records.
  pub fn clear_update_staging(&self, version: &Version) -> Result<(), AppStorageError> {
    validate_version(version)?;
    let staging_root = self.app_dir.join("staging");
    ensure_directory(&self.data_dir, &staging_root)?;
    let path = staging_root.join(format!("update-{version}"));
    match fs::symlink_metadata(&path) {
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
      Err(error) => return Err(error.into()),
      Ok(metadata) if metadata.file_type().is_symlink() => {
        return Err(AppStorageError::SymbolicLink(path));
      }
      Ok(metadata) if !metadata.is_dir() => return Err(AppStorageError::NotDirectory(path)),
      Ok(_) => {}
    }
    let canonical = fs::canonicalize(&path)?;
    ensure_within(&staging_root, &canonical)?;
    fs::remove_dir_all(canonical)?;
    Ok(())
  }

  pub fn state(&self) -> Result<AppVersionState, AppStorageError> {
    ensure_directory(&self.data_dir, &self.app_dir)?;
    let path = self.state_path();
    let bytes = match read_regular_file(&path) {
      Ok(bytes) => bytes,
      Err(AppStorageError::EntryNotFound(_)) => return Ok(AppVersionState::default()),
      Err(error) => return Err(error),
    };

    if bytes.len() as u64 > MAX_STATE_BYTES {
      return Err(AppStorageError::InvalidState(format!(
        "state file exceeds {MAX_STATE_BYTES} bytes"
      )));
    }

    let document: StateDocument = serde_json::from_slice(&bytes)
      .map_err(|error| AppStorageError::InvalidState(error.to_string()))?;
    if document.schema_version != LEGACY_STATE_SCHEMA_VERSION
      && document.schema_version != STATE_SCHEMA_VERSION
    {
      return Err(AppStorageError::InvalidState(format!(
        "unsupported schema version {}; expected {LEGACY_STATE_SCHEMA_VERSION} or {STATE_SCHEMA_VERSION}",
        document.schema_version
      )));
    }
    if document.schema_version == LEGACY_STATE_SCHEMA_VERSION
      && (document.activation_in_progress
        || document.failed_version.is_some()
        || document.activation_failure.is_some())
    {
      return Err(AppStorageError::InvalidState(
        "legacy state contains fields from a newer schema".into(),
      ));
    }

    let state = AppVersionState {
      active_version: parse_version(document.active_version)?,
      pending_version: parse_version(document.pending_version)?,
      previous_healthy_version: parse_version(document.previous_healthy_version)?,
      activation_in_progress: document.activation_in_progress,
      failed_version: parse_version(document.failed_version)?,
      activation_failure: document.activation_failure,
    };
    self.validate_state_references(&state, true)?;
    Ok(state)
  }

  /// Replace the whole status document atomically, so activation state cannot be half-written.
  pub fn write_state(&self, state: &AppVersionState) -> Result<(), AppStorageError> {
    self.validate_state_references(state, false)?;
    let document = StateDocument {
      schema_version: STATE_SCHEMA_VERSION,
      active_version: version_string(&state.active_version),
      pending_version: version_string(&state.pending_version),
      previous_healthy_version: version_string(&state.previous_healthy_version),
      activation_in_progress: state.activation_in_progress,
      failed_version: version_string(&state.failed_version),
      activation_failure: state.activation_failure,
    };
    let bytes = serde_json::to_vec(&document)?;
    atomic_replace(&self.data_dir, &self.state_path(), &bytes)?;
    Ok(())
  }

  pub fn stage_version(&self) -> Result<AppVersionStaging, AppStorageError> {
    ensure_directory(&self.data_dir, &self.app_dir)?;
    let staging_dir = self.app_dir.join("staging");
    ensure_directory(&self.data_dir, &staging_dir)?;
    let temp_dir = Builder::new().prefix("version-").tempdir_in(staging_dir)?;
    Ok(AppVersionStaging {
      store: self.clone(),
      temp_dir,
    })
  }

  pub fn published_version(
    &self,
    version: &Version,
  ) -> Result<PublishedAppVersion, AppStorageError> {
    let path = self.version_path(version)?;
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
      if error.kind() == std::io::ErrorKind::NotFound {
        AppStorageError::MissingVersion(version.clone())
      } else {
        AppStorageError::Io(error)
      }
    })?;
    if metadata.file_type().is_symlink() {
      return Err(AppStorageError::SymbolicLink(path));
    }
    if !metadata.is_dir() {
      return Err(AppStorageError::NotDirectory(path));
    }
    let canonical_path = fs::canonicalize(path)?;
    ensure_within(&self.versions_dir(), &canonical_path)?;

    Ok(PublishedAppVersion {
      store: self.clone(),
      version: version.clone(),
    })
  }

  pub fn resolve_active(&self) -> Result<Option<PublishedAppVersion>, AppStorageError> {
    let state = self.state()?;
    state
      .active_version
      .map(|version| self.published_version(&version))
      .transpose()
  }

  fn state_path(&self) -> PathBuf {
    self.app_dir.join("state.json")
  }

  fn version_path(&self, version: &Version) -> Result<PathBuf, AppStorageError> {
    validate_version(version)?;
    let versions_dir = self.versions_dir();
    ensure_directory(&self.data_dir, &versions_dir)?;
    Ok(versions_dir.join(version.to_string()))
  }

  fn validate_state_references(
    &self,
    state: &AppVersionState,
    allow_missing_attempt_version: bool,
  ) -> Result<(), AppStorageError> {
    if state.active_version == state.pending_version && state.active_version.is_some() {
      return Err(AppStorageError::InvalidState(
        "active and pending versions must differ".into(),
      ));
    }
    if state.activation_in_progress && state.active_version.is_none() {
      return Err(AppStorageError::InvalidState(
        "an activation attempt must have an active version".into(),
      ));
    }
    if state.failed_version.is_some() != state.activation_failure.is_some() {
      return Err(AppStorageError::InvalidState(
        "failed version and activation failure must be recorded together".into(),
      ));
    }

    if let Some(version) = state.active_version.as_ref() {
      validate_version(version)?;
      if !(allow_missing_attempt_version && state.activation_in_progress) {
        self.published_version(version)?;
      }
    }
    for version in [
      state.pending_version.as_ref(),
      state.previous_healthy_version.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
      validate_version(version)?;
      self.published_version(version)?;
    }
    if let Some(version) = state.failed_version.as_ref() {
      validate_version(version)?;
    }
    Ok(())
  }
}

#[derive(Debug)]
pub struct AppVersionStaging {
  store: AppVersionStore,
  temp_dir: TempDir,
}

impl AppVersionStaging {
  pub fn directory(&self) -> &Path {
    self.temp_dir.path()
  }

  /// Publish a fully prepared version directory without replacing an existing version.
  pub fn publish(self, version: &Version) -> Result<PublishedAppVersion, AppStorageError> {
    validate_version(version)?;
    let destination = self.store.version_path(version)?;
    if fs::symlink_metadata(&destination).is_ok() {
      return Err(AppStorageError::VersionAlreadyExists(version.clone()));
    }

    validate_package_tree(self.temp_dir.path())?;
    fs::rename(self.temp_dir.path(), &destination)?;
    self.store.published_version(version)
  }
}

#[derive(Debug, Clone)]
pub struct PublishedAppVersion {
  store: AppVersionStore,
  version: Version,
}

impl PublishedAppVersion {
  pub fn version(&self) -> &Version {
    &self.version
  }

  pub fn directory_path(&self) -> Result<PathBuf, AppStorageError> {
    self.directory()
  }

  pub fn open_entry(&self, entry: &str) -> Result<File, AppStorageError> {
    Ok(File::open(self.entry_path(entry)?)?)
  }

  /// Resolve a regular executable or resource inside the published package.
  pub fn entry_path(&self, entry: &str) -> Result<PathBuf, AppStorageError> {
    self.resolve_entry(entry)
  }

  fn directory(&self) -> Result<PathBuf, AppStorageError> {
    let path = self.store.version_path(&self.version)?;
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
      if error.kind() == std::io::ErrorKind::NotFound {
        AppStorageError::MissingVersion(self.version.clone())
      } else {
        AppStorageError::Io(error)
      }
    })?;
    if metadata.file_type().is_symlink() {
      return Err(AppStorageError::SymbolicLink(path));
    }
    if !metadata.is_dir() {
      return Err(AppStorageError::NotDirectory(path));
    }
    let canonical_path = fs::canonicalize(&path)?;
    ensure_within(&self.store.versions_dir(), &canonical_path)?;
    Ok(canonical_path)
  }

  /// Resolve a regular file inside the published package, rejecting links and escapes.
  fn resolve_entry(&self, entry: &str) -> Result<PathBuf, AppStorageError> {
    let entry_path = validate_entry(entry)?;
    let component_count = entry_path.components().count();
    let package_dir = self.directory()?;
    let mut candidate = package_dir.clone();
    for (index, component) in entry_path.components().enumerate() {
      let Component::Normal(component) = component else {
        return Err(AppStorageError::InvalidEntry(entry.to_owned()));
      };
      candidate.push(component);
      let metadata = fs::symlink_metadata(&candidate).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
          AppStorageError::EntryNotFound(candidate.clone())
        } else {
          AppStorageError::Io(error)
        }
      })?;
      if metadata.file_type().is_symlink() {
        return Err(AppStorageError::SymbolicLink(candidate));
      }
      let is_last = index + 1 == component_count;
      if is_last && !metadata.is_file() {
        return Err(AppStorageError::NotRegularFile(candidate));
      }
      if !is_last && !metadata.is_dir() {
        return Err(AppStorageError::NotDirectory(candidate));
      }
    }

    let canonical_candidate = fs::canonicalize(&candidate)?;
    ensure_within(&package_dir, &canonical_candidate)?;
    Ok(canonical_candidate)
  }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateDocument {
  schema_version: u32,
  active_version: Option<String>,
  pending_version: Option<String>,
  previous_healthy_version: Option<String>,
  #[serde(default)]
  activation_in_progress: bool,
  #[serde(default)]
  failed_version: Option<String>,
  #[serde(default)]
  activation_failure: Option<ActivationFailureReason>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateCheckDocument {
  schema_version: u32,
  last_check_unix_seconds: u64,
}

fn parse_version(value: Option<String>) -> Result<Option<Version>, AppStorageError> {
  value
    .map(|value| {
      let version =
        Version::parse(&value).map_err(|_| AppStorageError::InvalidVersion(value.clone()))?;
      if version.to_string() != value {
        return Err(AppStorageError::InvalidVersion(value));
      }
      Ok(version)
    })
    .transpose()
}

fn version_string(version: &Option<Version>) -> Option<String> {
  version.as_ref().map(ToString::to_string)
}

fn validate_version(version: &Version) -> Result<(), AppStorageError> {
  if Version::parse(&version.to_string()).is_ok() {
    Ok(())
  } else {
    Err(AppStorageError::InvalidVersion(version.to_string()))
  }
}

fn validate_entry(entry: &str) -> Result<PathBuf, AppStorageError> {
  let path = Path::new(entry);
  let valid = !entry.is_empty()
    && !entry.contains('\0')
    && !entry.starts_with('/')
    && !entry.contains('\\')
    && !entry.contains(':')
    && path
      .components()
      .all(|component| matches!(component, Component::Normal(_)))
    && path.components().count() == entry.split('/').count();
  if valid {
    Ok(path.to_path_buf())
  } else {
    Err(AppStorageError::InvalidEntry(entry.to_owned()))
  }
}

fn ensure_directory(root: &Path, path: &Path) -> Result<(), AppStorageError> {
  ensure_within(root, path)?;
  let root_metadata = fs::symlink_metadata(root)?;
  if root_metadata.file_type().is_symlink() {
    return Err(AppStorageError::SymbolicLink(root.to_path_buf()));
  }
  if !root_metadata.is_dir() {
    return Err(AppStorageError::NotDirectory(root.to_path_buf()));
  }

  let relative = path
    .strip_prefix(root)
    .map_err(|_| AppStorageError::InvalidEntry(path.display().to_string()))?;
  let mut current = root.to_path_buf();
  for component in relative.components() {
    let Component::Normal(component) = component else {
      return Err(AppStorageError::InvalidEntry(path.display().to_string()));
    };
    current.push(component);
    loop {
      match fs::symlink_metadata(&current) {
        Ok(metadata) => {
          if metadata.file_type().is_symlink() {
            return Err(AppStorageError::SymbolicLink(current));
          }
          if !metadata.is_dir() {
            return Err(AppStorageError::NotDirectory(current));
          }
          break;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
          match fs::create_dir(&current) {
            Ok(()) => break,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(AppStorageError::Io(error)),
          }
        }
        Err(error) => return Err(AppStorageError::Io(error)),
      }
    }
  }
  Ok(())
}

fn ensure_within(root: &Path, candidate: &Path) -> Result<(), AppStorageError> {
  if candidate.starts_with(root) {
    Ok(())
  } else {
    Err(AppStorageError::InvalidEntry(
      candidate.display().to_string(),
    ))
  }
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>, AppStorageError> {
  let metadata = fs::symlink_metadata(path).map_err(|error| {
    if error.kind() == std::io::ErrorKind::NotFound {
      AppStorageError::EntryNotFound(path.to_path_buf())
    } else {
      AppStorageError::Io(error)
    }
  })?;
  if metadata.file_type().is_symlink() {
    return Err(AppStorageError::SymbolicLink(path.to_path_buf()));
  }
  if !metadata.is_file() {
    return Err(AppStorageError::NotRegularFile(path.to_path_buf()));
  }
  if metadata.len() > MAX_STATE_BYTES {
    return Err(AppStorageError::InvalidState(format!(
      "state file exceeds {MAX_STATE_BYTES} bytes"
    )));
  }
  Ok(fs::read(path)?)
}

fn atomic_replace(root: &Path, path: &Path, bytes: &[u8]) -> Result<(), AppStorageError> {
  let parent = path
    .parent()
    .ok_or_else(|| AppStorageError::InvalidEntry(path.display().to_string()))?;
  ensure_directory(root, parent)?;
  match fs::symlink_metadata(path) {
    Ok(metadata) if metadata.file_type().is_symlink() => {
      return Err(AppStorageError::SymbolicLink(path.to_path_buf()));
    }
    Ok(metadata) if !metadata.is_file() => {
      return Err(AppStorageError::NotRegularFile(path.to_path_buf()));
    }
    Ok(_) => {}
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => return Err(AppStorageError::Io(error)),
  }

  let mut temporary = NamedTempFile::new_in(parent)?;
  temporary.write_all(bytes)?;
  temporary.as_file().sync_all()?;
  temporary
    .persist(path)
    .map_err(|error| AppStorageError::Io(error.error))?;
  sync_directory(parent)?;
  Ok(())
}

fn sync_directory(path: &Path) -> Result<(), AppStorageError> {
  #[cfg(unix)]
  File::open(path)?.sync_all()?;
  #[cfg(not(unix))]
  let _ = path;
  Ok(())
}

fn validate_package_tree(root: &Path) -> Result<(), AppStorageError> {
  let root_metadata = fs::symlink_metadata(root)?;
  if root_metadata.file_type().is_symlink() {
    return Err(AppStorageError::SymbolicLink(root.to_path_buf()));
  }
  if !root_metadata.is_dir() {
    return Err(AppStorageError::NotDirectory(root.to_path_buf()));
  }

  let mut pending = vec![root.to_path_buf()];
  while let Some(directory) = pending.pop() {
    for entry in fs::read_dir(&directory)? {
      let entry = entry?;
      let path = entry.path();
      let metadata = fs::symlink_metadata(&path)?;
      if metadata.file_type().is_symlink() {
        return Err(AppStorageError::SymbolicLink(path));
      }
      if metadata.is_dir() {
        pending.push(path);
      } else if !metadata.is_file() {
        return Err(AppStorageError::UnsupportedEntry(path));
      }
    }
  }
  Ok(())
}
