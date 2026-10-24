use super::*;

#[test]
fn canonical_download_destination_is_accepted() {
  let root = tempfile::tempdir().unwrap();
  let target = root.path().join("resource.jar");
  fs::write(&target, b"download fixture").unwrap();
  let returned = fs::canonicalize(root.path()).unwrap().join("resource.jar");
  #[cfg(windows)]
  assert_ne!(
    returned, target,
    "exercise Windows verbatim versus ordinary paths"
  );
  assert!(same_download_location(&returned, &target).unwrap());
}

#[test]
fn unexpected_or_missing_download_destination_is_refused() {
  let root = tempfile::tempdir().unwrap();
  let target = root.path().join("expected.jar");
  let other = root.path().join("other.jar");
  fs::write(&target, b"same bytes").unwrap();
  fs::write(&other, b"same bytes").unwrap();
  assert!(!same_download_location(&other, &target).unwrap());
  assert!(!same_download_location(root.path(), &target).unwrap());
  assert!(!same_download_location(&root.path().join("missing"), &target).unwrap());
}

#[cfg(unix)]
#[test]
fn linked_download_destination_is_refused() {
  let root = tempfile::tempdir().unwrap();
  let target = root.path().join("expected.jar");
  let link = root.path().join("link.jar");
  fs::write(&target, b"fixture").unwrap();
  std::os::unix::fs::symlink(&target, &link).unwrap();
  assert!(!same_download_location(&link, &target).unwrap());
}
