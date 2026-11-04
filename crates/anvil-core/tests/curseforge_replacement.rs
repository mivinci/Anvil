use super::*;
use crate::{
  curseforge::{PreviewItem, Project},
  resources::shaders::tests::{fixture, zip},
};
use serde_json::{Value, json};

fn cached(manager: &ResourceManager, project: u32, id: u32, dependencies: Value) -> PreviewItem {
  let data = &manager.context.paths.data;
  let bytes = zip(&[("fixture.txt", id.to_string().as_bytes())]);
  let sha1 = hex(&sha1::Sha1::digest(&bytes));
  let sha256 = hex(&Sha256::digest(&bytes));
  publish_object(data, &sha256, &bytes).unwrap();
  fs::create_dir_all(data.join("resources/sha1")).unwrap();
  storage::replace_json(
    &data.join(format!("resources/sha1/{sha1}.json")),
    &json!({"sha256":sha256,"size":bytes.len()}),
  )
  .unwrap();
  let project_data: Project = serde_json::from_value(json!({
    "id": project, "gameId": 432, "classId": 6, "name": format!("Fixture {project}"),
    "summary": "", "authors": [], "links": {"websiteUrl": null}, "allowModDistribution": true
  }))
  .unwrap();
  PreviewItem {
    project: project_data,
    file: serde_json::from_value(json!({
      "id": id, "modId": project, "gameId": 432, "displayName": format!("Version {id}"),
      "fileName": format!("project-{project}.jar"), "fileLength": bytes.len(),
      "fileDate": "2026-01-01T00:00:00Z", "releaseType": 1, "isAvailable": true,
      "gameVersions": ["1.21.1", "Fabric"], "hashes": [{"algo": 1, "value": sha1}],
      "dependencies": dependencies, "downloadUrl": "https://edge.forgecdn.net/files/fixture.jar"
    }))
    .unwrap(),
    direct: true,
  }
}

fn preview(item: PreviewItem) -> DependencyPreview {
  DependencyPreview {
    total_bytes: item.file.file_length,
    items: vec![item],
    optional_projects: vec![],
    warnings: vec![],
    warning_details: vec![],
  }
}

async fn install(manager: &ResourceManager, plan: ResourceInstallPlan) -> Result<ResourceManifest> {
  let progress = watch::channel(ResourceInstallProgress::for_plan(&plan)).0;
  manager
    .install_curseforge(plan, CancellationToken::new(), progress)
    .await
}

#[tokio::test]
async fn explicit_curseforge_replacement_preserves_project_dependents_and_rejects_conflicts() {
  let (_root, manager, directory) = fixture();
  let path = directory.join("installed.json");
  let mut record: Value = storage::read_json(&path).unwrap();
  record["loader"] = json!({"kind":"fabric","version":"0.16.10"});
  storage::replace_json(&path, &record).unwrap();
  let old = cached(&manager, 1, 11, json!([]));
  let first = manager
    .plan_curseforge("test", preview(old), 0)
    .await
    .unwrap();
  let first = install(&manager, first).await.unwrap();
  let dependent = cached(&manager, 2, 21, json!([{"modId":1, "relationType":3}]));
  let dependent = manager
    .plan_curseforge("test", preview(dependent), first.revision)
    .await
    .unwrap();
  let before = install(&manager, dependent).await.unwrap();

  let incompatible = cached(&manager, 1, 12, json!([{"modId":2, "relationType":5}]));
  assert!(
    manager
      .plan_curseforge_with_replacement("test", preview(incompatible), before.revision, Some(11))
      .await
      .is_err()
  );
  let target = cached(&manager, 1, 13, json!([]));
  assert!(
    manager
      .plan_curseforge("test", preview(target.clone()), before.revision)
      .await
      .is_err()
  );
  let plan = manager
    .plan_curseforge_with_replacement("test", preview(target), before.revision, Some(11))
    .await
    .unwrap();
  assert_eq!(
    plan
      .replaced_entry()
      .unwrap()
      .origin
      .as_ref()
      .unwrap()
      .file_id,
    11
  );
  assert_eq!(manager.list("test").await.unwrap(), before);

  let after = install(&manager, plan).await.unwrap();
  assert_eq!(after.mods.len(), 2);
  assert!(after.mods.iter().any(|entry| {
    entry
      .origin
      .as_ref()
      .is_some_and(|origin| origin.file_id == 13)
  }));
  assert_eq!(
    fs::read(directory.join("game/mods/project-1.jar")).unwrap(),
    zip(&[("fixture.txt", b"13")])
  );
  assert_eq!(
    fs::read(directory.join("game/mods/project-2.jar")).unwrap(),
    zip(&[("fixture.txt", b"21")])
  );
}
