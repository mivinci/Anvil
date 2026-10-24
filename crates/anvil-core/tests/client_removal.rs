use super::*;

fn fixture() -> (tempfile::TempDir, ClientInstaller, PathBuf) {
  let root = tempfile::tempdir().unwrap();
  let context = Context::new(crate::Paths {
    data: root.path().join("data"),
    cache: root.path().join("cache"),
  })
  .unwrap();
  let target = context.paths.data.join("instances/client-test");
  fs::create_dir_all(target.join("game/saves/world")).unwrap();
  fs::write(target.join("game/saves/world/level.dat"), b"world").unwrap();
  storage::write_json(
    &target.join("instance.json"),
    &ClientInstance {
      schema_version: 1,
      instance_type: "client".into(),
      id: "client-test".into(),
      name: "Test".into(),
    },
  )
  .unwrap();
  (root, ClientInstaller::new(context), target)
}

#[tokio::test]
async fn removes_only_instance_and_keeps_shared_data() {
  let (_root, installer, target) = fixture();
  let shared = installer.context.paths.data.join("resources/objects/blob");
  fs::create_dir_all(shared.parent().unwrap()).unwrap();
  fs::write(&shared, b"shared mod").unwrap();
  let cache = installer.context.paths.cache.join("minecraft/blob");
  fs::create_dir_all(cache.parent().unwrap()).unwrap();
  fs::write(&cache, b"shared game").unwrap();
  let other = installer
    .context
    .paths
    .data
    .join("instances/client-other/game");
  fs::create_dir_all(&other).unwrap();
  fs::write(other.join("keep"), b"other instance").unwrap();
  let result = installer.remove("Test").await.unwrap();
  assert!(result.removed && !result.cleanup_pending);
  assert!(!target.exists());
  assert_eq!(fs::read(shared).unwrap(), b"shared mod");
  assert_eq!(fs::read(cache).unwrap(), b"shared game");
  assert!(other.join("keep").exists());
  assert!(matches!(
    installer.remove("Test").await,
    Err(Error::NotFound(_))
  ));
}

#[tokio::test]
async fn refuses_running_install_and_settings_locks() {
  let (_root, installer, target) = fixture();
  let lock = storage::server_instance_lock(&installer.context.paths.data, "client-test").unwrap();
  assert!(matches!(
    installer.remove("test").await,
    Err(Error::Busy(_))
  ));
  assert!(target.join("game/saves/world/level.dat").exists());
  drop(lock);
  let _settings =
    storage::client_settings_lock(&installer.context.paths.data, "client-test").unwrap();
  assert!(matches!(
    installer.remove("test").await,
    Err(Error::Busy(_))
  ));
  assert!(target.exists());
}

#[tokio::test]
async fn refuses_traversal_and_changed_identity() {
  let (_root, installer, target) = fixture();
  assert!(matches!(
    installer.remove("../test").await,
    Err(Error::Invalid(_))
  ));
  fs::write(
    target.join("instance.json"),
    br#"{"schema_version":1,"instance_type":"client","id":"client-other","name":"other"}"#,
  )
  .unwrap();
  assert!(installer.remove("test").await.is_err());
  assert!(target.join("game/saves/world/level.dat").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn refuses_external_links_without_touching_target() {
  let (root, installer, target) = fixture();
  let external = root.path().join("external");
  fs::create_dir(&external).unwrap();
  fs::write(external.join("keep"), b"external").unwrap();
  std::os::unix::fs::symlink(&external, target.join("game/linked-world")).unwrap();
  assert!(matches!(
    installer.remove("test").await,
    Err(Error::Conflict(_))
  ));
  assert!(external.join("keep").exists());
  assert!(target.exists());
}

#[cfg(windows)]
#[tokio::test]
async fn refuses_windows_junction_without_touching_target() {
  let (root, installer, target) = fixture();
  let external = root.path().join("external");
  fs::create_dir(&external).unwrap();
  fs::write(external.join("keep"), b"external").unwrap();
  let status = std::process::Command::new("cmd")
    .args(["/c", "mklink", "/J"])
    .arg(
      target
        .join("game/linked-world")
        .to_string_lossy()
        .replace('/', "\\"),
    )
    .arg(&external)
    .output()
    .unwrap();
  assert!(
    status.status.success(),
    "fixture junction creation failed: {} {}",
    String::from_utf8_lossy(&status.stdout),
    String::from_utf8_lossy(&status.stderr)
  );
  assert!(matches!(
    installer.remove("test").await,
    Err(Error::Conflict(_))
  ));
  assert!(external.join("keep").exists());
  assert!(target.exists());
}
