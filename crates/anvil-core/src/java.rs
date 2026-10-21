use std::{
  fs,
  path::{Component, Path, PathBuf},
  process::Stdio,
  time::Duration,
};

use q2q_http::{Checksum, DownloadRequest};
use serde::{Deserialize, Serialize};
use tokio::{
  io::{AsyncRead, AsyncReadExt},
  sync::watch,
};
use tokio_util::sync::CancellationToken;

use crate::{
  Context, Error, Result, error::check_cancel, extract, platform::Platform, storage, temurin,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallStage {
  Resolving,
  Downloading,
  Extracting,
  Validating,
  Publishing,
  Completed,
}

#[derive(Clone, Debug, Serialize)]
pub struct JavaProgress {
  pub stage: InstallStage,
  pub downloaded_bytes: u64,
  pub total_bytes: Option<u64>,
}
impl Default for JavaProgress {
  fn default() -> Self {
    Self {
      stage: InstallStage::Resolving,
      downloaded_bytes: 0,
      total_bytes: None,
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstalledJava {
  pub id: String,
  pub vendor: String,
  pub version: String,
  pub major: u32,
  pub image_type: String,
  pub os: String,
  pub arch: String,
  pub java_home: PathBuf,
  pub executable: PathBuf,
}

#[derive(Serialize)]
pub struct InstallResult {
  pub runtime: InstalledJava,
  pub already_installed: bool,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
  schema_version: u32,
  id: String,
  vendor: String,
  version: String,
  major: u32,
  image_type: String,
  os: String,
  arch: String,
  archive_sha256: String,
  home: PathBuf,
}

impl Manifest {
  fn installed(&self, directory: &Path) -> Result<InstalledJava> {
    storage::valid_id(&self.id)?;
    if self.schema_version != 1
      || self.vendor != "temurin"
      || self.home.as_os_str().is_empty()
      || self
        .home
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
      return Err(Error::Metadata("invalid managed Java manifest".into()));
    }
    let java_home = directory.join(&self.home);
    let executable = java_home.join("bin").join(if self.os == "windows" {
      "java.exe"
    } else {
      "java"
    });
    Ok(InstalledJava {
      id: self.id.clone(),
      vendor: self.vendor.clone(),
      version: self.version.clone(),
      major: self.major,
      image_type: self.image_type.clone(),
      os: self.os.clone(),
      arch: self.arch.clone(),
      java_home,
      executable,
    })
  }
}

/// Operations share the Context's HTTP pool. Each runtime has an OS file lock.
#[derive(Clone)]
pub struct JavaManager {
  context: Context,
}

struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
  fn drop(&mut self) {
    self.0.cancel();
  }
}

impl JavaManager {
  pub fn new(context: Context) -> Self {
    Self { context }
  }

  /// Await to completion. Explicit cancellation drains download/extraction work.
  pub async fn install(
    &self,
    version: &str,
    cancel: &CancellationToken,
    progress: watch::Sender<JavaProgress>,
  ) -> Result<InstallResult> {
    let cancel = cancel.child_token();
    let _cancel_on_drop = CancelOnDrop(cancel.clone());
    check_cancel(&cancel)?;
    let request = temurin::VersionRequest::parse(version)?;
    let platform = Platform::current()?;
    progress.send_replace(JavaProgress::default());
    let http = self.context.http.executor();
    let resolved = tokio::select! {
        _ = cancel.cancelled() => return Err(Error::Cancelled),
        result = temurin::resolve(&http, &request, platform) => result?,
    };
    self
      .install_resolved(resolved, platform, &cancel, progress)
      .await
  }

  async fn install_resolved(
    &self,
    resolved: temurin::Resolved,
    platform: Platform,
    cancel: &CancellationToken,
    progress: watch::Sender<JavaProgress>,
  ) -> Result<InstallResult> {
    let id = format!(
      "temurin-{}-{}-{}-{}",
      resolved.version, resolved.image, platform.os, platform.arch
    );
    storage::valid_id(&id)?;
    let context = self.context.clone();
    let lock_id = id.clone();
    let (lock, existing) = self
      .blocking(cancel, move || {
        let lock = storage::lock(&context.paths.data, &lock_id)?;
        let target = context.paths.data.join("runtimes").join(&lock_id);
        let existing = if target.try_exists()? {
          if !fs::symlink_metadata(&target)?.is_dir() {
            return Err(Error::Conflict("runtime root is not a directory".into()));
          }
          Some(storage::read_json::<Manifest>(
            &target.join("anvil-runtime.json"),
          )?)
        } else {
          None
        };
        Ok((lock, existing))
      })
      .await?;
    let directory = self.context.paths.data.join("runtimes").join(&id);
    if let Some(manifest) = existing {
      if manifest.id != id
        || manifest.archive_sha256 != resolved.checksum
        || manifest.version != resolved.version
        || manifest.os != platform.os
        || manifest.arch != platform.arch
        || manifest.image_type != resolved.image
      {
        return Err(Error::Conflict(format!(
          "existing installation differs: {id}"
        )));
      }
      let runtime = manifest.installed(&directory)?;
      set_stage(&progress, InstallStage::Validating);
      validate(
        &runtime.executable,
        &resolved.version,
        platform.arch,
        cancel,
      )
      .await?;
      set_stage(&progress, InstallStage::Completed);
      return Ok(InstallResult {
        runtime,
        already_installed: true,
      });
    }
    let archive = self.context.paths.cache.join("java").join(format!(
      "{}.{}",
      resolved.checksum,
      if resolved.zip { "zip" } else { "tar.gz" }
    ));
    let parent = archive
      .parent()
      .ok_or_else(|| Error::Invalid("Java archive cache path has no parent".into()))?
      .to_path_buf();
    let cached = archive.clone();
    let resume = self
      .blocking(cancel, move || {
        fs::create_dir_all(parent)?;
        let mut journal = cached.as_os_str().to_owned();
        journal.push(".q2q-http.resume");
        let mut part = cached.as_os_str().to_owned();
        part.push(".q2q-http.part");
        Ok(
          cached.try_exists()?
            || Path::new(&journal).try_exists()?
            || Path::new(&part).try_exists()?,
        )
      })
      .await?;
    check_cancel(cancel)?;
    set_stage(&progress, InstallStage::Downloading);
    progress.send_modify(|p| p.total_bytes = Some(resolved.size));
    let download = DownloadRequest::new(&archive)
      .mirrors([resolved.url.as_str()])
      .expected_size(resolved.size)
      .checksum(Checksum::sha256(&resolved.checksum)?);
    let task = tokio::select! {
        _ = cancel.cancelled() => return Err(Error::Cancelled),
        task = async {
          if resume { self.context.http.resume(download).await } else { self.context.http.download(download).await }
        } => task?,
    };
    let mut updates = task.subscribe_progress();
    // Keep Task available for cancellation while its progress reaches a terminal state.
    loop {
      let state = *updates.borrow_and_update();
      progress.send_modify(|p| p.downloaded_bytes = state.staged_bytes);
      if state.state.is_terminal() {
        break;
      }
      tokio::select! {
          _ = cancel.cancelled() => { task.cancel(); break; }
          result = updates.changed() => { if result.is_err() { break; } }
      }
    }
    task.wait().await?;
    check_cancel(cancel)?;
    set_stage(&progress, InstallStage::Extracting);
    let runtimes = self.context.paths.data.join("runtimes");
    let unpack_cancel = cancel.clone();
    let unpack_lock = lock.clone();
    let zip = resolved.zip;
    let (staging, home) = self
      .blocking(cancel, move || {
        let _lock = unpack_lock;
        fs::create_dir_all(&runtimes)?;
        let staging = tempfile::Builder::new()
          .prefix(".install-")
          .tempdir_in(&runtimes)?;
        extract::unpack(&archive, staging.path(), zip, &unpack_cancel)?;
        let home = extract::java_home(staging.path(), platform.os == "mac")?;
        Ok((staging, home))
      })
      .await?;
    set_stage(&progress, InstallStage::Validating);
    let executable = staging
      .path()
      .join(&home)
      .join("bin")
      .join(if cfg!(windows) { "java.exe" } else { "java" });
    if let Err(error) = validate(&executable, &resolved.version, platform.arch, cancel).await {
      let cleanup_lock = lock.clone();
      self
        .blocking(&CancellationToken::new(), move || {
          let _lock = cleanup_lock;
          staging.close()?;
          Ok(())
        })
        .await?;
      return Err(error);
    }
    let manifest = Manifest {
      schema_version: 1,
      id,
      vendor: "temurin".into(),
      version: resolved.version,
      major: resolved.major,
      image_type: resolved.image,
      os: platform.os.into(),
      arch: platform.arch.into(),
      archive_sha256: resolved.checksum,
      home,
    };
    set_stage(&progress, InstallStage::Publishing);
    let publish_cancel = cancel.clone();
    let runtime = self
      .blocking(&CancellationToken::new(), move || {
        let _lock = lock;
        check_cancel(&publish_cancel)?;
        storage::write_json(&staging.path().join("anvil-runtime.json"), &manifest)?;
        if directory.try_exists()? {
          return Err(Error::Conflict(manifest.id.clone()));
        }
        let runtime = manifest.installed(staging.path())?;
        // The stable runtime lock serializes cooperating Anvil processes.
        fs::rename(staging.path(), &directory)?;
        #[cfg(unix)]
        {
          let parent = directory
            .parent()
            .ok_or_else(|| Error::Invalid("Java runtime path has no parent".into()))?;
          fs::File::open(parent)?.sync_all()?;
        }
        let executable_relative = runtime
          .executable
          .strip_prefix(staging.path())
          .map_err(|_| Error::Java("Java executable escaped staging directory".into()))?;
        Ok(InstalledJava {
          java_home: directory.join(&manifest.home),
          executable: directory.join(executable_relative),
          ..runtime
        })
      })
      .await?;
    set_stage(&progress, InstallStage::Completed);
    Ok(InstallResult {
      runtime,
      already_installed: false,
    })
  }

  /// Reads managed manifests only; does not scan disks or execute every installed JVM.
  pub async fn list(&self) -> Result<Vec<InstalledJava>> {
    let runtimes = self.read_managed_runtimes().await?;
    for runtime in &runtimes {
      if !runtime.executable.is_file() {
        return Err(Error::Java(format!(
          "managed executable missing: {}",
          runtime.id
        )));
      }
    }
    Ok(runtimes)
  }

  async fn read_managed_runtimes(&self) -> Result<Vec<InstalledJava>> {
    let root = self.context.paths.data.join("runtimes");
    self
      .blocking(&CancellationToken::new(), move || {
        if !root.try_exists()? {
          return Ok(Vec::new());
        }
        let mut result = Vec::new();
        for entry in fs::read_dir(root)? {
          let entry = entry?;
          let id = entry.file_name().to_string_lossy().into_owned();
          if id.starts_with('.') {
            continue;
          }
          storage::valid_id(&id)?;
          if !entry.file_type()?.is_dir() {
            return Err(Error::Conflict(format!("invalid runtime directory: {id}")));
          }
          let manifest: Manifest = storage::read_json(&entry.path().join("anvil-runtime.json"))?;
          if manifest.id != id {
            return Err(Error::Conflict(format!(
              "runtime manifest ID mismatch: {id}"
            )));
          }
          let runtime = manifest.installed(&entry.path())?;
          result.push(runtime);
        }
        result.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(result)
      })
      .await
  }

  /// Reuse a managed runtime for the current platform when its executable passes
  /// the same version and architecture probe used for newly installed runtimes;
  /// otherwise install the requested Java major through Temurin.
  pub async fn ensure_major(
    &self,
    major: u32,
    cancel: &CancellationToken,
    progress: watch::Sender<JavaProgress>,
  ) -> Result<InstallResult> {
    if !(8..=99).contains(&major) {
      return Err(Error::Invalid(
        "Java major version must be between 8 and 99".into(),
      ));
    }
    check_cancel(cancel)?;
    let platform = Platform::current()?;
    for runtime in self.read_managed_runtimes().await? {
      if runtime.major != major || runtime.os != platform.os || runtime.arch != platform.arch {
        continue;
      }
      if !runtime.executable.is_file() {
        continue;
      }
      set_stage(&progress, InstallStage::Validating);
      match validate(&runtime.executable, &runtime.version, platform.arch, cancel).await {
        Ok(()) => {
          set_stage(&progress, InstallStage::Completed);
          return Ok(InstallResult {
            runtime,
            already_installed: true,
          });
        }
        Err(Error::Cancelled) => return Err(Error::Cancelled),
        Err(_) => {}
      }
    }
    let result = self.install(&major.to_string(), cancel, progress).await?;
    if result.runtime.major != major
      || result.runtime.os != platform.os
      || result.runtime.arch != platform.arch
    {
      return Err(Error::Java(
        "installed Java does not match the requested major version and platform".into(),
      ));
    }
    Ok(result)
  }

  /// Remove a managed runtime; cached archives remain available for reinstallation.
  pub async fn uninstall(&self, id: &str) -> Result<InstalledJava> {
    storage::valid_id(id)?;
    let id = id.to_owned();
    let context = self.context.clone();
    self
      .blocking(&CancellationToken::new(), move || {
        let _lock = storage::lock(&context.paths.data, &id)?;
        let target = context.paths.data.join("runtimes").join(&id);
        if !target.try_exists()? {
          return Err(Error::NotFound(id));
        }
        if !fs::symlink_metadata(&target)?.is_dir() {
          return Err(Error::Conflict("runtime root is not a directory".into()));
        }
        let manifest: Manifest = storage::read_json(&target.join("anvil-runtime.json"))?;
        if manifest.id != id {
          return Err(Error::Conflict("runtime manifest ID mismatch".into()));
        }
        let runtime = manifest.installed(&target)?;
        let trash = tempfile::Builder::new().prefix(".remove-").tempdir_in(
          target
            .parent()
            .ok_or_else(|| Error::Invalid("Java runtime path has no parent".into()))?,
        )?;
        fs::rename(&target, trash.path().join("runtime"))?;
        trash.close()?;
        Ok(runtime)
      })
      .await
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

fn set_stage(progress: &watch::Sender<JavaProgress>, stage: InstallStage) {
  progress.send_modify(|p| p.stage = stage);
}

async fn read_output(mut pipe: impl AsyncRead + Unpin) -> Result<Vec<u8>> {
  let mut data = Vec::new();
  (&mut pipe)
    .take(64 * 1024 + 1)
    .read_to_end(&mut data)
    .await?;
  if data.len() > 64 * 1024 {
    return Err(Error::Java("Java probe output exceeds limit".into()));
  }
  Ok(data)
}

async fn validate(
  executable: &Path,
  expected: &str,
  arch: &str,
  cancel: &CancellationToken,
) -> Result<()> {
  check_cancel(cancel)?;
  let mut command = tokio::process::Command::new(executable);
  command
    .args(["-XshowSettings:properties", "-version"])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .env_remove("JAVA_TOOL_OPTIONS")
    .env_remove("_JAVA_OPTIONS")
    .env_remove("JDK_JAVA_OPTIONS")
    .kill_on_drop(true);
  #[cfg(windows)]
  command.creation_flags(0x08000000); // CREATE_NO_WINDOW
  let mut child = command
    .spawn()
    .map_err(|e| Error::Java(format!("cannot execute Java: {e}")))?;
  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| Error::Java("Java probe stdout pipe is unavailable".into()))?;
  let stderr = child
    .stderr
    .take()
    .ok_or_else(|| Error::Java("Java probe stderr pipe is unavailable".into()))?;
  let result = {
    let capture = async {
      let (stdout, stderr, status) =
        tokio::try_join!(read_output(stdout), read_output(stderr), async {
          child.wait().await.map_err(Error::Io)
        })?;
      Ok::<_, Error>((stdout, stderr, status))
    };
    tokio::select! {
        _ = cancel.cancelled() => Err(Error::Cancelled),
        result = tokio::time::timeout(Duration::from_secs(15), capture) => {
            result.unwrap_or_else(|_| Err(Error::Java("Java probe timed out".into())))
        }
    }
  };
  let (stdout, stderr, status) = match result {
    Ok(output) => output,
    Err(error) => {
      let _ = child.kill().await;
      let _ = child.wait().await;
      return Err(error);
    }
  };
  if !status.success() {
    return Err(Error::Java(format!("Java probe exited with {status}")));
  }
  let text = format!(
    "{}\n{}",
    String::from_utf8_lossy(&stdout),
    String::from_utf8_lossy(&stderr)
  );
  let property = |name: &str| {
    text
      .lines()
      .filter_map(|line| line.trim().split_once(" = "))
      .find_map(|(key, value)| (key == name).then_some(value))
  };
  let actual = property("java.runtime.version").unwrap_or("");
  let actual = actual.strip_suffix("-LTS").unwrap_or(actual);
  let actual_arch = match property("os.arch") {
    Some("amd64" | "x86_64") => "x64",
    Some("aarch64" | "arm64") => "aarch64",
    _ => "unknown",
  };
  if actual != expected || actual_arch != arch {
    return Err(Error::Java(
      "Java runtime version or architecture does not match package metadata".into(),
    ));
  }
  Ok(())
}

#[cfg(test)]
#[path = "../tests/java.rs"]
mod tests;
