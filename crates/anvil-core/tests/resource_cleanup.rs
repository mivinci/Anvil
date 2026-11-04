use super::*;
use crate::resources::shaders::tests::{fixture, zip};

#[tokio::test]
async fn rechecks_references_and_only_reclaims_unreferenced_objects_and_indexes() {
  let (_root, manager, directory) = fixture();
  let data = &manager.context.paths.data;
  let bytes = zip(&[("fixture.txt", b"fixture")]);
  let hash = hex(&Sha256::digest(&bytes));
  publish_object(data, &hash, &bytes).unwrap();
  for (kind, length) in [("sha1", 40), ("mrpack", 128)] {
    let root = data.join("resources").join(kind);
    fs::create_dir(&root).unwrap();
    storage::write_json(
      &root.join(format!("{}.json", "a".repeat(length))),
      &serde_json::json!({"sha256": hash, "size": bytes.len()}),
    )
    .unwrap();
  }
  let preview = manager.preview_cleanup().await.unwrap();
  assert_eq!((preview.objects, preview.bytes), (1, bytes.len() as u64));
  assert!(object_path(data, &hash).is_file());

  manager
    .import("test", "Fixture.jar".into(), bytes, 0)
    .await
    .unwrap();
  assert!(matches!(
    manager.confirm_cleanup(&preview.revision).await,
    Err(Error::Conflict(_))
  ));
  assert!(object_path(data, &hash).is_file());

  let manifest = read_manifest(&directory).unwrap();
  let record = directory.join("resources.json");
  fs::remove_file(&record).unwrap();
  storage::write_json(
    &directory.join("resource-layout.json"),
    &transaction::Layout {
      mods: manifest
        .mods
        .into_iter()
        .map(|mut entry| {
          entry.enabled = true;
          entry
        })
        .collect(),
      ..Default::default()
    },
  )
  .unwrap();
  assert_eq!(manager.preview_cleanup().await.unwrap().objects, 0);
  fs::remove_file(directory.join("resource-layout.json")).unwrap();

  let operation = manager.lock_resource_operation().await.unwrap();
  let other_operation = manager.lock_resource_operation().await.unwrap();
  assert!(matches!(
    manager.confirm_cleanup(&preview.revision).await,
    Err(Error::Busy(_))
  ));
  drop((operation, other_operation));
  fs::write(directory.join("resource-transaction.json"), b"pending").unwrap();
  assert!(manager.confirm_cleanup(&preview.revision).await.is_err());
  fs::remove_file(directory.join("resource-transaction.json")).unwrap();

  assert_eq!(
    manager.confirm_cleanup(&preview.revision).await.unwrap(),
    preview
  );
  assert!(!object_path(data, &hash).exists());
  for kind in ["sha1", "mrpack"] {
    assert!(
      fs::read_dir(data.join("resources").join(kind))
        .unwrap()
        .next()
        .is_none()
    );
  }
}
