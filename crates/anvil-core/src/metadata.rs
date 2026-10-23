//! Bounded Mojang version metadata for launcher and original-server workflows.
mod catalog;
pub use catalog::{CachedVersionCatalog, load_cached_version_catalog, refresh_version_catalog};
use std::collections::HashSet;

use q2q_http::Executor;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use url::Url;

use crate::{Context, Error, Result};

const VERSION_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const VERSION_MANIFEST_LIMIT: u64 = 8 * 1024 * 1024;
const VERSION_JSON_LIMIT: u64 = 4 * 1024 * 1024;
const MAX_VERSION_ENTRIES: usize = 100_000;
const MAX_SERVER_ARTIFACT: u64 = 1024 * 1024 * 1024;
const MAX_CLIENT_JAR: u64 = 1024 * 1024 * 1024;
const MAX_ASSET_INDEX: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GameVersion {
  pub id: String,
  #[serde(rename = "type")]
  pub version_type: String,
  pub release_time: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerArtifact {
  pub version_id: String,
  pub java_component: String,
  pub java_major: u32,
  pub url: Url,
  pub size: u64,
  pub sha1: String,
}

/// Client launch metadata parsed only after validating the version manifest SHA-1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientVersion {
  pub version_id: String,
  pub version_type: String,
  pub metadata_url: Url,
  pub metadata_sha1: String,
  pub java_component: String,
  pub java_major: u32,
  pub client_jar: ClientFile,
  pub asset_index: ClientAssetIndex,
  pub main_class: String,
  pub logging: Option<ClientLogging>,
  /// Raw library entries; platform rule evaluation belongs to the install planner.
  pub libraries: Vec<Value>,
  /// Raw string/object argument entries; rule evaluation belongs to launch planning.
  pub arguments: ClientArguments,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientLogging {
  pub kind: String,
  pub argument: String,
  pub file_id: String,
  pub file: ClientFile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientFile {
  pub url: Url,
  pub size: u64,
  pub sha1: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientAssetIndex {
  pub id: String,
  pub file: ClientFile,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientArguments {
  pub jvm: Vec<Value>,
  pub game: Vec<Value>,
}

#[derive(Serialize, Deserialize)]
struct VersionManifest {
  versions: Vec<ManifestEntry>,
}
#[derive(Serialize, Deserialize)]
struct ManifestEntry {
  id: String,
  url: String,
  sha1: String,
  #[serde(rename = "type")]
  version_type: String,
  #[serde(rename = "releaseTime")]
  release_time: String,
}
#[derive(Deserialize)]
struct VersionDocument {
  id: String,
  #[serde(rename = "javaVersion")]
  java_version: Option<JavaVersion>,
  downloads: Option<Downloads>,
}
#[derive(Deserialize)]
struct JavaVersion {
  component: String,
  #[serde(rename = "majorVersion")]
  major_version: u32,
}
#[derive(Deserialize)]
struct Downloads {
  server: Option<ServerDownload>,
}
#[derive(Deserialize)]
struct ServerDownload {
  sha1: Option<String>,
  size: Option<u64>,
  url: Option<String>,
}
#[derive(Deserialize)]
struct ClientVersionDocument {
  id: String,
  #[serde(rename = "type")]
  version_type: Option<String>,
  #[serde(rename = "javaVersion")]
  java_version: Option<JavaVersion>,
  downloads: Option<ClientDownloads>,
  #[serde(rename = "assetIndex")]
  asset_index: Option<AssetIndexDocument>,
  #[serde(rename = "mainClass")]
  main_class: Option<String>,
  libraries: Option<Vec<Value>>,
  arguments: Option<RawClientArguments>,
  logging: Option<LoggingDocument>,
}
#[derive(Deserialize)]
struct ClientDownloads {
  client: Option<FileDocument>,
}
#[derive(Deserialize)]
struct FileDocument {
  url: Option<String>,
  size: Option<u64>,
  sha1: Option<String>,
}
#[derive(Deserialize)]
struct AssetIndexDocument {
  id: Option<String>,
  url: Option<String>,
  size: Option<u64>,
  sha1: Option<String>,
}
#[derive(Deserialize)]
struct RawClientArguments {
  jvm: Option<Vec<Value>>,
  game: Option<Vec<Value>>,
}
#[derive(Deserialize)]
struct LoggingDocument {
  client: Option<LoggingClientDocument>,
}
#[derive(Deserialize)]
struct LoggingClientDocument {
  argument: Option<String>,
  file: Option<LoggingFileDocument>,
  #[serde(rename = "type")]
  kind: Option<String>,
}
#[derive(Deserialize)]
struct LoggingFileDocument {
  id: Option<String>,
  url: Option<String>,
  size: Option<u64>,
  sha1: Option<String>,
}

/// List versions from Mojang's bounded, official version manifest.
pub async fn list_versions(context: &Context) -> Result<Vec<GameVersion>> {
  Ok(refresh_version_catalog(context).await?.versions)
}

fn version_manifest_url() -> Result<Url> {
  Url::parse(VERSION_MANIFEST)
    .map_err(|_| Error::Metadata("invalid Mojang version manifest URL".into()))
}

#[cfg(test)]
async fn list_versions_from(http: &Executor, manifest_url: Url) -> Result<Vec<GameVersion>> {
  versions_from_manifest(&fetch_version_manifest(http, manifest_url).await?)
}

async fn fetch_version_manifest(http: &Executor, manifest_url: Url) -> Result<VersionManifest> {
  validate_metadata_url(&manifest_url)?;
  let response = http
    .get(manifest_url.as_str())
    .max_body_size(VERSION_MANIFEST_LIMIT)
    .send()
    .await?;
  let bytes = response
    .collect_bounded(VERSION_MANIFEST_LIMIT as usize)
    .await?;
  parse_json(&bytes)
}

fn versions_from_manifest(manifest: &VersionManifest) -> Result<Vec<GameVersion>> {
  if manifest.versions.len() > MAX_VERSION_ENTRIES {
    return Err(Error::Metadata("too many versions in manifest".into()));
  }

  let mut ids = HashSet::with_capacity(manifest.versions.len());
  let mut versions = Vec::with_capacity(manifest.versions.len());
  for entry in &manifest.versions {
    if !is_valid_version_id(&entry.id) {
      return Err(Error::Metadata(
        "invalid Minecraft version ID in manifest".into(),
      ));
    }
    validate_sha1(&entry.sha1)?;
    let metadata_url =
      Url::parse(&entry.url).map_err(|_| Error::Metadata("invalid version metadata URL".into()))?;
    validate_metadata_url(&metadata_url)?;
    if entry.version_type.is_empty()
      || entry.version_type.len() > 32
      || !entry
        .version_type
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
    {
      return Err(Error::Metadata("invalid Minecraft version type".into()));
    }
    if entry.release_time.is_empty()
      || entry.release_time.len() > 64
      || !entry
        .release_time
        .bytes()
        .all(|byte| byte.is_ascii_graphic())
    {
      return Err(Error::Metadata(
        "invalid Minecraft version release time".into(),
      ));
    }
    if !ids.insert(entry.id.clone()) {
      return Err(Error::Metadata("duplicate version ID in manifest".into()));
    }
    versions.push(GameVersion {
      id: entry.id.clone(),
      version_type: entry.version_type.clone(),
      release_time: entry.release_time.clone(),
    });
  }
  Ok(versions)
}

/// Resolve a released or snapshot version from Mojang's HTTPS metadata chain.
/// The version JSON is authenticated against the SHA-1 in the version manifest before
/// it is parsed. All metadata reads use the caller's shared HTTP Executor.
pub async fn resolve_server(http: &Executor, version_id: &str) -> Result<ServerArtifact> {
  resolve_server_from(http, version_id, version_manifest_url()?).await
}

async fn resolve_server_from(
  http: &Executor,
  version_id: &str,
  manifest_url: Url,
) -> Result<ServerArtifact> {
  let verified =
    fetch_verified_version_document::<VersionDocument>(http, version_id, manifest_url).await?;
  let document = verified.document;
  if document.id != version_id {
    return Err(Error::Metadata(
      "version metadata ID does not match the manifest entry".into(),
    ));
  }
  let java = document
    .java_version
    .ok_or_else(|| Error::Metadata("version has no Java requirement".into()))?;
  validate_java_requirement(&java)?;
  let server = document
    .downloads
    .and_then(|downloads| downloads.server)
    .ok_or_else(|| Error::Metadata("version has no official server download".into()))?;
  let url = Url::parse(
    server
      .url
      .as_deref()
      .ok_or_else(|| Error::Metadata("server download has no URL".into()))?,
  )
  .map_err(|_| Error::Metadata("invalid server download URL".into()))?;
  validate_artifact_url(&url)?;
  let size = server
    .size
    .filter(|size| (1..=MAX_SERVER_ARTIFACT).contains(size))
    .ok_or_else(|| Error::Metadata("invalid server download size".into()))?;
  let sha1 = server
    .sha1
    .ok_or_else(|| Error::Metadata("server download has no SHA-1".into()))?;
  validate_sha1(&sha1)?;

  Ok(ServerArtifact {
    version_id: version_id.to_owned(),
    java_component: java.component,
    java_major: java.major_version,
    url,
    size,
    sha1: sha1.to_ascii_lowercase(),
  })
}

/// Resolve the minimum verified metadata needed to install and launch a client.
/// The caller supplies the shared launcher context so this request reuses its HTTP pool.
pub async fn resolve_client(context: &Context, version_id: &str) -> Result<ClientVersion> {
  let http = context.http.executor();
  resolve_client_from(&http, version_id, version_manifest_url()?).await
}

async fn resolve_client_from(
  http: &Executor,
  version_id: &str,
  manifest_url: Url,
) -> Result<ClientVersion> {
  let verified =
    fetch_verified_version_document::<ClientVersionDocument>(http, version_id, manifest_url)
      .await?;
  let document = verified.document;
  if document.id != version_id {
    return Err(Error::Metadata(
      "version metadata ID does not match the manifest entry".into(),
    ));
  }
  let version_type = document
    .version_type
    .filter(|value| valid_version_type(value))
    .ok_or_else(|| Error::Metadata("invalid or missing client version type".into()))?;

  let java = document
    .java_version
    .ok_or_else(|| Error::Metadata("version has no Java requirement".into()))?;
  validate_java_requirement(&java)?;

  let client_jar = document
    .downloads
    .and_then(|downloads| downloads.client)
    .ok_or_else(|| Error::Metadata("version has no official client download".into()))?;
  let client_jar = parse_client_file(
    client_jar.url,
    client_jar.size,
    client_jar.sha1,
    "client JAR",
    MAX_CLIENT_JAR,
  )?;

  let asset_index = document
    .asset_index
    .ok_or_else(|| Error::Metadata("version has no asset index".into()))?;
  let asset_id = asset_index
    .id
    .filter(|id| is_valid_version_id(id))
    .ok_or_else(|| Error::Metadata("invalid asset index ID".into()))?;
  let asset_file = parse_client_file(
    asset_index.url,
    asset_index.size,
    asset_index.sha1,
    "asset index",
    MAX_ASSET_INDEX,
  )?;

  let main_class = document
    .main_class
    .filter(|class| valid_main_class(class))
    .ok_or_else(|| Error::Metadata("invalid or missing client main class".into()))?;
  let libraries = document
    .libraries
    .filter(|libraries| libraries.iter().all(Value::is_object))
    .ok_or_else(|| Error::Metadata("invalid or missing client libraries".into()))?;
  let arguments = document
    .arguments
    .ok_or_else(|| Error::Metadata("version has no modern client arguments".into()))?;
  let jvm = arguments
    .jvm
    .filter(|values| valid_argument_list(values))
    .ok_or_else(|| Error::Metadata("invalid or missing JVM arguments".into()))?;
  let game = arguments
    .game
    .filter(|values| valid_argument_list(values))
    .ok_or_else(|| Error::Metadata("invalid or missing game arguments".into()))?;
  let logging = document
    .logging
    .and_then(|logging| logging.client)
    .map(parse_client_logging)
    .transpose()?;

  Ok(ClientVersion {
    version_id: version_id.to_owned(),
    version_type,
    metadata_url: verified.url,
    metadata_sha1: verified.sha1,
    java_component: java.component,
    java_major: java.major_version,
    client_jar,
    asset_index: ClientAssetIndex {
      id: asset_id,
      file: asset_file,
    },
    main_class,
    logging,
    libraries,
    arguments: ClientArguments { jvm, game },
  })
}

fn parse_client_logging(logging: LoggingClientDocument) -> Result<ClientLogging> {
  let kind = logging
    .kind
    .filter(|kind| !kind.is_empty() && kind.len() <= 64)
    .ok_or_else(|| Error::Metadata("invalid client logging type".into()))?;
  let argument = logging
    .argument
    .filter(|argument| {
      argument.len() <= 4096
        && argument.matches("${path}").count() == 1
        && !argument.bytes().any(|byte| byte.is_ascii_control())
    })
    .ok_or_else(|| Error::Metadata("invalid client logging argument".into()))?;
  let file = logging
    .file
    .ok_or_else(|| Error::Metadata("client logging configuration has no file".into()))?;
  let file_id = file
    .id
    .filter(|id| validate_metadata_relative_path(id).is_ok())
    .ok_or_else(|| Error::Metadata("invalid client logging file ID".into()))?;
  let file = parse_client_file(
    file.url,
    file.size,
    file.sha1,
    "client logging configuration",
    16 * 1024 * 1024,
  )?;

  Ok(ClientLogging {
    kind,
    argument,
    file_id,
    file,
  })
}

struct VerifiedVersionDocument<T> {
  url: Url,
  sha1: String,
  document: T,
}

async fn fetch_verified_version_document<T>(
  http: &Executor,
  version_id: &str,
  manifest_url: Url,
) -> Result<VerifiedVersionDocument<T>>
where
  T: for<'de> Deserialize<'de>,
{
  validate_metadata_url(&manifest_url)?;
  validate_version_id(version_id)?;

  let response = http
    .get(manifest_url.as_str())
    .max_body_size(VERSION_MANIFEST_LIMIT)
    .send()
    .await?;
  let manifest_bytes = response
    .collect_bounded(VERSION_MANIFEST_LIMIT as usize)
    .await?;
  let manifest: VersionManifest = parse_json(&manifest_bytes)?;
  if manifest.versions.len() > MAX_VERSION_ENTRIES {
    return Err(Error::Metadata("too many versions in manifest".into()));
  }
  let mut matches = manifest
    .versions
    .iter()
    .filter(|entry| entry.id == version_id);
  let entry = matches
    .next()
    .ok_or_else(|| Error::NotFound(format!("Minecraft version {version_id}")))?;
  if matches.next().is_some() {
    return Err(Error::Metadata("duplicate version ID in manifest".into()));
  }
  validate_sha1(&entry.sha1)?;
  let metadata_sha1 = entry.sha1.to_ascii_lowercase();
  let metadata_url =
    Url::parse(&entry.url).map_err(|_| Error::Metadata("invalid version metadata URL".into()))?;
  validate_metadata_url(&metadata_url)?;

  let response = http
    .get(metadata_url.as_str())
    .max_body_size(VERSION_JSON_LIMIT)
    .send()
    .await?;
  let bytes = response
    .collect_bounded(VERSION_JSON_LIMIT as usize)
    .await?;
  if sha1_hex(&bytes) != metadata_sha1 {
    return Err(Error::Metadata(
      "version metadata SHA-1 does not match the manifest".into(),
    ));
  }
  let document: T = parse_json(&bytes)?;

  Ok(VerifiedVersionDocument {
    url: metadata_url,
    sha1: metadata_sha1,
    document,
  })
}

fn parse_client_file(
  url: Option<String>,
  size: Option<u64>,
  sha1: Option<String>,
  label: &str,
  max_size: u64,
) -> Result<ClientFile> {
  let url = Url::parse(
    url
      .as_deref()
      .ok_or_else(|| Error::Metadata(format!("{label} has no URL")))?,
  )
  .map_err(|_| Error::Metadata(format!("invalid {label} URL")))?;
  validate_artifact_url(&url)?;
  let size = size
    .filter(|size| (1..=max_size).contains(size))
    .ok_or_else(|| Error::Metadata(format!("invalid {label} size")))?;
  let sha1 = sha1.ok_or_else(|| Error::Metadata(format!("{label} has no SHA-1")))?;
  validate_sha1(&sha1)?;

  Ok(ClientFile {
    url,
    size,
    sha1: sha1.to_ascii_lowercase(),
  })
}

fn validate_java_requirement(java: &JavaVersion) -> Result<()> {
  if !(8..=99).contains(&java.major_version)
    || java.component.is_empty()
    || java.component.len() > 64
    || !java
      .component
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
  {
    return Err(Error::Metadata("invalid Java requirement".into()));
  }
  Ok(())
}

fn validate_artifact_url(url: &Url) -> Result<()> {
  if url.scheme() != "https"
    || url.host_str().is_none()
    || !url.username().is_empty()
    || url.password().is_some()
    || url.port().is_some_and(|port| port != 443)
  {
    return Err(Error::Metadata("artifact URL must use HTTPS".into()));
  }
  Ok(())
}

fn valid_main_class(value: &str) -> bool {
  !value.is_empty()
    && value.len() <= 256
    && value.split('.').all(|part| {
      let mut bytes = part.bytes();
      bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || matches!(first, b'_' | b'$'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
    })
}

pub(crate) fn valid_argument_list(values: &[Value]) -> bool {
  values.iter().all(|value| match value {
    Value::String(value) => !value.is_empty() && value.len() <= 16 * 1024,
    Value::Object(object) => {
      let valid_value = object.get("value").is_some_and(|value| match value {
        Value::String(value) => !value.is_empty() && value.len() <= 16 * 1024,
        Value::Array(values) => {
          !values.is_empty()
            && values.iter().all(|value| {
              value
                .as_str()
                .is_some_and(|value| !value.is_empty() && value.len() <= 16 * 1024)
            })
        }
        _ => false,
      });
      let valid_rules = object.get("rules").is_none_or(|rules| {
        rules
          .as_array()
          .is_some_and(|rules| rules.iter().all(Value::is_object))
      });
      valid_value && valid_rules
    }
    _ => false,
  })
}

pub(crate) fn valid_version_type(value: &str) -> bool {
  matches!(value, "release" | "snapshot" | "old_beta" | "old_alpha")
}

fn validate_metadata_url(url: &Url) -> Result<()> {
  let host = url.host_str().unwrap_or_default();
  let official_host = host == "mojang.com" || host.ends_with(".mojang.com");
  #[cfg(test)]
  let local_test_server = url.scheme() == "http"
    && url
      .host_str()
      .and_then(|host| host.parse::<std::net::IpAddr>().ok())
      .is_some_and(|ip| ip.is_loopback());
  #[cfg(not(test))]
  let local_test_server = false;
  if !(url.scheme() == "https" && official_host || local_test_server)
    || !url.username().is_empty()
    || url.password().is_some()
    || url
      .port()
      .is_some_and(|port| port != 443 && !local_test_server)
  {
    return Err(Error::Metadata(
      "metadata URL is outside the trusted HTTPS source".into(),
    ));
  }
  Ok(())
}

fn validate_version_id(version_id: &str) -> Result<()> {
  if !is_valid_version_id(version_id) {
    return Err(Error::Invalid("invalid Minecraft version ID".into()));
  }
  Ok(())
}

/// Whether an official version ID is a bounded, portable single path component.
/// Historical Mojang releases include ordinary spaces in their IDs.
pub fn is_valid_version_id(version_id: &str) -> bool {
  !version_id.is_empty()
    && version_id.len() <= 64
    && version_id.as_bytes()[0].is_ascii_alphanumeric()
    && !version_id.ends_with([' ', '.'])
    && version_id
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || b"._+- ".contains(&byte))
}

fn validate_sha1(value: &str) -> Result<()> {
  if value.len() != 40 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
    return Err(Error::Metadata("invalid SHA-1 in Mojang metadata".into()));
  }
  Ok(())
}

fn validate_metadata_relative_path(path: &str) -> Result<()> {
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

fn sha1_hex(bytes: &[u8]) -> String {
  Sha1::digest(bytes)
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect()
}

fn parse_json<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
  serde_json::from_slice(bytes).map_err(|error| Error::Metadata(error.to_string()))
}

#[cfg(test)]
#[path = "../tests/metadata.rs"]
mod tests;
