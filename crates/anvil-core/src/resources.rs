//! Local resource objects, instance references, and recoverable Fabric mod layouts.
use crate::{
  Context, Error, Result,
  client::{ClientInstaller, ClientInstance},
  storage,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
  collections::HashSet,
  fs::{self, File},
  io::{Read, Write},
  path::{Path, PathBuf},
};

pub const MAX_IMPORT_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 1024;
const MAX_RECORD_BYTES: usize = 1024 * 1024;

mod shader_environment;
pub use shader_environment::{
  ShaderEnvironment, ShaderEnvironmentStatus, ShaderMod, ShaderPackCollection,
};
mod shaders;
pub use shaders::ShaderPackEntry;
mod packs;
pub use packs::{PackFormat, PackMetadata, ResourcePackEntry};
mod modrinth;
mod platform;
pub use modrinth::{ModrinthInstallPlan, ModrinthOrigin, ModrinthPlanFile};
mod transfer;
pub use platform::{
  ResourceInstallPlan, ResourceInstallProgress, ResourceInstallStage, ResourcePlanFile,
};
mod transaction;
pub(crate) use transaction::sync_for_launch;
use transaction::{read_layout, recover, transact};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceSource {
  Local,
  Curseforge,
  Modrinth,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CurseForgeOrigin {
  pub project_id: u32,
  pub file_id: u32,
  pub project_name: String,
  pub sha1: String,
  pub direct: bool,
  pub required_projects: Vec<u32>,
  pub incompatible_projects: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceEntry {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub origin: Option<CurseForgeOrigin>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub modrinth_origin: Option<Box<ModrinthOrigin>>,
  pub source: ResourceSource,
  pub sha256: String,
  pub filename: String,
  pub size: u64,
  pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceManifest {
  pub schema_version: u32,
  pub revision: u64,
  #[serde(default)]
  pub applied_revision: Option<u64>,
  pub mods: Vec<ResourceEntry>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub resource_packs: Vec<ResourcePackEntry>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub shader_packs: Vec<ShaderPackEntry>,
}
impl ResourceManifest {
  fn entry_count(&self) -> usize {
    self.mods.len() + self.resource_packs.len() + self.shader_packs.len()
  }
}
impl Default for ResourceManifest {
  fn default() -> Self {
    Self {
      schema_version: 1,
      revision: 0,
      applied_revision: None,
      mods: Vec::new(),
      resource_packs: Vec::new(),
      shader_packs: Vec::new(),
    }
  }
}

#[derive(Clone)]
pub struct ResourceManager {
  context: Context,
}
impl ResourceManager {
  pub fn new(context: Context) -> Self {
    Self { context }
  }

  pub async fn list(&self, name: &str) -> Result<ResourceManifest> {
    self
      .access(name, move |directory, _, _| read_manifest(directory))
      .await
  }
  pub async fn import(
    &self,
    name: &str,
    filename: String,
    bytes: Vec<u8>,
    revision: u64,
  ) -> Result<ResourceManifest> {
    validate_filename(&filename)?;
    if bytes.is_empty() || bytes.len() > MAX_IMPORT_BYTES {
      return Err(Error::Invalid(
        "mod JAR must be between 1 byte and 64 MiB".into(),
      ));
    }
    self
      .access(name, move |directory, data, fabric| {
        let mut manifest = read_manifest(directory)?;
        check_revision(&manifest, revision)?;
        validate_jar(&bytes)?;
        let sha256 = hex(&Sha256::digest(&bytes));
        if let Some(entry) = manifest.mods.iter().find(|entry| entry.sha256 == sha256) {
          verify_object(&object_path(data, &sha256), &sha256, entry.size)?;
          return Ok(manifest);
        }
        if manifest.entry_count() >= MAX_ENTRIES {
          return Err(Error::Invalid(
            "at most 1024 managed resources per instance".into(),
          ));
        }
        if manifest
          .mods
          .iter()
          .any(|entry| entry.filename.eq_ignore_ascii_case(&filename))
        {
          return Err(Error::Conflict(
            "another managed mod uses this filename".into(),
          ));
        }
        publish_object(data, &sha256, &bytes)?;
        manifest.mods.push(ResourceEntry {
          origin: None,
          modrinth_origin: None,
          source: ResourceSource::Local,
          sha256,
          filename,
          size: bytes.len() as u64,
          enabled: false,
        });
        commit(directory, data, &mut manifest, fabric)?;
        Ok(manifest)
      })
      .await
  }
  pub async fn change(
    &self,
    name: &str,
    sha256: String,
    revision: u64,
    enabled: Option<bool>,
  ) -> Result<ResourceManifest> {
    validate_hash(&sha256)?;
    self
      .access(name, move |directory, data, fabric| {
        let mut manifest = read_manifest(directory)?;
        check_revision(&manifest, revision)?;
        let index = manifest
          .mods
          .iter()
          .position(|entry| entry.sha256 == sha256)
          .ok_or_else(|| Error::NotFound("managed mod reference".into()))?;
        match enabled {
          Some(value) => {
            if value && !fabric {
              return Err(Error::Invalid(
                "enabled mods require a Fabric instance".into(),
              ));
            }
            if value {
              let entry = &manifest.mods[index];
              verify_object(&object_path(data, &sha256), &sha256, entry.size)?;
            }
            if manifest.mods[index].enabled == value {
              return Ok(manifest);
            }
            manifest.mods[index].enabled = value;
          }
          None => {
            manifest.mods.remove(index);
          }
        }
        platform::validate_dependencies(&manifest)?;
        // The shared object is retained; only this instance layout changes.
        commit(directory, data, &mut manifest, fabric)?;
        Ok(manifest)
      })
      .await
  }

  pub async fn sync(&self, name: &str, revision: u64) -> Result<ResourceManifest> {
    self
      .access(name, move |directory, data, fabric| {
        let mut manifest = read_manifest(directory)?;
        check_revision(&manifest, revision)?;
        commit(directory, data, &mut manifest, fabric)?;
        Ok(manifest)
      })
      .await
  }

  async fn access<T, F>(&self, name: &str, action: F) -> Result<T>
  where
    T: Send + 'static,
    F: FnOnce(&Path, &Path, bool) -> Result<T> + Send + 'static,
  {
    let (instance, _) = ClientInstaller::new(self.context.clone())
      .details(name)
      .await?;
    let context = self.context.clone();
    let permit = context
      .blocking
      .clone()
      .acquire_owned()
      .await
      .map_err(|_| Error::Cancelled)?;
    tokio::task::spawn_blocking(move || {
      let _permit = permit;
      let _instance = storage::server_instance_lock(&context.paths.data, &instance.id)?;
      let root = context.paths.data.join("instances");
      ensure_directory(&root, false)?;
      let directory = root.join(&instance.id);
      ensure_directory(&directory, false)?;
      let record = directory.join("instance.json");
      regular_file(&record, 64 * 1024)?;
      let stored: ClientInstance = storage::read_json(&record)?;
      if stored != instance {
        return Err(Error::Conflict("client identity changed".into()));
      }
      ensure_directory(&directory.join("game"), false)?;
      let installed_path = directory.join("installed.json");
      regular_file(
        &installed_path,
        crate::client::MAX_INSTALL_RECORD_SIZE as u64,
      )?;
      let installed: crate::client::ClientInstallRecord =
        storage::read_json_bounded(&installed_path, crate::client::MAX_INSTALL_RECORD_SIZE)?;
      crate::client::validate_install_record(&installed)?;
      let _objects = storage::client_cache_lock(&context.paths.data, "resources-store")?;
      recover(&directory)?;
      action(&directory, &context.paths.data, installed.loader.is_some())
    })
    .await?
  }
}

fn read_manifest(directory: &Path) -> Result<ResourceManifest> {
  let path = directory.join("resources.json");
  match fs::symlink_metadata(&path) {
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
      return Ok(ResourceManifest::default());
    }
    Err(error) => return Err(error.into()),
    Ok(_) => regular_file(&path, MAX_RECORD_BYTES as u64)?,
  }
  let manifest: ResourceManifest = storage::read_json_bounded(&path, MAX_RECORD_BYTES)?;
  validate_manifest(&manifest)?;
  Ok(manifest)
}
fn validate_manifest(manifest: &ResourceManifest) -> Result<()> {
  if !matches!(manifest.schema_version, 1..=5)
    || manifest.entry_count() > MAX_ENTRIES
    || manifest.revision > 9_007_199_254_740_990
    || manifest
      .applied_revision
      .is_some_and(|revision| revision > manifest.revision)
  {
    return Err(Error::Metadata("invalid resources manifest".into()));
  }
  let mut projects = HashSet::new();
  let mut hashes = HashSet::new();
  let mut names = HashSet::new();
  for entry in &manifest.mods {
    match (&entry.source, &entry.origin) {
      (ResourceSource::Local, None) => {}
      (ResourceSource::Modrinth, None) if manifest.schema_version >= 4 => {}
      (ResourceSource::Curseforge, Some(origin)) if manifest.schema_version >= 2 => {
        platform::validate_origin(origin)?;
        if !projects.insert(origin.project_id) {
          return Err(Error::Metadata("duplicate CurseForge project".into()));
        }
      }
      _ => return Err(Error::Metadata("invalid resource source".into())),
    }
    validate_hash(&entry.sha256)?;
    validate_filename(&entry.filename)?;
    if entry.size == 0
      || entry.size > MAX_IMPORT_BYTES as u64
      || !hashes.insert(&entry.sha256)
      || !names.insert(entry.filename.to_ascii_lowercase())
    {
      return Err(Error::Metadata(
        "invalid or duplicate resource reference".into(),
      ));
    }
  }
  if manifest.schema_version < 3 && !manifest.resource_packs.is_empty() {
    return Err(Error::Metadata(
      "resource packs require manifest schema 3".into(),
    ));
  }
  packs::validate_entries(&manifest.resource_packs)?;
  if manifest.schema_version < 5 && !manifest.shader_packs.is_empty() {
    return Err(Error::Metadata(
      "shader packs require manifest schema 5".into(),
    ));
  }
  shaders::validate_entries(&manifest.shader_packs)?;
  if serde_json::to_vec_pretty(manifest)?.len() > MAX_RECORD_BYTES {
    return Err(Error::Metadata(
      "resource manifest exceeds size limit".into(),
    ));
  }
  platform::validate_dependencies(manifest)?;
  Ok(())
}
fn check_revision(manifest: &ResourceManifest, expected: u64) -> Result<()> {
  if manifest.revision != expected {
    return Err(Error::Conflict(
      "resources changed; refresh before retrying".into(),
    ));
  }
  Ok(())
}
fn commit(
  directory: &Path,
  data: &Path,
  manifest: &mut ResourceManifest,
  fabric: bool,
) -> Result<()> {
  commit_cancel(
    directory,
    data,
    manifest,
    fabric,
    &crate::CancellationToken::new(),
  )
}
fn commit_cancel(
  directory: &Path,
  data: &Path,
  manifest: &mut ResourceManifest,
  fabric: bool,
  cancel: &crate::CancellationToken,
) -> Result<()> {
  let before = read_manifest(directory)?;
  manifest.schema_version = 5;
  validate_manifest(manifest)?;
  if cancel.is_cancelled() {
    return Err(Error::Cancelled);
  }
  manifest.revision = manifest
    .revision
    .checked_add(1)
    .filter(|value| *value <= 9_007_199_254_740_990)
    .ok_or_else(|| Error::Conflict("resource revision exhausted".into()))?;
  if !fabric
    && (manifest.mods.iter().any(|entry| entry.enabled) || !read_layout(directory)?.mods.is_empty())
  {
    return Err(Error::Conflict(
      "enabled managed mods require Fabric".into(),
    ));
  }
  manifest.applied_revision = Some(manifest.revision);
  transact(directory, data, &before, manifest, cancel)
}
pub(crate) fn validate_filename(filename: &str) -> Result<()> {
  validate_resource_filename(filename, "jar")
}
fn validate_resource_filename(filename: &str, extension: &str) -> Result<()> {
  let stem = filename
    .split('.')
    .next()
    .unwrap_or("")
    .to_ascii_uppercase();
  if filename.len() > 128
    || filename.len() < 5
    || !filename
      .to_ascii_lowercase()
      .ends_with(&format!(".{extension}"))
    || !filename.as_bytes()[0].is_ascii_alphanumeric()
    || !filename
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || b"._+- ".contains(&byte))
    || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
    || (stem.len() == 4
      && (stem.starts_with("COM") || stem.starts_with("LPT"))
      && stem.as_bytes()[3].is_ascii_digit())
  {
    return Err(Error::Invalid(format!(
      "use a .{extension} filename starting with an ASCII letter/digit, containing only letters, digits, spaces and . _ + - (max 128 bytes)"
    )));
  }
  Ok(())
}
fn validate_hash(hash: &str) -> Result<()> {
  if hash.len() != 64
    || !hash
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
  {
    return Err(Error::Invalid("invalid resource digest".into()));
  }
  Ok(())
}
// Caller holds the shared object-store lock. Both local JAR and ZIP imports use this path.
fn publish_object(data: &Path, hash: &str, bytes: &[u8]) -> Result<()> {
  validate_hash(hash)?;
  for path in ["resources", "resources/objects", "resources/objects/sha256"] {
    ensure_directory(&data.join(path), true)?;
  }
  let path = object_path(data, hash);
  let parent = path.parent().expect("object parent");
  ensure_directory(parent, true)?;
  match fs::symlink_metadata(&path) {
    Ok(_) => verify_object(&path, hash, bytes.len() as u64),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
      let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
      temporary.write_all(bytes)?;
      temporary.as_file().sync_all()?;
      temporary
        .persist_noclobber(&path)
        .map_err(|error| Error::Io(error.error))?;
      #[cfg(unix)]
      File::open(parent)?.sync_all()?;
      Ok(())
    }
    Err(error) => Err(error.into()),
  }
}
fn object_path(data: &Path, hash: &str) -> PathBuf {
  data
    .join("resources/objects/sha256")
    .join(&hash[..2])
    .join(hash)
}
fn ensure_directory(path: &Path, create: bool) -> Result<()> {
  match fs::symlink_metadata(path) {
    Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => Err(Error::Conflict(
      "resource directory is not a regular directory".into(),
    )),
    Ok(_) => Ok(()),
    Err(error) if create && error.kind() == std::io::ErrorKind::NotFound => {
      fs::create_dir(path)?;
      #[cfg(unix)]
      if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
      }
      Ok(())
    }
    Err(error) => Err(error.into()),
  }
}
fn regular_file(path: &Path, max_size: u64) -> Result<()> {
  let meta = fs::symlink_metadata(path)?;
  if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > max_size {
    return Err(Error::Conflict(
      "resource file is invalid or exceeds its limit".into(),
    ));
  }
  Ok(())
}
fn verify_object(path: &Path, expected: &str, size: u64) -> Result<()> {
  // Validate every parent below the trusted data root, including existing objects.
  let mut parent = path.parent();
  for _ in 0..4 {
    if let Some(directory) = parent {
      ensure_directory(directory, false)?;
      parent = directory.parent();
    }
  }
  verify_file(path, expected, size)
}
fn verify_file(path: &Path, expected: &str, size: u64) -> Result<()> {
  regular_file(path, MAX_IMPORT_BYTES as u64)?;
  if fs::metadata(path)?.len() != size {
    return Err(Error::Conflict("shared resource size mismatch".into()));
  }
  let mut file = File::open(path)?;
  let mut hash = Sha256::new();
  let mut buffer = [0u8; 64 * 1024];
  let mut total = 0;
  loop {
    let count = file.read(&mut buffer)?;
    if count == 0 {
      break;
    }
    total += count;
    if total > MAX_IMPORT_BYTES {
      return Err(Error::Conflict("resource grew during validation".into()));
    }
    hash.update(&buffer[..count]);
  }
  if total as u64 != size || hex(&hash.finalize()) != expected {
    return Err(Error::Conflict("shared resource checksum mismatch".into()));
  }
  Ok(())
}

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// Reject oversized central-directory counts before ZIP parsing allocates entry records.
// ZIP64 is unnecessary for the bounded local-import format and is not accepted here.
fn validate_jar(bytes: &[u8]) -> Result<()> {
  open_zip(bytes).map(|_| ())
}
fn open_zip(bytes: &[u8]) -> Result<zip::ZipArchive<std::io::Cursor<&[u8]>>> {
  let invalid = || Error::Invalid("invalid JAR/ZIP; expected 1–4096 entries without ZIP64".into());
  let start = bytes.len().saturating_sub(65_557);
  let at = (start..bytes.len().saturating_sub(21))
    .rev()
    .find(|&at| {
      bytes.get(at..at + 4) == Some(b"PK\x05\x06".as_slice())
        && at + 22 + u16::from_le_bytes([bytes[at + 20], bytes[at + 21]]) as usize == bytes.len()
    })
    .ok_or_else(invalid)?;
  let short = |offset| u16::from_le_bytes([bytes[at + offset], bytes[at + offset + 1]]);
  let count = short(10);
  if at >= 20 && &bytes[at - 20..at - 16] == b"PK\x06\x07" {
    return Err(invalid());
  }
  if short(4) != 0 || short(6) != 0 || short(8) != count || count == 0 || count > 4096 {
    return Err(invalid());
  }
  let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| invalid())?;
  if archive.len() != count as usize {
    return Err(invalid());
  }
  Ok(archive)
}
