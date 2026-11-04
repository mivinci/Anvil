use super::*;
use serde_json::{Value, json};

fn version(id: &str, project: &str, dependencies: Value) -> Value {
  json!({
    "id": id, "project_id": project, "name": "Fixture",
    "date_published": "2026-01-01T00:00:00Z", "version_type": "release",
    "status": "listed", "game_versions": ["1.21.1"], "loaders": ["fabric"],
    "files": [{"url": "https://cdn.modrinth.com/fixture.jar", "filename": format!("{id}.jar"),
      "primary": true, "size": 10, "hashes": {"sha1": "a".repeat(40)}}],
    "dependencies": dependencies
  })
}

#[tokio::test]
async fn optional_selection_keeps_pinned_version_and_required_children() {
  let mut provider = Modrinth::new().unwrap();
  let mut fixture = BTreeMap::from([
    (
      "version/root0001".into(),
      version(
        "root0001",
        "rootproj",
        json!([
          {"project_id": "optproj1", "version_id": "optfile1", "dependency_type": "optional"}
        ]),
      ),
    ),
    (
      "version/optfile1".into(),
      version(
        "optfile1",
        "optproj1",
        json!([
          {"version_id": "reqfile1", "dependency_type": "required"}
        ]),
      ),
    ),
    (
      "version/reqfile1".into(),
      version("reqfile1", "reqproj1", json!([])),
    ),
  ]);
  for id in ["rootproj", "optproj1", "reqproj1"] {
    fixture.insert(format!("project/{id}"), json!({
      "id": id, "title": id, "description": "Fixture", "project_type": "mod", "client_side": "required"
    }));
  }
  provider.fixture = Some(fixture);
  let empty = BTreeMap::new();
  let default = provider
    .preview("1.21.1", "rootproj", "root0001", &empty)
    .await
    .unwrap();
  assert_eq!(default.items.len(), 1);
  assert_eq!(default.optional_projects, ["optproj1"]);
  let chosen = vec!["optproj1".into()];
  let selected = provider
    .preview_with_optional("1.21.1", "rootproj", "root0001", &empty, &chosen)
    .await
    .unwrap();
  assert_eq!(selected.total_bytes, 30);
  assert_eq!(selected.optional_projects, chosen);
  let root = selected.items.iter().find(|item| item.direct).unwrap();
  assert!(root.required.is_empty());
  let optional = selected
    .items
    .iter()
    .find(|item| item.project.id == "optproj1")
    .unwrap();
  assert_eq!(optional.file.id, "optfile1");
  assert_eq!(
    optional.required.get("reqproj1").map(String::as_str),
    Some("reqfile1")
  );
  let arbitrary = provider
    .preview_with_optional(
      "1.21.1",
      "rootproj",
      "root0001",
      &empty,
      &["reqproj1".into()],
    )
    .await;
  assert!(matches!(arbitrary, Err(Error::Invalid(_))));
  let conflicting = BTreeMap::from([("optproj1".into(), "existing".into())]);
  assert!(matches!(
    provider
      .preview_with_optional("1.21.1", "rootproj", "root0001", &conflicting, &chosen)
      .await,
    Err(Error::Conflict(_))
  ));
}
