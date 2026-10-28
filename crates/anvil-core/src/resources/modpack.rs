//! Register pack resources in an unpublished instance, reusing layout transactions.
use super::*;
use std::collections::BTreeSet;

pub(crate) fn prepare_modpack(
  directory: &Path,
  data: &Path,
  paths: &BTreeSet<String>,
  fabric: bool,
  cancel: &crate::CancellationToken,
) -> Result<()> {
  let mut manifest = ResourceManifest::default();
  let mut managed = Vec::new();
  for path in paths {
    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    let Some((kind, filename, extension)) = modpack_resource(path) else {
      continue;
    };
    if kind == "mods" && !fabric {
      return Err(Error::Invalid("整合包包含模组但没有受支持的加载器".into()));
    }
    validate_resource_filename(filename, extension)?;
    let local = directory.join("game").join(path);
    regular_file(&local, MAX_IMPORT_BYTES as u64)?;
    let bytes = fs::read(&local)?;
    let hash = hex(&Sha256::digest(&bytes));
    match kind {
      "mods" => {
        validate_jar(&bytes)?;
        manifest.mods.push(ResourceEntry {
          origin: None,
          modrinth_origin: None,
          source: ResourceSource::Local,
          sha256: hash.clone(),
          filename: filename.into(),
          size: bytes.len() as u64,
          enabled: true,
        });
      }
      "resourcepacks" => {
        let metadata = packs::inspect_zip(&bytes)?;
        manifest.resource_packs.push(ResourcePackEntry {
          source: ResourceSource::Local,
          sha256: hash.clone(),
          filename: filename.into(),
          size: bytes.len() as u64,
          metadata,
        });
      }
      _ => {
        shaders::inspect_zip(&bytes)?;
        manifest.shader_packs.push(ShaderPackEntry {
          source: ResourceSource::Local,
          sha256: hash.clone(),
          filename: filename.into(),
          size: bytes.len() as u64,
        });
      }
    }
    let _store = storage::client_cache_lock(data, "resources-store")?;
    publish_object(data, &hash, &bytes)?;
    managed.push(local);
  }
  // Files are private to the unpublished instance. Let the shared transaction
  // own their final copies so future sync/remove cannot treat them as external.
  for path in managed {
    fs::remove_file(path)?;
  }
  commit_cancel(directory, data, &mut manifest, fabric, cancel)
}

pub(crate) fn modpack_resource(path: &str) -> Option<(&str, &str, &'static str)> {
  let (kind, filename) = path.split_once('/')?;
  if filename.contains('/') {
    return None;
  }
  let extension = match kind {
    "mods" => "jar",
    "resourcepacks" | "shaderpacks" => "zip",
    _ => return None,
  };
  filename
    .to_ascii_lowercase()
    .ends_with(&format!(".{extension}"))
    .then_some((kind, filename, extension))
}

/// Read-only preflight for selected managed resources; never repairs game files.
/// Caller holds the instance lock, which serializes manifest/layout writers.
pub(crate) fn export_resources(
  directory: &Path,
  selection: &crate::modpack::ExportSelection,
) -> Result<std::collections::BTreeMap<String, (String, u64)>> {
  if !(selection.mods || selection.resource_packs || selection.shader_packs) {
    return Ok(Default::default());
  }
  match fs::symlink_metadata(directory.join("resource-transaction.json")) {
    Ok(_) => {
      return Err(Error::Conflict(
        "资源存在未完成事务，请先同步资源再导出".into(),
      ));
    }
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
    Err(e) => return Err(e.into()),
  }
  let manifest = read_manifest(directory)?;
  if manifest.entry_count() > 0 && manifest.applied_revision != Some(manifest.revision) {
    return Err(Error::Conflict("资源尚未同步，请先同步后再导出".into()));
  }
  let layout = read_layout(directory)?;
  if selection.mods
    && layout.mods
      != manifest
        .mods
        .iter()
        .filter(|e| e.enabled)
        .cloned()
        .collect::<Vec<_>>()
    || selection.resource_packs && layout.resource_packs != manifest.resource_packs
    || selection.shader_packs && layout.shader_packs != manifest.shader_packs
  {
    return Err(Error::Conflict(
      "资源布局与清单不一致，请先同步资源再导出".into(),
    ));
  }
  let mut expected = std::collections::BTreeMap::new();
  if selection.mods {
    for entry in manifest.mods.iter().filter(|e| e.enabled) {
      expected.insert(
        format!("mods/{}", entry.filename),
        (entry.sha256.clone(), entry.size),
      );
    }
  }
  if selection.resource_packs {
    for entry in &manifest.resource_packs {
      expected.insert(
        format!("resourcepacks/{}", entry.filename),
        (entry.sha256.clone(), entry.size),
      );
    }
  }
  if selection.shader_packs {
    for entry in &manifest.shader_packs {
      expected.insert(
        format!("shaderpacks/{}", entry.filename),
        (entry.sha256.clone(), entry.size),
      );
    }
  }
  Ok(expected)
}

pub(crate) fn validate_export_payload(path: &str, bytes: &[u8]) -> Result<()> {
  let Some((kind, filename, extension)) = modpack_resource(path) else {
    return Ok(());
  };
  validate_resource_filename(filename, extension)?;
  if bytes.is_empty() || bytes.len() > MAX_IMPORT_BYTES {
    return Err(Error::Invalid("导出资源文件为空或超过 64 MiB".into()));
  }
  match kind {
    "mods" => validate_jar(bytes),
    "resourcepacks" => packs::inspect_zip(bytes).map(|_| ()),
    _ => shaders::inspect_zip(bytes),
  }
}
