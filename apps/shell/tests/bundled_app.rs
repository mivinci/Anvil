use anvil_shell::{
  activation::AppActivation,
  app_storage::{AppStorage, PublishedAppVersion},
  bundled_app::{BundledAppInstallOutcome, install_bundled_app},
};
use semver::Version;
use std::{
  fs,
  io::Read,
  path::{Path, PathBuf},
};
use tempfile::{TempDir, tempdir};

#[test]
fn installs_and_activates_a_valid_bundled_package() {
  let (_root, storage, package, version) = fixture();
  let result = install(&storage, &package, &version).unwrap();
  assert_eq!(
    result,
    BundledAppInstallOutcome::Installed(Version::parse(&version).unwrap())
  );

  let active = active_package(&storage);
  assert_eq!(active.version().to_string(), version);
  assert_eq!(read_page(&active), "baseline page");
}

#[test]
fn keeps_the_existing_active_version_and_does_not_require_the_bundle_again() {
  let (_root, storage, package, version) = fixture();
  install(&storage, &package, &version).unwrap();
  fs::write(package.join("web/index.html"), "changed baseline").unwrap();

  let result = install_bundled_app(
    &storage,
    None,
    env!("ANVIL_DEFAULT_APP_ID"),
    env!("ANVIL_TARGET_PLATFORM"),
    env!("ANVIL_SHELL_API_VERSION"),
    &version,
  )
  .unwrap();
  assert_eq!(
    result,
    BundledAppInstallOutcome::AlreadyInstalled(Version::parse(&version).unwrap())
  );
  assert_eq!(read_page(&active_package(&storage)), "baseline page");
}

#[test]
fn newer_baseline_becomes_pending_and_activates_on_startup() {
  let (_root, storage, package, version) = fixture();
  let older = "0.1.4-alpha.4";
  let older_package = package.parent().unwrap().join("older-package");
  fs::create_dir(&older_package).unwrap();
  copy_tree(&package, &older_package);
  let mut manifest: serde_json::Value =
    serde_json::from_slice(&fs::read(older_package.join("manifest.json")).unwrap()).unwrap();
  manifest["version"] = serde_json::json!(older);
  fs::write(
    older_package.join("manifest.json"),
    serde_json::to_vec(&manifest).unwrap(),
  )
  .unwrap();
  install_bundled_app(
    &storage,
    Some(&older_package),
    env!("ANVIL_DEFAULT_APP_ID"),
    env!("ANVIL_TARGET_PLATFORM"),
    env!("ANVIL_SHELL_API_VERSION"),
    older,
  )
  .unwrap();
  assert_eq!(active_package(&storage).version().to_string(), older);

  let result = install(&storage, &package, &version).unwrap();
  assert_eq!(
    result,
    BundledAppInstallOutcome::Pending(Version::parse(&version).unwrap())
  );
  let store = storage.app(env!("ANVIL_DEFAULT_APP_ID")).unwrap();
  let state = store.state().unwrap();
  assert_eq!(state.active_version, Some(Version::parse(older).unwrap()));
  assert_eq!(
    state.pending_version,
    Some(Version::parse(&version).unwrap())
  );

  let activation = AppActivation::new(store.clone());
  assert!(activation.prepare_startup().unwrap());
  assert_eq!(
    store.state().unwrap().active_version,
    Some(Version::parse(&version).unwrap())
  );
  assert!(activation.confirm_health().unwrap());
  assert!(!store.state().unwrap().activation_in_progress);
}

#[test]
fn invalid_newer_baseline_keeps_the_active_version() {
  let (_root, storage, package, version) = fixture();
  let older = "0.1.4-alpha.4";
  let older_package = package.parent().unwrap().join("older-package");
  fs::create_dir(&older_package).unwrap();
  copy_tree(&package, &older_package);
  let mut manifest: serde_json::Value =
    serde_json::from_slice(&fs::read(older_package.join("manifest.json")).unwrap()).unwrap();
  manifest["version"] = serde_json::json!(older);
  fs::write(
    older_package.join("manifest.json"),
    serde_json::to_vec(&manifest).unwrap(),
  )
  .unwrap();
  install_bundled_app(
    &storage,
    Some(&older_package),
    env!("ANVIL_DEFAULT_APP_ID"),
    env!("ANVIL_TARGET_PLATFORM"),
    env!("ANVIL_SHELL_API_VERSION"),
    older,
  )
  .unwrap();
  fs::remove_file(package.join("web/index.html")).unwrap();

  assert_eq!(
    install(&storage, &package, &version).unwrap(),
    BundledAppInstallOutcome::AlreadyInstalled(Version::parse(older).unwrap())
  );
  let store = storage.app(env!("ANVIL_DEFAULT_APP_ID")).unwrap();
  assert_eq!(
    store.state().unwrap().active_version,
    Some(Version::parse(older).unwrap())
  );
  assert_eq!(store.state().unwrap().pending_version, None);
}

#[test]
fn activates_a_valid_package_that_was_published_before_a_previous_failure() {
  let (_root, storage, package, version) = fixture();
  let store = storage.app(env!("ANVIL_DEFAULT_APP_ID")).unwrap();
  let staging = store.stage_version().unwrap();
  copy_tree(&package, staging.directory());
  staging.publish(&Version::parse(&version).unwrap()).unwrap();

  assert_eq!(
    install(&storage, &package, &version).unwrap(),
    BundledAppInstallOutcome::RecoveredPublished(Version::parse(&version).unwrap())
  );
  assert_eq!(read_page(&active_package(&storage)), "baseline page");
}

#[test]
fn rejects_an_invalid_package_without_publishing_or_activating_it() {
  let (_root, storage, package, version) = fixture();
  fs::remove_file(package.join("web/index.html")).unwrap();

  assert!(install(&storage, &package, &version).is_err());
  let store = storage.app(env!("ANVIL_DEFAULT_APP_ID")).unwrap();
  assert!(store.resolve_active().unwrap().is_none());
  assert!(
    store
      .published_version(&Version::parse(&version).unwrap())
      .is_err()
  );
}

#[cfg(unix)]
#[test]
fn rejects_symbolic_links_inside_the_bundled_package() {
  use std::os::unix::fs::symlink;

  let (_root, storage, package, version) = fixture();
  let external_file = package.parent().unwrap().join("outside.html");
  fs::write(&external_file, "outside package").unwrap();
  fs::remove_file(package.join("web/index.html")).unwrap();
  symlink(&external_file, package.join("web/index.html")).unwrap();

  assert!(install(&storage, &package, &version).is_err());
  assert!(
    storage
      .app(env!("ANVIL_DEFAULT_APP_ID"))
      .unwrap()
      .resolve_active()
      .unwrap()
      .is_none()
  );
}

#[cfg(unix)]
#[test]
fn preserves_executable_permissions_when_publishing_the_sidecar() {
  use std::os::unix::fs::PermissionsExt;

  let (_root, storage, package, version) = fixture();
  let sidecar = package.join(sidecar_entry());
  fs::set_permissions(&sidecar, fs::Permissions::from_mode(0o755)).unwrap();
  install(&storage, &package, &version).unwrap();

  let metadata = fs::metadata(
    active_package(&storage)
      .entry_path(sidecar_entry())
      .unwrap(),
  )
  .unwrap();
  assert_ne!(metadata.permissions().mode() & 0o111, 0);
}

fn install(
  storage: &AppStorage,
  package: &Path,
  version: &str,
) -> Result<BundledAppInstallOutcome, anvil_shell::bundled_app::BundledAppInstallError> {
  install_bundled_app(
    storage,
    Some(package),
    env!("ANVIL_DEFAULT_APP_ID"),
    env!("ANVIL_TARGET_PLATFORM"),
    env!("ANVIL_SHELL_API_VERSION"),
    version,
  )
}

fn active_package(storage: &AppStorage) -> PublishedAppVersion {
  storage
    .app(env!("ANVIL_DEFAULT_APP_ID"))
    .unwrap()
    .resolve_active()
    .unwrap()
    .unwrap()
}

fn read_page(package: &PublishedAppVersion) -> String {
  let mut page = String::new();
  package
    .open_entry("web/index.html")
    .unwrap()
    .read_to_string(&mut page)
    .unwrap();
  page
}

fn fixture() -> (TempDir, AppStorage, PathBuf, String) {
  let root = tempdir().unwrap();
  let storage = AppStorage::open(root.path().join("data")).unwrap();
  let package = root.path().join("package");
  fs::create_dir_all(package.join("web")).unwrap();
  fs::create_dir_all(package.join("runtime")).unwrap();
  let version = env!("ANVIL_BUNDLED_APP_VERSION").to_owned();
  let manifest = serde_json::json!({
    "schema_version": 2,
    "app_id": env!("ANVIL_DEFAULT_APP_ID"),
    "version": version,
    "target": env!("ANVIL_TARGET_PLATFORM"),
    "page_entry": "web/index.html",
    "sidecar_entry": sidecar_entry(),
    "http_api_version": "1.0.0",
    "http_api_compatibility": "^1.0.0",
    "shell_api_compatibility": env!("ANVIL_SHELL_API_VERSION")
  });
  fs::write(
    package.join("manifest.json"),
    serde_json::to_vec(&manifest).unwrap(),
  )
  .unwrap();
  fs::write(package.join("web/index.html"), "baseline page").unwrap();
  fs::write(package.join(sidecar_entry()), "runtime binary").unwrap();
  (root, storage, package, version)
}

fn sidecar_entry() -> &'static str {
  if env!("ANVIL_TARGET_PLATFORM").contains("windows") {
    "runtime/anvil-desktop.exe"
  } else {
    "runtime/anvil-desktop"
  }
}

fn copy_tree(source: &Path, destination: &Path) {
  for entry in fs::read_dir(source).unwrap() {
    let entry = entry.unwrap();
    let source = entry.path();
    let destination = destination.join(entry.file_name());
    if source.is_dir() {
      fs::create_dir(&destination).unwrap();
      copy_tree(&source, &destination);
    } else {
      fs::copy(&source, &destination).unwrap();
    }
  }
}
