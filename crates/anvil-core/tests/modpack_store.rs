use super::*;
fn identity(bytes: &[u8]) -> PackFile {
  let value = serde_json::json!({"formatVersion":1,"game":"minecraft","versionId":"1","name":"Pack","dependencies":{"minecraft":"1.21.1"},"files":[{"path":"config/example.json","hashes":{"sha1":hex(&sha1::Sha1::digest(bytes)),"sha512":hex(&Sha512::digest(bytes))},"downloads":["https://cdn.modrinth.com/example"],"fileSize":bytes.len()}]});
  super::super::parse_index(&serde_json::to_vec(&value).unwrap())
    .unwrap()
    .files
    .remove(0)
}
#[test]
fn requires_both_hashes_and_exact_size() {
  let root = tempfile::tempdir().unwrap();
  let path = root.path().join("file");
  fs::write(&path, b"hello").unwrap();
  let spec = identity(b"hello");
  assert_eq!(
    verify(&path, &spec, &CancellationToken::new()).unwrap(),
    hex(&Sha256::digest(b"hello"))
  );
  let mut bad = spec.clone();
  bad.hashes.insert("sha512".into(), "0".repeat(128));
  assert!(verify(&path, &bad, &CancellationToken::new()).is_err());
  let mut bad = spec.clone();
  bad.hashes.insert("sha1".into(), "0".repeat(40));
  assert!(verify(&path, &bad, &CancellationToken::new()).is_err());
  let mut bad = spec;
  bad.file_size += 1;
  assert!(verify(&path, &bad, &CancellationToken::new()).is_err());
  let token = CancellationToken::new();
  token.cancel();
  assert!(matches!(
    verify(&path, &identity(b"hello"), &token),
    Err(Error::Cancelled)
  ));
}
#[tokio::test]
async fn shared_object_is_reused_offline_and_corruption_is_rejected() {
  let root = tempfile::tempdir().unwrap();
  let context = Context::new(crate::Paths {
    data: root.path().into(),
    cache: root.path().join("cache"),
  })
  .unwrap();
  let spec = identity(b"hello");
  let hash = hex(&Sha256::digest(b"hello"));
  let path = object(root.path(), &hash).unwrap();
  fs::write(&path, b"hello").unwrap();
  storage::replace_json(
    &cache_index(root.path(), &spec).unwrap(),
    &Index {
      sha256: hash,
      size: 5,
    },
  )
  .unwrap();
  let (progress, _) = watch::channel(super::super::ImportProgress::default());
  let (cached, reused) = prepare(&context, &spec, &CancellationToken::new(), &progress)
    .await
    .unwrap();
  assert!(reused);
  assert_eq!(cached, path);
  fs::write(&path, b"wrong").unwrap();
  assert!(matches!(
    prepare(&context, &spec, &CancellationToken::new(), &progress).await,
    Err(Error::Conflict(_))
  ));
  assert_eq!(fs::read(path).unwrap(), b"wrong");
}
#[cfg(unix)]
#[test]
fn rejects_symlink_object_without_reading_target() {
  let root = tempfile::tempdir().unwrap();
  let target = root.path().join("original");
  fs::write(&target, b"hello").unwrap();
  let link = root.path().join("link");
  std::os::unix::fs::symlink(&target, &link).unwrap();
  assert!(verify(&link, &identity(b"hello"), &CancellationToken::new()).is_err());
  assert_eq!(fs::read(target).unwrap(), b"hello");
}
