//! Read committed references under the same store lock used by resource writes.
use super::*;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceReferenceKind {
  Mod,
  ResourcePack,
  ShaderPack,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceReference {
  pub instance_id: String,
  pub instance_name: String,
  pub kind: ResourceReferenceKind,
  pub filename: String,
  pub enabled: Option<bool>,
}

impl ResourceManager {
  pub async fn references(&self, sha256: &str) -> Result<Vec<ResourceReference>> {
    validate_hash(sha256)?;
    let data = self.context.paths.data.clone();
    let sha256 = sha256.to_owned();
    self.blocking(move || scan(&data, &sha256)).await
  }
}

fn scan(data: &Path, sha256: &str) -> Result<Vec<ResourceReference>> {
  let _store = storage::client_cache_lock(data, "resources-store")?;
  let mut references = Vec::new();
  visit_instances(data, |_, instance, manifest| {
    let id = &instance.id;
    let mut add = |hash: &str, filename: String, kind, enabled| {
      if hash == sha256 {
        references.push(ResourceReference {
          instance_id: id.clone(),
          instance_name: instance.name.clone(),
          kind,
          filename,
          enabled,
        });
      }
    };
    for entry in manifest.mods {
      add(
        &entry.sha256,
        entry.filename,
        ResourceReferenceKind::Mod,
        Some(entry.enabled),
      );
    }
    for entry in manifest.resource_packs {
      add(
        &entry.sha256,
        entry.filename,
        ResourceReferenceKind::ResourcePack,
        None,
      );
    }
    for entry in manifest.shader_packs {
      add(
        &entry.sha256,
        entry.filename,
        ResourceReferenceKind::ShaderPack,
        None,
      );
    }
    Ok(())
  })?;
  references.sort_by(|left, right| {
    left
      .instance_id
      .cmp(&right.instance_id)
      .then(left.filename.cmp(&right.filename))
  });
  Ok(references)
}

pub(super) fn visit_instances(
  data: &Path,
  mut visit: impl FnMut(&Path, ClientInstance, ResourceManifest) -> Result<()>,
) -> Result<()> {
  let root = data.join("instances");
  match fs::symlink_metadata(&root) {
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
    Err(error) => return Err(error.into()),
    Ok(_) => ensure_directory(&root, false)?,
  }
  let root_canonical = fs::canonicalize(&root)?;
  if root_canonical.parent() != Some(fs::canonicalize(data)?.as_path()) {
    return Err(Error::Conflict(
      "instances root escaped the data directory".into(),
    ));
  }
  let mut inspected = 0;
  let mut record_bytes = 0;
  for entry in fs::read_dir(&root)? {
    let entry = entry?;
    let id = entry.file_name().to_string_lossy().into_owned();
    if !id.starts_with("client-") {
      continue;
    }
    inspected += 1;
    if inspected > 512 {
      return Err(Error::Conflict(
        "resource reference scan exceeds instance limit".into(),
      ));
    }
    let directory = entry.path();
    ensure_directory(&directory, false)?;
    if fs::canonicalize(&directory)?.parent() != Some(root_canonical.as_path()) {
      return Err(Error::Conflict(
        "resource instance path escaped its root".into(),
      ));
    }
    let record = directory.join("instance.json");
    regular_file(&record, 64 * 1024)?;
    let instance: ClientInstance = storage::read_json(&record)?;
    crate::client::validate_client_instance(&instance)?;
    if instance.id != id {
      return Err(Error::Conflict("client instance ID mismatch".into()));
    }
    if super::transaction::has_pending_transaction(&directory)? {
      return Err(Error::Conflict(
        "instance has a pending resource transaction".into(),
      ));
    }
    match fs::symlink_metadata(directory.join("resources.json")) {
      Ok(metadata) => record_bytes += metadata.len().min(MAX_RECORD_BYTES as u64 + 1),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
      Err(error) => return Err(error.into()),
    }
    if record_bytes > 64 * 1024 * 1024 {
      return Err(Error::Conflict(
        "resource reference scan exceeds metadata limit".into(),
      ));
    }
    visit(&directory, instance, read_manifest(&directory)?)?;
  }
  Ok(())
}

#[cfg(test)]
#[path = "../../tests/resource_references.rs"]
mod tests;
