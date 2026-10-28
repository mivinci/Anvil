use super::*;
use serde_json::json;

fn record(fabric: bool) -> ClientInstallRecord {
  serde_json::from_value(json!({
    "schema_version":1,"version_id":"1.21.1","version_type":"release",
    "metadata_url":"https://piston-meta.mojang.com/v1/packages/example/1.21.1.json",
    "metadata_sha1":"1".repeat(40),"java_component":"java-runtime-gamma","java_major":21,
    "os":"linux","architecture":"x86_64","asset_index_id":"1.21.1",
    "main_class":"net.minecraft.client.main.Main","arguments":{"game":[],"jvm":[]},
    "loader":if fabric {json!({"kind":"fabric","version":"0.16.0"})} else {json!(null)},
    "files":[
      {"path":"versions/1.21.1/1.21.1.jar","sha1":"1".repeat(40),"size":1,"cache_path":format!("clients/sha1/{}","1".repeat(40))},
      {"path":"assets/indexes/1.21.1.json","sha1":"1".repeat(40),"size":1,"cache_path":format!("clients/sha1/{}","1".repeat(40))}
    ]
  })).unwrap()
}
fn options() -> ExportOptions {
  serde_json::from_value(json!({"name":"海岛生存","versionId":"1.0.0"})).unwrap()
}

#[test]
fn default_selection_excludes_worlds_and_rejects_unknown_fields() {
  let selected = options().selection;
  assert!(selected.mods && selected.resource_packs && selected.shader_packs && selected.config);
  assert!(!selected.worlds);
  assert!(
    serde_json::from_value::<ExportOptions>(
      json!({"name":"Pack","versionId":"1","destination":"/arbitrary"})
    )
    .is_err()
  );
  let partial: ExportSelection = serde_json::from_value(json!({"worlds":true})).unwrap();
  assert!(partial.worlds && partial.mods);
}
#[test]
fn vanilla_and_fabric_metadata_round_trip_through_import_parser() {
  for fabric in [false, true] {
    let index = prepare_export_index(&record(fabric), &options()).unwrap();
    let json = serde_json::to_vec(&index).unwrap();
    let imported = crate::modpack::parse_index(&json).unwrap();
    assert_eq!(imported.name, "海岛生存");
    assert_eq!(imported.version_id, "1.0.0");
    assert_eq!(imported.minecraft, "1.21.1");
    assert_eq!(
      imported.fabric_loader.as_deref(),
      fabric.then_some("0.16.0")
    );
    assert!(imported.files.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(value["formatVersion"], 1);
    assert_eq!(value["files"], json!([]));
    assert!(value.get("selection").is_none());
    assert!(value.get("metadata_url").is_none());
  }
}
#[test]
fn rejects_bad_identity_empty_selection_and_invalid_install_snapshot() {
  for (name, version) in [("", "1"), ("Pack\n", "1"), ("Pack", ""), ("Pack", "1\n")] {
    let mut value = options();
    value.name = name.into();
    value.version_id = version.into();
    assert!(value.validate().is_err());
  }
  let mut value = options();
  value.selection = ExportSelection {
    mods: false,
    resource_packs: false,
    shader_packs: false,
    config: false,
    worlds: false,
  };
  assert!(value.validate().is_err());
  let mut invalid = record(false);
  invalid.files.clear();
  assert!(prepare_export_index(&invalid, &options()).is_err());
}
