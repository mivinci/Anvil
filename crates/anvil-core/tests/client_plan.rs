use super::*;
use crate::metadata::{ClientArguments, ClientAssetIndex, ClientLogging};

const OFFICIAL_VERSION: &str = include_str!("fixtures/1.21.1.json");
const OFFICIAL_ASSET_INDEX: &[u8] = include_bytes!("fixtures/1.21.1-assets-17.json");
const VERSION_SHA1: &str = "1757c8b1fbfc6b638504e1f6cceb05893f7389d5";
const ASSET_INDEX_SHA1: &str = "e61ea955de95de1f99d36616a8b46147e9f6f5d7";

fn official_version() -> ClientVersion {
  let document: Value = serde_json::from_str(OFFICIAL_VERSION).unwrap();
  let client = &document["downloads"]["client"];
  let asset_index = &document["assetIndex"];
  let logging = &document["logging"]["client"];

  ClientVersion {
    version_id: document["id"].as_str().unwrap().to_owned(),
    version_type: document["type"].as_str().unwrap().to_owned(),
    metadata_url: Url::parse(
      "https://piston-meta.mojang.com/v1/packages/1757c8b1fbfc6b638504e1f6cceb05893f7389d5/1.21.1.json",
    )
    .unwrap(),
    metadata_sha1: VERSION_SHA1.to_owned(),
    java_component: document["javaVersion"]["component"]
      .as_str()
      .unwrap()
      .to_owned(),
    java_major: document["javaVersion"]["majorVersion"].as_u64().unwrap() as u32,
    client_jar: client_file(client),
    asset_index: ClientAssetIndex {
      id: asset_index["id"].as_str().unwrap().to_owned(),
      file: client_file(asset_index),
    },
    main_class: document["mainClass"].as_str().unwrap().to_owned(),
    logging: Some(ClientLogging {
      kind: logging["type"].as_str().unwrap().to_owned(),
      argument: logging["argument"].as_str().unwrap().to_owned(),
      file_id: logging["file"]["id"].as_str().unwrap().to_owned(),
      file: client_file(&logging["file"]),
    }),
    libraries: document["libraries"].as_array().unwrap().clone(),
    arguments: ClientArguments {
      jvm: document["arguments"]["jvm"].as_array().unwrap().clone(),
      game: document["arguments"]["game"].as_array().unwrap().clone(),
    },
  }
}

fn client_file(value: &Value) -> ClientFile {
  ClientFile {
    url: Url::parse(value["url"].as_str().unwrap()).unwrap(),
    size: value["size"].as_u64().unwrap(),
    sha1: value["sha1"].as_str().unwrap().to_owned(),
  }
}

fn target(os: ClientOs, architecture: ClientArchitecture) -> ClientTarget {
  ClientTarget::new(os, architecture)
}

fn library(coordinate: &str, rules: Value, path: &str) -> Value {
  serde_json::json!({
    "name": coordinate,
    "rules": rules,
    "downloads": {"artifact": {
      "path": path,
      "url": format!("https://libraries.minecraft.net/{path}"),
      "size": 12,
      "sha1": "0123456789abcdef0123456789abcdef01234567"
    }}
  })
}

fn set_asset_index(version: &mut ClientVersion, bytes: &[u8]) {
  version.asset_index.file.size = bytes.len() as u64;
  version.asset_index.file.sha1 = sha1_hex(bytes);
}

#[test]
fn official_1_21_1_metadata_plans_platform_libraries_assets_and_logging() {
  assert_eq!(sha1_hex(OFFICIAL_VERSION.as_bytes()), VERSION_SHA1);
  assert_eq!(sha1_hex(OFFICIAL_ASSET_INDEX), ASSET_INDEX_SHA1);
  let version = official_version();

  let windows = plan_client(
    &version,
    OFFICIAL_ASSET_INDEX,
    target(ClientOs::Windows, ClientArchitecture::X86_64),
  )
  .unwrap();
  assert!(
    windows
      .libraries
      .iter()
      .any(|library| library.coordinate == "org.lwjgl:lwjgl:3.3.3:natives-windows")
  );
  assert!(
    !windows
      .libraries
      .iter()
      .any(|library| library.coordinate.ends_with("natives-windows-arm64"))
  );
  assert_eq!(windows.assets.len(), 3888);
  assert_eq!(windows.client_jar.path, "versions/1.21.1/1.21.1.jar");
  assert_eq!(windows.asset_index.path, "indexes/17.json");
  let logging = windows.logging.as_ref().unwrap();
  assert_eq!(logging.kind, "log4j2-xml");
  assert_eq!(logging.file.path, "log_configs/client-1.12.xml");
  assert_eq!(logging.argument, "-Dlog4j.configurationFile=${path}");

  let mac_x64 = plan_client(
    &version,
    OFFICIAL_ASSET_INDEX,
    target(ClientOs::MacOs, ClientArchitecture::X86_64),
  )
  .unwrap();
  assert!(
    mac_x64
      .libraries
      .iter()
      .any(|library| library.coordinate == "org.lwjgl:lwjgl-freetype:3.3.3:natives-macos-patch")
  );
  assert!(
    !mac_x64
      .libraries
      .iter()
      .any(|library| library.coordinate.ends_with("natives-macos-arm64"))
  );

  let mac_arm = plan_client(
    &version,
    OFFICIAL_ASSET_INDEX,
    target(ClientOs::MacOs, ClientArchitecture::Aarch64),
  )
  .unwrap();
  assert!(
    mac_arm
      .libraries
      .iter()
      .any(|library| library.coordinate == "org.lwjgl:lwjgl:3.3.3:natives-macos-arm64")
  );
  assert!(
    !mac_arm
      .libraries
      .iter()
      .any(|library| library.coordinate.ends_with("natives-macos-patch"))
  );

  let linux_x64 = plan_client(
    &version,
    OFFICIAL_ASSET_INDEX,
    target(ClientOs::Linux, ClientArchitecture::X86_64),
  )
  .unwrap();
  assert!(
    linux_x64
      .libraries
      .iter()
      .any(|library| library.coordinate == "org.lwjgl:lwjgl:3.3.3:natives-linux")
  );
  assert!(
    linux_x64
      .libraries
      .iter()
      .any(|library| library.coordinate.ends_with("linux-x86_64"))
  );

  let linux_arm = plan_client(
    &version,
    OFFICIAL_ASSET_INDEX,
    target(ClientOs::Linux, ClientArchitecture::Aarch64),
  );
  assert!(matches!(linux_arm, Err(Error::Unsupported(_))));
}

#[test]
fn library_rules_apply_in_order_and_unknown_conditions_fail_closed() {
  let mut version = official_version();
  version.libraries = vec![library(
    "org.example:sample:1.0",
    serde_json::json!([
      {"action":"allow", "os":{"name":"windows", "arch":"amd64"}},
      {"action":"disallow", "os":{"arch":"amd64"}}
    ]),
    "org/example/sample/1.0/sample-1.0.jar",
  )];
  let plan = plan_client(
    &version,
    OFFICIAL_ASSET_INDEX,
    target(ClientOs::Windows, ClientArchitecture::X86_64),
  )
  .unwrap();
  assert!(plan.libraries.is_empty());

  version.libraries = vec![
    library(
      "org.example:zeta:1.0",
      serde_json::json!([{"action":"allow"}]),
      "org/example/zeta/1.0/zeta-1.0.jar",
    ),
    library(
      "org.example:alpha:1.0",
      serde_json::json!([{"action":"allow"}]),
      "org/example/alpha/1.0/alpha-1.0.jar",
    ),
  ];
  let ordered = plan_client(
    &version,
    OFFICIAL_ASSET_INDEX,
    target(ClientOs::Windows, ClientArchitecture::X86_64),
  )
  .unwrap();
  assert_eq!(
    ordered
      .libraries
      .iter()
      .map(|library| library.coordinate.as_str())
      .collect::<Vec<_>>(),
    ["org.example:zeta:1.0", "org.example:alpha:1.0"]
  );

  version.libraries = vec![library(
    "org.example:feature:1.0",
    serde_json::json!([{"action":"allow", "features":{"mystery_feature":true}}]),
    "org/example/feature/1.0/feature-1.0.jar",
  )];
  assert!(matches!(
    plan_client(
      &version,
      OFFICIAL_ASSET_INDEX,
      target(ClientOs::Windows, ClientArchitecture::X86_64)
    ),
    Err(Error::Unsupported(_))
  ));

  version.libraries = vec![library(
    "org.example:enabled-feature:1.0",
    serde_json::json!([{"action":"allow", "features":{"has_custom_resolution":true}}]),
    "org/example/enabled-feature/1.0/enabled-feature-1.0.jar",
  )];
  let feature_plan = plan_client(
    &version,
    OFFICIAL_ASSET_INDEX,
    target(ClientOs::Windows, ClientArchitecture::X86_64)
      .with_feature("has_custom_resolution", true),
  )
  .unwrap();
  assert_eq!(feature_plan.libraries.len(), 1);

  version.libraries = vec![library(
    "org.example:version:1.0",
    serde_json::json!([{"action":"allow", "os":{"version":"^10\\."}}]),
    "org/example/version/1.0/version-1.0.jar",
  )];
  assert!(matches!(
    plan_client(
      &version,
      OFFICIAL_ASSET_INDEX,
      target(ClientOs::Windows, ClientArchitecture::X86_64)
    ),
    Err(Error::Unsupported(_))
  ));
  let versioned = plan_client(
    &version,
    OFFICIAL_ASSET_INDEX,
    target(ClientOs::Windows, ClientArchitecture::X86_64).with_os_version("10.0.19045"),
  )
  .unwrap();
  assert_eq!(versioned.libraries.len(), 1);

  version.libraries = vec![library(
    "org.example:future-rule:1.0",
    serde_json::json!([{
      "action":"allow",
      "os":{"versionRange":{"min":"10.0"}}
    }]),
    "org/example/future-rule/1.0/future-rule-1.0.jar",
  )];
  assert!(matches!(
    plan_client(
      &version,
      OFFICIAL_ASSET_INDEX,
      target(ClientOs::Windows, ClientArchitecture::X86_64)
    ),
    Err(Error::Unsupported(_))
  ));
}

#[test]
fn legacy_native_archives_are_selected_and_exclusion_paths_are_preserved() {
  let mut version = official_version();
  version.libraries = vec![serde_json::json!({
    "name":"org.example:legacy:1.0",
    "rules":[{"action":"allow"}],
    "natives":{"windows":"natives-windows-${arch}"},
    "extract":{"exclude":["META-INF/"]},
    "downloads":{
      "artifact":{
        "path":"org/example/legacy/1.0/legacy-1.0.jar",
        "url":"https://libraries.minecraft.net/org/example/legacy/1.0/legacy-1.0.jar",
        "size":12,
        "sha1":"0123456789abcdef0123456789abcdef01234567"
      },
      "classifiers":{"natives-windows-64":{
        "path":"org/example/legacy/1.0/legacy-1.0-natives-windows-64.jar",
        "url":"https://libraries.minecraft.net/org/example/legacy/1.0/legacy-1.0-natives-windows-64.jar",
        "size":24,
        "sha1":"1123456789abcdef0123456789abcdef01234567"
      }}
    }
  })];

  let plan = plan_client(
    &version,
    OFFICIAL_ASSET_INDEX,
    target(ClientOs::Windows, ClientArchitecture::X86_64),
  )
  .unwrap();
  assert_eq!(plan.libraries.len(), 1);
  assert_eq!(plan.libraries[0].kind, ClientLibraryKind::Classpath);
  assert_eq!(plan.native_archives.len(), 1);
  assert_eq!(plan.native_archives[0].excludes, ["META-INF/"]);
  let macos = plan_client(
    &version,
    OFFICIAL_ASSET_INDEX,
    target(ClientOs::MacOs, ClientArchitecture::X86_64),
  );
  assert!(matches!(macos, Err(Error::Unsupported(_))), "{macos:?}");

  version.libraries[0]["extract"]["exclude"][0] = Value::String("../outside/".into());
  assert!(matches!(
    plan_client(
      &version,
      OFFICIAL_ASSET_INDEX,
      target(ClientOs::Windows, ClientArchitecture::X86_64)
    ),
    Err(Error::Metadata(_))
  ));
}

#[test]
fn unsafe_paths_invalid_asset_hashes_and_legacy_asset_layouts_are_rejected() {
  let mut version = official_version();
  version.libraries = vec![library(
    "org.example:unsafe:1.0",
    serde_json::json!([{"action":"allow"}]),
    "../unsafe.jar",
  )];
  assert!(matches!(
    plan_client(
      &version,
      OFFICIAL_ASSET_INDEX,
      target(ClientOs::Windows, ClientArchitecture::X86_64)
    ),
    Err(Error::Metadata(_))
  ));

  version = official_version();
  version.asset_index.file.sha1 = "0".repeat(40);
  assert!(matches!(
    plan_client(
      &version,
      OFFICIAL_ASSET_INDEX,
      target(ClientOs::Windows, ClientArchitecture::X86_64)
    ),
    Err(Error::Metadata(message)) if message.contains("size and SHA-1")
  ));

  let legacy_index = br#"{"virtual":true,"objects":{"test":{"hash":"0123456789abcdef0123456789abcdef01234567","size":12}}}"#;
  version = official_version();
  set_asset_index(&mut version, legacy_index);
  assert!(matches!(
    plan_client(
      &version,
      legacy_index,
      target(ClientOs::Windows, ClientArchitecture::X86_64)
    ),
    Err(Error::Unsupported(_))
  ));
}
