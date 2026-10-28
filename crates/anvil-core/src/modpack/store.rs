//! Shared SHA-256 objects; mrpack SHA-512 indexes are rebuildable cache hints.
use super::{File as PackFile, invalid};
use crate::{CancellationToken, Context, Error, Result, storage};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use std::{
  fs::{self, File},
  io::{Read, Write},
  path::{Path, PathBuf},
};
use tokio::sync::watch;

#[derive(Serialize, Deserialize)]
struct Index {
  sha256: String,
  size: u64,
}

pub(super) fn directory(path: &Path) -> Result<()> {
  match fs::symlink_metadata(path) {
    Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(()),
    Ok(_) => Err(invalid("expected a regular directory")),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
      fs::create_dir(path)?;
      Ok(())
    }
    Err(error) => Err(error.into()),
  }
}
fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn verify(path: &Path, spec: &PackFile, cancel: &CancellationToken) -> Result<String> {
  let meta = fs::symlink_metadata(path)?;
  if !meta.is_file() || meta.file_type().is_symlink() || meta.len() != spec.file_size {
    return Err(invalid("shared file identity mismatch"));
  }
  let mut file = File::open(path)?;
  let (mut sha1, mut sha256, mut sha512) = (sha1::Sha1::new(), Sha256::new(), Sha512::new());
  let mut buffer = [0u8; 65_536];
  loop {
    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    let read = file.read(&mut buffer)?;
    if read == 0 {
      break;
    }
    sha1.update(&buffer[..read]);
    sha256.update(&buffer[..read]);
    sha512.update(&buffer[..read]);
  }
  if !hex(&sha1.finalize()).eq_ignore_ascii_case(&spec.hashes["sha1"])
    || !hex(&sha512.finalize()).eq_ignore_ascii_case(&spec.hashes["sha512"])
  {
    return Err(Error::Conflict("整合包文件哈希校验失败".into()));
  }
  Ok(hex(&sha256.finalize()))
}
fn object(data: &Path, hash: &str) -> Result<PathBuf> {
  if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
    return Err(invalid("invalid object digest"));
  }
  let resources = data.join("resources");
  directory(&resources)?;
  directory(&resources.join("objects"))?;
  directory(&resources.join("objects/sha256"))?;
  let parent = resources.join("objects/sha256").join(&hash[..2]);
  directory(&parent)?;
  Ok(parent.join(hash))
}
fn cache_index(data: &Path, spec: &PackFile) -> Result<PathBuf> {
  directory(&data.join("resources"))?;
  directory(&data.join("resources/mrpack"))?;
  Ok(data.join("resources/mrpack").join(format!(
    "{}.json",
    spec.hashes["sha512"].to_ascii_lowercase()
  )))
}

pub(super) async fn prepare(
  context: &Context,
  spec: &PackFile,
  cancel: &CancellationToken,
  progress: &watch::Sender<super::ImportProgress>,
) -> Result<(PathBuf, bool)> {
  let key = format!("mrpack-{}", spec.hashes["sha512"].to_ascii_lowercase());
  let _memory = context.cache_lock(key.clone(), cancel).await?;
  let data = context.paths.data.clone();
  let identity = spec.clone();
  let token = cancel.clone();
  let (lock, cached) = super::install::blocking(context, cancel, move || {
    let lock = storage::client_cache_lock(&data, &key)?;
    let index = cache_index(&data, &identity)?;
    let cached = match fs::symlink_metadata(&index) {
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
      Err(error) => return Err(error.into()),
      Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= 1024 => {
        let found: Index = storage::read_json_bounded(&index, 1024)?;
        let path = object(&data, &found.sha256)?;
        if found.size != identity.file_size || verify(&path, &identity, &token)? != found.sha256 {
          return Err(invalid("cached mrpack identity mismatch"));
        }
        Some(path)
      }
      Ok(_) => return Err(invalid("invalid mrpack cache index")),
    };
    Ok((lock, cached))
  })
  .await?;
  let _lock = lock;
  if let Some(path) = cached {
    return Ok((path, true));
  }
  let root = context.paths.cache.clone();
  let temporary = super::install::blocking(context, cancel, move || {
    fs::create_dir_all(&root)?;
    directory(&root)?;
    Ok(
      tempfile::Builder::new()
        .prefix("mrpack-")
        .tempdir_in(root)?,
    )
  })
  .await?;
  let target = temporary.path().join("file");
  let request = q2q_http::DownloadRequest::new(&target)
    .mirrors(spec.downloads.iter().map(|url| url.as_str()))
    .expected_size(spec.file_size)
    .checksum(q2q_http::Checksum::sha1(&spec.hashes["sha1"])?);
  let downloader = context.http.with_url_policy(|url| {
    url.scheme() == "https"
      && url.username().is_empty()
      && url.password().is_none()
      && url.port().is_none_or(|port| port == 443)
      && matches!(
        url.host_str(),
        Some(
          "cdn.modrinth.com"
            | "github.com"
            | "raw.githubusercontent.com"
            | "gitlab.com"
            | "release-assets.githubusercontent.com"
            | "objects.githubusercontent.com"
        )
      )
  });
  if cancel.is_cancelled() {
    return Err(Error::Cancelled);
  }
  let task = downloader.download(request).await?;
  let mut updates = task.subscribe_progress();
  let base = progress.borrow().clone();
  loop {
    let state = *updates.borrow_and_update();
    progress.send_modify(|p| {
      p.downloaded_bytes = base.downloaded_bytes + state.staged_bytes.min(spec.file_size);
      p.received_bytes = base.received_bytes + state.received_bytes;
    });
    if state.state.is_terminal() {
      break;
    }
    tokio::select! {
      _ = cancel.cancelled() => { task.cancel(); break; }
      changed = updates.changed() => { if changed.is_err() { break; } }
    }
  }
  let outcome = task.wait().await;
  if cancel.is_cancelled() {
    return Err(Error::Cancelled);
  }
  let downloaded = outcome?;
  if fs::canonicalize(&downloaded)? != fs::canonicalize(&target)? {
    return Err(invalid("unexpected downloaded object"));
  }
  let data = context.paths.data.clone();
  let spec = spec.clone();
  let token = cancel.clone();
  super::install::blocking(context, cancel, move || {
    let hash = verify(&target, &spec, &token)?;
    let destination = object(&data, &hash)?;
    let _store = storage::client_cache_lock(&data, "resources-store")?;
    if destination.try_exists()? {
      if verify(&destination, &spec, &token)? != hash {
        return Err(invalid("shared object conflict"));
      }
    } else {
      let mut output = tempfile::NamedTempFile::new_in(destination.parent().unwrap())?;
      let mut source = File::open(target)?;
      let mut buffer = [0u8; 65_536];
      loop {
        if token.is_cancelled() {
          return Err(Error::Cancelled);
        }
        let count = source.read(&mut buffer)?;
        if count == 0 {
          break;
        }
        output.write_all(&buffer[..count])?;
      }
      output.as_file().sync_all()?;
      output
        .persist_noclobber(&destination)
        .map_err(|e| Error::Io(e.error))?;
    }
    directory(&data.join("resources/sha1"))?;
    let sha1_index = data
      .join("resources/sha1")
      .join(format!("{}.json", spec.hashes["sha1"].to_ascii_lowercase()));
    match fs::symlink_metadata(&sha1_index) {
      Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= 1024 => {
        let existing: Index = storage::read_json_bounded(&sha1_index, 1024)?;
        if existing.sha256 != hash || existing.size != spec.file_size {
          return Err(Error::Conflict("shared SHA-1 identity conflict".into()));
        }
      }
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => storage::replace_json(
        &sha1_index,
        &Index {
          sha256: hash.clone(),
          size: spec.file_size,
        },
      )?,
      Err(error) => return Err(error.into()),
      Ok(_) => return Err(invalid("invalid shared SHA-1 index")),
    }
    #[cfg(unix)]
    File::open(destination.parent().unwrap())?.sync_all()?;
    storage::replace_json(
      &cache_index(&data, &spec)?,
      &Index {
        sha256: hash,
        size: spec.file_size,
      },
    )?;
    Ok((destination, false))
  })
  .await
}

#[cfg(test)]
#[path = "../../tests/modpack_store.rs"]
mod tests;
