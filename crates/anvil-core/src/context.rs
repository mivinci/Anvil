use std::path::PathBuf;
use std::{
  collections::HashMap,
  sync::{Arc, Weak},
};

use directories::ProjectDirs;
use q2q_http::Downloader;
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
}
