//! Verified client artifact downloads and atomic installation snapshots.
pub(crate) mod classpath;
mod crashes;
mod install_java;
mod logs;
mod removal;
mod settings;
pub use crashes::{ClientCrashReport, ClientCrashReportEntry, ClientCrashReports};
pub use logs::ClientGameLog;
pub use removal::ClientRemoval;
pub use settings::ClientLaunchSettings;
use std::{
  collections::{BTreeMap, HashMap, VecDeque},
  ffi::OsString,
  fs,
  io::Read,
  path::{Path, PathBuf},
  sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
  },
  time::{Duration, Instant},
};

use q2q_http::{Checksum, DownloadRequest, Task};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tokio::{sync::watch, task::JoinSet};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{
  Context, Error, Result,
  client_plan::{
    ClientArchitecture, ClientAssetObject, ClientInstallPlan, ClientOs, ClientTarget, plan_client,
  },
  error::check_cancel,
  extract,
  java::{InstalledJava, JavaManager, JavaProgress},
  metadata::{self, ClientFile},
  platform::Platform,
  storage,
};

const CLIENT_DOWNLOAD_CONCURRENCY: usize = 4;
const MAX_ASSET_INDEX_SIZE: usize = 16 * 1024 * 1024;
pub(crate) const MAX_INSTALL_RECORD_SIZE: usize = 32 * 1024 * 1024;
const MAX_INSTALL_FILES: usize = 120_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientInstallStage {
  Java,
  Resolving,
  Planning,
  Downloading,
  Verifying,
  Publishing,
  Completed,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClientProgress {
  pub java_major: Option<u32>,
  pub java_stage: Option<crate::java::InstallStage>,
  pub stage: ClientInstallStage,
  pub completed_files: usize,
  pub total_files: Option<usize>,
  pub downloaded_bytes: u64,
  pub total_bytes: Option<u64>,
  pub received_bytes: u64,
}

impl Default for ClientProgress {
  fn default() -> Self {
    Self {
      stage: ClientInstallStage::Resolving,
      java_major: None,
      java_stage: None,
      completed_files: 0,
      total_files: None,
      downloaded_bytes: 0,
      total_bytes: None,
      received_bytes: 0,
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientInstance {
  pub schema_version: u32,
  pub instance_type: String,
  pub id: String,
  pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClientListEntry {
  pub loader: Option<crate::fabric::ClientLoader>,
  #[serde(flatten)]
  pub instance: ClientInstance,
  pub version_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientInstalledFile {
  /// Logical path including its Minecraft root, such as `libraries/...` or `assets/objects/...`.
  pub path: String,
  pub sha1: String,
  pub size: u64,
  /// Path relative to the shared cache root; the artifact is not copied into each instance.
  pub cache_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientInstallRecord {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub loader: Option<crate::fabric::ClientLoader>,
  pub schema_version: u32,
  pub version_id: String,
  pub version_type: String,
  pub metadata_url: String,
  pub metadata_sha1: String,
  pub java_component: String,
  pub java_major: u32,
  pub os: String,
  pub architecture: String,
  pub asset_index_id: String,
  pub main_class: String,
  pub arguments: metadata::ClientArguments,
  pub files: Vec<ClientInstalledFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientInstallResult {
  pub instance: ClientInstance,
  pub installed: ClientInstallRecord,
  pub already_installed: bool,
  pub downloaded_files: usize,
  pub reused_files: usize,
}

/// Runtime paths prepared for one launch session. Dropping this handle removes
/// its temporary natives directory; assets remain in the shared cache.
pub struct ClientRuntimePreparation {
  pub instance: ClientInstance,
  pub installed: ClientInstallRecord,
  pub java: InstalledJava,
  pub cache_root: PathBuf,
  pub assets_root: PathBuf,
  pub asset_index: PathBuf,
  pub natives_directory: PathBuf,
  pub session_directory: PathBuf,
  _session: tempfile::TempDir,
  _instance_lock: Arc<fs::File>,
}

struct PreparedClientFiles {
  session: tempfile::TempDir,
  assets_root: PathBuf,
  asset_index: PathBuf,
  natives_directory: PathBuf,
}

#[derive(Clone)]
struct Artifact {
  sha1: String,
  size: u64,
  urls: Vec<String>,
}

struct CacheResult {
  path: PathBuf,
  reused: bool,
}

#[derive(Clone)]
struct DownloadBatch {
  cancel: CancellationToken,
  completed: Arc<AtomicUsize>,
  downloaded: Arc<AtomicUsize>,
  reused: Arc<AtomicUsize>,
  progress: watch::Sender<ClientProgress>,
  total: usize,
  bytes: Arc<Mutex<BatchBytes>>,
}

#[derive(Default)]
struct BatchBytes {
  // Only in-flight files are retained (bounded by download concurrency).
  active: HashMap<String, (u64, u64)>,
  completed: u64,
  received: u64,
  last_publish: Option<Instant>,
}

struct FileByteProgress {
  key: String,
  bytes: Arc<Mutex<BatchBytes>>,
  progress: watch::Sender<ClientProgress>,
}

impl FileByteProgress {
  fn begin(&self) {
    self
      .bytes
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .active
      .insert(self.key.clone(), (0, 0));
  }

  fn update(&self, received: u64, staged: u64, force: bool) {
    let mut bytes = self
      .bytes
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    let previous = bytes.active.get(&self.key).map_or(0, |entry| entry.0);
    bytes.received = bytes
      .received
      .saturating_add(received.saturating_sub(previous));
    bytes.active.insert(self.key.clone(), (received, staged));
    self.publish(&mut bytes, force);
  }

  fn finish(&self, size: u64) {
    let mut bytes = self
      .bytes
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    bytes.active.remove(&self.key);
    bytes.completed = bytes.completed.saturating_add(size);
    self.publish(&mut bytes, true);
  }

  fn publish(&self, bytes: &mut BatchBytes, force: bool) {
    if !force
      && bytes
        .last_publish
        .is_some_and(|last| last.elapsed() < Duration::from_millis(100))
    {
      return;
    }
    let staged = bytes.active.values().map(|entry| entry.1).sum::<u64>();
    self.progress.send_modify(|progress| {
      progress.downloaded_bytes = bytes.completed.saturating_add(staged);
      progress.received_bytes = bytes.received;
    });
    bytes.last_publish = Some(Instant::now());
  }
}

struct CancelOnDrop(CancellationToken);

impl Drop for CancelOnDrop {
  fn drop(&mut self) {
    self.0.cancel();
  }
}

/// Resolves official client metadata, downloads immutable files through q2q-http,
/// and atomically publishes per-instance installation records.
#[derive(Clone)]
pub struct ClientInstaller {
  context: Context,
}

impl ClientInstaller {
  pub fn new(context: Context) -> Self {
    Self { context }
  }

  /// Validate a user-facing client instance name before starting a network operation.
  pub fn validate_name(name: &str) -> Result<()> {
    client_instance_id(name).map(|_| ())
  }

  /// Resolve the official client metadata, fetch its verified asset index, and
  /// return the platform-specific plan. Asset-index bytes are bounded to 16 MiB.
  pub async fn plan(
    &self,
    version_id: &str,
    target: ClientTarget,
    cancel: &CancellationToken,
    progress: &watch::Sender<ClientProgress>,
  ) -> Result<ClientInstallPlan> {
    let cancel = cancel.child_token();
    let _cancel_on_drop = CancelOnDrop(cancel.clone());
    check_cancel(&cancel)?;
    progress.send_replace(ClientProgress::default());
    let version = tokio::select! {
      _ = cancel.cancelled() => return Err(Error::Cancelled),
      result = metadata::resolve_client(&self.context, version_id) => result?,
    };
    check_cancel(&cancel)?;
    progress.send_replace(ClientProgress {
      stage: ClientInstallStage::Planning,
      completed_files: 0,
      total_files: Some(1),
      ..ClientProgress::default()
    });
    let cached = self
      .download_cached(&version.asset_index.file, &cancel)
      .await?;
    let asset_index = self
      .blocking(&cancel, move || {
        read_bounded_regular(&cached.path, MAX_ASSET_INDEX_SIZE)
      })
      .await?;
    check_cancel(&cancel)?;
    let plan = plan_client(&version, &asset_index, target)?;
    progress.send_replace(ClientProgress {
      stage: ClientInstallStage::Planning,
      completed_files: 1,
      total_files: Some(1),
      ..ClientProgress::default()
    });
    Ok(plan)
  }

  /// Resolve the base game and an optional Fabric profile into one atomic snapshot.
  pub async fn plan_with_loader(
    &self,
    version_id: &str,
    target: ClientTarget,
    loader: Option<&crate::fabric::ClientLoader>,
    cancel: &CancellationToken,
    progress: &watch::Sender<ClientProgress>,
  ) -> Result<ClientInstallPlan> {
    let mut plan = self.plan(version_id, target, cancel, progress).await?;
    if let Some(loader) = loader {
      progress.send_modify(|state| state.stage = ClientInstallStage::Planning);
      crate::fabric::apply(&self.context, &mut plan, loader, cancel).await?;
    }
    Ok(plan)
  }

  /// Verify or download every planned artifact, then create or update a client
  /// instance. New instances appear with one directory rename; updates replace
  /// only `installed.json`, leaving saves and other game data untouched.
  pub async fn install(
    &self,
    name: &str,
    plan: &ClientInstallPlan,
    cancel: &CancellationToken,
    progress: &watch::Sender<ClientProgress>,
  ) -> Result<ClientInstallResult> {
    let cancel = cancel.child_token();
    let _cancel_on_drop = CancelOnDrop(cancel.clone());
    check_cancel(&cancel)?;
    let id = client_instance_id(name)?;
    let (installed_files, artifacts) = collect_plan_files(plan)?;
    let installed = make_install_record(plan, installed_files)?;
    validate_install_record(&installed)?;
    progress.send_replace(ClientProgress {
      stage: ClientInstallStage::Downloading,
      completed_files: 0,
      total_files: Some(artifacts.len()),
      total_bytes: Some(artifacts.iter().map(|artifact| artifact.size).sum()),
      ..ClientProgress::default()
    });

    let context = self.context.clone();
    let lock_id = id.clone();
    let _instance_lock = self
      .blocking(&cancel, move || {
        storage::server_instance_lock(&context.paths.data, &lock_id)
      })
      .await?;
    let context = self.context.clone();
    let instance_name = name.to_owned();
    let check_id = id.clone();
    let existing = self
      .blocking(&cancel, move || {
        check_client_instance(&context, &check_id, &instance_name)
      })
      .await?;
    if existing
      .as_ref()
      .is_some_and(|record| record.loader != installed.loader)
    {
      return Err(Error::Conflict(
        "changing an existing instance loader is not supported; create a new instance".into(),
      ));
    }
    let same_snapshot = existing.as_ref() == Some(&installed);

    let (downloaded_files, reused_files) = self.download_all(artifacts, &cancel, progress).await?;
    let already_installed = same_snapshot && downloaded_files == 0;
    check_cancel(&cancel)?;
    progress.send_modify(|state| state.stage = ClientInstallStage::Verifying);
    // q2q-http verifies each file before its task completes. This barrier makes
    // sure every cache entry has completed before the install record is written.
    check_cancel(&cancel)?;
    progress.send_modify(|state| state.stage = ClientInstallStage::Publishing);

    let context = self.context.clone();
    let commit_id = id.clone();
    let commit_name = name.to_owned();
    let committed = installed.clone();
    let commit_cancel = cancel.clone();
    let committed_instance = self
      .blocking(&cancel, move || {
        commit_instance(
          &context,
          &commit_id,
          &commit_name,
          &committed,
          existing.is_some(),
          &commit_cancel,
        )
      })
      .await?;
    progress.send_modify(|state| state.stage = ClientInstallStage::Completed);
    Ok(ClientInstallResult {
      instance: committed_instance,
      installed,
      already_installed,
      downloaded_files,
      reused_files,
    })
  }

  /// Prepare verified assets and session-local natives for one client instance.
  /// A matching managed Java runtime is reused; otherwise the Java manager
  /// installs the requested major before this method returns.
  pub async fn prepare_runtime(
    &self,
    instance: &ClientInstance,
    plan: &ClientInstallPlan,
    cancel: &CancellationToken,
    java_progress: watch::Sender<JavaProgress>,
  ) -> Result<ClientRuntimePreparation> {
    self
      .prepare_runtime_with_java_policy(instance, plan, cancel, java_progress, true)
      .await
  }

  /// Desktop launches may require explicit approval before acquiring missing Java.
  pub async fn prepare_runtime_with_java_policy(
    &self,
    instance: &ClientInstance,
    plan: &ClientInstallPlan,
    cancel: &CancellationToken,
    java_progress: watch::Sender<JavaProgress>,
    install_java: bool,
  ) -> Result<ClientRuntimePreparation> {
    let cancel = cancel.child_token();
    let _cancel_on_drop = CancelOnDrop(cancel.clone());
    check_cancel(&cancel)?;
    validate_client_instance(instance)?;
    validate_current_target(&plan.target)?;
    let (expected_files, _) = collect_plan_files(plan)?;
    let expected = make_install_record(plan, expected_files)?;

    // Keep installs from replacing this instance until the prepared runtime is dropped.
    let context = self.context.clone();
    let lock_id = instance.id.clone();
    let instance_lock = self
      .blocking(&cancel, move || {
        storage::server_instance_lock(&context.paths.data, &lock_id)
      })
      .await?;

    let paths = self.context.paths.clone();
    let expected_instance = instance.clone();
    let instance_id = instance.id.clone();
    let plan_for_files = plan.clone();
    let java_major = plan.java_major;
    let files_cancel = cancel.clone();
    let (installed, prepared) = self
      .blocking(&cancel, move || {
        ensure_directory(&paths.data.join("instances"), false)?;
        let directory = paths.data.join("instances").join(&instance_id);
        ensure_directory(&directory, false)?;
        let stored_instance: ClientInstance =
          read_json_regular(&directory.join("instance.json"), 64 * 1024)?;
        validate_client_instance(&stored_instance)?;
        if stored_instance != expected_instance {
          return Err(Error::Conflict(
            "client instance changed since it was selected".into(),
          ));
        }
        let game = fs::symlink_metadata(directory.join("game"))?;
        if game.file_type().is_symlink() || !game.is_dir() {
          return Err(Error::Conflict("client game directory is invalid".into()));
        }
        let installed: ClientInstallRecord =
          read_json_regular(&directory.join("installed.json"), MAX_INSTALL_RECORD_SIZE)?;
        validate_install_record(&installed)?;
        if installed != expected {
          return Err(Error::Conflict(
            "client installation does not match the supplied version plan; reinstall the client"
              .into(),
          ));
        }
        crate::resources::sync_for_launch(
          &directory,
          &paths.data,
          installed.loader.is_some(),
          &files_cancel,
        )?;
        let files = prepare_client_files(
          &paths,
          &instance_id,
          &installed,
          &plan_for_files,
          &files_cancel,
        )?;
        Ok((installed, files))
      })
      .await?;

    let manager = JavaManager::new(self.context.clone());
    let java = if install_java {
      manager
        .ensure_major(java_major, &cancel, java_progress)
        .await?
        .runtime
    } else {
      manager
        .find_major(java_major, &cancel, java_progress)
        .await?
        .ok_or_else(|| Error::Java(format!("managed Java {java_major} is required")))?
    };
    validate_java_for_plan(&java, plan)?;
    check_cancel(&cancel)?;

    let session_directory = prepared.session.path().to_path_buf();
    Ok(ClientRuntimePreparation {
      instance: instance.clone(),
      installed,
      java,
      cache_root: self.context.paths.cache.clone(),
      assets_root: prepared.assets_root.clone(),
      asset_index: prepared.asset_index.clone(),
      natives_directory: prepared.natives_directory.clone(),
      session_directory,
      _session: prepared.session,
      _instance_lock: instance_lock,
    })
  }

  /// Read one instance and its validated snapshot without network or artifact hashing.
  pub async fn details(&self, name: &str) -> Result<(ClientInstance, ClientInstallRecord)> {
    let id = client_instance_id(name)?;
    let context = self.context.clone();
    self
      .blocking(&CancellationToken::new(), move || {
        let instances = context.paths.data.join("instances");
        for directory in [&instances, &instances.join(&id)] {
          let metadata = fs::symlink_metadata(directory).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
              Error::NotFound("Minecraft client instance".into())
            } else {
              error.into()
            }
          })?;
          if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(Error::Conflict(
              "client instance directory is invalid".into(),
            ));
          }
        }
        // Read-only snapshots remain available while the game owns the instance lock.
        let directory = instances.join(&id);
        let instance: ClientInstance =
          read_json_regular(&directory.join("instance.json"), 64 * 1024)?;
        validate_client_instance(&instance)?;
        if instance.id != id {
          return Err(Error::Conflict(
            "client instance identity does not match its directory".into(),
          ));
        }
        let game = fs::symlink_metadata(directory.join("game"))?;
        if game.file_type().is_symlink() || !game.is_dir() {
          return Err(Error::Conflict("client game directory is invalid".into()));
        }
        let record: ClientInstallRecord =
          read_json_regular(&directory.join("installed.json"), MAX_INSTALL_RECORD_SIZE)?;
        validate_install_record(&record)?;
        Ok((instance, record))
      })
      .await
  }

  /// Read the validated installation snapshot for an instance returned by `list`.
  /// The instance record is checked again to catch replacement between listing and use.
  pub async fn load_install_record(
    &self,
    instance: &ClientInstance,
  ) -> Result<ClientInstallRecord> {
    validate_client_instance(instance)?;
    let paths = self.context.paths.clone();
    let expected = instance.clone();
    self
      .blocking(&CancellationToken::new(), move || {
        let instances = paths.data.join("instances");
        let metadata = fs::symlink_metadata(&instances)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
          return Err(Error::Conflict("client instances root is invalid".into()));
        }
        let directory = paths.data.join("instances").join(&expected.id);
        let metadata = fs::symlink_metadata(&directory)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
          return Err(Error::Conflict(
            "client instance directory is invalid".into(),
          ));
        }
        let stored: ClientInstance =
          read_json_regular(&directory.join("instance.json"), 64 * 1024)?;
        validate_client_instance(&stored)?;
        if stored != expected {
          return Err(Error::Conflict(
            "client instance changed since it was selected".into(),
          ));
        }
        let record: ClientInstallRecord =
          read_json_regular(&directory.join("installed.json"), MAX_INSTALL_RECORD_SIZE)?;
        validate_install_record(&record)?;
        Ok(record)
      })
      .await
  }

  /// List only client instances with a complete, valid installation snapshot.
  pub async fn list(&self) -> Result<Vec<ClientInstance>> {
    self
      .list_with_versions()
      .await
      .map(|entries| entries.into_iter().map(|entry| entry.instance).collect())
  }

  /// List complete client instances with their currently installed version IDs.
  pub async fn list_with_versions(&self) -> Result<Vec<ClientListEntry>> {
    let instances = self.context.paths.data.join("instances");
    self
      .blocking(&CancellationToken::new(), move || {
        match fs::symlink_metadata(&instances) {
          Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(Error::Conflict(
              "client instances root is not a directory".into(),
            ));
          }
          Ok(_) => {}
          Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
          Err(error) => return Err(error.into()),
        }
        let mut clients = Vec::new();
        for entry in fs::read_dir(instances)? {
          let entry = entry?;
          let id = entry.file_name().to_string_lossy().into_owned();
          if !id.starts_with("client-") {
            continue;
          }
          let entry_type = entry.file_type()?;
          if !entry_type.is_dir() || entry_type.is_symlink() {
            return Err(Error::Conflict(format!(
              "invalid client instance directory: {id}"
            )));
          }
          let directory = entry.path();
          let instance: ClientInstance =
            read_json_regular(&directory.join("instance.json"), 64 * 1024)?;
          validate_client_instance(&instance)?;
          if instance.id != id {
            return Err(Error::Conflict(format!(
              "client instance ID mismatch: {id}"
            )));
          }
          let game = fs::symlink_metadata(directory.join("game"))?;
          if game.file_type().is_symlink() || !game.is_dir() {
            return Err(Error::Conflict(format!(
              "invalid client game directory: {id}"
            )));
          }
          let installed: ClientInstallRecord =
            read_json_regular(&directory.join("installed.json"), MAX_INSTALL_RECORD_SIZE)?;
          validate_install_record(&installed)?;
          clients.push(ClientListEntry {
            loader: installed.loader,
            instance,
            version_id: installed.version_id,
          });
        }
        clients.sort_by(|left, right| left.instance.id.cmp(&right.instance.id));
        Ok(clients)
      })
      .await
  }

  async fn download_all(
    &self,
    artifacts: Vec<Artifact>,
    cancel: &CancellationToken,
    progress: &watch::Sender<ClientProgress>,
  ) -> Result<(usize, usize)> {
    let total = artifacts.len();
    if total == 0 {
      return Err(Error::Metadata("client install plan has no files".into()));
    }
    let mut pending = VecDeque::from(artifacts);
    let mut tasks = JoinSet::new();
    let operation_cancel = cancel.child_token();
    let batch = DownloadBatch {
      cancel: operation_cancel.clone(),
      completed: Arc::new(AtomicUsize::new(0)),
      downloaded: Arc::new(AtomicUsize::new(0)),
      reused: Arc::new(AtomicUsize::new(0)),
      progress: progress.clone(),
      total,
      bytes: Arc::new(Mutex::new(BatchBytes::default())),
    };
    let mut first_error = None;

    while !pending.is_empty() && tasks.len() < CLIENT_DOWNLOAD_CONCURRENCY {
      spawn_download(
        &mut tasks,
        self.clone(),
        pending.pop_front().expect("pending artifact"),
        batch.clone(),
      );
    }
    while let Some(result) = tasks.join_next().await {
      match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
          if first_error.is_none() {
            first_error = Some(error);
            operation_cancel.cancel();
          }
        }
        Err(error) => {
          if first_error.is_none() {
            first_error = Some(Error::Worker(error));
            operation_cancel.cancel();
          }
        }
      }
      if first_error.is_none() && !pending.is_empty() && !operation_cancel.is_cancelled() {
        spawn_download(
          &mut tasks,
          self.clone(),
          pending.pop_front().expect("pending artifact"),
          batch.clone(),
        );
      }
    }
    if let Some(error) = first_error {
      return Err(error);
    }
    check_cancel(cancel)?;
    Ok((
      batch.downloaded.load(Ordering::Relaxed),
      batch.reused.load(Ordering::Relaxed),
    ))
  }

  async fn download_cached(
    &self,
    file: &ClientFile,
    cancel: &CancellationToken,
  ) -> Result<CacheResult> {
    self
      .download_cached_with_urls(file, vec![file.url.as_str().to_owned()], cancel, None)
      .await
  }

  async fn download_artifact_cached(
    &self,
    artifact: &Artifact,
    cancel: &CancellationToken,
    observer: &FileByteProgress,
  ) -> Result<CacheResult> {
    let file = ClientFile {
      url: Url::parse(&artifact.urls[0])
        .map_err(|_| Error::Metadata("invalid client artifact URL".into()))?,
      size: artifact.size,
      sha1: artifact.sha1.clone(),
    };
    self
      .download_cached_with_urls(&file, artifact.urls.clone(), cancel, Some(observer))
      .await
  }

  async fn download_cached_with_urls(
    &self,
    file: &ClientFile,
    urls: Vec<String>,
    cancel: &CancellationToken,
    observer: Option<&FileByteProgress>,
  ) -> Result<CacheResult> {
    validate_client_file(file)?;
    for url in &urls {
      let parsed = Url::parse(url)
        .map_err(|_| Error::Metadata("invalid client artifact mirror URL".into()))?;
      let mirror = ClientFile {
        url: parsed,
        ..file.clone()
      };
      validate_client_file(&mirror)?;
    }
    self
      .download_cached_request(file, urls, cancel, observer)
      .await
  }

  async fn download_cached_request(
    &self,
    file: &ClientFile,
    urls: Vec<String>,
    cancel: &CancellationToken,
    observer: Option<&FileByteProgress>,
  ) -> Result<CacheResult> {
    // This is split from `download_cached` so duplicate content can use every
    // trusted URL as a q2q-http mirror while retaining one SHA-1 cache target.
    validate_client_file(file)?;
    let cache_path = self
      .context
      .paths
      .cache
      .join("clients")
      .join("sha1")
      .join(&file.sha1);
    let cache_key = format!("client:{}", file.sha1);
    let _memory_lock = self.context.cache_lock(cache_key, cancel).await?;
    let context = self.context.clone();
    let file_sha1 = file.sha1.clone();
    let lock = self
      .blocking(cancel, move || {
        storage::client_cache_lock(&context.paths.data, &file_sha1)
      })
      .await?;
    let (resume, target_existed) = self
      .blocking(cancel, {
        let cache_path = cache_path.clone();
        move || {
          ensure_cache_parent(&cache_path)?;
          let target_existed = regular_file_state(&cache_path)?;
          let part = sidecar(&cache_path, ".q2q-http.part");
          let journal = sidecar(&cache_path, ".q2q-http.resume");
          Ok((
            target_existed || part.try_exists()? || journal.try_exists()?,
            target_existed,
          ))
        }
      })
      .await?;
    let checksum = Checksum::sha1(&file.sha1)?;
    let request = DownloadRequest::new(&cache_path)
      .mirrors(urls.iter())
      .expected_size(file.size)
      .checksum(checksum);
    let first = self
      .run_download(request.clone(), resume, cancel, observer)
      .await;
    let mut reused = target_existed;
    let downloaded_path = match first {
      Ok(path) => path,
      Err(Error::Http(q2q_http::Error::Integrity)) if target_existed => {
        check_cancel(cancel)?;
        let path = cache_path.clone();
        let resume = self
          .blocking(cancel, move || {
            match fs::symlink_metadata(&path) {
              Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(Error::Conflict("invalid client cache artifact".into()));
              }
              Ok(_) => fs::remove_file(&path)?,
              Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
              Err(error) => return Err(error.into()),
            }
            let part = sidecar(&path, ".q2q-http.part");
            let journal = sidecar(&path, ".q2q-http.resume");
            Ok(part.try_exists()? || journal.try_exists()?)
          })
          .await?;
        reused = false;
        self.run_download(request, resume, cancel, observer).await?
      }
      Err(error) => return Err(error),
    };
    if !same_regular_file_location(&downloaded_path, &cache_path)? {
      return Err(Error::Conflict(
        "client cache published to an unexpected path".into(),
      ));
    }
    drop(lock);
    Ok(CacheResult {
      path: cache_path,
      reused,
    })
  }

  async fn run_download(
    &self,
    request: DownloadRequest,
    resume: bool,
    cancel: &CancellationToken,
    observer: Option<&FileByteProgress>,
  ) -> Result<PathBuf> {
    check_cancel(cancel)?;
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
    if let Some(observer) = observer {
      observer.begin();
    }
    match wait_download(task, cancel, observer).await {
      Err(q2q_http::Error::Cancelled) => Err(Error::Cancelled),
      result => result.map_err(Error::from),
    }
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

fn spawn_download(
  tasks: &mut JoinSet<Result<()>>,
  installer: ClientInstaller,
  artifact: Artifact,
  batch: DownloadBatch,
) {
  tasks.spawn(async move {
    let observer = FileByteProgress {
      key: artifact.sha1.clone(),
      bytes: batch.bytes.clone(),
      progress: batch.progress.clone(),
    };
    let result = installer
      .download_artifact_cached(&artifact, &batch.cancel, &observer)
      .await?;
    observer.finish(artifact.size);
    if result.reused {
      batch.reused.fetch_add(1, Ordering::Relaxed);
    } else {
      batch.downloaded.fetch_add(1, Ordering::Relaxed);
    }
    let count = batch.completed.fetch_add(1, Ordering::Relaxed) + 1;
    batch.progress.send_modify(|state| {
      state.stage = ClientInstallStage::Downloading;
      state.completed_files = count;
      state.total_files = Some(batch.total);
    });
    Ok(())
  });
}

async fn wait_download(
  task: Task,
  cancel: &CancellationToken,
  observer: Option<&FileByteProgress>,
) -> std::result::Result<PathBuf, q2q_http::Error> {
  let mut updates = task.subscribe_progress();
  let mut cancel_sent = false;
  loop {
    let state = *updates.borrow_and_update();
    if let Some(observer) = observer {
      observer.update(
        state.received_bytes,
        state.staged_bytes.min(state.total_bytes),
        state.state.is_terminal(),
      );
    }
    if state.state.is_terminal() {
      break;
    }
    if cancel.is_cancelled() && !cancel_sent {
      task.cancel();
      cancel_sent = true;
    }
    if cancel_sent {
      if updates.changed().await.is_err() {
        break;
      }
    } else {
      tokio::select! {
        _ = cancel.cancelled() => {
          task.cancel();
          cancel_sent = true;
        }
        result = updates.changed() => {
          if result.is_err() {
            break;
          }
        }
      }
    }
  }
  task.wait().await
}

fn collect_plan_files(
  plan: &ClientInstallPlan,
) -> Result<(Vec<ClientInstalledFile>, Vec<Artifact>)> {
  validate_version_id(&plan.version_id)?;
  validate_metadata_identity(&plan.metadata_url, &plan.metadata_sha1)?;
  validate_version_id(&plan.asset_index_id)?;
  let mut refs = Vec::<(String, ClientFile)>::new();
  refs.push((plan.client_jar.path.clone(), plan.client_jar.file.clone()));
  for library in &plan.libraries {
    refs.push((
      format!("libraries/{}", library.artifact.path),
      library.artifact.file.clone(),
    ));
  }
  for archive in &plan.native_archives {
    refs.push((
      format!("libraries/{}", archive.artifact.path),
      archive.artifact.file.clone(),
    ));
  }
  refs.push((
    format!("assets/{}", plan.asset_index.path),
    plan.asset_index.file.clone(),
  ));
  for asset in &plan.assets {
    refs.push((format!("assets/{}", asset.path), asset_file(asset)));
  }
  if let Some(logging) = &plan.logging {
    refs.push((logging.file.path.clone(), logging.file.file.clone()));
  }

  let mut refs_by_path = BTreeMap::<String, ClientInstalledFile>::new();
  let mut artifacts_by_hash = BTreeMap::<String, Artifact>::new();
  for (path, file) in refs {
    validate_layout_path(&path)?;
    validate_client_file(&file)?;
    let installed = ClientInstalledFile {
      path: path.clone(),
      sha1: file.sha1.to_ascii_lowercase(),
      size: file.size,
      cache_path: format!("clients/sha1/{}", file.sha1.to_ascii_lowercase()),
    };
    if let Some(previous) = refs_by_path.insert(path.clone(), installed.clone())
      && previous != installed
    {
      return Err(Error::Metadata("conflicting client install paths".into()));
    }
    match artifacts_by_hash.get_mut(&installed.sha1) {
      Some(artifact) if artifact.size != file.size => {
        return Err(Error::Metadata(
          "conflicting sizes for client content hash".into(),
        ));
      }
      Some(artifact) => {
        let url = file.url.as_str().to_owned();
        if !artifact.urls.contains(&url) && artifact.urls.len() < 32 {
          artifact.urls.push(url);
        }
      }
      None => {
        artifacts_by_hash.insert(
          installed.sha1.clone(),
          Artifact {
            sha1: installed.sha1,
            size: file.size,
            urls: vec![file.url.as_str().to_owned()],
          },
        );
      }
    }
  }
  if refs_by_path.is_empty() || refs_by_path.len() > MAX_INSTALL_FILES {
    return Err(Error::Metadata("invalid client install file count".into()));
  }
  Ok((
    refs_by_path.into_values().collect(),
    artifacts_by_hash.into_values().collect(),
  ))
}

fn asset_file(asset: &ClientAssetObject) -> ClientFile {
  ClientFile {
    url: asset.url.clone(),
    size: asset.size,
    sha1: asset.hash.clone(),
  }
}

fn make_install_record(
  plan: &ClientInstallPlan,
  files: Vec<ClientInstalledFile>,
) -> Result<ClientInstallRecord> {
  Ok(ClientInstallRecord {
    loader: plan.loader.clone(),
    schema_version: 1,
    version_id: plan.version_id.clone(),
    version_type: plan.version_type.clone(),
    metadata_url: plan.metadata_url.as_str().to_owned(),
    metadata_sha1: plan.metadata_sha1.to_ascii_lowercase(),
    java_component: plan.java_component.clone(),
    java_major: plan.java_major,
    os: match plan.target.os {
      ClientOs::Windows => "windows",
      ClientOs::MacOs => "macos",
      ClientOs::Linux => "linux",
    }
    .into(),
    architecture: match plan.target.architecture {
      ClientArchitecture::X86_64 => "x86_64",
      ClientArchitecture::Aarch64 => "aarch64",
    }
    .into(),
    asset_index_id: plan.asset_index_id.clone(),
    main_class: plan.main_class.clone(),
    arguments: plan.arguments.clone(),
    files,
  })
}

fn validate_current_target(target: &ClientTarget) -> Result<()> {
  let current = Platform::current()?;
  let (os, arch) = client_platform_names(target);
  if os != current.os || arch != current.arch {
    return Err(Error::Unsupported(format!(
      "client plan targets {os}/{arch}, but this process runs on {}/{}",
      current.os, current.arch
    )));
  }
  Ok(())
}

fn validate_java_for_plan(java: &InstalledJava, plan: &ClientInstallPlan) -> Result<()> {
  let (os, arch) = client_platform_names(&plan.target);
  if java.major != plan.java_major || java.os != os || java.arch != arch {
    return Err(Error::Java(
      "managed Java does not match the client version and platform".into(),
    ));
  }
  Ok(())
}

fn client_platform_names(target: &ClientTarget) -> (&'static str, &'static str) {
  let os = match target.os {
    ClientOs::Windows => "windows",
    ClientOs::MacOs => "mac",
    ClientOs::Linux => "linux",
  };
  let arch = match target.architecture {
    ClientArchitecture::X86_64 => "x64",
    ClientArchitecture::Aarch64 => "aarch64",
  };
  (os, arch)
}

fn prepare_client_files(
  paths: &crate::Paths,
  instance_id: &str,
  installed: &ClientInstallRecord,
  plan: &ClientInstallPlan,
  cancel: &CancellationToken,
) -> Result<PreparedClientFiles> {
  verify_installed_cache(paths, installed, cancel)?;
  let assets_root = paths.cache.join("assets");
  materialize_assets(paths, &assets_root, installed, plan, cancel)?;

  let sessions = paths.data.join("sessions");
  ensure_directory(&sessions, true)?;
  let session = tempfile::Builder::new()
    .prefix(&format!("{instance_id}-"))
    .tempdir_in(&sessions)?;
  if plan.loader.is_some() {
    classpath::prepare(paths, session.path(), installed, plan, cancel)?;
  }
  let natives_directory = session.path().join("natives");
  fs::create_dir(&natives_directory)?;
  for archive in &plan.native_archives {
    check_cancel(cancel)?;
    let path = format!("libraries/{}", archive.artifact.path);
    let file = installed
      .files
      .iter()
      .find(|file| file.path == path)
      .ok_or_else(|| Error::Conflict("native archive is missing from install record".into()))?;
    extract::unpack_excluding(
      &paths.cache.join(&file.cache_path),
      &natives_directory,
      true,
      &archive.excludes,
      cancel,
    )?;
  }
  let asset_index = assets_root.join(&plan.asset_index.path);
  Ok(PreparedClientFiles {
    session,
    assets_root,
    asset_index,
    natives_directory,
  })
}

fn verify_installed_cache(
  paths: &crate::Paths,
  installed: &ClientInstallRecord,
  cancel: &CancellationToken,
) -> Result<()> {
  let clients = paths.cache.join("clients");
  let sha1_root = clients.join("sha1");
  ensure_directory(&paths.cache, true)?;
  ensure_directory(&clients, true)?;
  ensure_directory(&sha1_root, true)?;
  let mut unique = BTreeMap::<String, u64>::new();
  for file in &installed.files {
    match unique.insert(file.sha1.to_ascii_lowercase(), file.size) {
      Some(previous) if previous != file.size => {
        return Err(Error::Conflict(
          "client cache hash is referenced with conflicting sizes".into(),
        ));
      }
      _ => {}
    }
  }
  for (sha1, size) in unique {
    check_cancel(cancel)?;
    let path = sha1_root.join(&sha1);
    if !file_matches(&path, size, &sha1, cancel)? {
      return Err(Error::Conflict(format!(
        "client cache artifact is missing or corrupt ({sha1}); reinstall the client"
      )));
    }
  }
  Ok(())
}

fn materialize_assets(
  paths: &crate::Paths,
  assets_root: &Path,
  installed: &ClientInstallRecord,
  plan: &ClientInstallPlan,
  cancel: &CancellationToken,
) -> Result<()> {
  ensure_directory(assets_root, true)?;
  let staging = tempfile::Builder::new()
    .prefix(".asset-refs-")
    .tempdir_in(assets_root)?;
  let index_path = format!("assets/{}", plan.asset_index.path);
  let index = installed
    .files
    .iter()
    .find(|file| file.path == index_path)
    .ok_or_else(|| Error::Conflict("asset index is missing from install record".into()))?;
  materialize_asset_reference(
    paths,
    assets_root,
    staging.path(),
    0,
    &plan.asset_index.path,
    index,
    cancel,
  )?;
  for (index, asset) in plan.assets.iter().enumerate() {
    check_cancel(cancel)?;
    let path = format!("assets/{}", asset.path);
    let file = installed
      .files
      .iter()
      .find(|file| file.path == path)
      .ok_or_else(|| Error::Conflict("asset object is missing from install record".into()))?;
    if file.sha1 != asset.hash || file.size != asset.size {
      return Err(Error::Conflict("asset object identity changed".into()));
    }
    materialize_asset_reference(
      paths,
      assets_root,
      staging.path(),
      index + 1,
      &asset.path,
      file,
      cancel,
    )?;
  }
  Ok(())
}

fn materialize_asset_reference(
  paths: &crate::Paths,
  assets_root: &Path,
  staging: &Path,
  staging_index: usize,
  relative_path: &str,
  file: &ClientInstalledFile,
  cancel: &CancellationToken,
) -> Result<()> {
  validate_layout_path(relative_path)?;
  let destination = safe_asset_destination(assets_root, relative_path)?;
  if file_matches(&destination, file.size, &file.sha1, cancel)? {
    return Ok(());
  }
  match fs::symlink_metadata(&destination) {
    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
      return Err(Error::Conflict(
        "asset cache target is not a regular file".into(),
      ));
    }
    Ok(_) => fs::remove_file(&destination)?,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => return Err(error.into()),
  }

  check_cancel(cancel)?;
  let source = paths.cache.join(&file.cache_path);
  let staged = staging.join(format!("artifact-{staging_index}"));
  if fs::hard_link(&source, &staged).is_err() {
    fs::copy(&source, &staged)?;
  }
  if !file_matches(&staged, file.size, &file.sha1, cancel)? {
    return Err(Error::Conflict(
      "materialized asset failed integrity check".into(),
    ));
  }
  check_cancel(cancel)?;
  if let Err(rename_error) = fs::rename(&staged, &destination)
    && !file_matches(&destination, file.size, &file.sha1, cancel)?
  {
    return Err(rename_error.into());
  }
  Ok(())
}

fn safe_asset_destination(root: &Path, relative_path: &str) -> Result<PathBuf> {
  let mut parts = relative_path.split('/').peekable();
  let mut parent = root.to_path_buf();
  while let Some(part) = parts.next() {
    parent.push(part);
    if parts.peek().is_some() {
      ensure_directory(&parent, true)?;
    }
  }
  match fs::symlink_metadata(&parent) {
    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(
      Error::Conflict("asset cache target is not a regular file".into()),
    ),
    Ok(_) => Ok(parent),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(parent),
    Err(error) => Err(error.into()),
  }
}

fn file_matches(
  path: &Path,
  expected_size: u64,
  expected_sha1: &str,
  cancel: &CancellationToken,
) -> Result<bool> {
  let metadata = match fs::symlink_metadata(path) {
    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
      return Err(Error::Conflict(format!(
        "managed cache entry is not a regular file: {}",
        path.display()
      )));
    }
    Ok(metadata) => metadata,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
    Err(error) => return Err(error.into()),
  };
  if metadata.len() != expected_size {
    return Ok(false);
  }
  let mut file = fs::File::open(path)?;
  let mut digest = Sha1::new();
  let mut buffer = [0u8; 64 * 1024];
  loop {
    check_cancel(cancel)?;
    let count = file.read(&mut buffer)?;
    if count == 0 {
      break;
    }
    digest.update(&buffer[..count]);
  }
  let actual = digest
    .finalize()
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect::<String>();
  Ok(actual == expected_sha1)
}

fn client_instance_id(name: &str) -> Result<String> {
  if name.is_empty()
    || name.len() > 48
    || name.ends_with('.')
    || !name.as_bytes()[0].is_ascii_alphanumeric()
    || !name
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
  {
    return Err(Error::Invalid(
      "client name must start with a letter or digit and contain only letters, digits, '.', '_' or '-' (max 48 bytes)".into(),
    ));
  }
  Ok(format!("client-{}", name.to_ascii_lowercase()))
}

fn validate_client_file(file: &ClientFile) -> Result<()> {
  if file.size > 1024 * 1024 * 1024
    || file.url.as_str().len() > 2048
    || file.sha1.len() != 40
    || !file.sha1.bytes().all(|byte| byte.is_ascii_hexdigit())
  {
    return Err(Error::Metadata(
      "invalid client artifact integrity metadata".into(),
    ));
  }
  #[cfg(test)]
  let local_test_server = file.url.scheme() == "http"
    && file
      .url
      .host_str()
      .and_then(|host| host.parse::<std::net::IpAddr>().ok())
      .is_some_and(|ip| ip.is_loopback());
  #[cfg(not(test))]
  let local_test_server = false;
  if !(file.url.scheme() == "https" || local_test_server)
    || file.url.host_str().is_none()
    || !file.url.username().is_empty()
    || file.url.password().is_some()
    || file
      .url
      .port()
      .is_some_and(|port| port != 443 && !local_test_server)
  {
    return Err(Error::Metadata("client artifact URL must use HTTPS".into()));
  }
  Ok(())
}

fn validate_layout_path(path: &str) -> Result<()> {
  if path.is_empty()
    || path.len() > 1024
    || path.starts_with('/')
    || path.contains('\\')
    || path.contains(':')
    || path.bytes().any(|byte| byte.is_ascii_control())
    || path
      .split('/')
      .any(|part| part.is_empty() || matches!(part, "." | ".."))
  {
    return Err(Error::Metadata("unsafe client install path".into()));
  }
  Ok(())
}

fn validate_version_id(version: &str) -> Result<()> {
  if !crate::metadata::is_valid_version_id(version) {
    return Err(Error::Metadata(
      "invalid Minecraft client version ID".into(),
    ));
  }
  Ok(())
}

fn validate_metadata_identity(url: &Url, sha1: &str) -> Result<()> {
  let host = url.host_str().unwrap_or_default();
  if url.scheme() != "https"
    || url.as_str().len() > 2048
    || !(host == "mojang.com" || host.ends_with(".mojang.com"))
    || !url.username().is_empty()
    || url.password().is_some()
    || url.port().is_some_and(|port| port != 443)
    || sha1.len() != 40
    || !sha1.bytes().all(|byte| byte.is_ascii_hexdigit())
  {
    return Err(Error::Metadata(
      "invalid trusted client metadata identity".into(),
    ));
  }
  Ok(())
}

fn validate_client_instance(instance: &ClientInstance) -> Result<()> {
  let expected_id = client_instance_id(&instance.name)
    .map_err(|_| Error::Conflict("invalid client instance name in record".into()))?;
  if instance.schema_version != 1
    || instance.instance_type != "client"
    || instance.id != expected_id
  {
    return Err(Error::Conflict("invalid client instance record".into()));
  }
  Ok(())
}

pub(crate) fn validate_install_record(record: &ClientInstallRecord) -> Result<()> {
  if let Some(loader) = &record.loader {
    loader.validate()?;
  }

  validate_version_id(&record.version_id)
    .map_err(|_| Error::Conflict("invalid client version in install record".into()))?;
  if record.schema_version != 1
    || !metadata::valid_version_type(&record.version_type)
    || record.java_major < 8
    || record.java_major > 99
    || record.java_component.is_empty()
    || record.java_component.len() > 128
    || !record
      .java_component
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
    || !matches!(record.os.as_str(), "windows" | "macos" | "linux")
    || !matches!(record.architecture.as_str(), "x86_64" | "aarch64")
    || !valid_main_class(&record.main_class)
    || !metadata::valid_argument_list(&record.arguments.jvm)
    || !metadata::valid_argument_list(&record.arguments.game)
    || record.files.is_empty()
    || record.files.len() > MAX_INSTALL_FILES
  {
    return Err(Error::Conflict("invalid client install record".into()));
  }
  let metadata_url = Url::parse(&record.metadata_url)
    .map_err(|_| Error::Conflict("invalid metadata URL in client install record".into()))?;
  validate_metadata_identity(&metadata_url, &record.metadata_sha1)
    .map_err(|_| Error::Conflict("invalid metadata identity in client install record".into()))?;
  validate_version_id(&record.asset_index_id)
    .map_err(|_| Error::Conflict("invalid asset index ID in client install record".into()))?;
  let mut paths = HashMap::with_capacity(record.files.len());
  let mut estimated_size = 512usize;
  for file in &record.files {
    validate_layout_path(&file.path)
      .map_err(|_| Error::Conflict("unsafe path in client install record".into()))?;
    if file.sha1.len() != 40
      || !file.sha1.bytes().all(|byte| byte.is_ascii_hexdigit())
      || file.size > 1024 * 1024 * 1024
      || file.cache_path != format!("clients/sha1/{}", file.sha1.to_ascii_lowercase())
      || paths.insert(file.path.as_str(), ()).is_some()
    {
      return Err(Error::Conflict("invalid client file reference".into()));
    }
    estimated_size = estimated_size
      .saturating_add(file.path.len().saturating_mul(2))
      .saturating_add(file.cache_path.len())
      .saturating_add(192);
    if estimated_size > MAX_INSTALL_RECORD_SIZE {
      return Err(Error::Conflict(
        "client install record exceeds its size limit".into(),
      ));
    }
  }
  let client_jar_path = format!("versions/{0}/{0}.jar", record.version_id);
  let asset_index_path = format!("assets/indexes/{}.json", record.asset_index_id);
  if !paths.contains_key(client_jar_path.as_str()) || !paths.contains_key(asset_index_path.as_str())
  {
    return Err(Error::Conflict(
      "client install record is missing required files".into(),
    ));
  }
  Ok(())
}

fn valid_main_class(value: &str) -> bool {
  !value.is_empty()
    && value.len() <= 256
    && value.split('.').all(|part| {
      let mut bytes = part.bytes();
      bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || matches!(first, b'_' | b'$'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
    })
}

fn check_client_instance(
  context: &Context,
  id: &str,
  name: &str,
) -> Result<Option<ClientInstallRecord>> {
  let instances = context.paths.data.join("instances");
  ensure_directory(&instances, true)?;
  let directory = instances.join(id);
  match fs::symlink_metadata(&directory) {
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
    Err(error) => return Err(error.into()),
    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
      return Err(Error::Conflict(
        "client instance path is not a directory".into(),
      ));
    }
    Ok(_) => {}
  }
  let instance: ClientInstance = read_json_regular(&directory.join("instance.json"), 64 * 1024)?;
  validate_client_instance(&instance)?;
  if instance.id != id || instance.name != name {
    return Err(Error::Conflict(
      "client instance name collides with an existing instance".into(),
    ));
  }
  let game = fs::symlink_metadata(directory.join("game"))?;
  if game.file_type().is_symlink() || !game.is_dir() {
    return Err(Error::Conflict(
      "client game directory is missing or invalid".into(),
    ));
  }
  let installed: ClientInstallRecord =
    read_json_regular(&directory.join("installed.json"), MAX_INSTALL_RECORD_SIZE)?;
  validate_install_record(&installed)?;
  Ok(Some(installed))
}

fn commit_instance(
  context: &Context,
  id: &str,
  name: &str,
  installed: &ClientInstallRecord,
  existing: bool,
  cancel: &CancellationToken,
) -> Result<ClientInstance> {
  let instances = context.paths.data.join("instances");
  ensure_directory(&instances, true)?;
  let target = instances.join(id);
  let instance = ClientInstance {
    schema_version: 1,
    instance_type: "client".into(),
    id: id.into(),
    name: name.into(),
  };
  validate_client_instance(&instance)?;
  validate_install_record(installed)?;
  if existing {
    let current: ClientInstance = read_json_regular(&target.join("instance.json"), 64 * 1024)?;
    validate_client_instance(&current)?;
    if current.id != id || current.name != name {
      return Err(Error::Conflict(
        "client instance changed during install".into(),
      ));
    }
    check_cancel(cancel)?;
    storage::replace_json(&target.join("installed.json"), installed)?;
  } else {
    match fs::symlink_metadata(&target) {
      Ok(_) => return Err(Error::Conflict("client instance already exists".into())),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
      Err(error) => return Err(error.into()),
    }
    let staging = tempfile::Builder::new()
      .prefix(".create-client-")
      .tempdir_in(&instances)?;
    fs::create_dir(staging.path().join("game"))?;
    storage::write_json(&staging.path().join("instance.json"), &instance)?;
    storage::write_json(&staging.path().join("installed.json"), installed)?;
    sync_directory(&staging.path().join("game"))?;
    sync_directory(staging.path())?;
    check_cancel(cancel)?;
    fs::rename(staging.path(), &target)?;
    sync_directory(&instances)?;
  }
  Ok(instance)
}

fn validated_game_directory(data: &std::path::Path, id: &str) -> Result<std::path::PathBuf> {
  let instances = data.join("instances");
  let directory = instances.join(id);
  let game = directory.join("game");
  for path in [&instances, &directory, &game] {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
      return Err(Error::Conflict("invalid instance log directory".into()));
    }
  }
  let instance: ClientInstance = read_json_regular(&directory.join("instance.json"), 64 * 1024)?;
  validate_client_instance(&instance)?;
  if instance.id != id {
    return Err(Error::Conflict("client instance identity changed".into()));
  }
  Ok(game)
}

fn read_json_regular<T: serde::de::DeserializeOwned>(path: &Path, limit: usize) -> Result<T> {
  let metadata = fs::symlink_metadata(path)?;
  if metadata.file_type().is_symlink() || !metadata.is_file() {
    return Err(Error::Conflict(format!(
      "managed record is not a regular file: {}",
      path.display()
    )));
  }
  storage::read_json_bounded(path, limit)
}

fn read_bounded_regular(path: &Path, limit: usize) -> Result<Vec<u8>> {
  let metadata = fs::symlink_metadata(path)?;
  if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > limit as u64 {
    return Err(Error::Conflict("asset index cache file is invalid".into()));
  }
  let mut bytes = Vec::with_capacity(metadata.len() as usize);
  fs::File::open(path)?
    .take(limit.saturating_add(1) as u64)
    .read_to_end(&mut bytes)?;
  if bytes.len() > limit {
    return Err(Error::Metadata("asset index exceeds the size limit".into()));
  }
  Ok(bytes)
}

fn regular_file_state(path: &Path) -> Result<bool> {
  match fs::symlink_metadata(path) {
    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(
      Error::Conflict("client cache target is not a regular file".into()),
    ),
    Ok(_) => Ok(true),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
    Err(error) => Err(error.into()),
  }
}

fn ensure_cache_parent(path: &Path) -> Result<()> {
  let cache = path
    .parent()
    .and_then(Path::parent)
    .and_then(Path::parent)
    .ok_or_else(|| Error::Invalid("client cache path has no managed parent".into()))?;
  ensure_directory(cache, true)?;
  ensure_directory(&cache.join("clients"), true)?;
  ensure_directory(&cache.join("clients/sha1"), true)?;
  Ok(())
}

fn ensure_directory(path: &Path, create: bool) -> Result<()> {
  match fs::symlink_metadata(path) {
    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => Err(
      Error::Conflict(format!("expected a regular directory: {}", path.display())),
    ),
    Ok(_) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
      fs::create_dir_all(path)?;
      let metadata = fs::symlink_metadata(path)?;
      if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(Error::Conflict(format!(
          "expected a regular directory: {}",
          path.display()
        )));
      }
      Ok(())
    }
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(error.into()),
    Err(error) => Err(error.into()),
  }
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

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
  let mut value = OsString::from(path.as_os_str());
  value.push(suffix);
  value.into()
}

fn sync_directory(path: &Path) -> Result<()> {
  #[cfg(unix)]
  fs::File::open(path)?.sync_all()?;
  #[cfg(not(unix))]
  let _ = path;
  Ok(())
}

#[cfg(test)]
#[path = "../tests/client.rs"]
mod tests;
