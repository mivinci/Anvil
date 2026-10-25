//! Local shader ZIP references; game-side selection remains with Iris.
use super::*;
use crate::CancellationToken;
use tokio::sync::watch;

const MAX_SOURCE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShaderPackEntry {
  pub source: ResourceSource,
  pub sha256: String,
  pub filename: String,
  pub size: u64,
}

/// A resolved Modrinth shader version ready for a background download/import.
#[derive(Clone, Debug)]
pub struct ShaderInstallPlan {
  pub name: String,
  pub revision: u64,
  pub project_id: String,
  pub project_name: String,
  pub version_id: String,
  pub version_name: String,
  pub file_name: String,
  pub file_size: u64,
  pub sha1: String,
  pub url: String,
}

impl ResourceManager {
  /// Import a validated ZIP into the shared store and current instance's reference collection.
  /// Publishes a private game/shaderpacks copy; shader selection and settings remain with the game.
  pub async fn import_shader_pack(
    &self,
    name: &str,
    filename: String,
    bytes: Vec<u8>,
    revision: u64,
  ) -> Result<ResourceManifest> {
    validate_resource_filename(&filename, "zip")?;
    if bytes.is_empty() || bytes.len() > MAX_IMPORT_BYTES {
      return Err(Error::Invalid(
        "shader pack ZIP must be between 1 byte and 64 MiB".into(),
      ));
    }
    self
      .access(name, move |directory, data, fabric| {
        let mut manifest = read_manifest(directory)?;
        check_revision(&manifest, revision)?;
        inspect_zip(&bytes)?;
        let sha256 = hex(&Sha256::digest(&bytes));
        if let Some(entry) = manifest
          .shader_packs
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
          .shader_packs
          .iter()
          .any(|entry| entry.filename.eq_ignore_ascii_case(&filename))
        {
          return Err(Error::Conflict(
            "another managed shader pack uses this filename".into(),
          ));
        }
        publish_object(data, &sha256, &bytes)?;
        manifest.shader_packs.push(ShaderPackEntry {
          source: ResourceSource::Local,
          sha256,
          filename,
          size: bytes.len() as u64,
        });
        commit(directory, data, &mut manifest, fabric)?;
        Ok(manifest)
      })
      .await
  }

  /// Download a resolved shader ZIP through the shared object store, then import it.
  pub async fn install_shader(
    &self,
    plan: ShaderInstallPlan,
    cancel: CancellationToken,
    progress: watch::Sender<ResourceInstallProgress>,
  ) -> Result<ResourceManifest> {
    let download = super::transfer::DownloadFile {
      filename: plan.file_name.clone(),
      size: plan.file_size,
      sha1: plan.sha1.clone(),
      url: plan.url.clone(),
      extension: "zip",
    };
    let sha256 = self.prepare_shared(&download, &cancel, &progress).await?;
    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    progress.send_modify(|value| value.stage = ResourceInstallStage::Publishing);
    let bytes = std::fs::read(object_path(&self.context.paths.data, &sha256))?;
    self
      .import_shader_pack(&plan.name, plan.file_name, bytes, plan.revision)
      .await
  }

  /// Remove only this instance's reference; shared objects are retained.
  pub async fn remove_shader_pack(
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
          .shader_packs
          .iter()
          .position(|entry| entry.sha256 == sha256)
          .ok_or_else(|| Error::NotFound("managed shader pack reference".into()))?;
        manifest.shader_packs.remove(index);
        commit(directory, data, &mut manifest, fabric)?;
        Ok(manifest)
      })
      .await
  }
}

pub(super) fn validate_entries(entries: &[ShaderPackEntry]) -> Result<()> {
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
        "invalid or duplicate shader pack reference".into(),
      ));
    }
  }
  Ok(())
}
fn invalid() -> Error {
  Error::Invalid("invalid shader pack ZIP; expected shader source files under root shaders/".into())
}
fn inspect_zip(bytes: &[u8]) -> Result<()> {
  let mut archive = open_zip(bytes)?;
  let mut names = HashSet::new();
  let mut files = HashSet::new();
  let mut directories = HashSet::new();
  let mut found = false;
  for index in 0..archive.len() {
    let mut file = archive.by_index(index).map_err(|_| invalid())?;
    packs::validate_zip_entry(&file).map_err(|_| invalid())?;
    let name = file.name().trim_end_matches('/').to_ascii_lowercase();
    if !names.insert(name.clone()) {
      return Err(invalid());
    }
    // A file may not also name a directory, explicitly or implicitly.
    let mut parts = name.split('/').peekable();
    let mut parent = String::new();
    while let Some(part) = parts.next() {
      if !parent.is_empty() {
        parent.push('/');
      }
      parent.push_str(part);
      if parts.peek().is_some() || file.is_dir() {
        if files.contains(&parent) {
          return Err(invalid());
        }
        directories.insert(parent.clone());
      }
    }
    if !file.is_dir() && (!files.insert(name.clone()) || directories.contains(&name)) {
      return Err(invalid());
    }
    let source = file.name().starts_with("shaders/")
      && Path::new(file.name())
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|ext| matches!(ext, "vsh" | "fsh" | "gsh" | "csh" | "glsl"));
    if !found && !file.is_dir() && source {
      if file.size() == 0 || file.size() > MAX_SOURCE_BYTES {
        return Err(invalid());
      }
      let mut content = Vec::new();
      file
        .by_ref()
        .take(MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut content)
        .map_err(|_| invalid())?;
      if content.len() as u64 != file.size()
        || std::str::from_utf8(&content)
          .map_err(|_| invalid())?
          .trim()
          .is_empty()
      {
        return Err(invalid());
      }
      found = true;
    }
  }
  if !found {
    return Err(invalid());
  }
  Ok(())
}

#[cfg(test)]
#[path = "../../tests/shader_packs.rs"]
pub(super) mod tests;
