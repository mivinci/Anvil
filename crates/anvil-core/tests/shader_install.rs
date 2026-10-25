use super::*;
use crate::modrinth::{DependencyPreview, Links, ModFile, PreviewItem, Project};
use crate::resources::shaders::tests::{fixture, zip};
use serde_json::{Value, json};

fn fabric(directory: &Path) {
  let path = directory.join("installed.json");
  let mut record: Value = storage::read_json(&path).unwrap();
  record["loader"] = json!({"kind":"fabric","version":"0.16.10"});
  storage::replace_json(&path, &record).unwrap();
}
fn jar(id: &str, depends: Value) -> Vec<u8> {
  zip(&[(
    "fabric.mod.json",
    &serde_json::to_vec(&json!({
      "schemaVersion":1,"id":id,"version":"1.0.0","depends":depends
    }))
    .unwrap(),
  )])
}
// Seed verified download-cache objects, not instance references; no network is used.
fn cached(
  manager: &ResourceManager,
  project: &str,
  version: &str,
  filename: &str,
  bytes: &[u8],
  direct: bool,
) -> PreviewItem {
  let data = &manager.context.paths.data;
  let hash = hex(&Sha256::digest(bytes));
  let sha1 = hex(&sha1::Sha1::digest(bytes));
  publish_object(data, &hash, bytes).unwrap();
  fs::create_dir_all(data.join("resources/sha1")).unwrap();
  storage::replace_json(
    &data.join(format!("resources/sha1/{sha1}.json")),
    &json!({"sha256":hash,"size":bytes.len()}),
  )
  .unwrap();
  PreviewItem {
    project: Project {
      id: project.into(),
      name: project.into(),
      summary: String::new(),
      authors: vec![],
      links: Links {
        website_url: "https://modrinth.com/mod/iris".into(),
      },
      icon_url: None,
      download_count: None,
      updated_at: None,
    },
    file: ModFile {
      id: version.into(),
      mod_id: project.into(),
      display_name: version.into(),
      file_name: filename.into(),
      file_length: bytes.len() as u64,
      file_date: "2026-10-25T00:00:00Z".into(),
      release_type: 1,
      sha1,
      url: "https://cdn.modrinth.com/fixture.jar".into(),
    },
    direct,
    required: BTreeMap::new(),
    incompatible: BTreeMap::new(),
  }
}
fn preview(items: Vec<PreviewItem>) -> DependencyPreview {
  DependencyPreview {
    total_bytes: items.iter().map(|item| item.file.file_length).sum(),
    items,
    optional_projects: vec![],
    warnings: vec![],
  }
}
fn iris(manager: &ResourceManager, depends: Value) -> PreviewItem {
  cached(
    manager,
    IRIS_PROJECT_ID,
    "iris0001",
    "Iris.jar",
    &jar("iris", depends),
    true,
  )
}
fn progress(plan: &ModrinthInstallPlan) -> watch::Sender<ResourceInstallProgress> {
  watch::channel(ResourceInstallProgress {
    stage: ResourceInstallStage::Preparing,
    completed_files: 0,
    total_files: plan.files().len(),
    downloaded_bytes: 0,
    total_bytes: plan.total_bytes(),
    received_bytes: 0,
  })
  .0
}
async fn install(manager: &ResourceManager, items: Vec<PreviewItem>) -> Result<ResourceManifest> {
  let revision = manager.list("test").await?.revision;
  let plan = manager
    .plan_modrinth("test", preview(items), revision)
    .await?;
  let progress = progress(&plan);
  manager
    .install_modrinth(plan, CancellationToken::new(), progress)
    .await
}

#[tokio::test]
async fn shader_install_uses_actual_required_graph_and_preserves_shader_packs() {
  let (_root, manager, directory) = fixture();
  fabric(&directory);
  let shader = zip(&[("shaders/main.fsh", b"void main() {}")]);
  manager
    .import_shader_pack("test", "Shader.zip".into(), shader, 0)
    .await
    .unwrap();
  let mut root = iris(
    &manager,
    json!({"sodium":">=1.0.0", "minecraft":"1.21.1", "fabricloader":">=0.16.0", "java":">=21"}),
  );
  root.required.insert("sodium00".into(), "sodium01".into());
  let dependency = cached(
    &manager,
    "sodium00",
    "sodium01",
    "Graphics.jar",
    &jar("sodium", json!({})),
    false,
  );
  let plan = manager
    .plan_modrinth("test", preview(vec![root, dependency]), 1)
    .await
    .unwrap();
  assert_eq!(plan.download_bytes(), 0);
  assert!(plan.files().iter().all(|file| file.reuse));
  manager
    .install_modrinth(plan.clone(), CancellationToken::new(), progress(&plan))
    .await
    .unwrap();
  let snapshot = manager.shader_packs("test").await.unwrap();
  assert_eq!(snapshot.environment.status, ShaderEnvironmentStatus::Ready);
  assert_eq!(snapshot.packs.len(), 1);
  assert!(directory.join("game/mods/Iris.jar").is_file());
  assert!(directory.join("game/mods/Graphics.jar").is_file());
  assert!(directory.join("game/shaderpacks/Shader.zip").is_file());
}

#[tokio::test]
async fn shader_install_does_not_add_unrequired_sodium_or_reinstall_iris() {
  let (_root, manager, directory) = fixture();
  fabric(&directory);
  install(&manager, vec![iris(&manager, json!({}))])
    .await
    .unwrap();
  let before = manager.list("test").await.unwrap();
  assert_eq!(before.mods.len(), 1);
  assert!(
    manager
      .shader_packs("test")
      .await
      .unwrap()
      .environment
      .sodium
      .is_none()
  );
  assert!(
    install(&manager, vec![iris(&manager, json!({}))])
      .await
      .is_err()
  );
  assert_eq!(manager.list("test").await.unwrap(), before);
}

#[tokio::test]
async fn shader_install_rejects_actual_jar_conflicts_without_committing() {
  for (id, depends) in [
    ("iris", json!({"sodium":">=2.0.0"})),
    ("other", json!({})),
    ("iris", json!({"minecraft":"1.20.1"})),
    ("iris", json!({"java":">=22"})),
  ] {
    let (_root, manager, directory) = fixture();
    fabric(&directory);
    let before = manager.list("test").await.unwrap();
    let candidate = cached(
      &manager,
      IRIS_PROJECT_ID,
      "iris0001",
      "Iris.jar",
      &jar(id, depends),
      true,
    );
    assert!(install(&manager, vec![candidate]).await.is_err());
    assert_eq!(manager.list("test").await.unwrap(), before);
    assert!(!directory.join("game/mods/Iris.jar").exists());
  }
}

#[tokio::test]
async fn shader_install_rejects_local_duplicate_and_disabled_dependency() {
  let (_root, manager, directory) = fixture();
  fabric(&directory);
  let bytes = jar("sodium", json!({}));
  manager
    .import("test", "Local.jar".into(), bytes.clone(), 0)
    .await
    .unwrap();
  let mut root = iris(&manager, json!({"sodium":"*"}));
  root.required.insert("sodium00".into(), "sodium01".into());
  // Different archive bytes, same Fabric ID: the platform filename/hash checks cannot detect this.
  let mut different = bytes;
  different.extend_from_slice(b"different archive comment");
  let dependency = cached(
    &manager,
    "sodium00",
    "sodium01",
    "Graphics.jar",
    &different,
    false,
  );
  let before = manager.list("test").await.unwrap();
  assert!(install(&manager, vec![root, dependency]).await.is_err());
  assert_eq!(manager.list("test").await.unwrap(), before);

  let (_root, manager, directory) = fixture();
  fabric(&directory);
  let dependency = cached(
    &manager,
    "sodium00",
    "sodium01",
    "Graphics.jar",
    &jar("sodium", json!({})),
    true,
  );
  let installed = install(&manager, vec![dependency.clone()]).await.unwrap();
  manager
    .change(
      "test",
      installed.mods[0].sha256.clone(),
      installed.revision,
      Some(false),
    )
    .await
    .unwrap();
  let before = manager.list("test").await.unwrap();
  let mut dependency = dependency;
  dependency.direct = false;
  let mut root = iris(&manager, json!({"sodium":"*"}));
  root.required.insert("sodium00".into(), "sodium01".into());
  assert!(
    manager
      .plan_modrinth("test", preview(vec![root, dependency]), before.revision)
      .await
      .is_err()
  );
  assert_eq!(manager.list("test").await.unwrap(), before);
}

#[tokio::test]
async fn shader_install_rechecks_revision_and_cancellation_before_publish() {
  let (_root, manager, directory) = fixture();
  fabric(&directory);
  let plan = manager
    .plan_modrinth("test", preview(vec![iris(&manager, json!({}))]), 0)
    .await
    .unwrap();
  let token = CancellationToken::new();
  token.cancel();
  assert!(matches!(
    manager
      .install_modrinth(plan.clone(), token, progress(&plan))
      .await,
    Err(Error::Cancelled)
  ));
  manager
    .import_shader_pack(
      "test",
      "Shader.zip".into(),
      zip(&[("shaders/main.fsh", b"void main() {}")]),
      0,
    )
    .await
    .unwrap();
  let before = manager.list("test").await.unwrap();
  assert!(
    manager
      .install_modrinth(plan.clone(), CancellationToken::new(), progress(&plan))
      .await
      .is_err()
  );
  assert_eq!(manager.list("test").await.unwrap(), before);
  assert!(!directory.join("game/mods/Iris.jar").exists());
}

/// Opt-in: caller supplies already downloaded, official files; never touches user instances.
#[tokio::test]
#[ignore = "requires official Iris/Sodium JAR paths via ANVIL_SHADER_SMOKE_IRIS / ANVIL_SHADER_SMOKE_SODIUM"]
async fn shader_install_official_iris_and_sodium_in_isolated_instance() {
  let iris_path = std::env::var_os("ANVIL_SHADER_SMOKE_IRIS").expect("set official Iris JAR path");
  let sodium_path =
    std::env::var_os("ANVIL_SHADER_SMOKE_SODIUM").expect("set official Sodium JAR path");
  let iris_bytes = fs::read(iris_path).unwrap();
  let sodium_bytes = fs::read(sodium_path).unwrap();
  assert_eq!(
    hex(&sha1::Sha1::digest(&iris_bytes)),
    "4e0df4011503b4b36ba69877ea7247c1a47fea00"
  );
  assert_eq!(
    hex(&sha1::Sha1::digest(&sodium_bytes)),
    "7953f77ac52ca2b1c6a7e483553b2a345f1b2a9c"
  );
  let (_root, manager, directory) = fixture();
  fabric(&directory);
  let mut root = cached(
    &manager,
    IRIS_PROJECT_ID,
    "zsoi0dso",
    "Iris.jar",
    &iris_bytes,
    true,
  );
  root.required.insert("AANobbMI".into(), "26nVNc41".into());
  let sodium = cached(
    &manager,
    "AANobbMI",
    "26nVNc41",
    "Sodium.jar",
    &sodium_bytes,
    false,
  );
  install(&manager, vec![root, sodium]).await.unwrap();
  let environment = manager.shader_packs("test").await.unwrap().environment;
  assert_eq!(
    environment.status,
    ShaderEnvironmentStatus::Ready,
    "{environment:?}"
  );
  assert_eq!(environment.iris.unwrap().version, "1.8.8+mc1.21.1");
  assert_eq!(environment.sodium.unwrap().version, "0.6.9+mc1.21.1");
}

/// Opt-in network acceptance: actual Modrinth metadata and q2q-http downloads, empty cache.
#[tokio::test]
#[ignore = "live Modrinth/CDN download smoke; set ANVIL_SHADER_LIVE_SMOKE=1"]
async fn shader_install_live_modrinth_plan_download_and_commit() {
  assert_eq!(std::env::var("ANVIL_SHADER_LIVE_SMOKE").as_deref(), Ok("1"));
  let (_root, manager, directory) = fixture();
  fabric(&directory);
  let provider = crate::modrinth::Modrinth::new().unwrap();
  // This official 1.21.1 release pins its Sodium dependency in platform metadata.
  let preview = provider
    .preview("1.21.1", IRIS_PROJECT_ID, "A99vfwJc", &BTreeMap::new())
    .await
    .unwrap();
  assert_eq!(preview.items.len(), 2);
  assert!(
    preview
      .items
      .iter()
      .any(|item| item.direct && item.project.id == IRIS_PROJECT_ID && item.file.id == "A99vfwJc")
  );
  assert!(
    preview
      .items
      .iter()
      .any(|item| !item.direct && item.project.id == "AANobbMI" && item.file.id == "26nVNc41")
  );
  let plan = manager.plan_modrinth("test", preview, 0).await.unwrap();
  assert!(plan.files().iter().all(|file| !file.reuse));
  assert_eq!(plan.download_bytes(), plan.total_bytes());
  let progress = progress(&plan);
  let token = CancellationToken::new();
  let deadline_token = token.clone();
  let deadline = tokio::spawn(async move {
    tokio::time::sleep(std::time::Duration::from_secs(180)).await;
    deadline_token.cancel();
  });
  let result = manager
    .install_modrinth(plan, token, progress.clone())
    .await;
  deadline.abort();
  let installed = result.unwrap();
  assert_eq!(installed.mods.len(), 2);
  assert_eq!(installed.revision, 1);
  assert_eq!(progress.borrow().completed_files, 2);
  assert!(progress.borrow().received_bytes > 0);
  for entry in &installed.mods {
    assert!(entry.enabled);
    assert!(directory.join("game/mods").join(&entry.filename).is_file());
  }
  let environment = manager.shader_packs("test").await.unwrap().environment;
  assert_eq!(
    environment.status,
    ShaderEnvironmentStatus::Ready,
    "{environment:?}"
  );
  assert_eq!(environment.iris.unwrap().version, "1.8.7+mc1.21.1");
  assert_eq!(environment.sodium.unwrap().version, "0.6.9+mc1.21.1");
}
