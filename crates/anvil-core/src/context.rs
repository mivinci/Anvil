use std::path::{Path, PathBuf};
use std::{
  collections::HashMap,
  sync::{Arc, Weak},
};

use directories::ProjectDirs;
use q2q_http::{Checksum, DownloadRequest, Downloader};
use tokio::sync::{Mutex, OwnedMutexGuard, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::{Error, Result};

#[derive(Clone, Debug)]
pub struct Paths {
  pub data: PathBuf,
  pub cache: PathBuf,
}

impl Paths {
  /// An override contains both persistent data and a clearable cache subtree.
  pub fn discover(root: Option<PathBuf>) -> Result<Self> {
    if let Some(root) = root {
      let data = std::path::absolute(root)?;
      return Ok(Self {
        cache: data.join("cache"),
        data,
      });
    }
    let dirs = ProjectDirs::from("", "", "Anvil").ok_or_else(|| {
      Error::Invalid("cannot determine application directories; use --data-dir".into())
    })?;
    Ok(Self {
      data: dirs.data_local_dir().into(),
      cache: dirs.cache_dir().into(),
    })
  }
}

/// One connection pool and bounded blocking work shared by all services in this process.
#[derive(Clone)]
pub struct Context {
  pub paths: Paths,
  pub(crate) http: Downloader,
  pub(crate) blocking: Arc<Semaphore>,
  cache_locks: Arc<Mutex<HashMap<String, Weak<Mutex<()>>>>>,
}

impl Context {
  pub fn new(paths: Paths) -> Result<Self> {
    Ok(Self {
      paths,
      http: Downloader::builder()
        .max_active_files(4)
        .max_requests(8)
        .max_requests_per_host(4)
        .max_disk_tasks(2)
        .build()?,
      blocking: Arc::new(Semaphore::new(2)),
      cache_locks: Arc::new(Mutex::new(HashMap::new())),
    })
  }

  /// Serialize same-process work for a content-addressed cache entry. The lock
  /// map keeps only weak references and prunes entries after their last user.
  pub(crate) async fn cache_lock(
    &self,
    key: String,
    cancel: &CancellationToken,
  ) -> Result<OwnedMutexGuard<()>> {
    let lock = {
      let mut locks = self.cache_locks.lock().await;
      locks.retain(|_, lock| lock.strong_count() > 0);
      if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
        lock
      } else {
        let lock = Arc::new(Mutex::new(()));
        locks.insert(key, Arc::downgrade(&lock));
        lock
      }
    };
    tokio::select! {
      _ = cancel.cancelled() => Err(Error::Cancelled),
      guard = lock.lock_owned() => Ok(guard),
    }
  }

  /// Fetch a bounded document over the shared HTTP executor, following the
  /// executor's redirect budget. Intended for signed or otherwise
  /// self-verifying metadata; bulk downloads use [`Self::download_file`].
  pub async fn fetch_bounded(&self, url: &url::Url, max_bytes: u64) -> Result<Vec<u8>> {
    let response = self
      .http
      .executor()
      .get(url.as_str())
      .max_body_size(max_bytes)
      .send()
      .await?;
    Ok(response.collect_bounded(max_bytes as usize).await?.to_vec())
  }

  /// Download a single file to `destination`, verifying an optional SHA-256
  /// hex digest and expected size. A partial download from a previous attempt
  /// (q2q-http journal or part files) is resumed instead of restarted.
  pub async fn download_file(
    &self,
    url: &url::Url,
    destination: &Path,
    sha256: Option<&str>,
    expected_size: Option<u64>,
    cancel: &CancellationToken,
  ) -> Result<()> {
    let parent = destination
      .parent()
      .ok_or_else(|| Error::Invalid("download destination has no parent directory".into()))?
      .to_path_buf();
    std::fs::create_dir_all(&parent)?;

    let target = destination.to_path_buf();
    let mut journal = destination.as_os_str().to_owned();
    journal.push(".q2q-http.resume");
    let mut part = destination.as_os_str().to_owned();
    part.push(".q2q-http.part");
    let resume =
      target.try_exists()? || Path::new(&journal).try_exists()? || Path::new(&part).try_exists()?;

    let mut request = DownloadRequest::new(&target).mirrors([url.as_str()]);
    if let Some(size) = expected_size {
      request = request.expected_size(size);
    }
    if let Some(sha256) = sha256 {
      request = request.checksum(Checksum::sha256(sha256)?);
    }

    let task = tokio::select! {
      _ = cancel.cancelled() => return Err(Error::Cancelled),
      task = async {
        if resume {
          self.http.resume(request).await
        } else {
          self.http.download(request).await
        }
      } => task?,
    };
    let mut updates = task.subscribe_progress();
    loop {
      if updates.borrow_and_update().state.is_terminal() {
        break;
      }
      if cancel.is_cancelled() {
        task.cancel();
        return Err(Error::Cancelled);
      }
      if updates.changed().await.is_err() {
        break;
      }
    }
    task.wait().await?;
    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    Ok(())
  }
}
