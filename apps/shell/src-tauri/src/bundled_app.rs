use crate::{
  app_manifest::AppManifest,
  app_storage::{AppStorage, AppStorageError, AppVersionState, PublishedAppVersion},
};
use semver::Version;
use std::{
  fs,
  io::{self, Read},
  path::{Component, Path, PathBuf},
};
use thiserror::Error;

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_PACKAGE_FILES: usize = 100_000;
const MAX_PACKAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundledAppInstallOutcome {
  Installed(Version),
  RecoveredPublished(Version),
  AlreadyInstalled(Version),
}

#[derive(Debug, Error)]
pub enum BundledAppInstallError {
  #[error("bundled App package is unavailable")]
  PackageUnavailable,
  #[error("invalid bundled App package: {0}")]
  InvalidPackage(String),
  #[error("bundled App manifest version {actual} does not match Shell baseline {expected}")]
  VersionMismatch { actual: String, expected: String },
  #[error("bundled App package exceeds its file or size limit")]
  PackageTooLarge,
  #[error("bundled App package contains a symbolic link: {0}")]
  SymbolicLink(PathBuf),
  #[error("bundled App package contains an unsupported filesystem entry: {0}")]
  UnsupportedEntry(PathBuf),
  #[error(transparent)]
  Storage(#[from] AppStorageError),
  #[error(transparent)]
  Io(#[from] io::Error),
}

/// Install the package shipped with this Shell build if no App version is active.
/// The package is copied into staging, validated there, published immutably, then activated.
pub fn install_bundled_app(
  storage: &AppStorage,
  package_directory: Option<&Path>,
  expected_app_id: &str,
  expected_target: &str,
  shell_api_version: &str,
  bundled_app_version: &str,
) -> Result<BundledAppInstallOutcome, BundledAppInstallError> {
  let store = storage.app(expected_app_id)?;
  if let Some(active) = store.resolve_active()? {
    return Ok(BundledAppInstallOutcome::AlreadyInstalled(
      active.version().clone(),
    ));
  }

  let package_directory = package_directory.ok_or(BundledAppInstallError::PackageUnavailable)?;
  let source_root = canonical_package_root(package_directory)?;
  let manifest = validate_package(
    &source_root,
    expected_app_id,
    expected_target,
    shell_api_version,
    bundled_app_version,
  )?;
  let version = Version::parse(&manifest.version)
    .map_err(|error| BundledAppInstallError::InvalidPackage(error.to_string()))?;

  match store.published_version(&version) {
    Ok(published) => {
      validate_published_package(
        &published,
        expected_app_id,
        expected_target,
        shell_api_version,
        bundled_app_version,
      )?;
      if let Some(active) = store.resolve_active()? {
        return Ok(BundledAppInstallOutcome::AlreadyInstalled(
          active.version().clone(),
        ));
      }
      activate(&store, version.clone())?;
      return Ok(BundledAppInstallOutcome::RecoveredPublished(version));
    }
    Err(AppStorageError::MissingVersion(_)) => {}
    Err(error) => return Err(error.into()),
  }

  let staging = store.stage_version()?;
  copy_package_tree(&source_root, staging.directory())?;
  validate_package(
    staging.directory(),
    expected_app_id,
    expected_target,
    shell_api_version,
    bundled_app_version,
  )?;

  let published = match staging.publish(&version) {
    Ok(published) => published,
    Err(AppStorageError::VersionAlreadyExists(_)) => store.published_version(&version)?,
    Err(error) => return Err(error.into()),
  };
  validate_published_package(
    &published,
    expected_app_id,
    expected_target,
    shell_api_version,
    bundled_app_version,
  )?;

  if let Some(active) = store.resolve_active()? {
    return Ok(BundledAppInstallOutcome::AlreadyInstalled(
      active.version().clone(),
    ));
  }
  activate(&store, version.clone())?;
  Ok(BundledAppInstallOutcome::Installed(version))
}

fn activate(
  store: &crate::app_storage::AppVersionStore,
  version: Version,
) -> Result<(), AppStorageError> {
  let state = AppVersionState {
    active_version: Some(version),
    pending_version: None,
    previous_healthy_version: None,
    activation_in_progress: false,
    failed_version: None,
    activation_failure: None,
  };
  store.write_state(&state)
}

fn canonical_package_root(path: &Path) -> Result<PathBuf, BundledAppInstallError> {
  let metadata = fs::symlink_metadata(path)?;
  if metadata.file_type().is_symlink() {
    return Err(BundledAppInstallError::SymbolicLink(path.to_path_buf()));
  }
  if !metadata.is_dir() {
    return Err(BundledAppInstallError::InvalidPackage(
      "package root is not a directory".into(),
    ));
  }
  Ok(fs::canonicalize(path)?)
}

pub(crate) fn validate_package(
  package_root: &Path,
  expected_app_id: &str,
  expected_target: &str,
  shell_api_version: &str,
  bundled_app_version: &str,
) -> Result<AppManifest, BundledAppInstallError> {
  validate_package_tree(package_root)?;
  let source = read_entry(package_root, "manifest.json", MAX_MANIFEST_BYTES)?;
  let manifest =
    AppManifest::parse_and_validate(&source, expected_app_id, expected_target, shell_api_version)
      .map_err(BundledAppInstallError::InvalidPackage)?;
  if manifest.version != bundled_app_version {
    return Err(BundledAppInstallError::VersionMismatch {
      actual: manifest.version,
      expected: bundled_app_version.to_owned(),
    });
  }
  read_entry(package_root, &manifest.page_entry, MAX_PACKAGE_BYTES)?;
  read_entry(package_root, &manifest.sidecar_entry, MAX_PACKAGE_BYTES)?;
  Ok(manifest)
}

fn validate_published_package(
  package: &PublishedAppVersion,
  expected_app_id: &str,
  expected_target: &str,
  shell_api_version: &str,
  bundled_app_version: &str,
) -> Result<(), BundledAppInstallError> {
  let manifest = package.open_entry("manifest.json")?;
  let mut source = Vec::new();
  manifest
    .take(MAX_MANIFEST_BYTES + 1)
    .read_to_end(&mut source)?;
  if source.len() as u64 > MAX_MANIFEST_BYTES {
    return Err(BundledAppInstallError::InvalidPackage(
      "manifest exceeds size limit".into(),
    ));
  }
  let manifest =
    AppManifest::parse_and_validate(&source, expected_app_id, expected_target, shell_api_version)
      .map_err(BundledAppInstallError::InvalidPackage)?;
  if manifest.version != bundled_app_version || package.version().to_string() != manifest.version {
    return Err(BundledAppInstallError::VersionMismatch {
      actual: manifest.version,
      expected: bundled_app_version.to_owned(),
    });
  }
  package.open_entry(&manifest.page_entry)?;
  package.open_entry(&manifest.sidecar_entry)?;
  Ok(())
}

fn read_entry(
  root: &Path,
  entry: &str,
  maximum_bytes: u64,
) -> Result<Vec<u8>, BundledAppInstallError> {
  let path = resolve_entry(root, entry)?;
  let metadata = fs::metadata(&path)?;
  if metadata.len() > maximum_bytes {
    return Err(BundledAppInstallError::PackageTooLarge);
  }
  Ok(fs::read(path)?)
}

fn resolve_entry(root: &Path, entry: &str) -> Result<PathBuf, BundledAppInstallError> {
  let path = Path::new(entry);
  let mut candidate = root.to_path_buf();
  for component in path.components() {
    let Component::Normal(component) = component else {
      return Err(BundledAppInstallError::InvalidPackage(format!(
        "invalid package entry {entry:?}"
      )));
    };
    candidate.push(component);
    let metadata = fs::symlink_metadata(&candidate)?;
    if metadata.file_type().is_symlink() {
      return Err(BundledAppInstallError::SymbolicLink(candidate));
    }
  }
  let metadata = fs::metadata(&candidate)?;
  if !metadata.is_file() {
    return Err(BundledAppInstallError::InvalidPackage(format!(
      "package entry {entry:?} is not a regular file"
    )));
  }
  let canonical = fs::canonicalize(&candidate)?;
  if !canonical.starts_with(root) {
    return Err(BundledAppInstallError::InvalidPackage(format!(
      "package entry {entry:?} escapes the package root"
    )));
  }
  Ok(canonical)
}

fn validate_package_tree(root: &Path) -> Result<(), BundledAppInstallError> {
  let mut pending = vec![root.to_path_buf()];
  let mut file_count = 0usize;
  let mut total_bytes = 0u64;
  while let Some(directory) = pending.pop() {
    for entry in fs::read_dir(&directory)? {
      let entry = entry?;
      let path = entry.path();
      let metadata = fs::symlink_metadata(&path)?;
      if metadata.file_type().is_symlink() {
        return Err(BundledAppInstallError::SymbolicLink(path));
      }
      if metadata.is_dir() {
        pending.push(path);
      } else if metadata.is_file() {
        file_count += 1;
        total_bytes = total_bytes.saturating_add(metadata.len());
        if file_count > MAX_PACKAGE_FILES || total_bytes > MAX_PACKAGE_BYTES {
          return Err(BundledAppInstallError::PackageTooLarge);
        }
      } else {
        return Err(BundledAppInstallError::UnsupportedEntry(path));
      }
    }
  }
  Ok(())
}

fn copy_package_tree(source: &Path, destination: &Path) -> Result<(), BundledAppInstallError> {
  let mut pending = vec![(source.to_path_buf(), destination.to_path_buf())];
  let mut file_count = 0usize;
  let mut total_bytes = 0u64;
  while let Some((source_directory, destination_directory)) = pending.pop() {
    for entry in fs::read_dir(&source_directory)? {
      let entry = entry?;
      let source_path = entry.path();
      let destination_path = destination_directory.join(entry.file_name());
      let metadata = fs::symlink_metadata(&source_path)?;
      if metadata.file_type().is_symlink() {
        return Err(BundledAppInstallError::SymbolicLink(source_path));
      }
      if metadata.is_dir() {
        fs::create_dir(&destination_path)?;
        pending.push((source_path, destination_path));
      } else if metadata.is_file() {
        file_count += 1;
        total_bytes = total_bytes.saturating_add(metadata.len());
        if file_count > MAX_PACKAGE_FILES || total_bytes > MAX_PACKAGE_BYTES {
          return Err(BundledAppInstallError::PackageTooLarge);
        }
        fs::copy(&source_path, &destination_path)?;
        set_copied_file_permissions(&destination_path, &metadata)?;
      } else {
        return Err(BundledAppInstallError::UnsupportedEntry(source_path));
      }
    }
  }
  Ok(())
}

#[cfg(unix)]
fn set_copied_file_permissions(path: &Path, source: &fs::Metadata) -> Result<(), io::Error> {
  use std::os::unix::fs::PermissionsExt;
  let permissions = if source.permissions().mode() & 0o111 != 0 {
    0o755
  } else {
    0o644
  };
  fs::set_permissions(path, fs::Permissions::from_mode(permissions))
}

#[cfg(not(unix))]
fn set_copied_file_permissions(_path: &Path, _source: &fs::Metadata) -> Result<(), io::Error> {
  Ok(())
}
