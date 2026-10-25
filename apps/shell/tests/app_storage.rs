use anvil_shell::app_storage::{
  AppStorage, AppStorageError, AppVersionState, AppVersionStore, PublishedAppVersion,
};
use semver::Version;
use std::{fs, io::Read};
use tempfile::TempDir;

fn store(temp: &TempDir) -> Result<AppVersionStore, AppStorageError> {
  AppStorage::open(temp.path().join("data"))?.app("anvil.desktop")
}

fn version(value: &str) -> Version {
  Version::parse(value).expect("test version is valid")
}

fn publish_with_page(
  store: &AppVersionStore,
  version: &Version,
  body: &[u8],
) -> Result<PublishedAppVersion, AppStorageError> {
  let staging = store.stage_version()?;
  fs::create_dir_all(staging.directory().join("web"))?;
  fs::write(staging.directory().join("web/index.html"), body)?;
  staging.publish(version)
}

#[test]
fn initial_state_is_empty_and_isolated_by_app_id() -> Result<(), AppStorageError> {
  let temp = TempDir::new()?;
  let storage = AppStorage::open(temp.path().join("data"))?;
  let desktop = storage.app("anvil.desktop")?;
  let mobile = storage.app("anvil.mobile")?;

  assert_eq!(desktop.state()?, AppVersionState::default());
  assert_eq!(mobile.state()?, AppVersionState::default());
  assert_ne!(desktop.app_dir(), mobile.app_dir());
  assert!(!desktop.app_dir().join("state.json").exists());
  Ok(())
}

#[test]
fn published_versions_resolve_and_cannot_be_replaced() -> Result<(), AppStorageError> {
  let temp = TempDir::new()?;
  let store = store(&temp)?;
  let active_version = version("0.1.0");
  let published = publish_with_page(&store, &active_version, b"original")?;
  let mut entry = published.open_entry("web/index.html")?;
  let mut body = Vec::new();
  entry.read_to_end(&mut body)?;
  assert_eq!(body, b"original");

  let state = AppVersionState {
    active_version: Some(active_version.clone()),
    pending_version: Some(version("0.2.0")),
    previous_healthy_version: Some(version("0.0.9")),
    activation_in_progress: false,
    failed_version: None,
    activation_failure: None,
  };
  publish_with_page(&store, &version("0.2.0"), b"pending")?;
  publish_with_page(&store, &version("0.0.9"), b"previous")?;
  store.write_state(&state)?;
  assert_eq!(store.state()?, state);
  assert_eq!(
    store
      .resolve_active()?
      .as_ref()
      .map(PublishedAppVersion::version),
    Some(&active_version)
  );

  let replacement = store.stage_version()?;
  fs::write(replacement.directory().join("replacement"), b"replacement")?;
  assert!(matches!(
    replacement.publish(&active_version),
    Err(AppStorageError::VersionAlreadyExists(existing)) if existing == active_version
  ));
  let mut entry = published.open_entry("web/index.html")?;
  let mut body = Vec::new();
  entry.read_to_end(&mut body)?;
  assert_eq!(body, b"original");
  Ok(())
}

#[test]
fn corrupt_or_unresolvable_state_is_rejected() -> Result<(), AppStorageError> {
  let temp = TempDir::new()?;
  let store = store(&temp)?;
  let state_path = store.app_dir().join("state.json");
  fs::write(&state_path, b"{not json")?;
  assert!(matches!(
    store.state(),
    Err(AppStorageError::InvalidState(_))
  ));

  fs::write(
    &state_path,
    br#"{"schema_version":1,"active_version":"1.2.3","pending_version":null,"previous_healthy_version":null}"#,
  )?;
  assert!(matches!(
    store.state(),
    Err(AppStorageError::MissingVersion(missing)) if missing == version("1.2.3")
  ));
  Ok(())
}

#[test]
fn legacy_state_schema_is_read_and_upgraded_on_the_next_write() -> Result<(), AppStorageError> {
  let temp = TempDir::new()?;
  let store = store(&temp)?;
  let active = version("1.2.3");
  publish_with_page(&store, &active, b"legacy page")?;
  fs::write(
    store.app_dir().join("state.json"),
    br#"{"schema_version":1,"active_version":"1.2.3","pending_version":null,"previous_healthy_version":null}"#,
  )?;

  let legacy = store.state()?;
  assert_eq!(legacy.active_version, Some(active));
  assert!(!legacy.activation_in_progress);
  assert_eq!(legacy.failed_version, None);
  assert_eq!(legacy.activation_failure, None);
  store.write_state(&legacy)?;

  let migrated: serde_json::Value =
    serde_json::from_slice(&fs::read(store.app_dir().join("state.json"))?)?;
  assert_eq!(migrated["schema_version"], 2);
  Ok(())
}

#[test]
fn failed_atomic_write_preserves_the_existing_state_path() -> Result<(), AppStorageError> {
  let temp = TempDir::new()?;
  let store = store(&temp)?;
  let state_path = store.app_dir().join("state.json");
  fs::create_dir(&state_path)?;

  assert!(store.write_state(&AppVersionState::default()).is_err());
  assert!(state_path.is_dir());
  let leftovers = fs::read_dir(store.app_dir())?.count();
  assert_eq!(leftovers, 1, "state.json obstruction only");
  Ok(())
}

#[test]
fn package_entries_reject_traversal_and_absolute_paths() -> Result<(), AppStorageError> {
  let temp = TempDir::new()?;
  let store = store(&temp)?;
  let published = publish_with_page(&store, &version("1.0.0"), b"page")?;

  for entry in [
    "../outside",
    "/etc/passwd",
    "C:/outside",
    "web\\..\\outside",
    "web//index.html",
  ] {
    assert!(
      matches!(
        published.open_entry(entry),
        Err(AppStorageError::InvalidEntry(_))
      ),
      "accepted unsafe entry {entry:?}"
    );
  }
  Ok(())
}

#[cfg(unix)]
#[test]
fn package_publishing_and_entry_resolution_reject_symlinks() -> Result<(), AppStorageError> {
  use std::os::unix::fs::symlink;

  let temp = TempDir::new()?;
  let store = store(&temp)?;
  let outside = temp.path().join("outside.txt");
  fs::write(&outside, b"outside")?;

  let staging = store.stage_version()?;
  fs::create_dir_all(staging.directory().join("web"))?;
  symlink(&outside, staging.directory().join("web/escape.txt"))?;
  assert!(matches!(
    staging.publish(&version("1.0.0")),
    Err(AppStorageError::SymbolicLink(_))
  ));

  let published = publish_with_page(&store, &version("1.0.0"), b"page")?;
  symlink(&outside, store.versions_dir().join("1.0.0/web/escape.txt"))?;
  assert!(matches!(
    published.open_entry("web/escape.txt"),
    Err(AppStorageError::SymbolicLink(_))
  ));
  Ok(())
}

#[cfg(unix)]
#[test]
fn app_storage_rejects_symlinked_app_directories() -> Result<(), AppStorageError> {
  use std::os::unix::fs::symlink;

  let temp = TempDir::new()?;
  let data_dir = temp.path().join("data");
  fs::create_dir_all(data_dir.join("apps"))?;
  let outside = temp.path().join("outside");
  fs::create_dir(&outside)?;
  symlink(&outside, data_dir.join("apps/anvil.desktop"))?;
  let storage = AppStorage::open(&data_dir)?;

  assert!(matches!(
    storage.app("anvil.desktop"),
    Err(AppStorageError::SymbolicLink(_))
  ));
  Ok(())
}

#[cfg(unix)]
#[test]
fn existing_store_handles_reject_replaced_app_directory_symlinks() -> Result<(), AppStorageError> {
  use std::os::unix::fs::symlink;

  let temp = TempDir::new()?;
  let storage = AppStorage::open(temp.path().join("data"))?;
  let store = storage.app("anvil.desktop")?;
  let apps_dir = store.app_dir().parent().expect("App dir has a parent");
  let moved_app_dir = temp.path().join("moved-app");
  fs::rename(store.app_dir(), &moved_app_dir)?;
  symlink(&moved_app_dir, apps_dir.join("anvil.desktop"))?;

  assert!(matches!(
    store.state(),
    Err(AppStorageError::SymbolicLink(_))
  ));
  Ok(())
}

#[test]
fn atomic_state_replacement_round_trips_multiple_fields() -> Result<(), AppStorageError> {
  let temp = TempDir::new()?;
  let store = store(&temp)?;
  let active = version("1.0.0");
  let pending = version("1.1.0");
  publish_with_page(&store, &active, b"active")?;
  publish_with_page(&store, &pending, b"pending")?;

  let first = AppVersionState {
    active_version: Some(active.clone()),
    pending_version: None,
    previous_healthy_version: None,
    activation_in_progress: false,
    failed_version: None,
    activation_failure: None,
  };
  store.write_state(&first)?;
  let second = AppVersionState {
    active_version: Some(active),
    pending_version: Some(pending),
    previous_healthy_version: None,
    activation_in_progress: false,
    failed_version: None,
    activation_failure: None,
  };
  store.write_state(&second)?;
  assert_eq!(store.state()?, second);
  Ok(())
}

#[test]
fn update_check_timestamp_survives_reopening_the_store() -> Result<(), AppStorageError> {
  let temp = TempDir::new()?;
  let store = store(&temp)?;
  let checked_at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_800_000_000);
  assert_eq!(store.last_update_check()?, None);
  store.record_update_check(checked_at)?;
  assert_eq!(store.last_update_check()?, Some(checked_at));

  let reopened = AppStorage::open(temp.path().join("data"))?.app("anvil.desktop")?;
  assert_eq!(reopened.last_update_check()?, Some(checked_at));
  Ok(())
}

#[test]
fn shell_update_check_timestamp_is_independent_and_survives_reopening()
-> Result<(), AppStorageError> {
  let temp = TempDir::new()?;
  let store = store(&temp)?;
  let checked_at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_800_000_100);
  assert_eq!(store.last_shell_update_check()?, None);
  assert_eq!(store.last_update_check()?, None);
  store.record_shell_update_check(checked_at)?;
  assert_eq!(store.last_shell_update_check()?, Some(checked_at));
  assert_eq!(store.last_update_check()?, None);

  let reopened = AppStorage::open(temp.path().join("data"))?.app("anvil.desktop")?;
  assert_eq!(reopened.last_shell_update_check()?, Some(checked_at));
  assert_eq!(reopened.last_update_check()?, None);
  Ok(())
}

#[test]
fn update_download_staging_is_stable_until_cleared() -> Result<(), AppStorageError> {
  let temp = TempDir::new()?;
  let store = store(&temp)?;
  let version = version("1.2.3");
  let first = store.update_staging_dir(&version)?;
  fs::write(first.join("package.tar.gz.q2q-http.part"), b"resume data")?;

  let reopened = AppStorage::open(temp.path().join("data"))?.app("anvil.desktop")?;
  let second = reopened.update_staging_dir(&version)?;
  assert_eq!(first, second);
  assert_eq!(
    fs::read(second.join("package.tar.gz.q2q-http.part"))?,
    b"resume data"
  );

  reopened.clear_update_staging(&version)?;
  assert!(!first.exists());
  Ok(())
}

#[cfg(unix)]
#[test]
fn update_staging_rejects_a_symlinked_version_directory() -> Result<(), AppStorageError> {
  use std::os::unix::fs::symlink;

  let temp = TempDir::new()?;
  let store = store(&temp)?;
  let version = version("1.2.3");
  let staging_root = store.app_dir().join("staging");
  fs::create_dir_all(&staging_root)?;
  let outside = temp.path().join("outside");
  fs::create_dir(&outside)?;
  symlink(&outside, staging_root.join("update-1.2.3"))?;

  assert!(matches!(
    store.update_staging_dir(&version),
    Err(AppStorageError::SymbolicLink(_))
  ));
  Ok(())
}
