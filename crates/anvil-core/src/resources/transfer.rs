//! Verified shared object lookup and q2q-http transfer; no instance mutation.
use super::*;
use crate::{CancellationToken, curseforge::ModFile};
use q2q_http::{Checksum, DownloadRequest};
use tokio::sync::watch;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObjectIndex {
  sha256: String,
  size: u64,
}

fn index_path(data: &Path, sha1: &str) -> PathBuf {
  data.join("resources/sha1").join(format!("{sha1}.json"))
}
pub(super) fn lookup(data: &Path, file: &ModFile) -> Result<Option<(String, u64)>> {
  let sha1 = upstream_hash(file)?;
  let path = index_path(data, &sha1);
  match fs::symlink_metadata(&path) {
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
    Err(error) => return Err(error.into()),
    Ok(_) => {}
  }
  ensure_directory(&data.join("resources/sha1"), false)?;
  regular_file(&path, 1024)?;
  let index: ObjectIndex = storage::read_json_bounded(&path, 1024)?;
  validate_hash(&index.sha256)?;
  if index.size != file.file_length {
    return Err(Error::Conflict(
      "shared resource identity has conflicting sizes".into(),
    ));
  }
  let path = object_path(data, &index.sha256);
  verify_object(&path, &index.sha256, index.size)?;
  let bytes = read_jar(&path, index.size)?;
  verify_upstream(&bytes, &sha1)?;
  Ok(Some((index.sha256, index.size)))
}
pub(super) fn upstream_hash(file: &ModFile) -> Result<String> {
  if !crate::curseforge::usable(file) {
    return Err(Error::Metadata("invalid CurseForge file metadata".into()));
  }
  let hashes: HashSet<_> = file
    .hashes
    .iter()
    .filter(|h| h.algo == 1)
    .map(|h| h.value.to_ascii_lowercase())
    .collect();
  if hashes.len() != 1 {
    return Err(Error::Metadata("missing or conflicting SHA-1".into()));
  }
  Ok(hashes.into_iter().next().expect("one hash"))
}
fn read_jar(path: &Path, size: u64) -> Result<Vec<u8>> {
  regular_file(path, MAX_IMPORT_BYTES as u64)?;
  let mut bytes = Vec::new();
  File::open(path)?.take(size + 1).read_to_end(&mut bytes)?;
  if bytes.len() as u64 != size {
    return Err(Error::Conflict("resource size mismatch".into()));
  }
  validate_jar(&bytes)?;
  Ok(bytes)
}
fn verify_upstream(bytes: &[u8], sha1: &str) -> Result<()> {
  use sha1::Digest;
  if hex(&sha1::Sha1::digest(bytes)) != sha1 {
    return Err(Error::Conflict("resource SHA-1 mismatch".into()));
  }
  Ok(())
}
impl ResourceManager {
  async fn blocking<T: Send + 'static>(
    &self,
    action: impl FnOnce() -> Result<T> + Send + 'static,
  ) -> Result<T> {
    let permit = self
      .context
      .blocking
      .clone()
      .acquire_owned()
      .await
      .map_err(|_| Error::Cancelled)?;
    tokio::task::spawn_blocking(move || {
      let _permit = permit;
      action()
    })
    .await?
  }
  pub(super) async fn prepare_shared(
    &self,
    file: &ModFile,
    cancel: &CancellationToken,
    progress: &watch::Sender<ResourceInstallProgress>,
  ) -> Result<String> {
    let sha1 = upstream_hash(file)?;
    let _memory = self
      .context
      .cache_lock(format!("resource-{sha1}"), cancel)
      .await?;
    let data = self.context.paths.data.clone();
    let identity = sha1.clone();
    let _file_lock = self
      .blocking(move || storage::client_cache_lock(&data, &format!("resource-{identity}")))
      .await?;
    let data = self.context.paths.data.clone();
    let spec = file.clone();
    if let Some((hash, _)) = self
      .blocking(move || {
        let _store = storage::client_cache_lock(&data, "resources-store")?;
        lookup(&data, &spec)
      })
      .await?
    {
      return Ok(hash);
    }
    let data = self.context.paths.data.clone();
    let identity = sha1.clone();
    let target = self
      .blocking(move || {
        for path in [
          data.join("resources"),
          data.join("resources/staging"),
          data.join("resources/staging/sha1"),
        ] {
          ensure_directory(&path, true)?;
        }
        let target = data.join("resources/staging/sha1").join(identity);
        for suffix in ["", ".q2q-http.part", ".q2q-http.resume"] {
          let path = PathBuf::from(format!("{}{suffix}", target.display()));
          match fs::symlink_metadata(&path) {
            Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
              return Err(Error::Conflict("invalid resource staging file".into()));
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
          }
        }
        Ok(target)
      })
      .await?;
    let resume = target.try_exists()?
      || PathBuf::from(format!("{}.q2q-http.resume", target.display())).try_exists()?
      || PathBuf::from(format!("{}.q2q-http.part", target.display())).try_exists()?;
    let request = DownloadRequest::new(&target)
      .mirrors([file.download_url.as_ref().expect("validated URL").as_str()])
      .expected_size(file.file_length)
      .checksum(Checksum::sha1(&sha1)?);
    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    // Do not drop the submission future on cancellation: once accepted, cancel and drain the task.
    let task = if resume {
      self.context.http.resume(request).await?
    } else {
      self.context.http.download(request).await?
    };
    let mut updates = task.subscribe_progress();
    let mut sent = false;
    let base = progress.borrow().clone();
    loop {
      let state = *updates.borrow_and_update();
      progress.send_modify(|p| {
        p.stage = ResourceInstallStage::Downloading;
        p.received_bytes = base.received_bytes.saturating_add(state.received_bytes);
        p.downloaded_bytes = base
          .downloaded_bytes
          .saturating_add(state.staged_bytes.min(file.file_length));
      });
      if state.state.is_terminal() {
        break;
      }
      if cancel.is_cancelled() && !sent {
        task.cancel();
        sent = true;
      }
      if sent {
        if updates.changed().await.is_err() {
          break;
        }
      } else {
        tokio::select! { _ = cancel.cancelled() => { task.cancel(); sent = true; }, changed = updates.changed() => { if changed.is_err() { break; } } }
      }
    }
    let downloaded = task.wait().await?;
    if downloaded != target {
      return Err(Error::Conflict("unexpected resource download path".into()));
    }
    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    let data = self.context.paths.data.clone();
    let spec = file.clone();
    self
      .blocking(move || {
        let bytes = read_jar(&target, spec.file_length)?;
        verify_upstream(&bytes, &sha1)?;
        let hash = hex(&Sha256::digest(&bytes));
        let _store = storage::client_cache_lock(&data, "resources-store")?;
        for path in [
          data.join("resources/objects"),
          data.join("resources/objects/sha256"),
          data.join("resources/sha1"),
        ] {
          ensure_directory(&path, true)?;
        }
        let object = object_path(&data, &hash);
        let parent = object.parent().expect("object parent");
        ensure_directory(parent, true)?;
        match fs::symlink_metadata(&object) {
          Ok(_) => verify_object(&object, &hash, spec.file_length)?,
          Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut temp = tempfile::NamedTempFile::new_in(parent)?;
            temp.write_all(&bytes)?;
            temp.as_file().sync_all()?;
            temp
              .persist_noclobber(&object)
              .map_err(|e| Error::Io(e.error))?;
            #[cfg(unix)]
            File::open(parent)?.sync_all()?;
          }
          Err(e) => return Err(e.into()),
        }
        storage::replace_json(
          &index_path(&data, &sha1),
          &ObjectIndex {
            sha256: hash.clone(),
            size: spec.file_length,
          },
        )?;
        fs::remove_file(target)?;
        Ok(hash)
      })
      .await
  }
}
