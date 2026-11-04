use super::*;
use crate::resources::shaders::tests::{fixture, zip};

#[tokio::test]
async fn scans_disabled_mod_and_pack_references_without_writing_instances() {
  let (_root, manager, directory) = fixture();
  let bytes = zip(&[(
    "pack.mcmeta",
    br#"{"pack":{"pack_format":15,"description":"fixture"}}"#,
  )]);
  let hash = hex(&Sha256::digest(&bytes));
  let manifest = manager
    .import("test", "Fixture.jar".into(), bytes.clone(), 0)
    .await
    .unwrap();
  let manifest = manager
    .import_pack("test", "Fixture.zip".into(), bytes, manifest.revision)
    .await
    .unwrap();
  let before = fs::read(directory.join("resources.json")).unwrap();

  let second = directory.parent().unwrap().join("client-second");
  fs::create_dir(&second).unwrap();
  storage::write_json(
    &second.join("instance.json"),
    &ClientInstance {
      schema_version: 1,
      instance_type: "client".into(),
      id: "client-second".into(),
      name: "second".into(),
    },
  )
  .unwrap();
  storage::write_json(
    &second.join("resources.json"),
    &ResourceManifest {
      resource_packs: manifest.resource_packs.clone(),
      mods: Vec::new(),
      ..manifest.clone()
    },
  )
  .unwrap();

  let refs = manager.references(&hash).await.unwrap();
  assert_eq!(refs.len(), 3);
  assert_eq!(refs[0].instance_name, "second");
  assert_eq!(refs[0].kind, ResourceReferenceKind::ResourcePack);
  assert!(
    refs
      .iter()
      .any(|entry| entry.kind == ResourceReferenceKind::Mod && entry.enabled == Some(false))
  );
  assert_eq!(fs::read(directory.join("resources.json")).unwrap(), before);
  assert!(
    manager
      .references(&"0".repeat(64))
      .await
      .unwrap()
      .is_empty()
  );
  assert!(manager.references("../invalid").await.is_err());

  fs::write(second.join("resource-transaction.json"), b"pending").unwrap();
  assert!(manager.references(&hash).await.is_err());
  assert!(second.join("resource-transaction.json").exists());
}
