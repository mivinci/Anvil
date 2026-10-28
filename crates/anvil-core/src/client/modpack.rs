//! Prepare the normal game snapshot without publishing an instance.
use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModpackMetadata {
  schema_version: u32,
  format: String,
  name: String,
  version_id: String,
  archive_sha256: String,
}

/// Read the original Modrinth pack version recorded during import, if present.
pub fn modpack_version(context: &Context, name: &str) -> Result<Option<String>> {
  let id = client_instance_id(name)?;
  let path = context
    .paths
    .data
    .join("instances")
    .join(&id)
    .join("modpack.json");
  let meta = match fs::symlink_metadata(&path) {
    Ok(meta) => meta,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
    Err(error) => return Err(error.into()),
  };
  if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > 64 * 1024 {
    return Err(Error::Conflict("整合包来源记录无效".into()));
  }
  let value: ModpackMetadata = read_json_regular(&path, 64 * 1024)?;
  if value.schema_version != 1
    || value.format != "modrinth"
    || value.name.is_empty()
    || value.name.len() > 256
    || value.name.chars().any(char::is_control)
    || value.version_id.is_empty()
    || value.version_id.len() > 128
    || value.version_id.chars().any(char::is_control)
    || value.archive_sha256.len() != 64
    || !value
      .archive_sha256
      .bytes()
      .all(|byte| byte.is_ascii_hexdigit())
  {
    return Err(Error::Conflict("整合包来源记录无效".into()));
  }
  Ok(Some(value.version_id))
}

impl ClientInstaller {
  pub(crate) async fn prepare_unpublished(
    &self,
    plan: &ClientInstallPlan,
    cancel: &CancellationToken,
    progress: &watch::Sender<ClientProgress>,
  ) -> Result<(ClientInstallRecord, usize, usize)> {
    check_cancel(cancel)?;
    let (files, artifacts) = collect_plan_files(plan)?;
    let record = make_install_record(plan, files)?;
    validate_install_record(&record)?;
    progress.send_replace(ClientProgress {
      stage: ClientInstallStage::Downloading,
      total_files: Some(artifacts.len()),
      total_bytes: Some(artifacts.iter().map(|file| file.size).sum()),
      ..ClientProgress::default()
    });
    let (downloaded, reused) = self.download_all(artifacts, cancel, progress).await?;
    Ok((record, downloaded, reused))
  }
}

/// The caller owns the instance lock; this only reads the validated snapshot.
pub(crate) fn export_snapshot(
  context: &Context,
  name: &str,
) -> Result<(ClientInstance, ClientInstallRecord, PathBuf)> {
  let id = client_instance_id(name)?;
  let game = validated_game_directory(&context.paths.data, &id)?;
  let directory = game
    .parent()
    .ok_or_else(|| Error::Invalid("invalid client directory".into()))?
    .to_owned();
  let instance: ClientInstance = read_json_regular(&directory.join("instance.json"), 64 * 1024)?;
  validate_client_instance(&instance)?;
  let installed: ClientInstallRecord =
    read_json_regular(&directory.join("installed.json"), MAX_INSTALL_RECORD_SIZE)?;
  validate_install_record(&installed)?;
  Ok((instance, installed, directory))
}
