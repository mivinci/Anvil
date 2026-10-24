//! Instance-only deletion; shared artifacts and credentials are never visited.
use super::*;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientRemoval {
  pub removed: bool,
  pub cleanup_pending: bool,
}

fn linked(metadata: &fs::Metadata) -> bool {
  if metadata.file_type().is_symlink() {
    return true;
  }
  #[cfg(windows)]
  {
    use std::os::windows::fs::MetadataExt;
    if metadata.file_attributes() & 0x400 != 0 {
      return true;
    }
  }
  false
}

fn directory(path: &Path) -> Result<()> {
  let metadata = fs::symlink_metadata(path).map_err(|error| {
    if error.kind() == std::io::ErrorKind::NotFound {
      Error::NotFound("实例不存在，请刷新列表。".into())
    } else {
      error.into()
    }
  })?;
  if linked(&metadata) || !metadata.is_dir() {
    return Err(Error::Conflict(
      "实例路径包含链接或无效目录，未删除。".into(),
    ));
  }
  Ok(())
}

fn inspect_tree(root: &Path) -> Result<()> {
  let mut pending = vec![(root.to_owned(), 0)];
  let mut count = 0usize;
  while let Some((path, depth)) = pending.pop() {
    count += 1;
    if count > 250_000 || depth > 128 {
      return Err(Error::Conflict(
        "实例文件数量或层级超出安全删除范围，未删除。".into(),
      ));
    }
    let metadata = fs::symlink_metadata(&path)?;
    if linked(&metadata) || !(metadata.is_dir() || metadata.is_file()) {
      return Err(Error::Conflict(
        "实例包含链接或特殊文件，请先移除链接后重试。".into(),
      ));
    }
    if metadata.is_dir() {
      for entry in fs::read_dir(&path)? {
        pending.push((entry?.path(), depth + 1));
        if pending.len() > 250_000 {
          return Err(Error::Conflict(
            "实例文件数量超出安全删除范围，未删除。".into(),
          ));
        }
      }
    }
  }
  Ok(())
}

impl ClientInstaller {
  /// Delete only this validated instance under the same lock used by installs,
  /// resource writes and the running game; settings writes have their own lock.
  pub async fn remove(&self, name: &str) -> Result<ClientRemoval> {
    let id = client_instance_id(name)?;
    let context = self.context.clone();
    self
      .blocking(&CancellationToken::new(), move || {
        directory(&context.paths.data)?;
        let _instance = storage::server_instance_lock(&context.paths.data, &id)?;
        let _settings = storage::client_settings_lock(&context.paths.data, &id)?;
        let root = context.paths.data.join("instances");
        directory(&root)?;
        let target = root.join(&id);
        directory(&target)?;
        let canonical_root = fs::canonicalize(&root)?;
        let canonical_target = fs::canonicalize(&target)?;
        if canonical_root.parent() != Some(fs::canonicalize(&context.paths.data)?.as_path())
          || canonical_target.parent() != Some(canonical_root.as_path())
        {
          return Err(Error::Conflict("实例路径超出管理目录，未删除。".into()));
        }
        let record: ClientInstance = read_json_regular(&target.join("instance.json"), 64 * 1024)?;
        validate_client_instance(&record)?;
        if record.id != id {
          return Err(Error::Conflict("实例身份已变化，未删除。".into()));
        }
        inspect_tree(&target)?;
        // The temporary parent is inside the checked instances directory and has
        // a non-client prefix, so listings ignore it even after a crash.
        let staging = tempfile::Builder::new()
          .prefix(".delete-")
          .tempdir_in(&root)?;
        let destination = staging.path().join("instance");
        if fs::canonicalize(staging.path())?.parent() != Some(canonical_root.as_path()) {
          return Err(Error::Conflict("删除暂存目录无效，未删除。".into()));
        }
        fs::rename(&target, &destination)?;
        let staging = staging.keep();
        // Rename commits removal. A cleanup failure must not suggest that the
        // instance still exists or invite a second destructive submission.
        let cleanup_pending = fs::remove_dir_all(staging).is_err();
        Ok(ClientRemoval {
          removed: true,
          cleanup_pending,
        })
      })
      .await
  }
}

#[cfg(test)]
#[path = "../../tests/client_removal.rs"]
mod tests;
