//! Loader-compatible session JARs; never expose hash-only blob names to JAR scanners.
use super::*;
use std::io::Write;

pub(crate) fn jar_path(session: &Path, sha1: &str) -> PathBuf {
  session.join("classpath").join(format!("{sha1}.jar"))
}

pub(super) fn prepare(
  paths: &crate::Paths,
  session: &Path,
  installed: &ClientInstallRecord,
  plan: &ClientInstallPlan,
  cancel: &CancellationToken,
) -> Result<()> {
  fs::create_dir(session.join("classpath"))?;
  let mut artifacts = plan
    .libraries
    .iter()
    .map(|library| {
      (
        format!("libraries/{}", library.artifact.path),
        &library.artifact,
      )
    })
    .collect::<Vec<_>>();
  artifacts.push((plan.client_jar.path.clone(), &plan.client_jar));
  let mut copied = std::collections::HashSet::new();
  for (logical, planned) in artifacts {
    check_cancel(cancel)?;
    let entry = installed
      .files
      .iter()
      .find(|entry| entry.path == logical)
      .ok_or_else(|| Error::Conflict("classpath artifact missing from install record".into()))?;
    if entry.sha1 != planned.file.sha1 || entry.size != planned.file.size {
      return Err(Error::Conflict(
        "classpath artifact does not match launch plan".into(),
      ));
    }
    if !copied.insert(&entry.sha1) {
      continue;
    }
    let source = paths.cache.join(&entry.cache_path);
    let metadata = fs::symlink_metadata(&source)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != entry.size {
      return Err(Error::Conflict(
        "classpath cache artifact is invalid".into(),
      ));
    }
    let target = jar_path(session, &entry.sha1);
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
      .write(true)
      .create_new(true)
      .open(&target)?;
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
      check_cancel(cancel)?;
      let count = input.read(&mut buffer)?;
      if count == 0 {
        break;
      }
      total = total.saturating_add(count as u64);
      if total > entry.size {
        return Err(Error::Conflict(
          "classpath artifact grew during copy".into(),
        ));
      }
      output.write_all(&buffer[..count])?;
    }
    if total != entry.size || !file_matches(&target, entry.size, &entry.sha1, cancel)? {
      return Err(Error::Conflict(
        "session classpath checksum mismatch".into(),
      ));
    }
  }
  Ok(())
}
