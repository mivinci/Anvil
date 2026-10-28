//! Validated Modrinth index contract. ZIP inspection and installation are separate.
mod archive;
mod export;
mod export_archive;
pub use export::{ExportIndex, ExportOptions, ExportSelection, prepare_export_index};
pub use export_archive::{ExportProgress, ExportResult, ExportStage, ModpackExporter};
mod install;
mod store;
pub use archive::{ArchivePlan, OverrideFile, inspect_archive};
pub use install::{ImportProgress, ImportStage, ModpackInstaller};

use crate::{Error, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use url::Url;

pub const MAX_INDEX_BYTES: usize = 8 * 1024 * 1024;
const MAX_FILES: usize = 10_000;
const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 16 * 1024 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Index {
  format_version: u32,
  game: String,
  version_id: String,
  name: String,
  files: Vec<File>,
  dependencies: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
  pub path: String,
  pub hashes: BTreeMap<String, String>,
  #[serde(default)]
  env: Option<Environment>,
  #[serde(deserialize_with = "deserialize_downloads")]
  pub downloads: Vec<Url>,
  pub file_size: u64,
}

fn deserialize_downloads<'de, D: serde::Deserializer<'de>>(
  deserializer: D,
) -> std::result::Result<Vec<Url>, D::Error> {
  Vec::<String>::deserialize(deserializer)?
    .into_iter()
    .map(|value| {
      if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(serde::de::Error::custom("download URL contains whitespace"));
      }
      Url::parse(&value).map_err(serde::de::Error::custom)
    })
    .collect()
}

#[derive(Clone, Debug, Deserialize)]
struct Environment {
  client: Support,
  #[serde(rename = "server")]
  _server: Support,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Support {
  Required,
  Optional,
  Unsupported,
}

#[derive(Debug)]
pub struct Plan {
  pub name: String,
  pub version_id: String,
  pub minecraft: String,
  pub fabric_loader: Option<String>,
  pub files: Vec<File>,
  pub skipped_files: usize,
  pub total_bytes: u64,
}

fn invalid(message: &str) -> Error {
  Error::Invalid(format!("modpack: {message}"))
}

fn identifier(value: &str) -> bool {
  !value.is_empty()
    && value.len() <= 128
    && value
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || b"._-+".contains(&byte))
}

/// Reject paths that cannot safely identify a portable file inside a new game directory.
pub fn validate_path(path: &str) -> Result<()> {
  if path.is_empty() || path.len() > 1024 || path.split('/').count() > 64 {
    return Err(invalid("invalid file path length"));
  }
  for segment in path.split('/') {
    let stem = segment.split('.').next().unwrap_or("").to_ascii_uppercase();
    if segment.is_empty()
      || segment == "."
      || segment == ".."
      || segment.ends_with([' ', '.'])
      || segment
        .chars()
        .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
      || ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
      || (stem.len() == 4
        && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
      return Err(invalid("unsafe or non-portable file path"));
    }
  }
  Ok(())
}

pub fn parse_index(bytes: &[u8]) -> Result<Plan> {
  if bytes.len() > MAX_INDEX_BYTES {
    return Err(invalid("index exceeds size limit"));
  }
  let index: Index = serde_json::from_slice(bytes)?;
  if index.format_version != 1 || index.game != "minecraft" {
    return Err(invalid("unsupported format or game"));
  }
  if index.name.trim().is_empty()
    || index.name.len() > 256
    || index.name.chars().any(char::is_control)
    || index.version_id.trim().is_empty()
    || index.version_id.len() > 128
  {
    return Err(invalid("invalid pack identity"));
  }
  if index
    .dependencies
    .keys()
    .any(|key| !matches!(key.as_str(), "minecraft" | "fabric-loader"))
  {
    return Err(invalid(
      "unsupported loader or dependency; only vanilla and Fabric are supported",
    ));
  }
  let minecraft = index
    .dependencies
    .get("minecraft")
    .filter(|value| identifier(value))
    .ok_or_else(|| invalid("missing or invalid Minecraft version"))?
    .clone();
  let fabric_loader = index.dependencies.get("fabric-loader").cloned();
  if fabric_loader
    .as_ref()
    .is_some_and(|value| !identifier(value))
  {
    return Err(invalid("invalid Fabric loader version"));
  }
  if index.files.len() > MAX_FILES {
    return Err(invalid("too many files"));
  }
  let mut paths = BTreeSet::new();
  let mut files = Vec::new();
  let mut skipped_files = 0;
  let mut total_bytes = 0u64;
  for file in index.files {
    validate_path(&file.path)?;
    if !paths.insert(file.path.to_lowercase()) {
      return Err(invalid("duplicate or case-conflicting file paths"));
    }
    for (algorithm, length) in [("sha1", 40), ("sha512", 128)] {
      if !file
        .hashes
        .get(algorithm)
        .is_some_and(|hash| hash.len() == length && hash.bytes().all(|b| b.is_ascii_hexdigit()))
      {
        return Err(invalid("missing or invalid required hash"));
      }
    }
    if file.downloads.is_empty()
      || file.downloads.len() > 8
      || file.downloads.iter().any(|url| {
        url.as_str().len() > 8192
          || url.scheme() != "https"
          || !url.username().is_empty()
          || url.password().is_some()
          || url.fragment().is_some()
          || url.port().is_some_and(|port| port != 443)
          || !matches!(
            url.host_str(),
            Some("cdn.modrinth.com" | "github.com" | "raw.githubusercontent.com" | "gitlab.com")
          )
      })
    {
      return Err(invalid("unsupported download URL"));
    }
    if file.file_size > MAX_FILE_BYTES {
      return Err(invalid("file exceeds size limit"));
    }
    total_bytes = total_bytes
      .checked_add(file.file_size)
      .filter(|size| *size <= MAX_TOTAL_BYTES)
      .ok_or_else(|| invalid("pack exceeds total size limit"))?;
    if file
      .env
      .as_ref()
      .is_some_and(|env| matches!(env.client, Support::Unsupported))
    {
      skipped_files += 1;
    } else {
      files.push(file);
    }
  }
  for path in &paths {
    for (offset, _) in path.match_indices('/') {
      if paths.contains(&path[..offset]) {
        return Err(invalid("file conflicts with a parent directory"));
      }
    }
  }
  let total_bytes = files.iter().map(|file| file.file_size).sum();
  Ok(Plan {
    name: index.name,
    version_id: index.version_id,
    minecraft,
    fabric_loader,
    files,
    skipped_files,
    total_bytes,
  })
}

#[cfg(test)]
#[path = "../tests/modpack.rs"]
mod tests;
