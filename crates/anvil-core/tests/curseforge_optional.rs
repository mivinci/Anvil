use super::*;
use serde_json::{Value, json};

fn file(project: u32, id: u32, dependencies: Value) -> Value {
  json!({
    "id": id, "modId": project, "gameId": 432, "displayName": "Fixture", "fileName": format!("{id}.jar"),
    "fileLength": 10, "fileDate": "2026-01-01T00:00:00Z", "releaseType": 1, "isAvailable": true,
    "gameVersions": ["1.21.1", "Fabric"], "hashes": [{"algo": 1, "value": "a".repeat(40)}],
    "dependencies": dependencies, "downloadUrl": "https://edge.forgecdn.net/files/fixture.jar"
  })
}

#[tokio::test]
async fn optional_selection_includes_required_children_and_rejects_arbitrary_ids() {
  let mut provider = CurseForge::new(None).unwrap();
  let mut fixture = BTreeMap::from([
    (
      "mods/1/files/11".into(),
      json!({"data": file(1, 11, json!([{"modId": 2, "relationType": 2}]))}),
    ),
    (
      "mods/2/files/22".into(),
      json!({"data": file(2, 22, json!([{"modId": 3, "relationType": 3}]))}),
    ),
    (
      "mods/3/files/33".into(),
      json!({"data": file(3, 33, json!([]))}),
    ),
  ]);
  for id in [1, 2, 3] {
    fixture.insert(
      format!("mods/{id}"),
      json!({"data": {
        "id": id, "gameId": 432, "classId": 6, "name": format!("Fixture {id}"), "summary": "",
        "authors": [], "links": {"websiteUrl": null}, "allowModDistribution": true
      }}),
    );
  }
  provider.fixture = Some(fixture);
  let existing = BTreeMap::from([(2, 22), (3, 33)]);
  let default = provider
    .preview_with_existing("1.21.1", 1, 11, &existing)
    .await
    .unwrap();
  assert_eq!(default.items.len(), 1);
  assert_eq!(default.optional_projects, [2]);
  let selected = provider
    .preview_with_optional("1.21.1", 1, 11, &existing, &[2])
    .await
    .unwrap();
  assert_eq!(selected.total_bytes, 30);
  assert_eq!(selected.optional_projects, [2]);
  assert_eq!(
    selected
      .items
      .iter()
      .map(|item| item.file.id)
      .collect::<Vec<_>>(),
    [11, 22, 33]
  );
  assert!(matches!(
    provider
      .preview_with_optional("1.21.1", 1, 11, &existing, &[3])
      .await,
    Err(Error::Invalid(_))
  ));
}
