use super::*;
use crate::resources::shaders::tests::{fixture, zip};
use serde_json::json;

fn fabric(dir: &Path) {
  let path = dir.join("installed.json");
  let mut record: Value = storage::read_json(&path).unwrap();
  record["loader"] = json!({"kind": "fabric", "version": "0.16.10"});
  storage::replace_json(&path, &record).unwrap();
}
async fn add(manager: &ResourceManager, filename: &str, meta: Value, enabled: bool) {
  let revision = manager.list("test").await.unwrap().revision;
  let bytes = zip(&[("fabric.mod.json", &serde_json::to_vec(&meta).unwrap())]);
  let manifest = manager
    .import("test", filename.into(), bytes, revision)
    .await
    .unwrap();
  let entry = manifest
    .mods
    .iter()
    .find(|entry| entry.filename == filename)
    .unwrap();
  if enabled {
    manager
      .change("test", entry.sha256.clone(), manifest.revision, Some(true))
      .await
      .unwrap();
  }
}
fn meta(id: &str, version: &str) -> Value {
  json!({"schemaVersion": 1, "id": id, "version": version})
}
fn iris() -> Value {
  json!({"schemaVersion": 1, "id": "iris", "version": "1.8.8+1.21.1",
    "environment": "client", "depends": {"sodium": ">=0.6.0 <0.7.0", "java": ">=21", "minecraft": "~1.21.1", "fabricloader": ">=0.16.0"}})
}

#[tokio::test]
async fn shader_environment_distinguishes_vanilla_missing_and_disabled_iris() {
  let (_root, manager, dir) = fixture();
  let snapshot = manager.shader_packs("test").await.unwrap();
  assert_eq!(
    snapshot.environment.status,
    ShaderEnvironmentStatus::RequiresFabric
  );
  fabric(&dir);
  assert_eq!(
    manager
      .shader_packs("test")
      .await
      .unwrap()
      .environment
      .status,
    ShaderEnvironmentStatus::MissingIris
  );
  add(&manager, "unrelated-name.jar", iris(), false).await;
  let environment = manager.shader_packs("test").await.unwrap().environment;
  assert_eq!(environment.status, ShaderEnvironmentStatus::IrisDisabled);
  assert!(!environment.iris.unwrap().enabled);
}

#[tokio::test]
async fn shader_environment_reads_real_ids_versions_and_dependency_closure() {
  let (_root, manager, dir) = fixture();
  fabric(&dir);
  add(&manager, "NotIris.jar", iris(), true).await;
  add(
    &manager,
    "Graphics.jar",
    meta("sodium", "0.6.13+mc1.21.1"),
    true,
  )
  .await;
  let environment = manager.shader_packs("test").await.unwrap().environment;
  assert_eq!(
    environment.status,
    ShaderEnvironmentStatus::Ready,
    "{environment:?}"
  );
  assert_eq!(environment.iris.unwrap().version, "1.8.8+1.21.1");
  assert_eq!(environment.sodium.unwrap().version, "0.6.13+mc1.21.1");
  assert!(environment.managed_only);
  // Reverse incompatibilities must also prevent a ready result.
  let mut incompatible = meta("another", "1.0.0");
  incompatible["breaks"] = json!({"iris": "*"});
  add(&manager, "Other.jar", incompatible, true).await;
  assert_eq!(
    manager
      .shader_packs("test")
      .await
      .unwrap()
      .environment
      .status,
    ShaderEnvironmentStatus::Unconfirmed
  );
}

#[tokio::test]
async fn shader_environment_refuses_disabled_or_wrong_dependency_versions() {
  let (_root, manager, dir) = fixture();
  fabric(&dir);
  add(&manager, "Iris.jar", iris(), true).await;
  add(&manager, "Sodium.jar", meta("sodium", "0.5.0"), false).await;
  assert_eq!(
    manager
      .shader_packs("test")
      .await
      .unwrap()
      .environment
      .status,
    ShaderEnvironmentStatus::MissingDependencies
  );
  let manifest = manager.list("test").await.unwrap();
  let sodium = manifest
    .mods
    .iter()
    .find(|entry| entry.filename == "Sodium.jar")
    .unwrap();
  manager
    .change("test", sodium.sha256.clone(), manifest.revision, Some(true))
    .await
    .unwrap();
  let result = manager.shader_packs("test").await.unwrap().environment;
  assert_eq!(result.status, ShaderEnvironmentStatus::MissingDependencies);
  assert!(result.reason.contains("版本不匹配"));
}

#[tokio::test]
async fn shader_environment_does_not_guess_from_filenames_or_unreadable_metadata() {
  let (_root, manager, dir) = fixture();
  fabric(&dir);
  add(&manager, "iris.jar", meta("ordinary", "1.0.0"), true).await;
  assert_eq!(
    manager
      .shader_packs("test")
      .await
      .unwrap()
      .environment
      .status,
    ShaderEnvironmentStatus::MissingIris
  );
  let revision = manager.list("test").await.unwrap().revision;
  manager
    .import(
      "test",
      "Broken.jar".into(),
      zip(&[("fabric.mod.json", b"invalid json")]),
      revision,
    )
    .await
    .unwrap();
  assert_eq!(
    manager
      .shader_packs("test")
      .await
      .unwrap()
      .environment
      .status,
    ShaderEnvironmentStatus::Unconfirmed
  );
}

#[tokio::test]
async fn shader_environment_flags_duplicate_ids_nested_jars_and_modified_copies() {
  for failure in ["duplicate", "nested", "modified"] {
    let (_root, manager, dir) = fixture();
    fabric(&dir);
    add(&manager, "Iris.jar", iris(), true).await;
    add(&manager, "Sodium.jar", meta("sodium", "0.6.13"), true).await;
    match failure {
      "duplicate" => add(&manager, "Second.jar", meta("iris", "1.8.7"), true).await,
      "nested" => {
        let mut nested = meta("nested", "1.0.0");
        nested["jars"] = json!([{"file": "META-INF/jars/child.jar"}]);
        add(&manager, "Nested.jar", nested, true).await;
      }
      _ => fs::write(dir.join("game/mods/Iris.jar"), b"changed").unwrap(),
    }
    let snapshot = manager.shader_packs("test").await.unwrap();
    assert_eq!(
      snapshot.environment.status,
      ShaderEnvironmentStatus::Unconfirmed,
      "{failure}"
    );
    assert!(snapshot.environment.iris.is_some());
  }
}

#[tokio::test]
async fn shader_environment_scan_budget_does_not_hide_shader_references() {
  let (_root, manager, dir) = fixture();
  fabric(&dir);
  let mut manifest = manager
    .import_shader_pack(
      "test",
      "Light.zip".into(),
      zip(&[("shaders/main.fsh", b"void main() {}")]),
      0,
    )
    .await
    .unwrap();
  manifest.mods = (0..3)
    .map(|index| ResourceEntry {
      origin: None,
      modrinth_origin: None,
      source: ResourceSource::Local,
      sha256: format!("{index:064x}"),
      filename: format!("Large{index}.jar"),
      size: MAX_IMPORT_BYTES as u64,
      enabled: false,
    })
    .collect();
  storage::replace_json(&dir.join("resources.json"), &manifest).unwrap();
  let snapshot = manager.shader_packs("test").await.unwrap();
  assert_eq!(
    snapshot.environment.status,
    ShaderEnvironmentStatus::Unconfirmed
  );
  assert_eq!(snapshot.revision, manifest.revision);
  assert_eq!(snapshot.packs, manifest.shader_packs);
  assert_eq!(snapshot.packs.len(), 1);
  assert!(snapshot.environment.reason.contains("128 MiB"));
}

#[test]
fn shader_environment_version_predicates_follow_supported_fabric_semantics() {
  for (version, requirement, expected) in [
    ("1.2.9", json!("1.2"), Some(false)),
    ("1.9.0", json!("~1"), Some(true)),
    ("0.0.7", json!("^0.0"), Some(true)),
    ("1.2.0", json!("1.2"), Some(true)),
    ("0.6.13+mc1.21.1", json!(">=0.6 <0.7"), Some(true)),
    ("0.7.0", json!(">=0.6 <0.7"), Some(false)),
    ("1.21.1", json!(["1.20.1", "~1.21.1"]), Some(true)),
    ("21", json!(">=21"), Some(true)),
    ("1.21.4", json!("1.21.x"), Some(true)),
    ("anything", json!("*"), Some(true)),
    ("1.2.3.4", json!(">=1.2"), None),
    ("1.2.3-rc.1", json!(">=1.2-"), None),
    ("1.2.3", json!(null), None),
    ("1.2.3", json!(["*", null]), None),
  ] {
    assert_eq!(
      matches_requirement(version, &requirement),
      expected,
      "{version} {requirement}"
    );
  }
}

#[tokio::test]
async fn shader_environment_follows_declared_dependencies_without_forcing_sodium() {
  let (_root, manager, dir) = fixture();
  fabric(&dir);
  add(&manager, "Iris.jar", meta("iris", "1.0.0"), true).await;
  let result = manager.shader_packs("test").await.unwrap().environment;
  assert_eq!(result.status, ShaderEnvironmentStatus::Ready);
  assert!(result.sodium.is_none());
}

fn bundle(id: &str, depends: Value, children: &[(&str, &[u8])]) -> Vec<u8> {
  let metadata = serde_json::to_vec(
    &json!({"schemaVersion":1,"id":id,"version":"1.0.0","depends":depends,
    "jars":children.iter().map(|(file, _)|json!({"file":file})).collect::<Vec<_>>() }),
  )
  .unwrap();
  let mut entries = vec![("fabric.mod.json", metadata.as_slice())];
  entries.extend_from_slice(children);
  zip(&entries)
}
async fn import_enabled(manager: &ResourceManager, filename: &str, bytes: Vec<u8>) {
  let revision = manager.list("test").await.unwrap().revision;
  let imported = manager
    .import("test", filename.into(), bytes, revision)
    .await
    .unwrap();
  let hash = imported
    .mods
    .iter()
    .find(|entry| entry.filename == filename)
    .unwrap()
    .sha256
    .clone();
  manager
    .change("test", hash, imported.revision, Some(true))
    .await
    .unwrap();
}

#[tokio::test]
async fn shader_nested_dependencies_are_checked_and_identical_bundles_coalesce() {
  for conflicting in [false, true] {
    let (_root, manager, directory) = fixture();
    fabric(&directory);
    let library = bundle("library", json!({"java":">=21"}), &[]);
    let second = if conflicting {
      bundle("library", json!({"java":">=22"}), &[])
    } else {
      // Same named contents with a different ZIP archive comment must still coalesce.
      let mut encoded = library.clone();
      let end = encoded.len();
      encoded[end - 2..].copy_from_slice(&5_u16.to_le_bytes());
      encoded.extend_from_slice(b"other");
      assert_ne!(encoded, library);
      encoded
    };
    let iris = bundle(
      "iris",
      json!({"library":"*"}),
      &[("META-INF/jars/lib.jar", &library)],
    );
    import_enabled(&manager, "Iris.jar", iris).await;
    import_enabled(
      &manager,
      "Another.jar",
      bundle("another", json!({}), &[("META-INF/jars/lib.jar", &second)]),
    )
    .await;
    let environment = manager.shader_packs("test").await.unwrap().environment;
    assert_eq!(
      environment.status,
      if conflicting {
        ShaderEnvironmentStatus::Unconfirmed
      } else {
        ShaderEnvironmentStatus::Ready
      },
      "{environment:?}"
    );
  }
  // Nested mods load even if the parent does not list them in its own depends.
  let (_root, manager, directory) = fixture();
  fabric(&directory);
  let missing = bundle("library", json!({"missing":"*"}), &[]);
  import_enabled(
    &manager,
    "Iris.jar",
    bundle("iris", json!({}), &[("META-INF/jars/lib.jar", &missing)]),
  )
  .await;
  assert_eq!(
    manager
      .shader_packs("test")
      .await
      .unwrap()
      .environment
      .status,
    ShaderEnvironmentStatus::MissingDependencies
  );
}

#[tokio::test]
async fn shader_nested_scan_rejects_unsafe_missing_and_duplicate_paths_and_limits() {
  let child = bundle("library", json!({}), &[]);
  for declarations in [
    json!([{"file":"../child.jar"}]),
    json!([{"file":"missing.jar"}]),
    json!([{"file":"child.jar"},{"file":"CHILD.jar"}]),
    json!([{"file":"child.jar","extra":true}]),
  ] {
    let (_root, manager, directory) = fixture();
    fabric(&directory);
    let meta = serde_json::to_vec(
      &json!({"schemaVersion":1,"id":"iris","version":"1.0.0","jars":declarations}),
    )
    .unwrap();
    import_enabled(
      &manager,
      "Iris.jar",
      zip(&[
        ("fabric.mod.json", &meta),
        ("../child.jar", &child),
        ("child.jar", &child),
      ]),
    )
    .await;
    assert_eq!(
      manager
        .shader_packs("test")
        .await
        .unwrap()
        .environment
        .status,
      ShaderEnvironmentStatus::Unconfirmed
    );
  }
  let mut deep = child.clone();
  for level in 0..5 {
    deep = bundle(
      &format!("level_{level}"),
      json!({}),
      &[("child.jar", &deep)],
    );
  }
  let (_root, manager, directory) = fixture();
  fabric(&directory);
  import_enabled(
    &manager,
    "Iris.jar",
    bundle("iris", json!({}), &[("child.jar", &deep)]),
  )
  .await;
  assert_eq!(
    manager
      .shader_packs("test")
      .await
      .unwrap()
      .environment
      .status,
    ShaderEnvironmentStatus::Unconfirmed
  );
  let archive = bundle("iris", json!({}), &[("child.jar", &child)]);
  for (bytes, nested) in [(MAX_SCAN_BYTES, 0), (0, MAX_NESTED_JARS)] {
    let mut budget = ScanBudget {
      bytes,
      expanded: 0,
      nested,
    };
    assert!(scan_jar(&archive, true, 0, &mut budget, &mut Vec::new()).is_err());
  }
}

#[tokio::test]
async fn shader_aliases_resolve_dependencies_but_reject_ambiguous_owners() {
  for conflict in ["none", "real_id", "other_alias", "reserved"] {
    let (_root, manager, directory) = fixture();
    fabric(&directory);
    let mut provider = meta("provider", "1.0.0");
    provider["provides"] = if conflict == "reserved" {
      json!(["java"])
    } else {
      json!(["legacy"])
    };
    add(&manager, "Provider.jar", provider, true).await;
    let mut iris = meta("iris", "1.0.0");
    iris["depends"] = json!({"legacy":">=1"});
    add(&manager, "Iris.jar", iris, true).await;
    if conflict == "real_id" {
      add(&manager, "Other.jar", meta("legacy", "1.0.0"), true).await;
    }
    if conflict == "other_alias" {
      let mut other = meta("other", "1.0.0");
      other["provides"] = json!(["legacy"]);
      add(&manager, "Other.jar", other, true).await;
    }
    let environment = manager.shader_packs("test").await.unwrap().environment;
    assert_eq!(
      environment.status,
      if conflict == "none" {
        ShaderEnvironmentStatus::Ready
      } else {
        ShaderEnvironmentStatus::Unconfirmed
      },
      "{conflict}: {environment:?}"
    );
  }
}

#[test]
fn shader_stable_versions_compare_to_prerelease_bounds_conservatively() {
  for (version, requirement, expected) in [
    ("26.3", "~26.3-", Some(true)),
    ("26.2", "~26.3-", Some(false)),
    ("1.21.1", ">=1.15-alpha.19.39.a", Some(true)),
    ("1.15", ">1.15-alpha.19.39.a", Some(true)),
    ("1.14.4", ">=1.15-alpha.19.39.a", Some(false)),
    ("1.15", "<=1.15-alpha.19.39.a", Some(false)),
    ("1.14.4", "<1.15-alpha.19.39.a", Some(true)),
    ("1.21.1", ">=24w14a", None),
    ("1.21.1", ">=1.15-alpha..1", None),
  ] {
    assert_eq!(
      matches_requirement(version, &json!(requirement)),
      expected,
      "{version} {requirement}"
    );
  }
}

#[tokio::test]
async fn shader_alias_reverse_conflicts_cover_provider_and_virtual_ids() {
  for (depends, breaks) in [("provider", "legacy"), ("legacy", "provider")] {
    let (_root, manager, directory) = fixture();
    fabric(&directory);
    let mut provider = meta("provider", "1.0.0");
    provider["provides"] = json!(["legacy"]);
    add(&manager, "Provider.jar", provider, true).await;
    let mut iris = meta("iris", "1.0.0");
    iris["depends"] = json!({depends:"*"});
    add(&manager, "Iris.jar", iris, true).await;
    let mut other = meta("other", "1.0.0");
    other["breaks"] = json!({breaks:"*"});
    add(&manager, "Other.jar", other, true).await;
    assert_eq!(
      manager
        .shader_packs("test")
        .await
        .unwrap()
        .environment
        .status,
      ShaderEnvironmentStatus::Unconfirmed
    );
  }
}

#[tokio::test]
async fn shader_environment_coalesces_root_and_nested_identical_payloads() {
  for identical in [true, false] {
    let (_root, manager, directory) = fixture();
    fabric(&directory);
    let placeholder = bundle("placeholder-api", json!({"java": ">=21"}), &[]);
    let bundled = if identical {
      placeholder.clone()
    } else {
      bundle("placeholder-api", json!({"java": ">=22"}), &[])
    };
    import_enabled(&manager, "placeholder-api.jar", placeholder).await;
    import_enabled(
      &manager,
      "modmenu.jar",
      bundle(
        "modmenu",
        json!({}),
        &[("META-INF/jars/placeholder-api.jar", &bundled)],
      ),
    )
    .await;
    import_enabled(&manager, "iris.jar", bundle("iris", json!({}), &[])).await;
    import_enabled(&manager, "sodium.jar", bundle("sodium", json!({}), &[])).await;
    let environment = manager.shader_packs("test").await.unwrap().environment;
    assert_eq!(
      environment.status,
      if identical {
        ShaderEnvironmentStatus::Ready
      } else {
        ShaderEnvironmentStatus::Unconfirmed
      },
      "identical={identical}: {environment:?}"
    );
    if !identical {
      assert!(environment.reason.contains("placeholder-api"));
    }
  }
}

#[tokio::test]
async fn shader_environment_coalesces_nested_copies_with_the_same_contract() {
  let (_root, manager, directory) = fixture();
  fabric(&directory);
  let metadata = serde_json::to_vec(
    &json!({"schemaVersion": 1, "id": "library", "version": "1.0.0",
      "depends": {"java": ">=21"}}),
  )
  .unwrap();
  let first = zip(&[
    ("fabric.mod.json", metadata.as_slice()),
    ("first.txt", b"first"),
  ]);
  let second = zip(&[
    ("fabric.mod.json", metadata.as_slice()),
    ("second.txt", b"second"),
  ]);
  import_enabled(
    &manager,
    "iris.jar",
    bundle("iris", json!({}), &[("META-INF/jars/library.jar", &first)]),
  )
  .await;
  import_enabled(
    &manager,
    "another.jar",
    bundle(
      "another",
      json!({}),
      &[("META-INF/jars/library.jar", &second)],
    ),
  )
  .await;
  import_enabled(&manager, "sodium.jar", bundle("sodium", json!({}), &[])).await;
  let environment = manager.shader_packs("test").await.unwrap().environment;
  assert_eq!(
    environment.status,
    ShaderEnvironmentStatus::Ready,
    "{environment:?}"
  );
}

#[tokio::test]
async fn shader_environment_lets_fabric_resolve_nested_version_differences() {
  let (_root, manager, directory) = fixture();
  fabric(&directory);
  let first = bundle("library", json!({"java": ">=21"}), &[]);
  let second = {
    let metadata = serde_json::to_vec(
      &json!({"schemaVersion": 1, "id": "library", "version": "2.0.0",
        "depends": {"java": ">=22"}}),
    )
    .unwrap();
    zip(&[
      ("fabric.mod.json", metadata.as_slice()),
      ("second.txt", b"second"),
    ])
  };
  import_enabled(
    &manager,
    "iris.jar",
    bundle("iris", json!({}), &[("META-INF/jars/library.jar", &first)]),
  )
  .await;
  import_enabled(
    &manager,
    "another.jar",
    bundle(
      "another",
      json!({}),
      &[("META-INF/jars/library.jar", &second)],
    ),
  )
  .await;
  import_enabled(&manager, "sodium.jar", bundle("sodium", json!({}), &[])).await;
  let environment = manager.shader_packs("test").await.unwrap().environment;
  assert_eq!(
    environment.status,
    ShaderEnvironmentStatus::Ready,
    "{environment:?}"
  );
}
