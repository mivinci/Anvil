//! Read-only instance snapshots and no-clobber, bounded self-contained ZIP export.
use super::{
  ExportOptions, ExportSelection, archive, inspect_archive, prepare_export_index, validate_path,
};
use crate::{CancellationToken, Context, Error, Result, storage};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
  collections::BTreeMap,
  fs::{self, File, Metadata},
  io::{Read, Seek, SeekFrom, Write},
  path::{Path, PathBuf},
  time::SystemTime,
};
use tokio::sync::watch;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportStage {
  Collecting,
  Packing,
  Verifying,
  Publishing,
  Completed,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
  pub stage: ExportStage,
  pub completed_files: usize,
  pub total_files: Option<usize>,
  pub processed_bytes: u64,
  pub total_bytes: Option<u64>,
}
impl Default for ExportProgress {
  fn default() -> Self {
    Self {
      stage: ExportStage::Collecting,
      completed_files: 0,
      total_files: None,
      processed_bytes: 0,
      total_bytes: None,
    }
  }
}
#[derive(Debug)]
pub struct ExportResult {
  pub path: PathBuf,
  pub size: u64,
  pub sha256: String,
  pub files: usize,
}

fn check(cancel: &CancellationToken) -> Result<()> {
  if cancel.is_cancelled() {
    Err(Error::Cancelled)
  } else {
    Ok(())
  }
}
fn conflict() -> Error {
  Error::Conflict("实例文件在导出期间发生变化，请停止修改后重试".into())
}
fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn linked(meta: &Metadata) -> bool {
  if meta.file_type().is_symlink() {
    return true;
  }
  #[cfg(windows)]
  {
    use std::os::windows::fs::MetadataExt;
    if meta.file_attributes() & 0x400 != 0 {
      return true;
    }
  }
  false
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Fingerprint {
  size: u64,
  modified: SystemTime,
  created: Option<SystemTime>,
  directory: bool,
  #[cfg(unix)]
  dev: u64,
  #[cfg(unix)]
  ino: u64,
}
impl Fingerprint {
  fn from(meta: &Metadata) -> Result<Self> {
    if linked(meta) || !(meta.is_file() || meta.is_dir()) {
      return Err(Error::Invalid("导出内容包含链接或特殊文件".into()));
    }
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Ok(Self {
      size: meta.len(),
      modified: meta.modified()?,
      created: meta.created().ok(),
      directory: meta.is_dir(),
      #[cfg(unix)]
      dev: meta.dev(),
      #[cfg(unix)]
      ino: meta.ino(),
    })
  }
}
#[derive(Debug, PartialEq, Eq)]
struct Source {
  fingerprint: Fingerprint,
  sha256: Option<String>,
}
fn hash_file(path: &Path, expected: &Fingerprint, cancel: &CancellationToken) -> Result<String> {
  let mut file = File::open(path)?;
  if &Fingerprint::from(&file.metadata()?)? != expected {
    return Err(conflict());
  }
  let mut hash = Sha256::new();
  let mut count = 0u64;
  let mut buffer = [0u8; 65_536];
  loop {
    check(cancel)?;
    let read = file.read(&mut buffer)?;
    if read == 0 {
      break;
    }
    count += read as u64;
    if count > expected.size {
      return Err(conflict());
    }
    hash.update(&buffer[..read]);
  }
  if count != expected.size
    || &Fingerprint::from(&file.metadata()?)? != expected
    || &Fingerprint::from(&fs::symlink_metadata(path)?)? != expected
  {
    return Err(conflict());
  }
  Ok(hex(&hash.finalize()))
}
fn private_filename(path: &str) -> bool {
  matches!(
    path
      .rsplit('/')
      .next()
      .unwrap_or("")
      .to_ascii_lowercase()
      .as_str(),
    "accounts.json"
      | "launcher_accounts.json"
      | "launcher_profiles.json"
      | "usercache.json"
      | "usernamecache.json"
      | "credentials.json"
      | "tokens.json"
      | "auth.json"
  )
}
fn selected_roots(selection: &ExportSelection) -> Vec<&'static str> {
  let mut roots = Vec::new();
  if selection.mods {
    roots.push("mods");
  }
  if selection.resource_packs {
    roots.push("resourcepacks");
  }
  if selection.shader_packs {
    roots.push("shaderpacks");
  }
  if selection.config {
    roots.extend([
      "config",
      "defaultconfigs",
      "kubejs",
      "scripts",
      "options.txt",
      "optionsof.txt",
      "optionsshaders.txt",
    ]);
  }
  if selection.worlds {
    roots.push("saves");
  }
  roots
}
fn collect(
  game: &Path,
  selection: &ExportSelection,
  cancel: &CancellationToken,
) -> Result<BTreeMap<String, Source>> {
  Fingerprint::from(&fs::symlink_metadata(game)?)?;
  let mut pending = selected_roots(selection)
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
  let mut snapshot = BTreeMap::new();
  let mut paths = BTreeMap::new();
  let mut total = 0u64;
  let mut count = 0usize;
  let mut visited = 0usize;
  while let Some(relative) = pending.pop() {
    check(cancel)?;
    visited += 1;
    if visited > archive::MAX_ENTRIES {
      return Err(Error::Invalid("导出目录条目过多".into()));
    }
    if private_filename(&relative) {
      continue;
    }
    validate_path(&format!("overrides/{relative}"))?;
    let path = game.join(&relative);
    let meta = match fs::symlink_metadata(&path) {
      Ok(meta) => meta,
      Err(e) if e.kind() == std::io::ErrorKind::NotFound && !relative.contains('/') => continue,
      Err(e) => return Err(e.into()),
    };
    let fingerprint = Fingerprint::from(&meta)?;
    if meta.is_dir()
      && matches!(
        relative
          .rsplit('/')
          .next()
          .unwrap_or("")
          .to_ascii_lowercase()
          .as_str(),
        "cache"
          | "caches"
          | ".cache"
          | "webcache"
          | "logs"
          | "crash-reports"
          | "screenshots"
          | ".git"
          | ".svn"
      )
    {
      continue;
    }
    archive::insert_path(&mut paths, &relative, meta.is_dir())?;
    let sha256 = if meta.is_dir() {
      for child in fs::read_dir(&path)? {
        let child = child?;
        let name = child
          .file_name()
          .into_string()
          .map_err(|_| Error::Invalid("文件名不是 UTF-8".into()))?;
        pending.push(format!("{relative}/{name}"));
        if pending.len() > archive::MAX_ENTRIES {
          return Err(Error::Invalid("导出目录条目过多".into()));
        }
      }
      None
    } else {
      count += 1;
      if count >= archive::MAX_ENTRIES || meta.len() > archive::MAX_ENTRY_BYTES {
        return Err(Error::Invalid(
          "导出文件数量或单文件大小超出导入限制".into(),
        ));
      }
      total = total
        .checked_add(meta.len())
        .filter(|size| *size <= archive::MAX_UNPACKED_BYTES)
        .ok_or_else(|| Error::Invalid("导出总量超出导入限制".into()))?;
      if crate::resources::modpack_resource(&relative).is_some()
        && meta.len() > crate::resources::MAX_IMPORT_BYTES as u64
      {
        return Err(Error::Invalid("单个模组、资源包或光影超过 64 MiB".into()));
      }
      let hash = hash_file(&path, &fingerprint, cancel)?;
      if crate::resources::modpack_resource(&relative).is_some() {
        let mut input = File::open(&path)?;
        if Fingerprint::from(&input.metadata()?)? != fingerprint {
          return Err(conflict());
        }
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 65_536];
        loop {
          check(cancel)?;
          let count = input.read(&mut buffer)?;
          if count == 0 {
            break;
          }
          if bytes.len() + count > crate::resources::MAX_IMPORT_BYTES {
            return Err(conflict());
          }
          bytes.extend_from_slice(&buffer[..count]);
        }
        if Fingerprint::from(&input.metadata()?)? != fingerprint
          || hex(&Sha256::digest(&bytes)) != hash
        {
          return Err(conflict());
        }
        crate::resources::validate_export_payload(&relative, &bytes)?;
      }
      Some(hash)
    };
    snapshot.insert(
      relative,
      Source {
        fingerprint,
        sha256,
      },
    );
  }
  archive::check_tree(&paths)?;
  Ok(snapshot)
}
fn metadata_snapshot(directory: &Path) -> Result<BTreeMap<String, Option<Fingerprint>>> {
  [
    "instance.json",
    "installed.json",
    "resources.json",
    "resource-layout.json",
    "resource-transaction.json",
    "launch-settings.json",
    "modpack.json",
  ]
  .into_iter()
  .map(|name| {
    let fingerprint = match fs::symlink_metadata(directory.join(name)) {
      Ok(meta) => Some(Fingerprint::from(&meta)?),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
      Err(e) => return Err(e.into()),
    };
    Ok((name.to_owned(), fingerprint))
  })
  .collect()
}

struct LimitedFile<'a> {
  file: &'a mut File,
  position: u64,
}
impl Write for LimitedFile<'_> {
  fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
    if bytes.len() as u64 > archive::MAX_ARCHIVE_BYTES.saturating_sub(self.position) {
      return Err(std::io::Error::other("导出压缩包超过 512 MiB"));
    }
    let written = self.file.write(bytes)?;
    self.position += written as u64;
    Ok(written)
  }
  fn flush(&mut self) -> std::io::Result<()> {
    self.file.flush()
  }
}
impl Seek for LimitedFile<'_> {
  fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
    let old = self.position;
    let new = self.file.seek(position)?;
    if new > archive::MAX_ARCHIVE_BYTES {
      self.file.seek(SeekFrom::Start(old))?;
      return Err(std::io::Error::other("export ZIP seek exceeds limit"));
    }
    self.position = new;
    Ok(new)
  }
}
fn zip_error(error: zip::result::ZipError) -> Error {
  Error::Archive(error.to_string())
}
fn pack(
  file: &mut File,
  index: &[u8],
  game: &Path,
  snapshot: &BTreeMap<String, Source>,
  cancel: &CancellationToken,
  progress: &watch::Sender<ExportProgress>,
) -> Result<()> {
  let mut writer = zip::ZipWriter::new(LimitedFile { file, position: 0 });
  let options = zip::write::SimpleFileOptions::default()
    .compression_method(zip::CompressionMethod::Deflated)
    .unix_permissions(0o644);
  writer
    .start_file("modrinth.index.json", options)
    .map_err(zip_error)?;
  writer.write_all(index)?;
  for (relative, source) in snapshot
    .iter()
    .filter(|(_, file)| !file.fingerprint.directory)
  {
    check(cancel)?;
    let path = game.join(relative);
    let mut input = File::open(&path)?;
    if Fingerprint::from(&input.metadata()?)? != source.fingerprint {
      return Err(conflict());
    }
    writer
      .start_file(format!("overrides/{relative}"), options)
      .map_err(zip_error)?;
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 65_536];
    loop {
      check(cancel)?;
      let count = input.read(&mut buffer)?;
      if count == 0 {
        break;
      }
      bytes += count as u64;
      if bytes > source.fingerprint.size {
        return Err(conflict());
      }
      hash.update(&buffer[..count]);
      writer.write_all(&buffer[..count])?;
      progress.send_modify(|p| p.processed_bytes += count as u64);
    }
    if bytes != source.fingerprint.size
      || Some(hex(&hash.finalize())) != source.sha256
      || Fingerprint::from(&input.metadata()?)? != source.fingerprint
    {
      return Err(conflict());
    }
    progress.send_modify(|p| p.completed_files += 1);
  }
  writer.finish().map_err(zip_error)?;
  Ok(())
}

#[derive(Clone)]
pub struct ModpackExporter {
  context: Context,
}
impl ModpackExporter {
  pub fn new(context: Context) -> Self {
    Self { context }
  }
  /// The caller selects an output path. Existing files are never replaced.
  /// This holds the normal instance/settings locks until verified publication.
  pub async fn export(
    &self,
    name: &str,
    options: ExportOptions,
    destination: PathBuf,
    cancel: &CancellationToken,
    progress: &watch::Sender<ExportProgress>,
  ) -> Result<ExportResult> {
    options.validate()?;
    let context = self.context.clone();
    let name = name.to_owned();
    let cancel_copy = cancel.clone();
    let progress = progress.clone();
    super::install::blocking(&self.context, cancel, move || {
      export_locked(
        &context,
        &name,
        &options,
        &destination,
        &cancel_copy,
        &progress,
      )
    })
    .await
  }
}
fn export_locked(
  context: &Context,
  name: &str,
  options: &ExportOptions,
  destination: &Path,
  cancel: &CancellationToken,
  progress: &watch::Sender<ExportProgress>,
) -> Result<ExportResult> {
  check(cancel)?;
  progress.send_replace(ExportProgress::default());
  let id = crate::client::client_instance_id(name)?;
  let _instance = storage::server_instance_lock(&context.paths.data, &id)?;
  let _settings = storage::client_settings_lock(&context.paths.data, &id)?;
  let (_, record, directory) = crate::client::export_snapshot(context, name)?;
  let game = directory.join("game");
  let metadata = metadata_snapshot(&directory)?;
  let expected = crate::resources::export_resources(&directory, &options.selection)?;
  let index = serde_json::to_vec(&prepare_export_index(&record, options)?)?;
  let absolute = std::path::absolute(destination)?;
  if absolute
    .extension()
    .and_then(|ext| ext.to_str())
    .is_none_or(|ext| !ext.eq_ignore_ascii_case("mrpack"))
  {
    return Err(Error::Invalid("导出文件必须使用 .mrpack 扩展名".into()));
  }
  let parent = fs::canonicalize(
    absolute
      .parent()
      .ok_or_else(|| Error::Invalid("无效导出目录".into()))?,
  )?;
  if parent.starts_with(fs::canonicalize(&directory)?) {
    return Err(Error::Invalid("导出文件不能保存在源实例内".into()));
  }
  let destination = parent.join(
    absolute
      .file_name()
      .ok_or_else(|| Error::Invalid("无效导出文件名".into()))?,
  );
  match fs::symlink_metadata(&destination) {
    Ok(_) => return Err(Error::Conflict("导出文件已存在，请换一个名称".into())),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
    Err(e) => return Err(e.into()),
  }
  let snapshot = collect(&game, &options.selection, cancel)?;
  for (path, (hash, size)) in expected {
    let Some(source) = snapshot.get(&path) else {
      return Err(Error::Conflict("受管资源文件缺失，请先同步资源".into()));
    };
    if source.sha256.as_ref() != Some(&hash) || source.fingerprint.size != size {
      return Err(Error::Conflict(
        "受管资源文件已修改，请先检查并同步资源".into(),
      ));
    }
  }
  let total = snapshot
    .values()
    .filter(|s| !s.fingerprint.directory)
    .map(|s| s.fingerprint.size)
    .sum::<u64>();
  if total.saturating_add(index.len() as u64) > archive::MAX_UNPACKED_BYTES {
    return Err(Error::Invalid("导出总量超出导入限制".into()));
  }
  let files = snapshot
    .values()
    .filter(|s| !s.fingerprint.directory)
    .count();
  progress.send_replace(ExportProgress {
    stage: ExportStage::Packing,
    total_files: Some(files),
    total_bytes: Some(total),
    ..ExportProgress::default()
  });
  let mut temporary = tempfile::Builder::new()
    .prefix(".anvil-export-")
    .tempfile_in(&parent)?;
  pack(
    temporary.as_file_mut(),
    &index,
    &game,
    &snapshot,
    cancel,
    progress,
  )?;
  temporary.as_file().sync_all()?;
  progress.send_modify(|p| p.stage = ExportStage::Verifying);
  let validated = inspect_archive(File::open(temporary.path())?, cancel)?;
  super::install::validate_resources(&validated)?;
  if validated.overrides.len() != files
    || collect(&game, &options.selection, cancel)? != snapshot
    || metadata_snapshot(&directory)? != metadata
  {
    return Err(conflict());
  }
  check(cancel)?;
  progress.send_modify(|p| p.stage = ExportStage::Publishing);
  let size = temporary.as_file().metadata()?.len();
  temporary.persist_noclobber(&destination).map_err(|e| {
    if e.error.kind() == std::io::ErrorKind::AlreadyExists {
      Error::Conflict("导出文件已存在，未覆盖".into())
    } else {
      Error::Io(e.error)
    }
  })?;
  // Publication is the commit point. Do not report a committed output as a
  // failed/cancelled export; retry must never invite replacing its contents.
  #[cfg(unix)]
  {
    let _ = File::open(&parent).and_then(|file| file.sync_all());
  }
  progress.send_modify(|p| p.stage = ExportStage::Completed);
  Ok(ExportResult {
    path: destination,
    size,
    sha256: validated.archive_sha256,
    files,
  })
}

#[cfg(test)]
#[path = "../../tests/modpack_export_archive.rs"]
mod tests;
