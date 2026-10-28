//! New-instance staging; no published instance is mutated by an import.
use super::{ArchivePlan, inspect_archive, store};
use crate::{
  CancellationToken, Context, Error, Result,
  client::{ClientInstallResult, ClientInstaller, ClientInstance, ClientProgress},
  client_plan::ClientTarget,
  storage,
};
use serde::Serialize;
use std::{
  collections::BTreeSet,
  fs::{self, File},
  io::{Read, Write},
  path::{Path, PathBuf},
  sync::Arc,
};
use tokio::sync::watch;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStage {
  Parsing,
  Preparing,
  Java,
  InstallingGame,
  Downloading,
  Applying,
  Publishing,
  Completed,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
  pub stage: ImportStage,
  pub name: Option<String>,
  pub version_id: Option<String>,
  pub completed_files: usize,
  pub total_files: Option<usize>,
  pub downloaded_bytes: u64,
  pub total_bytes: Option<u64>,
  pub received_bytes: u64,
}
impl Default for ImportProgress {
  fn default() -> Self {
    Self {
      stage: ImportStage::Parsing,
      name: None,
      version_id: None,
      completed_files: 0,
      total_files: None,
      downloaded_bytes: 0,
      total_bytes: None,
      received_bytes: 0,
    }
  }
}

pub(super) async fn blocking<T: Send + 'static>(
  context: &Context,
  cancel: &CancellationToken,
  work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
  let permit = tokio::select! { _=cancel.cancelled()=>return Err(Error::Cancelled), permit=context.blocking.clone().acquire_owned()=>permit.map_err(|_| Error::Cancelled)?, };
  tokio::task::spawn_blocking(move || {
    let _permit = permit;
    work()
  })
  .await?
}
fn check(cancel: &CancellationToken) -> Result<()> {
  if cancel.is_cancelled() {
    Err(Error::Cancelled)
  } else {
    Ok(())
  }
}
fn copy(source: &Path, destination: &Path, cancel: &CancellationToken, limit: u64) -> Result<()> {
  let meta = fs::symlink_metadata(source)?;
  if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > limit {
    return Err(Error::Invalid("invalid import source file".into()));
  }
  let mut source = File::open(source)?;
  let mut output = fs::OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(destination)?;
  let mut buffer = [0u8; 65_536];
  let mut total = 0u64;
  loop {
    check(cancel)?;
    let count = source.read(&mut buffer)?;
    if count == 0 {
      break;
    }
    total += count as u64;
    if total > limit {
      return Err(Error::Invalid("source grew past import limit".into()));
    }
    output.write_all(&buffer[..count])?;
  }
  output.sync_all()?;
  Ok(())
}
struct Staging {
  root: tempfile::TempDir,
  instance: ClientInstance,
  _reservation: Arc<File>,
  archive: PathBuf,
}
fn reserve(data: &Path, root: tempfile::TempDir, plan: &ArchivePlan) -> Result<Staging> {
  let mut base = plan
    .index
    .name
    .trim()
    .chars()
    .map(|c| {
      if c.is_alphanumeric() || "._-".contains(c) {
        c
      } else {
        '-'
      }
    })
    .collect::<String>();
  while base.len() > if base.is_ascii() { 36 } else { 96 } {
    base.pop();
  }
  if !base.chars().next().is_some_and(char::is_alphanumeric) {
    base.insert_str(0, "modpack-");
  }
  while base.len() > if base.is_ascii() { 36 } else { 96 } {
    base.pop();
  }
  while base.ends_with('.') {
    base.pop();
  }
  for number in 1..=10_000 {
    let name = if number == 1 {
      base.clone()
    } else {
      format!("{base}-{number}")
    };
    let id = crate::client::client_instance_id(&name)?;
    let lock = match storage::server_instance_lock(data, &id) {
      Ok(lock) => lock,
      Err(Error::Busy(_)) => continue,
      Err(error) => return Err(error),
    };
    match fs::symlink_metadata(data.join("instances").join(&id)) {
      Ok(_) => continue,
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
      Err(e) => return Err(e.into()),
    }
    let archive = root.path().join("source.mrpack");
    return Ok(Staging {
      root,
      instance: ClientInstance {
        schema_version: 1,
        instance_type: "client".into(),
        id,
        name,
      },
      _reservation: lock,
      archive,
    });
  }
  Err(Error::Busy("no free modpack instance name".into()))
}
fn apply_archive(
  staging: &Path,
  archive_path: &Path,
  expected: &str,
  cancel: &CancellationToken,
) -> Result<(ArchivePlan, BTreeSet<String>)> {
  let plan = inspect_archive(File::open(archive_path)?, cancel)?;
  if plan.archive_sha256 != expected {
    return Err(Error::Conflict("import package changed".into()));
  }
  let mut archive =
    zip::ZipArchive::new(File::open(archive_path)?).map_err(|e| Error::Archive(e.to_string()))?;
  let game = staging.join("game");
  let mut paths = plan
    .index
    .files
    .iter()
    .map(|f| f.path.clone())
    .collect::<BTreeSet<_>>();
  for entry in &plan.overrides {
    check(cancel)?;
    let target = game.join(&entry.path);
    fs::create_dir_all(target.parent().unwrap())?;
    // Every path is validated and this game directory belongs only to this operation.
    if target.try_exists()? {
      if !plan.index.files.iter().any(|file| file.path == entry.path) {
        return Err(Error::Conflict(
          "override collides with another normalized filesystem path".into(),
        ));
      }
      fs::remove_file(&target)?;
    }
    let mut output = fs::OpenOptions::new()
      .write(true)
      .create_new(true)
      .open(target)?;
    let mut source = archive
      .by_index(entry.entry_index)
      .map_err(|e| Error::Archive(e.to_string()))?;
    let mut buffer = [0u8; 65_536];
    let mut count = 0u64;
    loop {
      check(cancel)?;
      let read = source.read(&mut buffer)?;
      if read == 0 {
        break;
      }
      count += read as u64;
      if count > entry.size {
        return Err(Error::Archive("override grew".into()));
      }
      output.write_all(&buffer[..read])?;
    }
    if count != entry.size {
      return Err(Error::Archive("truncated override".into()));
    }
    output.sync_all()?;
    paths.insert(entry.path.clone());
  }
  Ok((plan, paths))
}

#[derive(Clone)]
pub struct ModpackInstaller {
  context: Context,
}
impl ModpackInstaller {
  pub fn new(context: Context) -> Self {
    Self { context }
  }
  /// Import a local archive as a new instance; cancellation drains owned I/O.
  pub async fn install(
    &self,
    archive: PathBuf,
    target: ClientTarget,
    cancel: &CancellationToken,
    progress: &watch::Sender<ImportProgress>,
  ) -> Result<ClientInstallResult> {
    progress.send_replace(ImportProgress::default());
    check(cancel)?;
    let context = self.context.clone();
    let token = cancel.clone();
    let (root, plan) = blocking(&self.context, cancel, move || {
      fs::create_dir_all(&context.paths.data)?;
      store::directory(&context.paths.data)?;
      let instances = context.paths.data.join("instances");
      store::directory(&instances)?;
      let root = tempfile::Builder::new()
        .prefix(".import-modpack-")
        .tempdir_in(instances)?;
      let destination = root.path().join("source.mrpack");
      copy(&archive, &destination, &token, 512 * 1024 * 1024)?;
      let plan = inspect_archive(File::open(destination)?, &token)?;
      validate_resources(&plan)?;
      Ok((root, plan))
    })
    .await?;
    let data = self.context.paths.data.clone();
    let staging = reserve(&data, root, &plan)?;
    let loader =
      plan
        .index
        .fabric_loader
        .as_ref()
        .map(|version| crate::fabric::ClientLoader::Fabric {
          version: version.clone(),
        });
    progress.send_modify(|p| {
      p.stage = ImportStage::Preparing;
      p.name = Some(staging.instance.name.clone());
      p.version_id = Some(plan.index.minecraft.clone());
    });
    let installer = ClientInstaller::new(self.context.clone());
    let (game_tx, mut game_rx) = watch::channel(ClientProgress::default());
    let game_install = async {
      let game = installer
        .plan_with_loader(
          &plan.index.minecraft,
          target,
          loader.as_ref(),
          cancel,
          &game_tx,
        )
        .await?;
      installer
        .prepare_install_java(&game, cancel, &game_tx)
        .await?;
      installer.prepare_unpublished(&game, cancel, &game_tx).await
    };
    tokio::pin!(game_install);
    let (installed, mut downloaded_files, mut reused_files) = loop {
      tokio::select! {
        result=&mut game_install=>break result?,
        changed=game_rx.changed()=>{ if changed.is_ok() { let game=game_rx.borrow_and_update().clone();progress.send_modify(|p| {p.stage=if matches!(game.stage,crate::client::ClientInstallStage::Java) {ImportStage::Java} else {ImportStage::InstallingGame};p.completed_files=game.completed_files;p.total_files=game.total_files;p.downloaded_bytes=game.downloaded_bytes;p.total_bytes=game.total_bytes;p.received_bytes=game.received_bytes;}); } }
      }
    };
    let game = staging.root.path().join("game");
    fs::create_dir(&game)?;
    progress.send_modify(|p| {
      p.stage = ImportStage::Downloading;
      p.total_files = Some(plan.index.files.len());
      p.completed_files = 0;
      p.downloaded_bytes = 0;
      p.total_bytes = Some(plan.index.total_bytes);
    });
    for file in &plan.index.files {
      check(cancel)?;
      let (object, reused) = store::prepare(&self.context, file, cancel, progress).await?;
      if reused {
        reused_files += 1;
      } else {
        downloaded_files += 1;
      }
      let destination = game.join(&file.path);
      let token = cancel.clone();
      let size = file.file_size;
      blocking(&self.context, cancel, move || {
        fs::create_dir_all(destination.parent().unwrap())?;
        copy(&object, &destination, &token, size)
      })
      .await?;
      progress.send_modify(|p| {
        p.completed_files += 1;
        p.downloaded_bytes = plan
          .index
          .files
          .iter()
          .take(p.completed_files)
          .map(|f| f.file_size)
          .sum();
      });
    }
    progress.send_modify(|p| p.stage = ImportStage::Applying);
    let token = cancel.clone();
    let expected = plan.archive_sha256.clone();
    let data = self.context.paths.data.clone();
    let result_instance = staging.instance.clone();
    let committed = installed.clone();
    let publish_progress = progress.clone();
    blocking(&self.context, cancel, move || {
      publish_staged(
        staging,
        &data,
        &expected,
        &committed,
        &token,
        &publish_progress,
      )
    })
    .await?;
    progress.send_modify(|p| p.stage = ImportStage::Completed);
    Ok(ClientInstallResult {
      instance: result_instance,
      installed,
      already_installed: false,
      downloaded_files,
      reused_files,
    })
  }
}

fn publish_staged(
  staging: Staging,
  data: &Path,
  expected: &str,
  installed: &crate::client::ClientInstallRecord,
  cancel: &CancellationToken,
  progress: &watch::Sender<ImportProgress>,
) -> Result<()> {
  crate::client::validate_install_record(installed)?;
  let (plan, paths) = apply_archive(staging.root.path(), &staging.archive, expected, cancel)?;
  crate::resources::prepare_modpack(
    staging.root.path(),
    data,
    &paths,
    installed.loader.is_some(),
    cancel,
  )?;
  storage::write_json(
    &staging.root.path().join("instance.json"),
    &staging.instance,
  )?;
  storage::write_json(&staging.root.path().join("installed.json"), installed)?;
  storage::write_json(
    &staging.root.path().join("modpack.json"),
    &serde_json::json!({"schemaVersion":1,"format":"modrinth","name":plan.index.name,"versionId":plan.index.version_id,"archiveSha256":expected}),
  )?;
  fs::remove_file(&staging.archive)?;
  progress.send_modify(|p| p.stage = ImportStage::Publishing);
  check(cancel)?;
  let destination = data.join("instances").join(&staging.instance.id);
  match fs::symlink_metadata(&destination) {
    Ok(_) => return Err(Error::Conflict("reserved instance name was taken".into())),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
    Err(e) => return Err(e.into()),
  }
  sync_tree(staging.root.path(), cancel)?;
  check(cancel)?;
  fs::rename(staging.root.path(), destination)?;
  #[cfg(unix)]
  File::open(data.join("instances"))?.sync_all()?;
  Ok(())
}

pub(super) fn validate_resources(plan: &ArchivePlan) -> Result<()> {
  let mut paths = std::collections::BTreeMap::new();
  for file in &plan.index.files {
    paths.insert(file.path.as_str(), file.file_size);
  }
  for file in &plan.overrides {
    paths.insert(file.path.as_str(), file.size);
  }
  let mut resources = 0;
  for (path, size) in paths {
    if let Some((kind, filename, extension)) = crate::resources::modpack_resource(path) {
      crate::resources::validate_resource_filename(filename, extension)?;
      if size > crate::resources::MAX_IMPORT_BYTES as u64 {
        return Err(Error::Invalid(
          "整合包中单个模组、资源包或光影超过 64 MiB".into(),
        ));
      }
      if kind == "mods" && plan.index.fabric_loader.is_none() {
        return Err(Error::Invalid(
          "整合包包含模组，但未声明受支持的加载器".into(),
        ));
      }
      resources += 1;
    }
  }
  if resources > crate::resources::MAX_ENTRIES {
    return Err(Error::Invalid("整合包中受管资源超过 1024 个".into()));
  }
  Ok(())
}
fn sync_tree(root: &Path, cancel: &CancellationToken) -> Result<()> {
  let mut pending = vec![root.to_owned()];
  let mut directories = Vec::new();
  let mut entries = 0;
  while let Some(path) = pending.pop() {
    check(cancel)?;
    entries += 1;
    if entries > 100_000 {
      return Err(Error::Invalid("整合包实例文件数量超出限制".into()));
    }
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !(metadata.is_dir() || metadata.is_file()) {
      return Err(Error::Invalid("实例暂存目录出现链接或特殊文件".into()));
    }
    if metadata.is_dir() {
      for child in fs::read_dir(&path)? {
        pending.push(child?.path());
      }
      directories.push(path);
    }
  }
  #[cfg(unix)]
  for directory in directories.into_iter().rev() {
    check(cancel)?;
    File::open(directory)?.sync_all()?;
  }
  Ok(())
}

#[cfg(test)]
#[path = "../../tests/modpack_install.rs"]
mod tests;
