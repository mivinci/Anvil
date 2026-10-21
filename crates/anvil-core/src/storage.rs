use crate::{Error, Result};
use std::{
  fs::{self, File, OpenOptions},
  io::Write,
  path::Path,
  sync::Arc,
};

pub(crate) fn lock(data: &Path, id: &str) -> Result<Arc<File>> {
  lock_named(data, "java", id)
}

pub(crate) fn server_instance_lock(data: &Path, id: &str) -> Result<Arc<File>> {
  lock_named(data, "instance", id)
}

pub(crate) fn client_cache_lock(data: &Path, sha1: &str) -> Result<Arc<File>> {
  lock_named(data, "client-cache", sha1)
}

pub(crate) fn server_cache_lock(data: &Path, sha1: &str) -> Result<Arc<File>> {
  lock_named(data, "server-cache", sha1)
}

fn lock_named(data: &Path, namespace: &str, id: &str) -> Result<Arc<File>> {
  if namespace.is_empty()
    || !namespace
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    || id.is_empty()
    || !id
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
  {
    return Err(Error::Invalid("invalid lock identity".into()));
  }
  let dir = data.join("locks");
  fs::create_dir_all(&dir)?;
  let file = OpenOptions::new()
    .read(true)
    .write(true)
    .create(true)
    .truncate(false)
    .open(dir.join(format!("{namespace}-{id}.lock")))?;
  file.try_lock().map_err(|e| match e {
    std::fs::TryLockError::WouldBlock => Error::Busy(id.into()),
    std::fs::TryLockError::Error(e) => Error::Io(e),
  })?;
  Ok(Arc::new(file))
}

pub(crate) fn valid_id(id: &str) -> Result<()> {
  if id.is_empty()
    || id.len() > 180
    || !id.starts_with("temurin-")
    || !id
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b"-._+".contains(&b))
  {
    return Err(Error::Invalid("invalid managed runtime ID".into()));
  }
  Ok(())
}

pub(crate) fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
  let bytes = serde_json::to_vec_pretty(value)?;
  let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
  use std::io::Write;
  file.write_all(&bytes)?;
  file.sync_all()?;
  Ok(())
}

/// Atomically replace a managed file from a temporary sibling and sync its parent.
pub(crate) fn replace_file(path: &Path, bytes: &[u8]) -> Result<()> {
  let parent = path
    .parent()
    .ok_or_else(|| Error::Invalid("managed file path has no parent".into()))?;
  match fs::symlink_metadata(path) {
    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
      return Err(Error::Conflict("managed file is not a regular file".into()));
    }
    Ok(_) => {}
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => return Err(error.into()),
  }
  let mut temp = tempfile::NamedTempFile::new_in(parent)?;
  temp.write_all(bytes)?;
  temp.as_file().sync_all()?;
  temp.persist(path).map_err(|error| Error::Io(error.error))?;
  #[cfg(unix)]
  File::open(parent)?.sync_all()?;
  Ok(())
}

pub(crate) fn replace_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
  replace_file(path, &serde_json::to_vec_pretty(value)?)
}

pub(crate) fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
  read_json_bounded(path, 64 * 1024)
}

pub(crate) fn read_json_bounded<T: serde::de::DeserializeOwned>(
  path: &Path,
  limit: usize,
) -> Result<T> {
  use std::io::Read;
  let mut bytes = Vec::new();
  File::open(path)?
    .take(limit.saturating_add(1) as u64)
    .read_to_end(&mut bytes)?;
  if bytes.len() > limit {
    return Err(Error::Metadata("managed record exceeds limit".into()));
  }
  Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
#[path = "../tests/storage.rs"]
mod tests;
