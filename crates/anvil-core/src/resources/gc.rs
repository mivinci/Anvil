//! Manual reclamation of unreferenced objects from the managed resource store.
use super::*;
use std::{collections::BTreeMap, sync::Arc};

const MAX_SCAN_ENTRIES: usize = 100_000;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceCleanupPreview {
  pub revision: String,
  pub objects: usize,
  pub bytes: u64,
}

impl ResourceManager {
  pub(super) async fn lock_resource_operation(&self) -> Result<Arc<File>> {
    let data = self.context.paths.data.clone();
    self
      .blocking(move || storage::resource_operation_lock(&data, false))
      .await
  }

  pub async fn preview_cleanup(&self) -> Result<ResourceCleanupPreview> {
    let data = self.context.paths.data.clone();
    self.blocking(move || cleanup(&data, None)).await
  }

  pub async fn confirm_cleanup(&self, revision: &str) -> Result<ResourceCleanupPreview> {
    validate_hash(revision)?;
    let data = self.context.paths.data.clone();
    let revision = revision.to_owned();
    self.blocking(move || cleanup(&data, Some(&revision))).await
  }
}

fn cleanup(data: &Path, confirmed: Option<&str>) -> Result<ResourceCleanupPreview> {
  let _operation = confirmed
    .map(|_| storage::resource_operation_lock(data, true))
    .transpose()?;
  let _store = storage::client_cache_lock(data, "resources-store")?;
  let candidates = candidates(data)?;
  let mut digest = Sha256::new();
  let mut bytes = 0u64;
  for (hash, size) in &candidates {
    digest.update(hash.as_bytes());
    digest.update(size.to_le_bytes());
    bytes = bytes
      .checked_add(*size)
      .ok_or_else(|| Error::Conflict("resource cleanup size overflow".into()))?;
  }
  let preview = ResourceCleanupPreview {
    revision: hex(&digest.finalize()),
    objects: candidates.len(),
    bytes,
  };
  let indexes = if candidates.is_empty() {
    Vec::new()
  } else {
    candidate_indexes(data, &candidates)?
  };
  if let Some(revision) = confirmed {
    if revision != preview.revision {
      return Err(Error::Conflict("resource cleanup preview changed".into()));
    }
    if !candidates.is_empty() {
      // Remove lookup records first so an interrupted cleanup cannot leave
      // a cache hit pointing at an object we have already removed.
      for path in &indexes {
        fs::remove_file(path)?;
      }
      #[cfg(unix)]
      for parent in indexes
        .iter()
        .filter_map(|path| path.parent())
        .collect::<HashSet<_>>()
      {
        File::open(parent)?.sync_all()?;
      }
      for hash in candidates.keys() {
        fs::remove_file(object_path(data, hash))?;
      }
    }
  }
  Ok(preview)
}

fn optional_directory(path: &Path) -> Result<bool> {
  match fs::symlink_metadata(path) {
    Ok(meta) if meta.is_dir() => Ok(true),
    Ok(_) => Err(Error::Conflict("invalid resource cache directory".into())),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
    Err(error) => Err(error.into()),
  }
}

fn candidates(data: &Path) -> Result<BTreeMap<String, u64>> {
  let mut candidates = BTreeMap::new();
  for part in ["resources", "resources/objects", "resources/objects/sha256"] {
    if !optional_directory(&data.join(part))? {
      return Ok(candidates);
    }
  }
  let mut retained = HashSet::new();
  references::visit_instances(data, |directory, _, manifest| {
    let layout = transaction::read_layout(directory)?;
    retained.extend(
      manifest
        .mods
        .into_iter()
        .chain(layout.mods)
        .map(|entry| entry.sha256),
    );
    retained.extend(
      manifest
        .resource_packs
        .into_iter()
        .chain(layout.resource_packs)
        .map(|entry| entry.sha256),
    );
    retained.extend(
      manifest
        .shader_packs
        .into_iter()
        .chain(layout.shader_packs)
        .map(|entry| entry.sha256),
    );
    Ok(())
  })?;
  let root = data.join("resources/objects/sha256");
  let mut inspected = 0;
  for shard in fs::read_dir(root)? {
    let shard = shard?;
    let prefix = shard.file_name();
    let prefix = prefix.to_string_lossy();
    if prefix.len() != 2 || !is_lower_hex(&prefix) || !shard.file_type()?.is_dir() {
      continue;
    }
    for entry in fs::read_dir(shard.path())? {
      let entry = entry?;
      inspected += 1;
      if inspected > MAX_SCAN_ENTRIES {
        return Err(Error::Conflict(
          "resource cleanup object limit exceeded".into(),
        ));
      }
      let name = entry.file_name();
      let Some(hash) = name.to_str() else { continue };
      if validate_hash(hash).is_err()
        || !hash.starts_with(prefix.as_ref())
        || retained.contains(hash)
      {
        continue;
      }
      let meta = fs::symlink_metadata(entry.path())?;
      if meta.is_file() {
        candidates.insert(hash.to_owned(), meta.len());
      }
    }
  }
  Ok(candidates)
}

fn is_lower_hex(value: &str) -> bool {
  value
    .bytes()
    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn candidate_indexes(data: &Path, candidates: &BTreeMap<String, u64>) -> Result<Vec<PathBuf>> {
  #[derive(Deserialize)]
  struct Index {
    sha256: String,
  }

  let mut paths = Vec::new();
  for (kind, length) in [("sha1", 40), ("mrpack", 128)] {
    let root = data.join("resources").join(kind);
    if !optional_directory(&root)? {
      continue;
    }
    for (count, entry) in fs::read_dir(root)?.enumerate() {
      if count >= MAX_SCAN_ENTRIES {
        return Err(Error::Conflict(
          "resource cleanup index limit exceeded".into(),
        ));
      }
      let entry = entry?;
      let name = entry.file_name();
      let Some(key) = name.to_str().and_then(|name| name.strip_suffix(".json")) else {
        continue;
      };
      if key.len() != length || !is_lower_hex(key) {
        continue;
      }
      let path = entry.path();
      regular_file(&path, 1024)?;
      let index: Index = storage::read_json_bounded(&path, 1024)?;
      validate_hash(&index.sha256)?;
      if candidates.contains_key(&index.sha256) {
        paths.push(path);
      }
    }
  }
  Ok(paths)
}

#[cfg(test)]
#[path = "../../tests/resource_cleanup.rs"]
mod tests;
