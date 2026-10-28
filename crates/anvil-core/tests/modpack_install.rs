use super::*;
use crate::client::{ClientInstallRecord, ClientInstalledFile};
use std::io::Cursor;

fn packed(name: &str) -> Vec<u8> {
  let index = serde_json::json!({"formatVersion":1,"game":"minecraft","versionId":"1.0","name":name,"files":[],"dependencies":{"minecraft":"1.21.1","fabric-loader":"0.16.0"}});
  let jar = super::super::tests::archive(&[(
    "fabric.mod.json",
    br#"{"schemaVersion":1,"id":"example","version":"1.0"}"#,
  )]);
  super::super::tests::archive(&[
    ("modrinth.index.json", &serde_json::to_vec(&index).unwrap()),
    ("overrides/config/example.json", b"common"),
    ("client-overrides/config/example.json", b"client"),
    ("overrides/mods/example.jar", &jar),
  ])
}
fn fixture() -> (tempfile::TempDir, Context) {
  let root = tempfile::tempdir().unwrap();
  fs::create_dir(root.path().join("instances")).unwrap();
  let context = Context::new(crate::Paths {
    data: root.path().to_owned(),
    cache: root.path().join("cache"),
  })
  .unwrap();
  (root, context)
}
fn staged(data: &Path, bytes: &[u8]) -> (Staging, ArchivePlan) {
  let root = tempfile::Builder::new()
    .prefix(".import-")
    .tempdir_in(data.join("instances"))
    .unwrap();
  fs::write(root.path().join("source.mrpack"), bytes).unwrap();
  fs::create_dir(root.path().join("game")).unwrap();
  let plan = inspect_archive(Cursor::new(bytes), &CancellationToken::new()).unwrap();
  (reserve(data, root, &plan).unwrap(), plan)
}
fn record() -> ClientInstallRecord {
  ClientInstallRecord {
    loader: Some(crate::fabric::ClientLoader::Fabric {
      version: "0.16.0".into(),
    }),
    schema_version: 1,
    version_id: "1.21.1".into(),
    version_type: "release".into(),
    metadata_url: "https://piston-meta.mojang.com/v1/packages/example/1.21.1.json".into(),
    metadata_sha1: "1".repeat(40),
    java_component: "java-runtime-gamma".into(),
    java_major: 21,
    os: "linux".into(),
    architecture: "x86_64".into(),
    asset_index_id: "1.21.1".into(),
    main_class: "net.minecraft.client.main.Main".into(),
    arguments: crate::metadata::ClientArguments::default(),
    files: ["versions/1.21.1/1.21.1.jar", "assets/indexes/1.21.1.json"]
      .iter()
      .map(|path| ClientInstalledFile {
        path: (*path).into(),
        sha1: "1".repeat(40),
        size: 1,
        cache_path: format!("clients/sha1/{}", "1".repeat(40)),
      })
      .collect(),
  }
}
#[tokio::test]
async fn publishes_complete_unicode_instance_and_managed_resources_atomically() {
  let (root, context) = fixture();
  let (staging, plan) = staged(root.path(), &packed("海岛生存"));
  let id = staging.instance.id.clone();
  let name = staging.instance.name.clone();
  assert!(
    ClientInstaller::new(context.clone())
      .list()
      .await
      .unwrap()
      .is_empty()
  );
  let (progress, _) = watch::channel(ImportProgress::default());
  publish_staged(
    staging,
    root.path(),
    &plan.archive_sha256,
    &record(),
    &CancellationToken::new(),
    &progress,
  )
  .unwrap();
  let installer = ClientInstaller::new(context.clone());
  let list = installer.list().await.unwrap();
  assert_eq!(list.len(), 1);
  assert_eq!(list[0].name, "海岛生存");
  let directory = root.path().join("instances").join(id);
  assert_eq!(
    fs::read(directory.join("game/config/example.json")).unwrap(),
    b"client"
  );
  assert!(!directory.join("source.mrpack").exists());
  assert!(directory.join("modpack.json").is_file());
  let resources = crate::resources::ResourceManager::new(context.clone())
    .list(&name)
    .await
    .unwrap();
  assert_eq!(resources.mods.len(), 1);
  assert!(resources.mods[0].enabled);
  let original = directory.join("game/mods/example.jar");
  let content = fs::read(&original).unwrap();
  let hash = &resources.mods[0].sha256;
  let shared = root
    .path()
    .join("resources/objects/sha256")
    .join(&hash[..2])
    .join(hash);
  assert_eq!(fs::read(&shared).unwrap(), content);
  fs::write(original, b"modified instance copy").unwrap();
  assert_eq!(fs::read(shared).unwrap(), content);
}
#[test]
fn names_are_reserved_case_insensitively_and_ascii_ids_stay_stable() {
  let (root, _) = fixture();
  let (one, _) = staged(root.path(), &packed("Pack"));
  let (two, _) = staged(root.path(), &packed("pack"));
  assert_eq!(one.instance.id, "client-pack");
  assert_eq!(two.instance.name, "pack-2");
  let (unicode, _) = staged(root.path(), &packed("海岛生存"));
  let (other, _) = staged(root.path(), &packed("海岛生存"));
  assert_eq!(unicode.instance.name, "海岛生存");
  assert_eq!(other.instance.name, "海岛生存-2");
}
#[test]
fn changed_archive_and_cancelled_import_do_not_publish() {
  for cancelled in [false, true] {
    let (root, _) = fixture();
    let (staging, plan) = staged(root.path(), &packed("Pack"));
    let hidden = staging.root.path().to_owned();
    let id = staging.instance.id.clone();
    let token = CancellationToken::new();
    if cancelled {
      token.cancel();
    } else {
      fs::write(&staging.archive, packed("Another")).unwrap();
    }
    let (progress, _) = watch::channel(ImportProgress::default());
    assert!(
      publish_staged(
        staging,
        root.path(),
        &plan.archive_sha256,
        &record(),
        &token,
        &progress
      )
      .is_err()
    );
    assert!(!root.path().join("instances").join(id).exists());
    assert!(!hidden.exists());
  }
}
#[test]
fn existing_instance_is_never_replaced() {
  let (root, _) = fixture();
  let (staging, plan) = staged(root.path(), &packed("Pack"));
  let final_path = root.path().join("instances").join(&staging.instance.id);
  fs::create_dir(&final_path).unwrap();
  fs::write(final_path.join("keep"), b"existing").unwrap();
  let (progress, _) = watch::channel(ImportProgress::default());
  assert!(
    publish_staged(
      staging,
      root.path(),
      &plan.archive_sha256,
      &record(),
      &CancellationToken::new(),
      &progress
    )
    .is_err()
  );
  assert_eq!(fs::read(final_path.join("keep")).unwrap(), b"existing");
}
