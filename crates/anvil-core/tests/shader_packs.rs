use super::*;
use serde_json::json;
use std::io::Cursor;

pub(in crate::resources) fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
  let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
  for (name, content) in entries {
    writer
      .start_file(*name, zip::write::SimpleFileOptions::default())
      .unwrap();
    writer.write_all(content).unwrap();
  }
  writer.finish().unwrap().into_inner()
}
fn shader() -> Vec<u8> {
  zip(&[("shaders/composite.fsh", b"#version 150\nvoid main() {}")])
}
pub(in crate::resources) fn fixture() -> (tempfile::TempDir, ResourceManager, PathBuf) {
  let root = tempfile::tempdir().unwrap();
  let context = Context::new(crate::Paths {
    data: root.path().join("data"),
    cache: root.path().join("cache"),
  })
  .unwrap();
  let directory = instance(&context, "test");
  (root, ResourceManager::new(context), directory)
}
fn instance(context: &Context, name: &str) -> PathBuf {
  let directory = context.paths.data.join(format!("instances/client-{name}"));
  fs::create_dir_all(directory.join("game")).unwrap();
  storage::write_json(
    &directory.join("instance.json"),
    &ClientInstance {
      schema_version: 1,
      instance_type: "client".into(),
      id: format!("client-{name}"),
      name: name.into(),
    },
  )
  .unwrap();
  let sha = "a".repeat(40);
  storage::write_json(&directory.join("installed.json"), &json!({
    "schema_version": 1, "version_id": "1.21.1", "version_type": "release",
    "metadata_url": "https://piston-meta.mojang.com/fixture.json", "metadata_sha1": sha,
    "java_component": "java-runtime-delta", "java_major": 21, "os": "macos",
    "architecture": "aarch64", "asset_index_id": "17", "main_class": "net.minecraft.client.main.Main",
    "arguments": {"game": [], "jvm": []},
    "files": (["versions/1.21.1/1.21.1.jar", "assets/indexes/17.json"].map(|path| json!({
      "path": path, "sha1": sha, "size": 0, "cache_path": format!("clients/sha1/{sha}")
    })))
  })).unwrap();
  directory
}

#[test]
fn shader_zip_requires_root_source_and_rejects_ambiguous_paths() {
  inspect_zip(&shader()).unwrap();
  for entries in [
    vec![("pack.mcmeta", b"{}".as_slice())],
    vec![("outer/shaders/main.fsh", b"void main() {}".as_slice())],
    vec![("shaders/main.fsh", b" \n".as_slice())],
    vec![("shaders/../main.fsh", b"source".as_slice())],
    vec![("/shaders/main.fsh", b"source".as_slice())],
    vec![("shaders\\main.fsh", b"source".as_slice())],
    vec![
      ("shaders/main.fsh", b"source".as_slice()),
      ("SHADERS/MAIN.FSH", b"other"),
    ],
    vec![
      ("shaders", b"file".as_slice()),
      ("shaders/main.fsh", b"source"),
    ],
    vec![
      ("shaders/main.fsh", b"source".as_slice()),
      ("shaders", b"file"),
    ],
  ] {
    assert!(inspect_zip(&zip(&entries)).is_err(), "{entries:?}");
  }
  let huge = vec![b'a'; MAX_SOURCE_BYTES as usize + 1];
  assert!(inspect_zip(&zip(&[("shaders/main.glsl", &huge)])).is_err());
  let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
  writer
    .add_symlink(
      "shaders/main.fsh",
      "../external",
      zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
  assert!(inspect_zip(&writer.finish().unwrap().into_inner()).is_err());
}

#[tokio::test]
async fn shader_old_manifests_are_read_without_upgrade() {
  let (_root, manager, dir) = fixture();
  for schema in 1..=4 {
    let old = json!({"schemaVersion": schema, "revision": 0, "mods": []});
    storage::replace_json(&dir.join("resources.json"), &old).unwrap();
    let before = fs::read(dir.join("resources.json")).unwrap();
    let result = manager.list("test").await.unwrap();
    assert!(result.shader_packs.is_empty());
    assert_eq!(result.schema_version, schema);
    assert_eq!(before, fs::read(dir.join("resources.json")).unwrap());
  }
}

#[tokio::test]
async fn shader_import_reuses_objects_and_isolates_instance_files() {
  let (_root, manager, dir) = fixture();
  let other = instance(&manager.context, "other");
  let bytes = shader();
  let first = manager
    .import_shader_pack("test", "Light.zip".into(), bytes.clone(), 0)
    .await
    .unwrap();
  assert_eq!(first.schema_version, 5);
  fs::remove_file(dir.join("game/shaderpacks/Light.zip")).unwrap();
  transaction::sync_for_launch(
    &dir,
    &manager.context.paths.data,
    false,
    &crate::CancellationToken::new(),
  )
  .unwrap();
  assert_eq!(
    fs::read(dir.join("game/shaderpacks/Light.zip")).unwrap(),
    bytes
  );

  let hash = first.shader_packs[0].sha256.clone();
  let object = object_path(&manager.context.paths.data, &hash);
  assert_eq!(
    manager
      .import_shader_pack(
        "test",
        "Duplicate.zip".into(),
        bytes.clone(),
        first.revision
      )
      .await
      .unwrap(),
    first
  );
  manager
    .import_shader_pack("other", "Other.zip".into(), bytes.clone(), 0)
    .await
    .unwrap();
  fs::write(dir.join("game/shaderpacks/Light.zip"), b"external edit").unwrap();
  assert_eq!(fs::read(&object).unwrap(), bytes);
  assert_eq!(
    fs::read(other.join("game/shaderpacks/Other.zip")).unwrap(),
    bytes
  );
  assert!(matches!(
    manager
      .remove_shader_pack("test", hash.clone(), first.revision)
      .await,
    Err(Error::Conflict(_))
  ));
  fs::write(dir.join("game/shaderpacks/Light.zip"), &bytes).unwrap();
  let removed = manager
    .remove_shader_pack("test", hash, first.revision)
    .await
    .unwrap();
  assert!(removed.shader_packs.is_empty());
  assert!(!dir.join("game/shaderpacks/Light.zip").exists());
  assert!(object.exists() && other.join("game/shaderpacks/Other.zip").exists());
}

#[tokio::test]
async fn shader_import_refuses_stale_revision_external_files_and_running_instance() {
  let (_root, manager, dir) = fixture();
  fs::create_dir(dir.join("game/shaderpacks")).unwrap();
  fs::write(dir.join("game/shaderpacks/Light.zip"), b"external").unwrap();
  assert!(matches!(
    manager
      .import_shader_pack("test", "Light.zip".into(), shader(), 0)
      .await,
    Err(Error::Conflict(_))
  ));
  assert_eq!(
    fs::read(dir.join("game/shaderpacks/Light.zip")).unwrap(),
    b"external"
  );
  assert!(manager.list("test").await.unwrap().shader_packs.is_empty());
  fs::remove_file(dir.join("game/shaderpacks/Light.zip")).unwrap();
  let state = manager
    .import_shader_pack("test", "Light.zip".into(), shader(), 0)
    .await
    .unwrap();
  assert!(matches!(
    manager
      .remove_shader_pack("test", state.shader_packs[0].sha256.clone(), 0)
      .await,
    Err(Error::Conflict(_))
  ));
  let different = zip(&[("shaders/main.fsh", b"different")]);
  assert!(matches!(
    manager
      .import_shader_pack("test", "light.zip".into(), different, state.revision)
      .await,
    Err(Error::Conflict(_))
  ));
  let _lock = storage::server_instance_lock(&manager.context.paths.data, "client-test").unwrap();
  assert!(matches!(
    manager
      .remove_shader_pack("test", state.shader_packs[0].sha256.clone(), state.revision)
      .await,
    Err(Error::Busy(_))
  ));
}

#[tokio::test]
async fn shader_operations_preserve_mods_and_resource_packs_and_repair_on_launch() {
  let (_root, manager, dir) = fixture();
  let mods = manager
    .import("test", "Local.jar".into(), zip(&[("fixture", b"mod")]), 0)
    .await
    .unwrap();
  let packs = manager
    .import_pack(
      "test",
      "Textures.zip".into(),
      zip(&[(
        "pack.mcmeta",
        br#"{"pack":{"pack_format":34,"description":"fixture"}}"#,
      )]),
      mods.revision,
    )
    .await
    .unwrap();
  let shaders = manager
    .import_shader_pack("test", "Light.zip".into(), shader(), packs.revision)
    .await
    .unwrap();
  assert_eq!(shaders.mods, packs.mods);
  assert_eq!(shaders.resource_packs, packs.resource_packs);
  let mod_changed = manager
    .change("test", mods.mods[0].sha256.clone(), shaders.revision, None)
    .await
    .unwrap();
  assert_eq!(mod_changed.shader_packs, shaders.shader_packs);
  fs::remove_file(dir.join("game/shaderpacks/Light.zip")).unwrap();
  transaction::sync_for_launch(
    &dir,
    &manager.context.paths.data,
    false,
    &crate::CancellationToken::new(),
  )
  .unwrap();
  assert_eq!(
    fs::read(dir.join("game/shaderpacks/Light.zip")).unwrap(),
    shader()
  );
  assert_eq!(manager.list("test").await.unwrap(), mod_changed);
  let removed = manager
    .remove_pack(
      "test",
      packs.resource_packs[0].sha256.clone(),
      mod_changed.revision,
    )
    .await
    .unwrap();
  assert_eq!(removed.shader_packs, shaders.shader_packs);
}

#[tokio::test]
async fn shader_pending_removal_rolls_back_from_backup() {
  let (_root, manager, dir) = fixture();
  let before = manager
    .import_shader_pack("test", "Light.zip".into(), shader(), 0)
    .await
    .unwrap();
  let old_layout: serde_json::Value =
    storage::read_json(&dir.join("resource-layout.json")).unwrap();
  let mut after = before.clone();
  after.shader_packs.clear();
  after.revision += 1;
  after.applied_revision = Some(after.revision);
  fs::create_dir_all(dir.join(".anvil-resource-work/backup")).unwrap();
  fs::create_dir(dir.join(".anvil-resource-work/new")).unwrap();
  fs::rename(
    dir.join("game/shaderpacks/Light.zip"),
    dir.join(".anvil-resource-work/backup/0"),
  )
  .unwrap();
  storage::write_json(&dir.join("resource-transaction.json"), &json!({
    "schema_version": 3, "before": before, "after": after,
    "old_layout": old_layout, "new_layout": {"mods": []}, "committed": false,
    "changes": [{"kind": "shader_pack", "filename": "Light.zip", "before": before.shader_packs[0], "after": null}]
  })).unwrap();
  assert_eq!(manager.list("test").await.unwrap(), before);
  assert_eq!(
    fs::read(dir.join("game/shaderpacks/Light.zip")).unwrap(),
    shader()
  );
  assert!(!dir.join("resource-transaction.json").exists());
  assert!(!dir.join(".anvil-resource-work").exists());
}

#[tokio::test]
async fn shader_recovery_preserves_legacy_journals() {
  let (_root, manager, dir) = fixture();
  for schema in [1, 2] {
    let manifest = ResourceManifest::default();
    let mut after = manifest.clone();
    after.revision = 1;
    after.applied_revision = Some(1);
    storage::write_json(
      &dir.join("resource-transaction.json"),
      &json!({
        "schema_version": schema, "before": manifest, "after": after,
        "old_layout": {"mods": []}, "new_layout": {"mods": []}, "changes": []
      }),
    )
    .unwrap();
    assert_eq!(manager.list("test").await.unwrap(), manifest);
    assert!(!dir.join("resource-transaction.json").exists());
  }
}

#[test]
fn shader_counts_toward_shared_resource_limit_and_requires_schema_five() {
  let entry = ShaderPackEntry {
    source: ResourceSource::Local,
    sha256: "a".repeat(64),
    filename: "Light.zip".into(),
    size: 1,
  };
  let mut manifest = ResourceManifest {
    schema_version: 4,
    shader_packs: vec![entry.clone()],
    ..ResourceManifest::default()
  };
  assert!(validate_manifest(&manifest).is_err());
  manifest.schema_version = 5;
  validate_manifest(&manifest).unwrap();
  manifest.mods = (0..MAX_ENTRIES)
    .map(|index| ResourceEntry {
      origin: None,
      modrinth_origin: None,
      source: ResourceSource::Local,
      sha256: format!("{index:064x}"),
      filename: format!("Mod{index}.jar"),
      size: 1,
      enabled: false,
    })
    .collect();
  assert!(validate_manifest(&manifest).is_err());
  manifest.mods.pop();
  validate_manifest(&manifest).unwrap();
}
