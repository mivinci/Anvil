//! Verified shared object lookup and q2q-http transfer; no instance mutation.
use super::*;
use crate::{CancellationToken, curseforge::ModFile};
use q2q_http::{Checksum, DownloadRequest};
use std::{
  collections::HashMap,
  sync::{Arc, Mutex, Weak},
};
use tokio::sync::watch;

type TransferResult = std::result::Result<String, Arc<Error>>;

#[derive(Default)]
pub(crate) struct ResourceTransfers {
  entries: Mutex<HashMap<(PathBuf, String), Weak<SharedTransfer>>>,
}

struct SharedTransfer {
  size: u64,
  consumers: Mutex<usize>,
  cancel: CancellationToken,
  progress: watch::Receiver<ResourceInstallProgress>,
  result: watch::Receiver<Option<TransferResult>>,
}

struct Consumer {
  transfer: Arc<SharedTransfer>,
  attached: bool,
}
impl Consumer {
  fn detach(&mut self) -> bool {
    if !std::mem::take(&mut self.attached) {
      return false;
    }
    let mut consumers = self
      .transfer
      .consumers
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    *consumers -= 1;
    if *consumers == 0 {
      self.transfer.cancel.cancel();
      return true;
    }
    false
  }
}
impl Drop for Consumer {
  fn drop(&mut self) {
    self.detach();
  }
}

enum Admission {
  Attached(Consumer),
  Draining(watch::Receiver<Option<TransferResult>>),
}

#[derive(Clone)]
pub(super) struct DownloadFile {
  pub filename: String,
  pub size: u64,
  pub sha1: String,
  pub url: String,
  pub extension: &'static str,
}
impl DownloadFile {
  pub fn from_curseforge(file: &ModFile) -> Result<Self> {
    Ok(Self {
      filename: file.file_name.clone(),
      size: file.file_length,
      sha1: upstream_hash(file)?,
      url: file.download_url.clone().expect("validated URL"),
      extension: "jar",
    })
  }
  fn validate(&self) -> Result<()> {
    validate_resource_filename(&self.filename, self.extension)?;
    if self.size == 0
      || self.size > MAX_IMPORT_BYTES as u64
      || self.sha1.len() != 40
      || !self
        .sha1
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
      return Err(Error::Metadata("invalid resource download identity".into()));
    }
    let url =
      url::Url::parse(&self.url).map_err(|_| Error::Metadata("invalid download URL".into()))?;
    if url.scheme() != "https"
      || !url.username().is_empty()
      || url.password().is_some()
      || url.port().is_some()
    {
      return Err(Error::Metadata("invalid download URL".into()));
    }
    Ok(())
  }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObjectIndex {
  sha256: String,
  size: u64,
}

fn index_path(data: &Path, sha1: &str) -> PathBuf {
  data.join("resources/sha1").join(format!("{sha1}.json"))
}
pub(super) fn lookup(data: &Path, file: &DownloadFile) -> Result<Option<(String, u64)>> {
  file.validate()?;
  lookup_validated(data, file)
}

fn lookup_validated(data: &Path, file: &DownloadFile) -> Result<Option<(String, u64)>> {
  let sha1 = &file.sha1;
  let path = index_path(data, sha1);
  match fs::symlink_metadata(&path) {
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
    Err(error) => return Err(error.into()),
    Ok(_) => {}
  }
  ensure_directory(&data.join("resources/sha1"), false)?;
  regular_file(&path, 1024)?;
  let index: ObjectIndex = storage::read_json_bounded(&path, 1024)?;
  validate_hash(&index.sha256)?;
  if index.size != file.size {
    return Err(Error::Conflict(
      "shared resource identity has conflicting sizes".into(),
    ));
  }
  let path = object_path(data, &index.sha256);
  verify_object(&path, &index.sha256, index.size)?;
  let bytes = read_jar(&path, index.size)?;
  verify_upstream(&bytes, sha1)?;
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
  pub(super) async fn blocking<T: Send + 'static>(
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
    file: &DownloadFile,
    cancel: &CancellationToken,
    progress: &watch::Sender<ResourceInstallProgress>,
  ) -> Result<String> {
    file.validate()?;
    self.prepare_validated_shared(file, cancel, progress).await
  }

  async fn prepare_validated_shared(
    &self,
    file: &DownloadFile,
    cancel: &CancellationToken,
    progress: &watch::Sender<ResourceInstallProgress>,
  ) -> Result<String> {
    let base = progress.borrow().clone();
    let mut consumer = loop {
      crate::error::check_cancel(cancel)?;
      match self.join_transfer(file)? {
        Admission::Attached(consumer) => break consumer,
        Admission::Draining(mut previous) => {
          tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(Error::Cancelled),
            _ = previous.wait_for(Option::is_some) => {}
          }
        }
      }
    };
    let mut updates = consumer.transfer.progress.clone();
    let mut result = consumer.transfer.result.clone();
    let mut watching_progress = true;
    loop {
      if cancel.is_cancelled() {
        if consumer.detach() {
          let _ = result.wait_for(Option::is_some).await;
        }
        return Err(Error::Cancelled);
      }
      let latest = updates.borrow_and_update().clone();
      if matches!(latest.stage, ResourceInstallStage::Downloading) {
        progress.send_modify(|value| {
          value.stage = latest.stage;
          value.received_bytes = base.received_bytes.saturating_add(latest.received_bytes);
          value.downloaded_bytes = base
            .downloaded_bytes
            .saturating_add(latest.downloaded_bytes);
        });
      }
      if let Some(outcome) = result.borrow_and_update().clone() {
        return outcome.map_err(Error::SharedResource);
      }
      tokio::select! {
        biased;
        _ = cancel.cancelled() => {}
        changed = result.changed() => {
          if changed.is_err() {
            return Err(Error::Conflict("shared resource transfer ended without a result".into()));
          }
        }
        changed = updates.changed(), if watching_progress => { watching_progress = changed.is_ok(); }
      }
    }
  }

  fn join_transfer(&self, file: &DownloadFile) -> Result<Admission> {
    let key = (self.context.paths.data.clone(), file.sha1.clone());
    let mut entries = self
      .context
      .resource_transfers
      .entries
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    entries.retain(|_, entry| entry.strong_count() > 0);
    if let Some(transfer) = entries.get(&key).and_then(Weak::upgrade) {
      let mut consumers = transfer
        .consumers
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      if *consumers > 0 {
        if transfer.size != file.size {
          return Err(Error::Conflict(
            "shared resource identity has conflicting sizes".into(),
          ));
        }
        *consumers += 1;
        drop(consumers);
        return Ok(Admission::Attached(Consumer {
          transfer,
          attached: true,
        }));
      }
      if transfer.result.borrow().is_none() && transfer.result.has_changed().is_ok() {
        return Ok(Admission::Draining(transfer.result.clone()));
      }
    }
    let (progress_tx, progress) = watch::channel(ResourceInstallProgress {
      stage: ResourceInstallStage::Preparing,
      completed_files: 0,
      total_files: 1,
      downloaded_bytes: 0,
      total_bytes: file.size,
      received_bytes: 0,
    });
    let (result_tx, result) = watch::channel(None);
    let transfer = Arc::new(SharedTransfer {
      size: file.size,
      consumers: Mutex::new(1),
      cancel: CancellationToken::new(),
      progress,
      result,
    });
    entries.insert(key, Arc::downgrade(&transfer));
    let manager = self.clone();
    let file = file.clone();
    let worker = transfer.clone();
    tokio::spawn(async move {
      let token = worker.cancel.clone();
      let outcome =
        tokio::spawn(async move { manager.download_shared(&file, &token, &progress_tx).await })
          .await
          .map_err(Error::from)
          .and_then(|result| result);
      result_tx.send_replace(Some(outcome.map_err(Arc::new)));
    });
    Ok(Admission::Attached(Consumer {
      transfer,
      attached: true,
    }))
  }

  async fn download_shared(
    &self,
    file: &DownloadFile,
    cancel: &CancellationToken,
    progress: &watch::Sender<ResourceInstallProgress>,
  ) -> Result<String> {
    let _operation = self.lock_resource_operation().await?;
    let sha1 = file.sha1.clone();
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
        lookup_validated(&data, &spec)
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
      .mirrors([file.url.as_str()])
      .expected_size(file.size)
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
          .saturating_add(state.staged_bytes.min(file.size));
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
    if !same_download_location(&downloaded, &target)? {
      return Err(Error::Conflict("unexpected resource download path".into()));
    }
    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    let data = self.context.paths.data.clone();
    let spec = file.clone();
    self
      .blocking(move || {
        let bytes = read_jar(&target, spec.size)?;
        verify_upstream(&bytes, &sha1)?;
        let hash = hex(&Sha256::digest(&bytes));
        let _store = storage::client_cache_lock(&data, "resources-store")?;
        publish_object(&data, &hash, &bytes)?;
        ensure_directory(&data.join("resources/sha1"), true)?;
        storage::replace_json(
          &index_path(&data, &sha1),
          &ObjectIndex {
            sha256: hash.clone(),
            size: spec.size,
          },
        )?;
        fs::remove_file(target)?;
        Ok(hash)
      })
      .await
  }
}

// q2q-http canonicalizes its destination parent (including the Windows verbatim
// prefix). Compare normalized regular files, not the spelling of their paths.
fn same_download_location(left: &Path, right: &Path) -> Result<bool> {
  for path in [left, right] {
    match fs::symlink_metadata(path) {
      Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
      Ok(_) => return Ok(false),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
      Err(error) => return Err(error.into()),
    }
  }
  Ok(fs::canonicalize(left)? == fs::canonicalize(right)?)
}

#[cfg(test)]
#[path = "../../tests/resource_transfer.rs"]
mod tests;
