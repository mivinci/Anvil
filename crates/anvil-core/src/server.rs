//! Original-server artifact planning and verified cache downloads.
use std::{
  ffi::OsString,
  fs,
  io::{Read, Write},
  path::{Path, PathBuf},
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  time::{SystemTime, UNIX_EPOCH},
};

use q2q_http::{Checksum, DownloadRequest, DownloadState, Task};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{
  Context, Error, Result,
  error::check_cancel,
  java::{InstalledJava, JavaManager, JavaProgress},
  metadata::{self, ServerArtifact},
  storage,
};

pub const MINECRAFT_EULA_URL: &str = "https://aka.ms/MinecraftEULA";
const EULA_FILE: &[u8] = b"# https://aka.ms/MinecraftEULA\neula=true\n";
static SERVER_SESSION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInstallPlan {
  artifact: ServerArtifact,
}

impl ServerInstallPlan {
  pub fn artifact(&self) -> &ServerArtifact {
    &self.artifact
  }

  /// Content-addressed cache key. The file is not an instance or install marker.
  pub fn cache_key(&self) -> &str {
    &self.artifact.sha1
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerInstallStage {
  Resolving,
  Downloading,
  Verifying,
  Publishing,
  Completed,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServerProgress {
  pub stage: ServerInstallStage,
  pub downloaded_bytes: u64,
  pub total_bytes: Option<u64>,
}

impl Default for ServerProgress {
  fn default() -> Self {
    Self {
      stage: ServerInstallStage::Resolving,
      downloaded_bytes: 0,
      total_bytes: None,
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInstallResult {
  pub cached_jar: PathBuf,
  pub reused_cache: bool,
}

fn same_regular_file_location(left: &Path, right: &Path) -> Result<bool> {
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

/// Plans server artifacts and downloads them into the shared, verified cache.
/// Instance records and server directories are owned by the later instance layer.
#[derive(Clone)]
pub struct ServerInstaller {
  context: Context,
}

struct CancelOnDrop(CancellationToken);

impl Drop for CancelOnDrop {
  fn drop(&mut self) {
    self.0.cancel();
  }
}

impl ServerInstaller {
  pub fn new(context: Context) -> Self {
    Self { context }
  }

  /// Resolve Mojang metadata and derive the content-addressed cache plan.
  pub async fn plan(
    &self,
    version_id: &str,
    cancel: &CancellationToken,
    progress: &watch::Sender<ServerProgress>,
  ) -> Result<ServerInstallPlan> {
    let cancel = cancel.child_token();
    let _cancel_on_drop = CancelOnDrop(cancel.clone());
    check_cancel(&cancel)?;
    progress.send_replace(ServerProgress::default());
    let http = self.context.http.executor();
    let artifact = tokio::select! {
      _ = cancel.cancelled() => return Err(Error::Cancelled),
      result = metadata::resolve_server(&http, version_id) => result?,
    };
    make_plan(artifact)
  }

  /// Download a plan into `cache/servers/sha1/` and publish only after q2q-http
  /// verifies both the official length and SHA-1. Failed attempts remain retryable
  /// through q2q-http's resume records and are never treated as installed servers.
  pub async fn download(
    &self,
    plan: &ServerInstallPlan,
    cancel: &CancellationToken,
    progress: &watch::Sender<ServerProgress>,
  ) -> Result<ServerInstallResult> {
    let cancel = cancel.child_token();
    let _cancel_on_drop = CancelOnDrop(cancel.clone());
    check_cancel(&cancel)?;
    let cache_path = self
      .context
      .paths
      .cache
      .join("servers")
      .join("sha1")
      .join(format!("{}.jar", plan.cache_key()));
    let cache_id = plan.cache_key().to_owned();
    let cache_data = self.context.paths.data.clone();
    let _cache_lock = self
      .blocking(&cancel, move || {
        storage::server_cache_lock(&cache_data, &cache_id)
      })
      .await?;
    let (mut resume, mut reused_cache) = self
      .blocking(&cancel, {
        let cache_path = cache_path.clone();
        move || {
          let parent = cache_path
            .parent()
            .ok_or_else(|| Error::Invalid("server cache path has no parent".into()))?;
          fs::create_dir_all(parent)?;
          let part = sidecar(&cache_path, ".q2q-http.part");
          let journal = sidecar(&cache_path, ".q2q-http.resume");
          let target_exists = cache_path.try_exists()?;
          Ok((
            target_exists || part.try_exists()? || journal.try_exists()?,
            target_exists,
          ))
        }
      })
      .await?;

    check_cancel(&cancel)?;
    progress.send_replace(ServerProgress {
      stage: ServerInstallStage::Downloading,
      downloaded_bytes: 0,
      total_bytes: Some(plan.artifact.size),
    });
    let checksum = Checksum::sha1(&plan.artifact.sha1)?;
    let request = DownloadRequest::new(&cache_path)
      .mirrors([plan.artifact.url.as_str()])
      .expected_size(plan.artifact.size)
      .checksum(checksum);
    let retry_request = request.clone();
    let task = tokio::select! {
      _ = cancel.cancelled() => return Err(Error::Cancelled),
      result = async {
        if resume {
          self.context.http.resume(request).await
        } else {
          self.context.http.download(request).await
        }
      } => result?,
    };
    let first_result = watch_download(task, &cancel, progress).await;
    let downloaded_path = match first_result {
      Ok(path) => path,
      Err(Error::Http(q2q_http::Error::Integrity)) if reused_cache => {
        check_cancel(&cancel)?;
        let path = cache_path.clone();
        resume = self
          .blocking(&cancel, move || {
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
              return Err(Error::Conflict("invalid server cache artifact".into()));
            }
            fs::remove_file(&path)?;
            let part = sidecar(&path, ".q2q-http.part");
            let journal = sidecar(&path, ".q2q-http.resume");
            Ok(part.try_exists()? || journal.try_exists()?)
          })
          .await?;
        reused_cache = false;
        progress.send_replace(ServerProgress {
          stage: ServerInstallStage::Downloading,
          downloaded_bytes: 0,
          total_bytes: Some(plan.artifact.size),
        });
        let task = tokio::select! {
          _ = cancel.cancelled() => return Err(Error::Cancelled),
          result = async {
            if resume {
              self.context.http.resume(retry_request).await
            } else {
              self.context.http.download(retry_request).await
            }
          } => result?,
        };
        watch_download(task, &cancel, progress).await?
      }
      Err(error) => return Err(error),
    };
    if !same_regular_file_location(&downloaded_path, &cache_path)? {
      return Err(Error::Conflict(
        "server cache published to an unexpected path".into(),
      ));
    }
    progress.send_modify(|p| p.stage = ServerInstallStage::Completed);
    Ok(ServerInstallResult {
      cached_jar: cache_path,
      reused_cache,
    })
  }

  async fn blocking<T, F>(&self, cancel: &CancellationToken, work: F) -> Result<T>
  where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
  {
    let permit = tokio::select! {
      _ = cancel.cancelled() => return Err(Error::Cancelled),
      permit = self.context.blocking.clone().acquire_owned() => permit.map_err(|_| Error::Cancelled)?,
    };
    tokio::task::spawn_blocking(move || {
      let _permit = permit;
      work()
    })
    .await?
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerJar {
  /// Relative path within the instance root.
  pub file: String,
  pub sha1: String,
  pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerInstanceType {
  Server,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerInstance {
  pub schema_version: u32,
  #[serde(rename = "type")]
  pub instance_type: ServerInstanceType,
  pub id: String,
  pub name: String,
  pub version_id: String,
  pub java_component: String,
  pub java_major: u32,
  pub server_jar: ServerJar,
  /// True only after an explicit acceptance, either on this operation or earlier.
  pub eula_accepted: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub eula_url: Option<String>,
}

/// Verified inputs for a server process. The instance lock remains held for the
/// lifetime of this value, including the foreground process session.
#[derive(Clone, Debug)]
pub struct ServerStartPreparation {
  pub instance: ServerInstance,
  pub instance_directory: PathBuf,
  pub game_directory: PathBuf,
  pub server_jar: PathBuf,
  pub java: InstalledJava,
  _instance_lock: Arc<fs::File>,
  session: Option<ServerSessionRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerSessionState {
  Starting,
  Running,
  SpawnFailed,
  Exited,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServerSessionRecord {
  pub schema_version: u32,
  pub session_id: String,
  pub state: ServerSessionState,
  pub started_at_unix_ms: u64,
  pub finished_at_unix_ms: Option<u64>,
  pub exit_code: Option<i32>,
  pub stop_requested: bool,
  pub error: Option<String>,
}

impl ServerStartPreparation {
  /// Persist that this process launch is being attempted before spawning Java.
  pub fn begin_session(&mut self) -> Result<ServerSessionRecord> {
    let started_at_unix_ms = unix_time_ms();
    let record = ServerSessionRecord {
      schema_version: 1,
      session_id: format!(
        "{}-{}-{}",
        started_at_unix_ms,
        std::process::id(),
        SERVER_SESSION_SEQUENCE.fetch_add(1, Ordering::Relaxed)
      ),
      state: ServerSessionState::Starting,
      started_at_unix_ms,
      finished_at_unix_ms: None,
      exit_code: None,
      stop_requested: false,
      error: None,
    };
    self.persist_session(&record)?;
    self.session = Some(record.clone());
    Ok(record)
  }

  pub fn mark_running(&mut self) -> Result<ServerSessionRecord> {
    let mut record = self.current_session()?;
    record.state = ServerSessionState::Running;
    self.persist_session(&record)?;
    self.session = Some(record.clone());
    Ok(record)
  }

  pub fn mark_spawn_failed(&mut self, error: &str) -> Result<ServerSessionRecord> {
    let mut record = self.current_session()?;
    record.state = ServerSessionState::SpawnFailed;
    record.finished_at_unix_ms = Some(unix_time_ms());
    record.error = Some(error.to_owned());
    self.persist_session(&record)?;
    self.session = Some(record.clone());
    Ok(record)
  }

  pub fn mark_exited(
    &mut self,
    exit_code: Option<i32>,
    stop_requested: bool,
  ) -> Result<ServerSessionRecord> {
    let mut record = self.current_session()?;
    record.state = ServerSessionState::Exited;
    record.finished_at_unix_ms = Some(unix_time_ms());
    record.exit_code = exit_code;
    record.stop_requested = stop_requested;
    self.persist_session(&record)?;
    self.session = Some(record.clone());
    Ok(record)
  }

  fn current_session(&self) -> Result<ServerSessionRecord> {
    self
      .session
      .clone()
      .ok_or_else(|| Error::Conflict("server session has not started".into()))
  }

  fn persist_session(&self, record: &ServerSessionRecord) -> Result<()> {
    storage::replace_json(&self.instance_directory.join("last-session.json"), record)
  }
}

fn unix_time_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_millis()
    .min(u64::MAX as u128) as u64
}

/// Creates and lists server instances. Each new instance is assembled in a
/// same-filesystem staging directory and becomes visible with one directory rename.
#[derive(Clone)]
pub struct ServerInstanceManager {
  context: Context,
}

impl ServerInstanceManager {
  pub fn new(context: Context) -> Self {
    Self { context }
  }

  /// Create from a verified cache result. The jar is re-hashed as it is copied so
  /// a cache change between download and publication cannot create a bad instance.
  pub async fn create(
    &self,
    name: &str,
    plan: &ServerInstallPlan,
    download: &ServerInstallResult,
    cancel: &CancellationToken,
  ) -> Result<ServerInstance> {
    let cancel = cancel.child_token();
    let _cancel_on_drop = CancelOnDrop(cancel.clone());
    check_cancel(&cancel)?;
    let id = server_instance_id(name)?;
    let expected_cache_path = self
      .context
      .paths
      .cache
      .join("servers")
      .join("sha1")
      .join(format!("{}.jar", plan.cache_key()));
    if !same_regular_file_location(&download.cached_jar, &expected_cache_path)? {
      return Err(Error::Conflict(
        "server artifact did not come from the managed cache".into(),
      ));
    }

    let context = self.context.clone();
    let artifact = plan.artifact.clone();
    let cached_jar = download.cached_jar.clone();
    let instance_name = name.to_owned();
    let worker_cancel = cancel.clone();
    self
      .blocking(&cancel, move || {
        let _lock = storage::server_instance_lock(&context.paths.data, &id)?;
        let instances = context.paths.data.join("instances");
        ensure_directory(&instances, true)?;
        let target = instances.join(&id);
        match fs::symlink_metadata(&target) {
          Ok(_) => {
            return Err(Error::Conflict(format!(
              "server instance already exists: {id}"
            )));
          }
          Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
          Err(error) => return Err(error.into()),
        }
        check_cancel(&worker_cancel)?;

        let staging = tempfile::Builder::new()
          .prefix(".create-server-")
          .tempdir_in(&instances)?;
        fs::create_dir(staging.path().join("game"))?;
        let jar = ServerJar {
          file: "server.jar".into(),
          sha1: artifact.sha1.clone(),
          size: artifact.size,
        };
        copy_verified_jar(
          &cached_jar,
          &staging.path().join(&jar.file),
          &artifact,
          &worker_cancel,
        )?;
        check_cancel(&worker_cancel)?;
        let instance = ServerInstance {
          schema_version: 1,
          instance_type: ServerInstanceType::Server,
          id: id.clone(),
          name: instance_name,
          version_id: artifact.version_id,
          java_component: artifact.java_component,
          java_major: artifact.java_major,
          server_jar: jar,
          eula_accepted: false,
          eula_url: None,
        };
        validate_instance_record(&instance)?;
        storage::write_json(&staging.path().join("instance.json"), &instance)?;
        sync_directory(&staging.path().join("game"))?;
        sync_directory(staging.path())?;
        check_cancel(&worker_cancel)?;
        fs::rename(staging.path(), &target)?;
        sync_directory(&instances)?;
        Ok(instance)
      })
      .await
  }

  /// Read server records without scanning client instances or hashing every jar.
  pub async fn list(&self) -> Result<Vec<ServerInstance>> {
    let instances = self.context.paths.data.join("instances");
    self
      .blocking(&CancellationToken::new(), move || {
        match fs::symlink_metadata(&instances) {
          Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(Error::Conflict(
              "server instances root is not a directory".into(),
            ));
          }
          Ok(_) => {}
          Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
          Err(error) => return Err(error.into()),
        }
        let mut servers = Vec::new();
        for entry in fs::read_dir(&instances)? {
          let entry = entry?;
          let id = entry.file_name().to_string_lossy().into_owned();
          if !id.starts_with("server-") {
            continue;
          }
          let file_type = entry.file_type()?;
          if !file_type.is_dir() || file_type.is_symlink() {
            return Err(Error::Conflict(format!(
              "invalid server instance directory: {id}"
            )));
          }
          let directory = entry.path();
          let manifest = directory.join("instance.json");
          let metadata = fs::symlink_metadata(&manifest)?;
          if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(Error::Conflict(format!(
              "invalid server instance manifest: {id}"
            )));
          }
          let instance: ServerInstance = storage::read_json(&manifest)?;
          validate_instance_record(&instance)?;
          if instance.id != id {
            return Err(Error::Conflict(format!(
              "server instance ID mismatch: {id}"
            )));
          }
          validate_instance_files(&directory, &instance)?;
          servers.push(instance);
        }
        servers.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(servers)
      })
      .await
  }

  /// Resolve the EULA decision and prepare a verified Java runtime for a future
  /// server process. A missing decision fails before Java installation begins.
  pub async fn prepare_start(
    &self,
    name: &str,
    accept_eula: bool,
    cancel: &CancellationToken,
    java_progress: watch::Sender<JavaProgress>,
  ) -> Result<ServerStartPreparation> {
    let cancel = cancel.child_token();
    let _cancel_on_drop = CancelOnDrop(cancel.clone());
    check_cancel(&cancel)?;
    let id = server_instance_id(name)?;
    let context = self.context.clone();
    let lock_id = id.clone();
    let instance_lock = self
      .blocking(&cancel, move || {
        storage::server_instance_lock(&context.paths.data, &lock_id)
      })
      .await?;

    let context = self.context.clone();
    let worker_cancel = cancel.clone();
    let accepted_with_option = accept_eula;
    let instance = self
      .blocking(&cancel, move || {
        check_cancel(&worker_cancel)?;
        let (directory, mut instance) = read_server_instance(&context, &id)?;
        let eula_path = directory.join("game").join("eula.txt");
        let file_state = read_eula_state(&eula_path)?;
        let revoked = file_state == EulaState::Declined;
        if !accepted_with_option && (!instance.eula_accepted || revoked) {
          return Err(Error::EulaRequired);
        }
        if accepted_with_option || instance.eula_url.as_deref() != Some(MINECRAFT_EULA_URL) {
          instance.eula_accepted = true;
          instance.eula_url = Some(MINECRAFT_EULA_URL.into());
          storage::replace_file(&eula_path, EULA_FILE)?;
          storage::replace_json(&directory.join("instance.json"), &instance)?;
        } else if matches!(file_state, EulaState::Missing | EulaState::Unknown) {
          // The persisted explicit decision is authoritative if the generated file
          // was removed or edited without setting eula=false.
          storage::replace_file(&eula_path, EULA_FILE)?;
        }
        validate_instance_record(&instance)?;
        validate_instance_files(&directory, &instance)?;
        Ok(instance)
      })
      .await?;

    check_cancel(&cancel)?;
    let java = JavaManager::new(self.context.clone())
      .ensure_major(instance.java_major, &cancel, java_progress)
      .await?
      .runtime;
    let instance_directory = self.context.paths.data.join("instances").join(&instance.id);
    Ok(ServerStartPreparation {
      server_jar: instance_directory.join(&instance.server_jar.file),
      game_directory: instance_directory.join("game"),
      instance_directory,
      instance,
      java,
      _instance_lock: instance_lock,
      session: None,
    })
  }

  async fn blocking<T, F>(&self, cancel: &CancellationToken, work: F) -> Result<T>
  where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
  {
    let permit = tokio::select! {
      _ = cancel.cancelled() => return Err(Error::Cancelled),
      permit = self.context.blocking.clone().acquire_owned() => permit.map_err(|_| Error::Cancelled)?,
    };
    tokio::task::spawn_blocking(move || {
      let _permit = permit;
      work()
    })
    .await?
  }
}

async fn watch_download(
  task: Task,
  cancel: &CancellationToken,
  progress: &watch::Sender<ServerProgress>,
) -> Result<PathBuf> {
  let mut updates = task.subscribe_progress();
  loop {
    let state = *updates.borrow_and_update();
    progress.send_modify(|p| {
      p.downloaded_bytes = state.staged_bytes;
      p.total_bytes = Some(state.total_bytes);
      p.stage = match state.state {
        DownloadState::Verifying => ServerInstallStage::Verifying,
        DownloadState::Publishing => ServerInstallStage::Publishing,
        DownloadState::Completed | DownloadState::Failed | DownloadState::Cancelled => p.stage,
        DownloadState::Queued | DownloadState::Downloading => ServerInstallStage::Downloading,
      };
    });
    if state.state.is_terminal() {
      break;
    }
    tokio::select! {
      _ = cancel.cancelled() => {
        task.cancel();
        break;
      }
      result = updates.changed() => {
        if result.is_err() {
          break;
        }
      }
    }
  }
  task.wait().await.map_err(Error::from)
}

fn ensure_directory(path: &Path, create: bool) -> Result<()> {
  match fs::symlink_metadata(path) {
    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => Err(
      Error::Conflict(format!("expected a regular directory: {}", path.display())),
    ),
    Ok(_) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
      fs::create_dir_all(path)?;
      Ok(())
    }
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(error.into()),
    Err(error) => Err(error.into()),
  }
}

/// Validate an instance name and return its stable, case-insensitive ID.
pub fn server_instance_id(name: &str) -> Result<String> {
  if name.is_empty()
    || name.len() > 48
    || name.ends_with('.')
    || !name.as_bytes()[0].is_ascii_alphanumeric()
    || !name
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
  {
    return Err(Error::Invalid(
      "server name must start with a letter or digit and contain only letters, digits, '.', '_' or '-' (max 48 bytes)".into(),
    ));
  }
  Ok(format!("server-{}", name.to_ascii_lowercase()))
}

fn validate_instance_record(instance: &ServerInstance) -> Result<()> {
  let expected_id = server_instance_id(&instance.name)
    .map_err(|_| Error::Conflict("invalid server instance name in record".into()))?;
  if instance.schema_version != 1
    || instance.instance_type != ServerInstanceType::Server
    || instance.id != expected_id
    || !valid_version_id(&instance.version_id)
    || instance.java_component.is_empty()
    || instance.java_component.len() > 64
    || !instance
      .java_component
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
    || !(8..=99).contains(&instance.java_major)
    || instance.server_jar.file != "server.jar"
    || instance.server_jar.size == 0
    || !valid_sha1(&instance.server_jar.sha1)
    || (instance.eula_accepted && instance.eula_url.as_deref() != Some(MINECRAFT_EULA_URL))
    || (!instance.eula_accepted && instance.eula_url.is_some())
  {
    return Err(Error::Conflict(format!(
      "invalid server instance record: {}",
      instance.id
    )));
  }
  Ok(())
}

#[derive(PartialEq, Eq)]
enum EulaState {
  Missing,
  Accepted,
  Declined,
  Unknown,
}

fn read_eula_state(path: &Path) -> Result<EulaState> {
  let file = match fs::symlink_metadata(path) {
    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
      return Err(Error::Conflict(
        "server EULA file is not a regular file".into(),
      ));
    }
    Ok(_) => fs::File::open(path)?,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
      return Ok(EulaState::Missing);
    }
    Err(error) => return Err(error.into()),
  };
  let mut bytes = Vec::new();
  file.take(16 * 1024 + 1).read_to_end(&mut bytes)?;
  if bytes.len() > 16 * 1024 {
    return Err(Error::Conflict(
      "server EULA file exceeds size limit".into(),
    ));
  }
  let text = String::from_utf8_lossy(&bytes);
  let value = text
    .lines()
    .filter_map(|line| {
      let line = line.trim();
      if line.starts_with('#') || line.starts_with('!') {
        None
      } else {
        line.strip_prefix("eula=").map(str::trim)
      }
    })
    .next_back();
  Ok(match value {
    Some(value) if value.eq_ignore_ascii_case("true") => EulaState::Accepted,
    Some(value) if value.eq_ignore_ascii_case("false") => EulaState::Declined,
    _ => EulaState::Unknown,
  })
}

fn read_server_instance(context: &Context, id: &str) -> Result<(PathBuf, ServerInstance)> {
  let directory = context.paths.data.join("instances").join(id);
  let metadata = fs::symlink_metadata(&directory).map_err(|error| match error.kind() {
    std::io::ErrorKind::NotFound => Error::NotFound(id.into()),
    _ => Error::Io(error),
  })?;
  if metadata.file_type().is_symlink() || !metadata.is_dir() {
    return Err(Error::Conflict(format!(
      "invalid server instance directory: {id}"
    )));
  }
  let manifest = directory.join("instance.json");
  let manifest_metadata = fs::symlink_metadata(&manifest)?;
  if manifest_metadata.file_type().is_symlink() || !manifest_metadata.is_file() {
    return Err(Error::Conflict(format!(
      "invalid server instance manifest: {id}"
    )));
  }
  let instance: ServerInstance = storage::read_json(&manifest)?;
  validate_instance_record(&instance)?;
  if instance.id != id {
    return Err(Error::Conflict(format!(
      "server instance ID mismatch: {id}"
    )));
  }
  validate_instance_files(&directory, &instance)?;
  Ok((directory, instance))
}

fn validate_instance_files(directory: &Path, instance: &ServerInstance) -> Result<()> {
  let jar = directory.join(&instance.server_jar.file);
  let metadata = fs::symlink_metadata(&jar)?;
  if metadata.file_type().is_symlink()
    || !metadata.is_file()
    || metadata.len() != instance.server_jar.size
  {
    return Err(Error::Conflict(format!(
      "server jar is missing or changed: {}",
      instance.id
    )));
  }
  let game = fs::symlink_metadata(directory.join("game"))?;
  if game.file_type().is_symlink() || !game.is_dir() {
    return Err(Error::Conflict(format!(
      "server game directory is missing or invalid: {}",
      instance.id
    )));
  }
  Ok(())
}

fn copy_verified_jar(
  source: &Path,
  destination: &Path,
  artifact: &ServerArtifact,
  cancel: &CancellationToken,
) -> Result<()> {
  let metadata = fs::symlink_metadata(source)?;
  if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != artifact.size {
    return Err(Error::Http(q2q_http::Error::Integrity));
  }
  let mut input = fs::File::open(source)?;
  let mut output = fs::OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(destination)?;
  let mut digest = Sha1::new();
  let mut copied = 0u64;
  let mut buffer = [0u8; 64 * 1024];
  loop {
    check_cancel(cancel)?;
    let count = input.read(&mut buffer)?;
    if count == 0 {
      break;
    }
    copied = copied
      .checked_add(count as u64)
      .ok_or_else(|| Error::Metadata("server jar size overflow".into()))?;
    if copied > artifact.size {
      return Err(Error::Http(q2q_http::Error::Integrity));
    }
    digest.update(&buffer[..count]);
    output.write_all(&buffer[..count])?;
  }
  let digest = digest
    .finalize()
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect::<String>();
  if copied != artifact.size || digest != artifact.sha1 {
    return Err(Error::Http(q2q_http::Error::Integrity));
  }
  output.sync_all()?;
  Ok(())
}

fn sync_directory(path: &Path) -> Result<()> {
  #[cfg(unix)]
  fs::File::open(path)?.sync_all()?;
  #[cfg(not(unix))]
  let _ = path;
  Ok(())
}

fn valid_version_id(version: &str) -> bool {
  crate::metadata::is_valid_version_id(version)
}

fn valid_sha1(value: &str) -> bool {
  value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn make_plan(artifact: ServerArtifact) -> Result<ServerInstallPlan> {
  validate_artifact(&artifact)?;
  Ok(ServerInstallPlan { artifact })
}

fn validate_artifact(artifact: &ServerArtifact) -> Result<()> {
  if !valid_version_id(&artifact.version_id)
    || artifact.java_major < 8
    || artifact.java_major > 99
    || artifact.size == 0
    || artifact.size > 1024 * 1024 * 1024
    || artifact.sha1.len() != 40
    || !artifact.sha1.bytes().all(|b| b.is_ascii_hexdigit())
  {
    return Err(Error::Metadata("invalid server install artifact".into()));
  }
  validate_artifact_url(&artifact.url)
}

fn validate_artifact_url(url: &Url) -> Result<()> {
  #[cfg(test)]
  let local_test_server = url.scheme() == "http"
    && url
      .host_str()
      .and_then(|host| host.parse::<std::net::IpAddr>().ok())
      .is_some_and(|ip| ip.is_loopback());
  #[cfg(not(test))]
  let local_test_server = false;

  if !(url.scheme() == "https" || local_test_server)
    || url.host_str().is_none()
    || !url.username().is_empty()
    || url.password().is_some()
    || url
      .port()
      .is_some_and(|port| port != 443 && !local_test_server)
  {
    return Err(Error::Metadata("server artifact URL must use HTTPS".into()));
  }
  Ok(())
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
  let mut value = OsString::from(path.as_os_str());
  value.push(suffix);
  value.into()
}

#[cfg(test)]
#[path = "../tests/server.rs"]
mod tests;
