//! Shared bounded disk snapshots for public catalogue metadata; policy stays with each provider.
use crate::{Context, Error, Result, storage};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
  fs,
  path::PathBuf,
  time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn valid_image_url(value: &str, hosts: &[&str]) -> bool {
  value.len() <= 2048
    && url::Url::parse(value).ok().is_some_and(|url| {
      url.scheme() == "https"
        && url.host_str().is_some_and(|host| hosts.contains(&host))
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
    })
}
#[derive(Clone, Copy)]
pub(crate) struct Policy {
  pub namespace: &'static str,
  pub entries: usize,
  pub bytes: usize,
}
pub(crate) const MINECRAFT: Policy = Policy {
  namespace: "minecraft",
  entries: 1,
  bytes: 8 * 1024 * 1024 + 4096,
};
pub(crate) const FABRIC: Policy = Policy {
  namespace: "fabric",
  entries: 8,
  bytes: 512 * 1024,
};
pub(crate) const CURSEFORGE: Policy = Policy {
  namespace: "curseforge",
  entries: 16,
  bytes: 2 * 1024 * 1024 + 4096,
};
#[cfg(test)]
#[path = "../tests/discovery_cache.rs"]
mod tests;

pub(crate) const MODRINTH: Policy = Policy {
  namespace: "modrinth",
  entries: 16,
  bytes: 4 * 1024 * 1024 + 4096,
};
pub(crate) const MODRINTH_SHADER: Policy = Policy {
  namespace: "modrinth-shader",
  entries: 16,
  bytes: 2 * 1024 * 1024 + 4096,
};
pub(crate) const NEWS: Policy = Policy {
  namespace: "news",
  entries: 1,
  bytes: 256 * 1024,
};
pub(crate) const ACCOUNT_SKINS: Policy = Policy {
  namespace: "account-skins",
  entries: 16,
  bytes: 16 * 1024,
};
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope<T> {
  schema_version: u32,
  fetched_at_seconds: u64,
  #[serde(alias = "manifest")]
  payload: T,
}
pub(crate) struct Snapshot<T> {
  pub value: T,
  pub fetched_at: SystemTime,
}
fn directory(context: &Context, policy: Policy, create: bool) -> Result<Option<PathBuf>> {
  let root = &context.paths.cache;
  let mut paths = vec![root.clone(), root.join("metadata")];
  if policy.namespace != "minecraft" {
    paths.push(root.join("metadata").join(policy.namespace));
  }
  for path in &paths {
    if create {
      fs::create_dir_all(path)?;
    }
    let meta = match fs::symlink_metadata(path) {
      Ok(meta) => meta,
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
      Err(e) => return Err(e.into()),
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
      return Err(Error::Conflict("invalid catalogue cache directory".into()));
    }
  }
  Ok(paths.pop())
}
fn filename(policy: Policy, key: &str) -> String {
  if policy.namespace == "minecraft" {
    "version-catalog.json".into()
  } else {
    {
      let hash: String = Sha256::digest(key.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
      format!("{hash}.json")
    }
  }
}
pub(crate) async fn load<T: DeserializeOwned + Send + 'static>(
  context: &Context,
  policy: Policy,
  key: &str,
) -> Result<Option<Snapshot<T>>> {
  let context = context.clone();
  let key = key.to_owned();
  let permit = context
    .blocking
    .clone()
    .acquire_owned()
    .await
    .map_err(|_| Error::Cancelled)?;
  tokio::task::spawn_blocking(move || {
    let _permit = permit;
    let Some(dir) = directory(&context, policy, false)? else {
      return Ok(None);
    };
    let path = dir.join(filename(policy, &key));
    let meta = match fs::symlink_metadata(&path) {
      Ok(meta) => meta,
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
      Err(e) => return Err(e.into()),
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
      return Err(Error::Conflict("invalid catalogue cache file".into()));
    }
    let stored: Envelope<T> = storage::read_json_bounded(&path, policy.bytes)?;
    if stored.schema_version != 1 {
      return Err(Error::Metadata("unsupported catalogue cache schema".into()));
    }
    let fetched_at = UNIX_EPOCH
      .checked_add(std::time::Duration::from_secs(stored.fetched_at_seconds))
      .ok_or_else(|| Error::Metadata("invalid catalogue cache timestamp".into()))?;
    Ok(Some(Snapshot {
      value: stored.payload,
      fetched_at,
    }))
  })
  .await?
}
pub(crate) async fn save<T: Serialize + Send + 'static>(
  context: &Context,
  policy: Policy,
  key: &str,
  value: T,
  fetched_at: SystemTime,
) -> Result<()> {
  let context = context.clone();
  let key = key.to_owned();
  let permit = context
    .blocking
    .clone()
    .acquire_owned()
    .await
    .map_err(|_| Error::Cancelled)?;
  tokio::task::spawn_blocking(move || {
    let _permit = permit;
    let _lock = storage::catalog_cache_lock(&context.paths.data, policy.namespace)?;
    let fetched_at_seconds = fetched_at
      .duration_since(UNIX_EPOCH)
      .map_err(|_| Error::Metadata("invalid catalogue timestamp".into()))?
      .as_secs();
    let bytes = serde_json::to_vec(&Envelope {
      schema_version: 1,
      fetched_at_seconds,
      payload: value,
    })?;
    if bytes.len() > policy.bytes {
      return Err(Error::Metadata("catalogue cache exceeds limit".into()));
    }
    let dir = directory(&context, policy, true)?
      .ok_or_else(|| Error::Metadata("catalogue cache unavailable".into()))?;
    let path = dir.join(filename(policy, &key));
    storage::replace_file(&path, &bytes)?;
    if policy.namespace != "minecraft" {
      let mut files = Vec::new();
      for (index, entry) in fs::read_dir(&dir)?.enumerate() {
        if index >= 256 {
          return Err(Error::Metadata(
            "catalogue cache directory exceeds limit".into(),
          ));
        }
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
          continue;
        };
        if name.len() != 69
          || !name.ends_with(".json")
          || !name.as_bytes()[..64].iter().all(u8::is_ascii_hexdigit)
        {
          continue;
        }
        let meta = fs::symlink_metadata(entry.path())?;
        if !meta.file_type().is_symlink() && meta.is_file() && entry.path() != path {
          files.push((meta.modified().unwrap_or(UNIX_EPOCH), entry.path()));
        }
      }
      files.sort_by_key(|entry| entry.0);
      let remove = files.len().saturating_sub(policy.entries.saturating_sub(1));
      for (_, old) in files.into_iter().take(remove) {
        fs::remove_file(old)?;
      }
    }
    Ok(())
  })
  .await?
}
