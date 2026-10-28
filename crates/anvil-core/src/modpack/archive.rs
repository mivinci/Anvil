//! Read-only archive inspection. Installation must reopen the same immutable upload
//! and verify `archive_sha256` before using the recorded entry indexes.
use super::{MAX_INDEX_BYTES, Plan, invalid, parse_index, validate_path};
use crate::{CancellationToken, Error, Result};
use sha2::{Digest, Sha256};
use std::{
  collections::BTreeMap,
  io::{Read, Seek, SeekFrom},
};

pub(super) const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
pub(super) const MAX_ENTRIES: usize = 20_000;
pub(super) const MAX_ENTRY_BYTES: u64 = 128 * 1024 * 1024;
pub(super) const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug)]
pub struct OverrideFile {
  pub path: String,
  pub entry_index: usize,
  pub size: u64,
}

#[derive(Debug)]
pub struct ArchivePlan {
  pub index: Plan,
  pub overrides: Vec<OverrideFile>,
  pub archive_sha256: String,
  pub archive_bytes: u64,
}

fn cancelled(cancel: &CancellationToken) -> Result<()> {
  if cancel.is_cancelled() {
    Err(Error::Cancelled)
  } else {
    Ok(())
  }
}

fn zip_error(error: zip::result::ZipError) -> Error {
  Error::Archive(format!("modpack: {error}"))
}

// Bound central-directory allocation before handing the reader to the ZIP crate.
// ZIP64 and split archives are deliberately outside the first import format subset.
fn preflight<R: Read + Seek>(reader: &mut R, cancel: &CancellationToken) -> Result<(u64, String)> {
  cancelled(cancel)?;
  let size = reader.seek(SeekFrom::End(0))?;
  if !(22..=MAX_ARCHIVE_BYTES).contains(&size) {
    return Err(invalid("archive exceeds size limit or is truncated"));
  }
  let tail_size = size.min(65_557) as usize;
  reader.seek(SeekFrom::End(-(tail_size as i64)))?;
  let mut tail = vec![0; tail_size];
  reader.read_exact(&mut tail)?;
  let offset = (0..=tail.len() - 22)
    .rev()
    .find(|&i| {
      tail[i..i + 4] == *b"PK\x05\x06"
        && i + 22 + usize::from(u16::from_le_bytes([tail[i + 20], tail[i + 21]])) == tail.len()
    })
    .ok_or_else(|| invalid("missing ZIP end record"))?;
  let end = &tail[offset..];
  let u16_at = |i| u16::from_le_bytes([end[i], end[i + 1]]);
  let u32_at = |i| u32::from_le_bytes(end[i..i + 4].try_into().unwrap());
  let entries = u16_at(10);
  let directory_size = u32_at(12);
  let directory_offset = u32_at(16);
  if u16_at(4) != 0
    || u16_at(6) != 0
    || u16_at(8) != entries
    || entries == u16::MAX
    || usize::from(entries) > MAX_ENTRIES
    || directory_size > 16 * 1024 * 1024
    || directory_offset == u32::MAX
    || u64::from(directory_offset) + u64::from(directory_size) != size - (tail_size - offset) as u64
  {
    return Err(invalid(
      "unsupported split/ZIP64 archive or oversized directory",
    ));
  }
  reader.seek(SeekFrom::Start(u64::from(directory_offset)))?;
  let mut directory = vec![0; directory_size as usize];
  reader.read_exact(&mut directory)?;
  let mut cursor = 0usize;
  let mut actual_entries = 0usize;
  while cursor < directory.len() {
    cancelled(cancel)?;
    let header = directory
      .get(cursor..cursor + 46)
      .ok_or_else(|| invalid("truncated ZIP directory"))?;
    if &header[..4] != b"PK\x01\x02" {
      return Err(invalid("invalid ZIP directory record"));
    }
    let field = |i| usize::from(u16::from_le_bytes([header[i], header[i + 1]]));
    cursor += 46 + field(28) + field(30) + field(32);
    actual_entries += 1;
    if cursor > directory.len() || actual_entries > MAX_ENTRIES {
      return Err(invalid("oversized or truncated ZIP directory"));
    }
  }
  if actual_entries != usize::from(entries) {
    return Err(invalid("ZIP directory entry count mismatch"));
  }
  reader.rewind()?;
  let mut digest = Sha256::new();
  let mut buffer = [0u8; 65_536];
  loop {
    cancelled(cancel)?;
    let read = reader.read(&mut buffer)?;
    if read == 0 {
      break;
    }
    digest.update(&buffer[..read]);
  }
  reader.rewind()?;
  Ok((
    size,
    digest
      .finalize()
      .iter()
      .map(|byte| format!("{byte:02x}"))
      .collect(),
  ))
}

pub(super) fn insert_path(
  paths: &mut BTreeMap<String, (String, bool)>,
  path: &str,
  directory: bool,
) -> Result<()> {
  let key = path.to_lowercase();
  if paths.insert(key, (path.to_owned(), directory)).is_some() {
    return Err(invalid("duplicate or case-conflicting ZIP entries"));
  }
  Ok(())
}

pub(super) fn check_tree(paths: &BTreeMap<String, (String, bool)>) -> Result<()> {
  // Explicit and implicit directory spellings must also agree across platforms.
  let mut directories = BTreeMap::<String, String>::new();
  for (path, directory) in paths.values() {
    for end in path
      .match_indices('/')
      .map(|(i, _)| i)
      .chain(directory.then_some(path.len()))
    {
      let parent = &path[..end];
      let key = parent.to_lowercase();
      if paths.get(&key).is_some_and(|(_, directory)| !directory) {
        return Err(invalid("file conflicts with a parent directory"));
      }
      if let Some(previous) = directories.insert(key, parent.to_owned())
        && previous != parent
      {
        return Err(invalid("case-conflicting directories"));
      }
    }
  }
  Ok(())
}

/// Validate every entry and CRC without extracting files or creating an instance.
/// This performs blocking, bounded I/O; async callers must use their blocking pool.
pub fn inspect_archive<R: Read + Seek>(
  mut reader: R,
  cancel: &CancellationToken,
) -> Result<ArchivePlan> {
  let (archive_bytes, archive_sha256) = preflight(&mut reader, cancel)?;
  let mut archive = zip::ZipArchive::new(reader).map_err(zip_error)?;
  if archive.len() > MAX_ENTRIES {
    return Err(invalid("too many ZIP entries"));
  }
  let mut paths = BTreeMap::new();
  let mut overrides = BTreeMap::<String, (u8, OverrideFile)>::new();
  let mut override_paths = BTreeMap::<String, (String, bool)>::new();
  let mut index_bytes = None;
  let mut total = 0u64;
  for entry_index in 0..archive.len() {
    cancelled(cancel)?;
    let mut entry = archive.by_index(entry_index).map_err(zip_error)?;
    let directory = entry.is_dir();
    let name = entry
      .name()
      .strip_suffix('/')
      .unwrap_or(entry.name())
      .to_owned();
    validate_path(&name)?;
    let mode = entry.unix_mode().unwrap_or(0) & 0o170000;
    if !matches!(mode, 0 | 0o100000 | 0o040000)
      || (mode == 0o040000 && !directory)
      || (directory && entry.size() != 0)
    {
      return Err(invalid("ZIP links or special files are not supported"));
    }
    insert_path(&mut paths, &name, directory)?;
    if entry.size() > MAX_ENTRY_BYTES
      || (name == "modrinth.index.json" && entry.size() > MAX_INDEX_BYTES as u64)
    {
      return Err(invalid("ZIP entry exceeds size limit"));
    }
    total = total
      .checked_add(entry.size())
      .filter(|size| *size <= MAX_UNPACKED_BYTES)
      .ok_or_else(|| invalid("ZIP unpacked size exceeds limit"))?;
    let is_index = name == "modrinth.index.json" && !directory;
    let mut bytes = Vec::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65_536];
    loop {
      cancelled(cancel)?;
      let read = entry.read(&mut buffer)?;
      if read == 0 {
        break;
      }
      count += read as u64;
      if count > entry.size() {
        return Err(invalid("ZIP entry exceeds declared size"));
      }
      if is_index {
        bytes.extend_from_slice(&buffer[..read]);
      }
    }
    if count != entry.size() {
      return Err(invalid("truncated ZIP entry"));
    }
    if is_index {
      index_bytes = Some(bytes);
    }
    for (prefix, layer) in [("overrides/", 0), ("client-overrides/", 1)] {
      if let Some(path) = name.strip_prefix(prefix) {
        validate_path(path)?;
        let key = path.to_lowercase();
        if let Some((previous, previous_directory)) = override_paths.get(&key) {
          if previous != path || *previous_directory != directory {
            return Err(invalid("conflicting override path types or casing"));
          }
        } else {
          override_paths.insert(key.clone(), (path.to_owned(), directory));
        }
        if !directory
          && overrides
            .get(&key)
            .is_none_or(|(old_layer, _)| layer > *old_layer)
        {
          overrides.insert(
            key,
            (
              layer,
              OverrideFile {
                path: path.to_owned(),
                entry_index,
                size: entry.size(),
              },
            ),
          );
        }
      }
    }
  }
  check_tree(&paths)?;
  check_tree(&override_paths)?;
  let index =
    parse_index(&index_bytes.ok_or_else(|| invalid("missing root modrinth.index.json"))?)?;
  for file in &index.files {
    let key = file.path.to_lowercase();
    if let Some((path, directory)) = override_paths.get(&key) {
      if path != &file.path || *directory {
        return Err(invalid("download and override path conflict"));
      }
    } else {
      override_paths.insert(key, (file.path.clone(), false));
    }
  }
  check_tree(&override_paths)?;
  Ok(ArchivePlan {
    index,
    overrides: overrides.into_values().map(|(_, file)| file).collect(),
    archive_sha256,
    archive_bytes,
  })
}
