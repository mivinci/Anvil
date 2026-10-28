use super::*;
use serde_json::json;
use std::io::Cursor;

fn fixture() -> (tempfile::TempDir, Context, String) {
  let root = tempfile::tempdir().unwrap();
  let context = Context::new(crate::Paths {
    data: root.path().join("data"),
    cache: root.path().join("cache"),
  })
  .unwrap();
  let name = "海岛生存";
  let id = crate::client::client_instance_id(name).unwrap();
  let directory = context.paths.data.join("instances").join(id);
  fs::create_dir_all(directory.join("game")).unwrap();
  storage::write_json(&directory.join("instance.json"),&json!({"schema_version":1,"instance_type":"client","id":crate::client::client_instance_id(name).unwrap(),"name":name})).unwrap();
  storage::write_json(&directory.join("installed.json"),&json!({"schema_version":1,"version_id":"1.21.1","version_type":"release","metadata_url":"https://piston-meta.mojang.com/v1/packages/example/1.21.1.json","metadata_sha1":"1".repeat(40),"java_component":"java-runtime-gamma","java_major":21,"os":"linux","architecture":"x86_64","asset_index_id":"1.21.1","main_class":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]},"loader":{"kind":"fabric","version":"0.16.0"},"files":[{"path":"versions/1.21.1/1.21.1.jar","sha1":"1".repeat(40),"size":1,"cache_path":format!("clients/sha1/{}","1".repeat(40))},{"path":"assets/indexes/1.21.1.json","sha1":"1".repeat(40),"size":1,"cache_path":format!("clients/sha1/{}","1".repeat(40))}]})).unwrap();
  (root, context, name.into())
}
fn game(context: &Context, name: &str) -> PathBuf {
  context
    .paths
    .data
    .join("instances")
    .join(crate::client::client_instance_id(name).unwrap())
    .join("game")
}
fn options() -> ExportOptions {
  ExportOptions {
    name: "海岛生存".into(),
    version_id: "1.0.0".into(),
    selection: ExportSelection::default(),
  }
}
fn add(game: &Path, path: &str, bytes: &[u8]) {
  let file = game.join(path);
  fs::create_dir_all(file.parent().unwrap()).unwrap();
  fs::write(file, bytes).unwrap();
}
fn jar() -> Vec<u8> {
  let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
  writer
    .start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
    .unwrap();
  writer
    .write_all(br#"{"schemaVersion":1,"id":"example","version":"1"}"#)
    .unwrap();
  writer.finish().unwrap().into_inner()
}
fn progress() -> watch::Sender<ExportProgress> {
  watch::channel(ExportProgress::default()).0
}
fn entries(path: &Path) -> BTreeMap<String, Vec<u8>> {
  let mut archive = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
  let mut files = BTreeMap::new();
  for index in 0..archive.len() {
    let mut entry = archive.by_index(index).unwrap();
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).unwrap();
    files.insert(entry.name().into(), bytes);
  }
  files
}

#[tokio::test]
async fn exports_selected_files_and_optional_worlds_as_reimportable_pack() {
  let (root, context, name) = fixture();
  let game = game(&context, &name);
  let jar = jar();
  let textures = crate::modpack::tests::archive(&[(
    "pack.mcmeta",
    br#"{"pack":{"pack_format":34,"description":"Fixture"}}"#,
  )]);
  let shaders =
    crate::modpack::tests::archive(&[("shaders/composite.fsh", b"#version 150\nvoid main(){}")]);
  for (path, bytes) in [
    ("config/example.json", b"config".as_slice()),
    ("options.txt", b"options"),
    ("saves/World/level.dat", b"world"),
    ("logs/latest.log", b"private log"),
    ("screenshots/example.png", b"private image"),
    ("config/accounts.json", b"private auth"),
    ("kubejs/cache/example", b"transient cache"),
    ("scripts/.git/config", b"private repository metadata"),
    ("mods/example.jar", jar.as_slice()),
    ("resourcepacks/example.zip", textures.as_slice()),
    ("shaderpacks/example.zip", shaders.as_slice()),
  ] {
    add(&game, path, bytes);
  }
  let exporter = ModpackExporter::new(context.clone());
  let output = root.path().join("pack.mrpack");
  let result = exporter
    .export(
      &name,
      options(),
      output.clone(),
      &CancellationToken::new(),
      &progress(),
    )
    .await
    .unwrap();
  let files = entries(&output);
  assert_eq!(result.files, 5);
  assert_eq!(files["overrides/config/example.json"], b"config");
  assert_eq!(files["overrides/mods/example.jar"], jar);
  assert!(!files.keys().any(|path| path.contains("saves")
    || path.contains("logs")
    || path.contains("screenshots")
    || path.contains("accounts")));
  let plan = inspect_archive(File::open(&output).unwrap(), &CancellationToken::new()).unwrap();
  assert_eq!(plan.archive_sha256, result.sha256);
  assert_eq!(plan.archive_bytes, result.size);
  assert_eq!(plan.index.fabric_loader.as_deref(), Some("0.16.0"));
  assert_eq!(plan.overrides.len(), 5);
  let mut with_world = options();
  with_world.selection.worlds = true;
  let other = root.path().join("world.mrpack");
  exporter
    .export(
      &name,
      with_world,
      other.clone(),
      &CancellationToken::new(),
      &progress(),
    )
    .await
    .unwrap();
  assert_eq!(entries(&other)["overrides/saves/World/level.dat"], b"world");
  assert_eq!(
    fs::read(game.join("config/example.json")).unwrap(),
    b"config"
  );
  assert_eq!(
    fs::read(game.join("config/accounts.json")).unwrap(),
    b"private auth"
  );
}
#[tokio::test]
async fn disabled_groups_are_not_included_and_outputs_are_never_replaced() {
  let (root, context, name) = fixture();
  let game = game(&context, &name);
  add(&game, "config/a", b"config");
  add(&game, "mods/example.jar", &jar());
  let mut selected = options();
  selected.selection.mods = false;
  let output = root.path().join("config.mrpack");
  let exporter = ModpackExporter::new(context);
  exporter
    .export(
      &name,
      selected,
      output.clone(),
      &CancellationToken::new(),
      &progress(),
    )
    .await
    .unwrap();
  assert_eq!(entries(&output).len(), 2);
  let bytes = fs::read(&output).unwrap();
  assert!(
    exporter
      .export(
        &name,
        options(),
        output.clone(),
        &CancellationToken::new(),
        &progress()
      )
      .await
      .is_err()
  );
  assert_eq!(fs::read(output).unwrap(), bytes);
}
#[tokio::test]
async fn instance_and_settings_locks_exclude_export_and_source_output_is_rejected() {
  let (root, context, name) = fixture();
  let id = crate::client::client_instance_id(&name).unwrap();
  let exporter = ModpackExporter::new(context.clone());
  for settings in [false, true] {
    let lock = if settings {
      storage::client_settings_lock(&context.paths.data, &id)
    } else {
      storage::server_instance_lock(&context.paths.data, &id)
    }
    .unwrap();
    let output = root.path().join("locked.mrpack");
    assert!(matches!(
      exporter
        .export(
          &name,
          options(),
          output.clone(),
          &CancellationToken::new(),
          &progress()
        )
        .await,
      Err(Error::Busy(_))
    ));
    assert!(!output.exists());
    drop(lock);
  }
  let output = game(&context, &name).join("pack.mrpack");
  assert!(
    exporter
      .export(
        &name,
        options(),
        output.clone(),
        &CancellationToken::new(),
        &progress()
      )
      .await
      .is_err()
  );
  assert!(!output.exists());
}
#[tokio::test]
async fn source_changes_and_incomplete_resource_transactions_are_not_exported() {
  let (root, context, name) = fixture();
  let game = game(&context, &name);
  add(&game, "config/a", b"before");
  let cancel = CancellationToken::new();
  let snapshot = collect(&game, &options().selection, &cancel).unwrap();
  add(&game, "config/a", b"after!");
  let mut temp = tempfile::tempfile().unwrap();
  assert!(pack(&mut temp, b"{}", &game, &snapshot, &cancel, &progress()).is_err());
  add(&game, "config/new", b"new");
  assert_ne!(
    collect(&game, &options().selection, &cancel).unwrap(),
    snapshot
  );
  fs::write(
    game.parent().unwrap().join("resource-transaction.json"),
    b"pending",
  )
  .unwrap();
  let output = root.path().join("pending.mrpack");
  assert!(
    ModpackExporter::new(context)
      .export(&name, options(), output.clone(), &cancel, &progress())
      .await
      .is_err()
  );
  assert!(!output.exists());
}
#[tokio::test]
async fn rejects_limits_bad_payloads_and_cancellation_without_leaving_outputs() {
  let (root, context, name) = fixture();
  let game = game(&context, &name);
  add(&game, "mods/broken.jar", b"not a jar");
  let exporter = ModpackExporter::new(context.clone());
  let output = root.path().join("rejected.mrpack");
  assert!(
    exporter
      .export(
        &name,
        options(),
        output.clone(),
        &CancellationToken::new(),
        &progress()
      )
      .await
      .is_err()
  );
  assert!(!output.exists());
  fs::remove_file(game.join("mods/broken.jar")).unwrap();
  let large = game.join("config/large");
  fs::create_dir_all(large.parent().unwrap()).unwrap();
  File::create(&large)
    .unwrap()
    .set_len(archive::MAX_ENTRY_BYTES + 1)
    .unwrap();
  assert!(
    exporter
      .export(
        &name,
        options(),
        output.clone(),
        &CancellationToken::new(),
        &progress()
      )
      .await
      .is_err()
  );
  fs::remove_file(large).unwrap();
  let cancel = CancellationToken::new();
  cancel.cancel();
  assert!(matches!(
    exporter
      .export(&name, options(), output.clone(), &cancel, &progress())
      .await,
    Err(Error::Cancelled)
  ));
  assert!(!output.exists());
  assert!(!fs::read_dir(root.path()).unwrap().any(|entry| {
    entry
      .unwrap()
      .file_name()
      .to_string_lossy()
      .starts_with(".anvil-export-")
  }));
  let mut file = tempfile::tempfile().unwrap();
  let mut bounded = LimitedFile {
    file: &mut file,
    position: archive::MAX_ARCHIVE_BYTES - 1,
  };
  assert!(bounded.write_all(b"xx").is_err());
}
#[cfg(unix)]
#[tokio::test]
async fn rejects_symlinks_and_case_conflicts_in_selected_tree() {
  let (root, context, name) = fixture();
  let game = game(&context, &name);
  fs::create_dir(game.join("config")).unwrap();
  let external = root.path().join("private");
  fs::write(&external, b"private").unwrap();
  std::os::unix::fs::symlink(&external, game.join("config/link")).unwrap();
  let output = root.path().join("bad.mrpack");
  let exporter = ModpackExporter::new(context.clone());
  assert!(
    exporter
      .export(
        &name,
        options(),
        output.clone(),
        &CancellationToken::new(),
        &progress()
      )
      .await
      .is_err()
  );
  assert!(!output.exists());
  assert_eq!(fs::read(external).unwrap(), b"private");
  fs::remove_file(game.join("config/link")).unwrap();
  add(&game, "config/Options", b"one");
  add(&game, "config/options", b"two");
  // Case-sensitive filesystems can create both entries; case-insensitive ones
  // naturally expose only one file and are already covered by portable paths.
  if fs::read_dir(game.join("config")).unwrap().count() == 2 {
    assert!(
      exporter
        .export(
          &name,
          options(),
          output,
          &CancellationToken::new(),
          &progress()
        )
        .await
        .is_err()
    );
  }
}

#[tokio::test]
async fn changed_managed_resource_is_rejected_without_repairing_source() {
  let (root, context, name) = fixture();
  let manager = crate::resources::ResourceManager::new(context.clone());
  let imported = manager
    .import(&name, "example.jar".into(), jar(), 0)
    .await
    .unwrap();
  manager
    .change(
      &name,
      imported.mods[0].sha256.clone(),
      imported.revision,
      Some(true),
    )
    .await
    .unwrap();
  let file = game(&context, &name).join("mods/example.jar");
  let original = fs::read(&file).unwrap();
  let exporter = ModpackExporter::new(context.clone());
  exporter
    .export(
      &name,
      options(),
      root.path().join("good.mrpack"),
      &CancellationToken::new(),
      &progress(),
    )
    .await
    .unwrap();
  let mut modified = original;
  modified.push(0);
  fs::write(&file, &modified).unwrap();
  let output = root.path().join("changed.mrpack");
  assert!(
    exporter
      .export(
        &name,
        options(),
        output.clone(),
        &CancellationToken::new(),
        &progress()
      )
      .await
      .is_err()
  );
  assert!(!output.exists());
  assert_eq!(fs::read(file).unwrap(), modified);
}

#[tokio::test]
async fn cancellation_during_packing_cleans_private_output_and_keeps_source() {
  let (root, context, name) = fixture();
  let path = game(&context, &name).join("config/large.dat");
  let mut seed = 123456789u32;
  let bytes = (0..8 * 1024 * 1024)
    .map(|_| {
      seed ^= seed << 13;
      seed ^= seed >> 17;
      seed ^= seed << 5;
      seed as u8
    })
    .collect::<Vec<_>>();
  add(&game(&context, &name), "config/large.dat", &bytes);
  let before = hex(&Sha256::digest(&bytes));
  let output = root.path().join("cancelled.mrpack");
  let exporter = ModpackExporter::new(context);
  let token = CancellationToken::new();
  let (progress, mut updates) = watch::channel(ExportProgress::default());
  let work = exporter.export(&name, options(), output.clone(), &token, &progress);
  tokio::pin!(work);
  let result=tokio::time::timeout(std::time::Duration::from_secs(10),async {loop {tokio::select! {
    result=&mut work=>break result,
    changed=updates.changed()=>{changed.unwrap();let state=updates.borrow_and_update().clone();if state.stage==ExportStage::Packing && state.processed_bytes>0 {token.cancel();}}
  }}}).await.unwrap();
  assert!(matches!(result, Err(Error::Cancelled)));
  assert!(!output.exists());
  assert_eq!(hex(&Sha256::digest(fs::read(path).unwrap())), before);
  assert!(!fs::read_dir(root.path()).unwrap().any(|entry| {
    entry
      .unwrap()
      .file_name()
      .to_string_lossy()
      .starts_with(".anvil-export-")
  }));
}
