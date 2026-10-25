//! Local ZIP validation and resource-pack references using the shared layout transaction.
use super::*;
use serde_json::Value;

const MAX_METADATA_BYTES: u64 = 256 * 1024;
const MAX_DESCRIPTION_CHARS: usize = 1024;
const MAX_FORMAT: u32 = i32::MAX as u32;

/// Normalized major/minor format; an omitted maximum minor means all minor versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackFormat {
  pub major: u32,
  pub minor: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackMetadata {
  pub description: String,
  pub pack_format: Option<u32>,
  pub min_format: PackFormat,
  pub max_format: PackFormat,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourcePackEntry {
  pub source: ResourceSource,
  pub sha256: String,
  pub filename: String,
  pub size: u64,
  pub metadata: PackMetadata,
}

impl ResourceManager {
  /// Import a validated ZIP into the shared store and current instance's reference collection.
  /// Publishes a private game/resourcepacks copy; selection in options.txt remains with the game.
  pub async fn import_pack(
    &self,
    name: &str,
    filename: String,
    bytes: Vec<u8>,
    revision: u64,
  ) -> Result<ResourceManifest> {
    validate_resource_filename(&filename, "zip")?;
    if bytes.is_empty() || bytes.len() > MAX_IMPORT_BYTES {
      return Err(Error::Invalid(
        "resource pack ZIP must be between 1 byte and 64 MiB".into(),
      ));
    }
    self
      .access(name, move |directory, data, fabric| {
        let mut manifest = read_manifest(directory)?;
        check_revision(&manifest, revision)?;
        let metadata = inspect_zip(&bytes)?;
        let sha256 = hex(&Sha256::digest(&bytes));
        if let Some(entry) = manifest
          .resource_packs
          .iter()
          .find(|entry| entry.sha256 == sha256)
        {
          verify_object(&object_path(data, &sha256), &sha256, entry.size)?;
          return Ok(manifest);
        }
        if manifest.entry_count() >= MAX_ENTRIES {
          return Err(Error::Invalid(
            "at most 1024 managed resources per instance".into(),
          ));
        }
        if manifest
          .resource_packs
          .iter()
          .any(|entry| entry.filename.eq_ignore_ascii_case(&filename))
        {
          return Err(Error::Conflict(
            "another managed resource pack uses this filename".into(),
          ));
        }
        publish_object(data, &sha256, &bytes)?;
        manifest.resource_packs.push(ResourcePackEntry {
          source: ResourceSource::Local,
          sha256,
          filename,
          size: bytes.len() as u64,
          metadata,
        });
        commit(directory, data, &mut manifest, fabric)?;
        Ok(manifest)
      })
      .await
  }

  /// Remove only this instance's reference; shared objects are retained.
  pub async fn remove_pack(
    &self,
    name: &str,
    sha256: String,
    revision: u64,
  ) -> Result<ResourceManifest> {
    validate_hash(&sha256)?;
    self
      .access(name, move |directory, data, fabric| {
        let mut manifest = read_manifest(directory)?;
        check_revision(&manifest, revision)?;
        let index = manifest
          .resource_packs
          .iter()
          .position(|entry| entry.sha256 == sha256)
          .ok_or_else(|| Error::NotFound("managed resource pack reference".into()))?;
        manifest.resource_packs.remove(index);
        commit(directory, data, &mut manifest, fabric)?;
        Ok(manifest)
      })
      .await
  }
}

pub(super) fn validate_entries(entries: &[ResourcePackEntry]) -> Result<()> {
  let mut hashes = HashSet::new();
  let mut names = HashSet::new();
  for entry in entries {
    validate_hash(&entry.sha256)?;
    validate_resource_filename(&entry.filename, "zip")?;
    if entry.source != ResourceSource::Local
      || entry.size == 0
      || entry.size > MAX_IMPORT_BYTES as u64
      || !hashes.insert(&entry.sha256)
      || !names.insert(entry.filename.to_ascii_lowercase())
    {
      return Err(Error::Metadata(
        "invalid or duplicate resource pack reference".into(),
      ));
    }
    validate_metadata(&entry.metadata)?;
  }
  Ok(())
}
fn invalid() -> Error {
  Error::Invalid("invalid resource pack ZIP or pack.mcmeta".into())
}
fn number(value: &Value) -> Result<u32> {
  value
    .as_u64()
    .filter(|v| *v <= u64::from(MAX_FORMAT))
    .map(|v| v as u32)
    .ok_or_else(invalid)
}
fn format(value: &Value, maximum: bool) -> Result<PackFormat> {
  let (major, minor) = match value {
    Value::Array(values) if (1..=2).contains(&values.len()) => {
      (number(&values[0])?, values.get(1).map(number).transpose()?)
    }
    Value::Number(_) => (number(value)?, None),
    _ => return Err(invalid()),
  };
  Ok(PackFormat {
    major,
    minor: minor.unwrap_or(if maximum { MAX_FORMAT } else { 0 }),
  })
}
fn legacy_range(value: &Value) -> Result<(PackFormat, PackFormat)> {
  let (min, max) = match value {
    Value::Number(_) => (number(value)?, number(value)?),
    Value::Array(values) if values.len() == 2 => (number(&values[0])?, number(&values[1])?),
    Value::Object(map) => (
      number(map.get("min_inclusive").ok_or_else(invalid)?)?,
      number(map.get("max_inclusive").ok_or_else(invalid)?)?,
    ),
    _ => return Err(invalid()),
  };
  if min > max {
    return Err(invalid());
  }
  Ok((
    PackFormat {
      major: min,
      minor: 0,
    },
    PackFormat {
      major: max,
      minor: MAX_FORMAT,
    },
  ))
}
fn validate_metadata(metadata: &PackMetadata) -> Result<()> {
  if metadata.min_format > metadata.max_format
    || [metadata.min_format, metadata.max_format]
      .iter()
      .any(|f| f.major > MAX_FORMAT || f.minor > MAX_FORMAT)
    || metadata.description.chars().count() > MAX_DESCRIPTION_CHARS
    || metadata
      .description
      .chars()
      .any(|c| c.is_control() && c != '\n' && c != '\t')
    || metadata.pack_format.is_some_and(|v| {
      v > MAX_FORMAT || v < metadata.min_format.major || v > metadata.max_format.major
    })
  {
    return Err(invalid());
  }
  Ok(())
}
fn metadata(bytes: &[u8]) -> Result<PackMetadata> {
  let root: Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
  let pack = root
    .get("pack")
    .and_then(Value::as_object)
    .ok_or_else(invalid)?;
  let pack_format = pack.get("pack_format").map(number).transpose()?;
  let supported = pack
    .get("supported_formats")
    .map(legacy_range)
    .transpose()?;
  let (min_format, max_format) = match (pack.get("min_format"), pack.get("max_format")) {
    (Some(min), Some(max)) => {
      let range = (format(min, false)?, format(max, true)?);
      if let Some((min, max)) = supported
        && (min.major != range.0.major || max.major != range.1.major)
      {
        return Err(invalid());
      }
      range
    }
    (None, None) => supported.unwrap_or({
      let major = pack_format.ok_or_else(invalid)?;
      (
        PackFormat { major, minor: 0 },
        PackFormat {
          major,
          minor: MAX_FORMAT,
        },
      )
    }),
    _ => return Err(invalid()),
  };
  // Older resource formats require their legacy version declaration.
  if min_format.major < 65 && pack_format.is_none() {
    return Err(invalid());
  }
  let mut description = String::new();
  let mut budget = 512;
  text(
    pack.get("description").ok_or_else(invalid)?,
    &mut description,
    0,
    &mut budget,
  )?;
  let result = PackMetadata {
    description,
    pack_format,
    min_format,
    max_format,
  };
  validate_metadata(&result)?;
  Ok(result)
}
// Keep plain text only. Styling, click/hover events and URLs are never rendered or followed.
fn text(value: &Value, output: &mut String, depth: usize, budget: &mut usize) -> Result<()> {
  if depth > 16 || *budget == 0 {
    return Err(invalid());
  }
  *budget -= 1;
  let mut push = |value: &str| {
    let remaining = MAX_DESCRIPTION_CHARS.saturating_sub(output.chars().count());
    output.extend(
      value
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(remaining),
    );
  };
  match value {
    Value::String(value) => push(value),
    Value::Bool(_) | Value::Number(_) => push(&value.to_string()),
    Value::Array(values) => {
      for value in values {
        text(value, output, depth + 1, budget)?;
      }
    }
    Value::Object(map) => {
      if let Some(value) = map.get("text") {
        let value = value.as_str().ok_or_else(invalid)?;
        push(value);
      } else if let Some(value) = map.get("translate") {
        push(value.as_str().ok_or_else(invalid)?);
      }
      if let Some(extra) = map.get("extra") {
        if !extra.is_array() {
          return Err(invalid());
        }
        text(extra, output, depth + 1, budget)?;
      }
    }
    _ => return Err(invalid()),
  }
  Ok(())
}
fn safe_entry(name: &[u8], directory: bool) -> Result<()> {
  let name = std::str::from_utf8(name).map_err(|_| invalid())?;
  if name.is_empty()
    || name.len() > 1024
    || name.contains(['\\', ':'])
    || name.chars().any(char::is_control)
  {
    return Err(invalid());
  }
  let path = if directory {
    name.strip_suffix('/').unwrap_or(name)
  } else {
    name
  };
  if path
    .split('/')
    .any(|part| part.is_empty() || part == "." || part == "..")
  {
    return Err(invalid());
  }
  Ok(())
}
pub(super) fn validate_zip_entry<R: Read>(file: &zip::read::ZipFile<'_, R>) -> Result<()> {
  safe_entry(file.name_raw(), file.is_dir())?;
  if file.encrypted()
    || file.is_symlink()
    || file
      .unix_mode()
      .is_some_and(|m| !matches!(m & 0o170000, 0 | 0o040000 | 0o100000))
  {
    return Err(invalid());
  }
  // Per-entry ZIP64 fields can appear even without a ZIP64 end-of-directory record.
  let mut extra = file.extra_data().unwrap_or_default();
  while !extra.is_empty() {
    if extra.len() < 4 {
      return Err(invalid());
    }
    let tag = u16::from_le_bytes([extra[0], extra[1]]);
    let length = u16::from_le_bytes([extra[2], extra[3]]) as usize;
    if tag == 1 || length > extra.len() - 4 {
      return Err(invalid());
    }
    extra = &extra[4 + length..];
  }
  Ok(())
}
fn inspect_zip(bytes: &[u8]) -> Result<PackMetadata> {
  let mut archive = open_zip(bytes)?;
  let mut found = None;
  let mut names = HashSet::new();
  for index in 0..archive.len() {
    let mut file = archive.by_index(index).map_err(|_| invalid())?;
    validate_zip_entry(&file)?;
    if !names.insert(file.name().to_owned()) {
      return Err(invalid());
    }
    if file.name() == "pack.mcmeta" {
      if found.is_some() || file.is_dir() || file.size() > MAX_METADATA_BYTES {
        return Err(invalid());
      }
      let mut content = Vec::new();
      file
        .by_ref()
        .take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut content)
        .map_err(|_| invalid())?;
      if content.len() as u64 > MAX_METADATA_BYTES {
        return Err(invalid());
      }
      found = Some(metadata(&content)?);
    }
  }
  found.ok_or_else(invalid)
}
