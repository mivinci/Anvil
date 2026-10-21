use std::path::PathBuf;
use std::sync::Arc;

use directories::ProjectDirs;
use q2q_http::Downloader;
use tokio::sync::Semaphore;

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
    })
  }
}
