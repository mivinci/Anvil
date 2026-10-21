use super::*;
#[test]
fn locks_exclude_other_handles_and_release_without_deleting_lock_file() {
  let root = tempfile::tempdir().unwrap();
  let first = lock(root.path(), "temurin-test").unwrap();
  assert!(matches!(
    lock(root.path(), "temurin-test"),
    Err(Error::Busy(_))
  ));
  drop(first);
  assert!(lock(root.path(), "temurin-test").is_ok());
  assert!(root.path().join("locks/java-temurin-test.lock").exists());
}
#[test]
fn manifest_size_is_bounded_and_json_write_does_not_clobber() {
  let root = tempfile::tempdir().unwrap();
  let path = root.path().join("record.json");
  write_json(&path, &serde_json::json!({"ok":true})).unwrap();
  assert!(write_json(&path, &serde_json::json!({"ok":false})).is_err());
  assert_eq!(read_json::<serde_json::Value>(&path).unwrap()["ok"], true);
  fs::write(&path, vec![b' '; 65537]).unwrap();
  assert!(matches!(
    read_json::<serde_json::Value>(&path),
    Err(Error::Metadata(_))
  ));
}
