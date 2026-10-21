//! Minimal Mojang metadata resolution for original game servers.
use q2q_http::Executor;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use url::Url;

use crate::{Error, Result};

const VERSION_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const VERSION_MANIFEST_LIMIT: u64 = 8 * 1024 * 1024;
const VERSION_JSON_LIMIT: u64 = 4 * 1024 * 1024;
const MAX_VERSION_ENTRIES: usize = 100_000;
const MAX_SERVER_ARTIFACT: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerArtifact {
  pub version_id: String,
  pub java_component: String,
  pub java_major: u32,
  pub url: Url,
  pub size: u64,
  pub sha1: String,
}

#[derive(Deserialize)]
struct VersionManifest {
  versions: Vec<ManifestEntry>,
}
#[derive(Deserialize)]
struct ManifestEntry {
  id: String,
  url: String,
  sha1: String,
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

/// Resolve a released or snapshot version from Mojang's HTTPS metadata chain.
/// The version JSON is authenticated against the SHA-1 in the version manifest before
/// it is parsed. All metadata reads use the caller's shared HTTP Executor.
pub async fn resolve_server(http: &Executor, version_id: &str) -> Result<ServerArtifact> {
  resolve_server_from(
    http,
    version_id,
    Url::parse(VERSION_MANIFEST).expect("constant URL"),
  )
  .await
}

async fn resolve_server_from(
  http: &Executor,
  version_id: &str,
  manifest_url: Url,
) -> Result<ServerArtifact> {
  validate_metadata_url(&manifest_url)?;
  if version_id.is_empty()
    || version_id.len() > 64
    || !version_id
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
  {
    return Err(Error::Invalid("invalid Minecraft version ID".into()));
  }

  let response = http
    .get(manifest_url.as_str())
    .max_body_size(VERSION_MANIFEST_LIMIT)
    .send()
    .await?;
  let bytes = response
    .collect_bounded(VERSION_MANIFEST_LIMIT as usize)
    .await?;
  let manifest: VersionManifest = parse_json(&bytes)?;
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
  let document_url =
    Url::parse(&entry.url).map_err(|_| Error::Metadata("invalid version metadata URL".into()))?;
  validate_metadata_url(&document_url)?;

  let response = http
    .get(document_url.as_str())
    .max_body_size(VERSION_JSON_LIMIT)
    .send()
    .await?;
  let document_bytes = response
    .collect_bounded(VERSION_JSON_LIMIT as usize)
    .await?;
  if sha1_hex(&document_bytes) != entry.sha1.to_ascii_lowercase() {
    return Err(Error::Metadata(
      "version metadata SHA-1 does not match the manifest".into(),
    ));
  }
  let document: VersionDocument = parse_json(&document_bytes)?;
  if document.id != version_id {
    return Err(Error::Metadata(
      "version metadata ID does not match the manifest entry".into(),
    ));
  }
  let java = document
    .java_version
    .ok_or_else(|| Error::Metadata("version has no Java requirement".into()))?;
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
  if url.scheme() != "https"
    || url.host_str().is_none()
    || !url.username().is_empty()
    || url.password().is_some()
    || url.port().is_some_and(|port| port != 443)
  {
    return Err(Error::Metadata("server download must use HTTPS".into()));
  }
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

fn validate_sha1(value: &str) -> Result<()> {
  if value.len() != 40 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
    return Err(Error::Metadata("invalid SHA-1 in Mojang metadata".into()));
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
#[path = "../tests/unit/server_metadata.rs"]
mod tests;
