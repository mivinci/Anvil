//! Extract only bounded regular files/directories, then materialize confined links.
//! No archive link is installed while file entries are being written.
use std::{
  collections::HashSet,
  fs::{self, File, OpenOptions},
  io::{self, Read},
  path::{Component, Path, PathBuf},
};

use flate2::read::GzDecoder;
use tokio_util::sync::CancellationToken;

use crate::{Error, Result, error::check_cancel};

const MAX_ENTRIES: usize = 100_000;
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

struct Extractor<'a> {
  root: &'a Path,
  cancel: &'a CancellationToken,
  paths: HashSet<PathBuf>,
  bytes: u64,
  links: Vec<(PathBuf, PathBuf)>,
}

/// Require portable relative names; reject traversal, drive prefixes and Windows aliases.
fn relative(path: &Path) -> Result<PathBuf> {
  let mut result = PathBuf::new();
  for c in path.components() {
    match c {
      Component::CurDir => (),
      Component::Normal(s) => {
        let s = s
          .to_str()
          .ok_or_else(|| Error::Archive("non-UTF8 path".into()))?;
        let name = s.split('.').next().unwrap_or("").to_ascii_uppercase();
        if s.contains(['\\', ':'])
          || s.ends_with(['.', ' '])
          || s.chars().any(char::is_control)
          || matches!(name.as_str(), "CON" | "PRN" | "AUX" | "NUL")
          || (name.len() == 4
            && (name.starts_with("COM") || name.starts_with("LPT"))
            && name.as_bytes()[3].is_ascii_digit())
        {
          return Err(Error::Archive("non-portable path".into()));
        }
        result.push(s);
      }
      _ => return Err(Error::Archive("path escapes extraction root".into())),
    }
  }
  if result.as_os_str().is_empty() || result.as_os_str().len() > 4096 {
    return Err(Error::Archive("empty or excessive path".into()));
  }
  Ok(result)
}

impl Extractor<'_> {
  fn register(&mut self, path: &Path, size: u64) -> Result<PathBuf> {
    check_cancel(self.cancel)?;
    let path = relative(path)?;
    self.bytes = self
      .bytes
      .checked_add(size)
      .ok_or_else(|| Error::Archive("size overflow".into()))?;
    if self.paths.len() >= MAX_ENTRIES || self.bytes > MAX_BYTES || !self.paths.insert(path.clone())
    {
      return Err(Error::Archive(
        "duplicate entry or archive exceeds limits".into(),
      ));
    }
    Ok(path)
  }
  fn file(&self, path: &Path, size: u64, mode: u32, input: &mut impl Read) -> Result<()> {
    let dest = self.root.join(path);
    let parent = dest
      .parent()
      .ok_or_else(|| Error::Archive("archive entry has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let mut output = OpenOptions::new()
      .write(true)
      .create_new(true)
      .open(&dest)?;
    let mut remaining = size;
    let mut buffer = [0u8; 64 * 1024];
    while remaining > 0 {
      check_cancel(self.cancel)?;
      let len = remaining.min(buffer.len() as u64) as usize;
      let n = input.read(&mut buffer[..len])?;
      if n == 0 {
        return Err(Error::Archive("truncated file".into()));
      }
      use std::io::Write;
      output.write_all(&buffer[..n])?;
      remaining -= n as u64;
    }
    // Also forces ZIP CRC verification; never accept more bytes than declared.
    if input.read(&mut [0u8; 1])? != 0 {
      return Err(Error::Archive("oversized entry".into()));
    }
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      fs::set_permissions(
        &dest,
        fs::Permissions::from_mode(if mode & 0o111 != 0 { 0o755 } else { 0o644 }),
      )?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    output.sync_all()?;
    Ok(())
  }
  fn links(&mut self) -> Result<()> {
    // Materialize links as file copies. This avoids Windows symlink privileges and
    // prevents later extraction/publication from traversing archive-provided links.
    for _ in 0..64 {
      if self.links.is_empty() {
        return Ok(());
      }
      let before = self.links.len();
      let mut pending = Vec::new();
      for (path, target) in std::mem::take(&mut self.links) {
        check_cancel(self.cancel)?;
        let source = self.root.join(&target);
        if !source.try_exists()? {
          pending.push((path, target));
          continue;
        }
        let metadata = fs::symlink_metadata(&source)?;
        if !metadata.is_file() {
          return Err(Error::Archive("link target must be a regular file".into()));
        }
        self.bytes = self
          .bytes
          .checked_add(metadata.len())
          .ok_or_else(|| Error::Archive("size overflow".into()))?;
        if self.bytes > MAX_BYTES {
          return Err(Error::Archive("expanded links exceed size limit".into()));
        }
        self.file(
          &path,
          metadata.len(),
          mode(&metadata),
          &mut File::open(source)?,
        )?;
      }
      if pending.len() == before {
        return Err(Error::Archive("missing or cyclic link target".into()));
      }
      self.links = pending;
    }
    if self.links.is_empty() {
      Ok(())
    } else {
      Err(Error::Archive("link chain exceeds depth limit".into()))
    }
  }
}

fn mode(metadata: &fs::Metadata) -> u32 {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode()
  }
  #[cfg(not(unix))]
  {
    let _ = metadata;
    0
  }
}

pub(crate) fn unpack(
  archive: &Path,
  root: &Path,
  zip: bool,
  cancel: &CancellationToken,
) -> Result<()> {
  let mut extractor = Extractor {
    root,
    cancel,
    paths: HashSet::new(),
    bytes: 0,
    links: Vec::new(),
  };
  if zip {
    let mut archive = zip::ZipArchive::new(File::open(archive)?).map_err(zip_error)?;
    if archive.len() > MAX_ENTRIES {
      return Err(Error::Archive("too many entries".into()));
    }
    for i in 0..archive.len() {
      let mut entry = archive.by_index(i).map_err(zip_error)?;
      let path = extractor.register(Path::new(entry.name()), entry.size())?;
      let mode = entry.unix_mode().unwrap_or(0o644);
      if mode & 0o170000 == 0o120000 {
        return Err(Error::Archive("ZIP symlinks are unsupported".into()));
      }
      if entry.is_dir() {
        fs::create_dir_all(root.join(path))?;
      } else {
        extractor.file(&path, entry.size(), mode, &mut entry)?;
      }
    }
  } else {
    let mut archive = tar::Archive::new(GzDecoder::new(File::open(archive)?));
    for entry in archive.entries()? {
      let mut entry = entry?;
      let path = extractor.register(&entry.path()?, entry.size())?;
      let kind = entry.header().entry_type();
      if kind.is_file() {
        extractor.file(&path, entry.size(), entry.header().mode()?, &mut entry)?;
      } else if kind.is_dir() {
        fs::create_dir_all(root.join(path))?;
      } else if kind.is_symlink() || kind.is_hard_link() {
        let link = entry
          .link_name()?
          .ok_or_else(|| Error::Archive("missing link target".into()))?;
        let target = if kind.is_hard_link() {
          relative(&link)?
        } else {
          let mut target = path.parent().unwrap_or_else(|| Path::new("")).to_path_buf();
          for c in link.components() {
            match c {
              Component::CurDir => (),
              Component::Normal(s) => target.push(s),
              Component::ParentDir if target.pop() => (),
              _ => return Err(Error::Archive("link escapes extraction root".into())),
            }
          }
          relative(&target)?
        };
        extractor.links.push((path, target));
      } else {
        return Err(Error::Archive("unsupported archive entry type".into()));
      }
    }
  }
  extractor.links()
}

fn zip_error(e: zip::result::ZipError) -> Error {
  Error::Archive(e.to_string())
}

pub(crate) fn java_home(root: &Path, mac: bool) -> Result<PathBuf> {
  let entries = fs::read_dir(root)?.collect::<io::Result<Vec<_>>>()?;
  if entries.len() != 1 || !entries[0].file_type()?.is_dir() {
    return Err(Error::Archive("expected one Java root directory".into()));
  }
  let top = entries[0].file_name();
  let home = if mac {
    PathBuf::from(top).join("Contents/Home")
  } else {
    PathBuf::from(top)
  };
  let executable = home
    .join("bin")
    .join(if cfg!(windows) { "java.exe" } else { "java" });
  if !root.join(executable).is_file() {
    return Err(Error::Archive("Java executable missing".into()));
  }
  Ok(home)
}

#[cfg(test)]
#[path = "../tests/extract.rs"]
mod tests;
