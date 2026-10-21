//! Platform-specific file planning for the supported Minecraft client metadata format.
use std::collections::{BTreeMap, BTreeSet, HashMap};

use regex::Regex;
use serde::Deserialize;
use serde_json::{Map, Value};
use sha1::{Digest, Sha1};
use url::Url;

use crate::{
  Error, Result,
  metadata::{ClientFile, ClientVersion},
};

const MAX_ASSET_INDEX_SIZE: usize = 16 * 1024 * 1024;
const MAX_ASSET_OBJECTS: usize = 100_000;
const MAX_ARTIFACT_SIZE: u64 = 1024 * 1024 * 1024;
const MAX_CLIENT_LIBRARIES: usize = 10_000;
const MAX_RULES_PER_LIBRARY: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClientOs {
  Windows,
  MacOs,
  Linux,
}

impl ClientOs {
  fn metadata_name(self) -> &'static str {
    match self {
      Self::Windows => "windows",
      Self::MacOs => "osx",
      Self::Linux => "linux",
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClientArchitecture {
  X86_64,
  Aarch64,
}

impl ClientArchitecture {
  fn java_name(self) -> &'static str {
    match self {
      Self::X86_64 => "amd64",
      Self::Aarch64 => "aarch64",
    }
  }
}

/// OS and architecture used when evaluating Mojang library rules.
/// `Linux` denotes glibc Linux; musl is not in the initial target set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientTarget {
  pub os: ClientOs,
  pub architecture: ClientArchitecture,
  /// Required only by rules that constrain `os.version`.
  pub os_version: Option<String>,
  /// Extra Mojang features, such as `has_custom_resolution`; unspecified known features are false.
  pub features: BTreeMap<String, bool>,
}

impl ClientTarget {
  pub fn new(os: ClientOs, architecture: ClientArchitecture) -> Self {
    Self {
      os,
      architecture,
      os_version: None,
      features: BTreeMap::new(),
    }
  }

  pub fn current() -> Result<Self> {
    let platform = crate::platform::Platform::current()?;
    let os = match platform.os {
      "windows" => ClientOs::Windows,
      "mac" => ClientOs::MacOs,
      "linux" => ClientOs::Linux,
      other => return Err(Error::Unsupported(format!("Minecraft client OS {other}"))),
    };
    let architecture = match platform.arch {
      "x64" => ClientArchitecture::X86_64,
      "aarch64" => ClientArchitecture::Aarch64,
      other => {
        return Err(Error::Unsupported(format!(
          "Minecraft client architecture {other}"
        )));
      }
    };
    Ok(Self::new(os, architecture))
  }

  pub fn with_os_version(mut self, version: impl Into<String>) -> Self {
    self.os_version = Some(version.into());
    self
  }

  pub fn with_feature(mut self, name: impl Into<String>, enabled: bool) -> Self {
    self.features.insert(name.into(), enabled);
    self
  }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedFile {
  /// Safe relative path required by the Minecraft layout or library classpath.
  pub path: String,
  pub file: ClientFile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientLibraryKind {
  /// Regular classpath library.
  Classpath,
  /// Modern native JAR that remains on the classpath.
  NativeClasspath,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientLibrary {
  pub coordinate: String,
  pub artifact: PlannedFile,
  pub kind: ClientLibraryKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientNativeArchive {
  pub coordinate: String,
  pub artifact: PlannedFile,
  /// Relative archive paths excluded during extraction.
  pub excludes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientAssetObject {
  pub hash: String,
  pub size: u64,
  pub url: Url,
  /// Content-addressed path under the shared assets object directory.
  pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientLoggingPlan {
  pub kind: String,
  /// Argument template; the launch planner replaces `${path}` with the installed file path.
  pub argument: String,
  pub file: PlannedFile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientInstallPlan {
  pub version_id: String,
  pub version_type: String,
  /// Identity of the version JSON selected from Mojang's manifest chain.
  pub metadata_url: Url,
  pub metadata_sha1: String,
  pub target: ClientTarget,
  pub java_component: String,
  pub java_major: u32,
  pub client_jar: PlannedFile,
  pub libraries: Vec<ClientLibrary>,
  pub native_archives: Vec<ClientNativeArchive>,
  pub asset_index_id: String,
  pub asset_index: PlannedFile,
  /// Unique objects, deduplicated by SHA-1 from the verified asset index.
  pub assets: Vec<ClientAssetObject>,
  pub logging: Option<ClientLoggingPlan>,
  pub arguments: crate::metadata::ClientArguments,
  pub main_class: String,
}

/// Build a bounded, deterministic plan from a resolved version and its verified asset-index bytes.
/// The bytes must match the size and SHA-1 recorded in `version.asset_index`.
pub fn plan_client(
  version: &ClientVersion,
  asset_index_bytes: &[u8],
  target: ClientTarget,
) -> Result<ClientInstallPlan> {
  validate_version_id(&version.version_id)?;
  validate_version_id(&version.asset_index.id)?;
  validate_target(&target)?;
  validate_client_file(&version.client_jar, "client JAR")?;
  validate_client_file(&version.asset_index.file, "asset index")?;
  if asset_index_bytes.len() > MAX_ASSET_INDEX_SIZE
    || asset_index_bytes.len() as u64 != version.asset_index.file.size
    || sha1_hex(asset_index_bytes) != version.asset_index.file.sha1
  {
    return Err(Error::Metadata(
      "asset index does not match its declared size and SHA-1".into(),
    ));
  }
  let assets = parse_assets(asset_index_bytes)?;
  let (libraries, native_archives) = plan_libraries(&version.libraries, &target)?;
  let logging = version
    .logging
    .as_ref()
    .map(|logging| {
      validate_relative_path(&logging.file_id)
        .map_err(|_| Error::Metadata("invalid client logging file ID".into()))?;
      validate_client_file(&logging.file, "client logging configuration")?;
      if logging.kind != "log4j2-xml" {
        return Err(Error::Unsupported(format!(
          "client logging type {}",
          logging.kind
        )));
      }
      Ok(ClientLoggingPlan {
        kind: logging.kind.clone(),
        argument: logging.argument.clone(),
        file: PlannedFile {
          path: format!("log_configs/{}", logging.file_id),
          file: logging.file.clone(),
        },
      })
    })
    .transpose()?;

  Ok(ClientInstallPlan {
    version_id: version.version_id.clone(),
    version_type: version.version_type.clone(),
    metadata_url: version.metadata_url.clone(),
    metadata_sha1: version.metadata_sha1.clone(),
    target,
    java_component: version.java_component.clone(),
    java_major: version.java_major,
    client_jar: PlannedFile {
      path: format!("versions/{0}/{0}.jar", version.version_id),
      file: version.client_jar.clone(),
    },
    libraries,
    native_archives,
    asset_index_id: version.asset_index.id.clone(),
    asset_index: PlannedFile {
      path: format!("indexes/{}.json", version.asset_index.id),
      file: version.asset_index.file.clone(),
    },
    assets,
    logging,
    arguments: version.arguments.clone(),
    main_class: version.main_class.clone(),
  })
}

#[derive(Deserialize)]
struct AssetIndexDocument {
  objects: BTreeMap<String, AssetObjectDocument>,
  #[serde(rename = "virtual", default)]
  virtual_assets: bool,
  #[serde(rename = "map_to_resources", default)]
  map_to_resources: bool,
}

#[derive(Deserialize)]
struct AssetObjectDocument {
  hash: String,
  size: u64,
}

fn parse_assets(bytes: &[u8]) -> Result<Vec<ClientAssetObject>> {
  let index: AssetIndexDocument = serde_json::from_slice(bytes)
    .map_err(|error| Error::Metadata(format!("invalid asset index: {error}")))?;
  if index.virtual_assets || index.map_to_resources {
    return Err(Error::Unsupported(
      "legacy virtual or mapped Minecraft assets".into(),
    ));
  }
  if index.objects.is_empty() || index.objects.len() > MAX_ASSET_OBJECTS {
    return Err(Error::Metadata("invalid asset object count".into()));
  }

  let mut unique = BTreeMap::<String, u64>::new();
  for (name, object) in index.objects {
    validate_relative_path(&name)
      .map_err(|_| Error::Metadata("invalid logical path in asset index".into()))?;
    validate_hash(&object.hash)?;
    if object.size > MAX_ARTIFACT_SIZE {
      return Err(Error::Metadata(
        "asset object exceeds the size limit".into(),
      ));
    }
    let hash = object.hash.to_ascii_lowercase();
    if let Some(previous_size) = unique.insert(hash, object.size)
      && previous_size != object.size
    {
      return Err(Error::Metadata(
        "asset index assigns different sizes to the same content hash".into(),
      ));
    }
  }

  unique
    .into_iter()
    .map(|(hash, size)| {
      let prefix = &hash[..2];
      let url = Url::parse(&format!(
        "https://resources.download.minecraft.net/{prefix}/{hash}"
      ))
      .map_err(|_| Error::Metadata("invalid asset object URL".into()))?;
      Ok(ClientAssetObject {
        hash: hash.clone(),
        size,
        url,
        path: format!("objects/{prefix}/{hash}"),
      })
    })
    .collect()
}

fn plan_libraries(
  entries: &[Value],
  target: &ClientTarget,
) -> Result<(Vec<ClientLibrary>, Vec<ClientNativeArchive>)> {
  if entries.len() > MAX_CLIENT_LIBRARIES {
    return Err(Error::Metadata("too many client libraries".into()));
  }
  let mut libraries = Vec::<ClientLibrary>::new();
  let mut library_paths = HashMap::<String, ClientLibrary>::new();
  let mut native_archives = BTreeMap::<String, ClientNativeArchive>::new();
  let mut native_families = BTreeSet::<String>::new();
  let mut selected_native_families = BTreeSet::<String>::new();

  for entry in entries {
    let object = entry
      .as_object()
      .ok_or_else(|| Error::Metadata("client library entry is not an object".into()))?;
    let coordinate = object
      .get("name")
      .and_then(Value::as_str)
      .ok_or_else(|| Error::Metadata("client library has no Maven coordinate".into()))?;
    let parts = validate_coordinate(coordinate)?;
    if !library_rules_allow(object, target)? {
      continue;
    }
    let downloads = object.get("downloads").and_then(Value::as_object);
    let legacy_natives = object.get("natives");
    let mut has_selected_artifact = false;

    if let Some(classifier) = parts.get(3).copied() {
      if let Some(native) = parse_native_classifier(classifier, parts[1])? {
        has_selected_artifact = true;
        if native.os == target.os {
          let family = parts[..3].join(":");
          native_families.insert(family.clone());
          if native.matches(target) {
            let artifact = get_download_artifact(downloads, "artifact", coordinate)?;
            insert_library(
              &mut libraries,
              &mut library_paths,
              ClientLibrary {
                coordinate: coordinate.to_owned(),
                artifact,
                kind: ClientLibraryKind::NativeClasspath,
              },
            )?;
            selected_native_families.insert(family);
          }
        }
      } else {
        return Err(Error::Unsupported(format!(
          "client library classifier {classifier}"
        )));
      }
    } else if let Some(downloads) = downloads
      && downloads.contains_key("artifact")
    {
      let artifact = get_download_artifact(Some(downloads), "artifact", coordinate)?;
      insert_library(
        &mut libraries,
        &mut library_paths,
        ClientLibrary {
          coordinate: coordinate.to_owned(),
          artifact,
          kind: ClientLibraryKind::Classpath,
        },
      )?;
      has_selected_artifact = true;
    }

    if let Some(natives) = legacy_natives
      && let Some(archive) = plan_legacy_native(
        coordinate,
        natives,
        object.get("extract"),
        downloads,
        target,
      )?
    {
      if native_archives
        .insert(archive.artifact.path.clone(), archive.clone())
        .is_some_and(|previous| previous != archive)
      {
        return Err(Error::Metadata(
          "conflicting native archive paths in client libraries".into(),
        ));
      }
      has_selected_artifact = true;
    }

    if !has_selected_artifact {
      return Err(Error::Metadata(format!(
        "allowed client library {coordinate} has no artifact for the target"
      )));
    }
  }

  if let Some(missing) = native_families.difference(&selected_native_families).next() {
    return Err(Error::Unsupported(format!(
      "Minecraft native libraries for {} {} (missing {missing})",
      target.os.metadata_name(),
      target.architecture.java_name()
    )));
  }

  Ok((libraries, native_archives.into_values().collect()))
}

#[derive(Clone, Copy)]
struct NativeClassifier {
  os: ClientOs,
  architecture: Option<ClientArchitecture>,
  supplementary_x64: bool,
}

impl NativeClassifier {
  fn matches(self, target: &ClientTarget) -> bool {
    self.os == target.os
      && (self.architecture == Some(target.architecture)
        || self.supplementary_x64 && target.architecture == ClientArchitecture::X86_64)
  }
}

fn parse_native_classifier(classifier: &str, module: &str) -> Result<Option<NativeClassifier>> {
  let value = classifier.strip_prefix("natives-").unwrap_or(classifier);
  let (os, suffix) = if value.starts_with("windows") {
    (
      ClientOs::Windows,
      value.strip_prefix("windows").unwrap_or_default(),
    )
  } else if value.starts_with("macos") {
    (
      ClientOs::MacOs,
      value.strip_prefix("macos").unwrap_or_default(),
    )
  } else if value.starts_with("osx") {
    (
      ClientOs::MacOs,
      value.strip_prefix("osx").unwrap_or_default(),
    )
  } else if value.starts_with("linux") {
    (
      ClientOs::Linux,
      value.strip_prefix("linux").unwrap_or_default(),
    )
  } else {
    return Ok(None);
  };

  let (architecture, supplementary_x64) = match suffix {
    "" | "-x86_64" => (Some(ClientArchitecture::X86_64), false),
    "-arm64" | "-aarch64" | "-aarch_64" => (Some(ClientArchitecture::Aarch64), false),
    "-patch" if os == ClientOs::MacOs => (Some(ClientArchitecture::X86_64), true),
    "-x86" => (None, false),
    _ => {
      return Err(Error::Unsupported(format!(
        "native classifier {classifier}"
      )));
    }
  };

  if !classifier.starts_with("natives-") && !module.contains("native") {
    return Ok(None);
  }
  Ok(Some(NativeClassifier {
    os,
    architecture,
    supplementary_x64,
  }))
}

fn plan_legacy_native(
  coordinate: &str,
  natives: &Value,
  extract: Option<&Value>,
  downloads: Option<&Map<String, Value>>,
  target: &ClientTarget,
) -> Result<Option<ClientNativeArchive>> {
  let native_map = natives
    .as_object()
    .ok_or_else(|| Error::Metadata("legacy library natives field is not an object".into()))?;
  let Some(template) = native_map.get(target.os.metadata_name()) else {
    return Err(Error::Unsupported(format!(
      "legacy native classifier for {} in {coordinate}",
      target.os.metadata_name()
    )));
  };
  let template = template
    .as_str()
    .ok_or_else(|| Error::Metadata("legacy native classifier is not a string".into()))?;
  let classifier = if template.contains("${arch}") {
    if target.architecture != ClientArchitecture::X86_64 {
      return Err(Error::Unsupported(format!(
        "legacy native architecture for {coordinate}"
      )));
    }
    template.replace("${arch}", "64")
  } else {
    if target.architecture != ClientArchitecture::X86_64 {
      return Err(Error::Unsupported(format!(
        "legacy native classifier has no ARM64 mapping for {coordinate}"
      )));
    }
    template.to_owned()
  };
  let classifier_artifact = downloads
    .and_then(|downloads| downloads.get("classifiers"))
    .and_then(Value::as_object)
    .and_then(|classifiers| classifiers.get(&classifier))
    .ok_or_else(|| {
      Error::Metadata(format!(
        "legacy native classifier {classifier} is missing for {coordinate}"
      ))
    })?;
  let artifact = parse_download_artifact(classifier_artifact, "legacy native archive")?;
  let excludes = match extract {
    None => Vec::new(),
    Some(extract) => {
      let extract = extract
        .as_object()
        .ok_or_else(|| Error::Metadata("legacy native extract field is not an object".into()))?;
      if extract.keys().any(|key| key != "exclude") {
        return Err(Error::Unsupported(
          "unknown legacy native extraction option".into(),
        ));
      }
      match extract.get("exclude") {
        None => Vec::new(),
        Some(value) => value
          .as_array()
          .ok_or_else(|| Error::Metadata("native extraction excludes are not an array".into()))?
          .iter()
          .map(|value| {
            let path = value
              .as_str()
              .ok_or_else(|| Error::Metadata("native extraction exclude is not a string".into()))?;
            let normalized = path.trim_end_matches('/');
            validate_relative_path(normalized)
              .map_err(|_| Error::Metadata("unsafe path in native extraction excludes".into()))?;
            Ok(path.to_owned())
          })
          .collect::<Result<Vec<_>>>()?,
      }
    }
  };

  Ok(Some(ClientNativeArchive {
    coordinate: coordinate.to_owned(),
    artifact,
    excludes,
  }))
}

fn get_download_artifact(
  downloads: Option<&Map<String, Value>>,
  key: &str,
  coordinate: &str,
) -> Result<PlannedFile> {
  let value = downloads
    .and_then(|downloads| downloads.get(key))
    .ok_or_else(|| Error::Metadata(format!("client library {coordinate} has no {key}")))?;
  parse_download_artifact(value, "client library artifact")
}

fn parse_download_artifact(value: &Value, label: &str) -> Result<PlannedFile> {
  let object = value
    .as_object()
    .ok_or_else(|| Error::Metadata(format!("{label} is not an object")))?;
  let path = object
    .get("path")
    .and_then(Value::as_str)
    .ok_or_else(|| Error::Metadata(format!("{label} has no path")))?;
  validate_relative_path(path)
    .map_err(|_| Error::Metadata(format!("{label} has an unsafe path")))?;
  let url = object
    .get("url")
    .and_then(Value::as_str)
    .ok_or_else(|| Error::Metadata(format!("{label} has no URL")))?;
  let url = Url::parse(url).map_err(|_| Error::Metadata(format!("invalid {label} URL")))?;
  if url.scheme() != "https"
    || url.host_str().is_none()
    || !url.username().is_empty()
    || url.password().is_some()
    || url.port().is_some_and(|port| port != 443)
  {
    return Err(Error::Metadata(format!("{label} URL must use HTTPS")));
  }
  let size = object
    .get("size")
    .and_then(Value::as_u64)
    .filter(|size| (1..=MAX_ARTIFACT_SIZE).contains(size))
    .ok_or_else(|| Error::Metadata(format!("invalid {label} size")))?;
  let sha1 = object
    .get("sha1")
    .and_then(Value::as_str)
    .ok_or_else(|| Error::Metadata(format!("{label} has no SHA-1")))?;
  validate_hash(sha1)?;

  Ok(PlannedFile {
    path: path.to_owned(),
    file: ClientFile {
      url,
      size,
      sha1: sha1.to_ascii_lowercase(),
    },
  })
}

fn insert_library(
  libraries: &mut Vec<ClientLibrary>,
  paths: &mut HashMap<String, ClientLibrary>,
  library: ClientLibrary,
) -> Result<()> {
  let path = library.artifact.path.clone();
  if let Some(previous) = paths.get(&path) {
    if previous != &library {
      return Err(Error::Metadata(
        "conflicting client library paths in metadata".into(),
      ));
    }
    return Ok(());
  }
  paths.insert(path, library.clone());
  libraries.push(library);
  Ok(())
}

fn validate_coordinate(coordinate: &str) -> Result<Vec<&str>> {
  let parts = coordinate.split(':').collect::<Vec<_>>();
  if !(3..=4).contains(&parts.len())
    || parts.iter().any(|part| {
      part.is_empty()
        || part.len() > 256
        || !part
          .bytes()
          .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
    })
  {
    return Err(Error::Metadata("invalid client library coordinate".into()));
  }
  Ok(parts)
}

fn library_rules_allow(library: &Map<String, Value>, target: &ClientTarget) -> Result<bool> {
  let Some(rules) = library.get("rules") else {
    return Ok(true);
  };
  evaluate_rules(rules, target)
}

pub(crate) fn evaluate_rules(rules: &Value, target: &ClientTarget) -> Result<bool> {
  let rules = rules
    .as_array()
    .ok_or_else(|| Error::Metadata("Minecraft rules are not an array".into()))?;
  if rules.len() > MAX_RULES_PER_LIBRARY {
    return Err(Error::Metadata("too many Minecraft rules".into()));
  }

  let mut allowed = false;
  for rule in rules {
    let rule = rule
      .as_object()
      .ok_or_else(|| Error::Metadata("Minecraft rule is not an object".into()))?;
    if rule
      .keys()
      .any(|key| !matches!(key.as_str(), "action" | "os" | "features"))
    {
      return Err(Error::Unsupported(
        "unknown Minecraft library rule condition".into(),
      ));
    }
    let action = rule
      .get("action")
      .and_then(Value::as_str)
      .ok_or_else(|| Error::Metadata("Minecraft rule has no action".into()))?;
    let allow = match action {
      "allow" => true,
      "disallow" => false,
      _ => {
        return Err(Error::Unsupported(format!(
          "Minecraft rule action {action}"
        )));
      }
    };
    if rule_matches(rule, target)? {
      allowed = allow;
    }
  }
  Ok(allowed)
}

fn rule_matches(rule: &Map<String, Value>, target: &ClientTarget) -> Result<bool> {
  if let Some(os) = rule.get("os") {
    let os = os
      .as_object()
      .ok_or_else(|| Error::Metadata("Minecraft OS rule is not an object".into()))?;
    if os
      .keys()
      .any(|key| !matches!(key.as_str(), "name" | "arch" | "version"))
    {
      return Err(Error::Unsupported("unknown Minecraft OS rule field".into()));
    }
    if let Some(name) = os.get("name") {
      let name = name
        .as_str()
        .ok_or_else(|| Error::Metadata("Minecraft OS rule name is not a string".into()))?;
      if !matches!(name, "windows" | "osx" | "linux" | "freebsd" | "solaris") {
        return Err(Error::Unsupported(format!("Minecraft OS rule {name}")));
      }
      if name != target.os.metadata_name() {
        return Ok(false);
      }
    }
    if let Some(arch) = os.get("arch") {
      let arch = arch
        .as_str()
        .ok_or_else(|| Error::Metadata("Minecraft OS architecture rule is not a string".into()))?;
      if arch.len() > 256 {
        return Err(Error::Metadata(
          "Minecraft OS architecture rule is too long".into(),
        ));
      }
      if !matches_regex(arch, target.architecture.java_name())? {
        return Ok(false);
      }
    }
    if let Some(version) = os.get("version") {
      let version = version
        .as_str()
        .ok_or_else(|| Error::Metadata("Minecraft OS version rule is not a string".into()))?;
      if version.len() > 256 {
        return Err(Error::Metadata(
          "Minecraft OS version rule is too long".into(),
        ));
      }
      let target_version = target.os_version.as_deref().ok_or_else(|| {
        Error::Unsupported("Minecraft library rule requires the OS version".into())
      })?;
      if !matches_regex(version, target_version)? {
        return Ok(false);
      }
    }
  }
  if let Some(features) = rule.get("features") {
    let features = features
      .as_object()
      .ok_or_else(|| Error::Metadata("Minecraft feature rule is not an object".into()))?;
    if features.len() > 64 {
      return Err(Error::Metadata("too many Minecraft feature rules".into()));
    }
    for (name, expected) in features {
      let expected = expected
        .as_bool()
        .ok_or_else(|| Error::Metadata("Minecraft feature value is not a boolean".into()))?;
      let actual = match target.features.get(name) {
        Some(value) => *value,
        None if known_feature(name) => false,
        None => {
          return Err(Error::Unsupported(format!(
            "unknown Minecraft feature rule {name}"
          )));
        }
      };
      if actual != expected {
        return Ok(false);
      }
    }
  }
  Ok(true)
}

fn known_feature(name: &str) -> bool {
  matches!(
    name,
    "is_demo_user"
      | "has_custom_resolution"
      | "has_quick_plays_support"
      | "is_quick_play_singleplayer"
      | "is_quick_play_multiplayer"
      | "is_quick_play_realms"
  )
}

fn validate_target(target: &ClientTarget) -> Result<()> {
  if target.os_version.as_ref().is_some_and(|version| {
    version.is_empty() || version.len() > 128 || version.bytes().any(|byte| byte.is_ascii_control())
  }) {
    return Err(Error::Invalid("invalid target OS version".into()));
  }
  if target.features.len() > 64
    || target.features.keys().any(|name| {
      name.is_empty()
        || name.len() > 128
        || !name
          .bytes()
          .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    })
  {
    return Err(Error::Invalid("invalid Minecraft target features".into()));
  }
  Ok(())
}

fn matches_regex(pattern: &str, value: &str) -> Result<bool> {
  Regex::new(pattern)
    .map(|pattern| pattern.is_match(value))
    .map_err(|_| Error::Metadata("invalid regular expression in Minecraft rules".into()))
}

fn validate_relative_path(path: &str) -> Result<()> {
  if path.is_empty()
    || path.len() > 1024
    || path.starts_with('/')
    || path.contains('\\')
    || path.contains(':')
    || path.bytes().any(|byte| byte.is_ascii_control())
    || path
      .split('/')
      .any(|part| part.is_empty() || matches!(part, "." | ".."))
  {
    return Err(Error::Metadata("unsafe relative path".into()));
  }
  Ok(())
}

fn validate_hash(hash: &str) -> Result<()> {
  if hash.len() != 40 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    return Err(Error::Metadata("invalid SHA-1 in client metadata".into()));
  }
  Ok(())
}

fn validate_version_id(version_id: &str) -> Result<()> {
  if version_id.is_empty()
    || version_id.len() > 64
    || !version_id
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
  {
    return Err(Error::Metadata(
      "invalid Minecraft version ID in client plan".into(),
    ));
  }
  Ok(())
}

fn validate_client_file(file: &ClientFile, label: &str) -> Result<()> {
  if file.size == 0 || file.size > MAX_ARTIFACT_SIZE {
    return Err(Error::Metadata(format!("invalid {label} size")));
  }
  validate_hash(&file.sha1)?;
  if file.url.scheme() != "https"
    || file.url.host_str().is_none()
    || !file.url.username().is_empty()
    || file.url.password().is_some()
    || file.url.port().is_some_and(|port| port != 443)
  {
    return Err(Error::Metadata(format!("{label} URL must use HTTPS")));
  }
  Ok(())
}

fn sha1_hex(bytes: &[u8]) -> String {
  Sha1::digest(bytes)
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect()
}

#[cfg(test)]
#[path = "../tests/client_plan.rs"]
mod tests;
