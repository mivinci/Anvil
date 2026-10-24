//! Read-only crash reports from an instance-owned directory.
use super::{ClientInstaller, client_instance_id, validated_game_directory};
use crate::{CancellationToken, Error, Result};
use serde::Serialize;
use std::{
  fs,
  io::Read,
  path::{Path, PathBuf},
  time::UNIX_EPOCH,
};
const MAX_SCAN: usize = 4096;
const MAX_REPORTS: usize = 50;
pub const MAX_REPORT_BYTES: u64 = 2 * 1024 * 1024;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCrashReportEntry {
  pub name: String,
  pub size: u64,
  pub modified_at: Option<u64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCrashReports {
  pub reports: Vec<ClientCrashReportEntry>,
  pub truncated: bool,
  pub max_report_bytes: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCrashReport {
  pub name: String,
  pub text: String,
  pub total_bytes: u64,
}
fn valid_filename(name: &str) -> bool {
  !name.is_empty()
    && name.len() <= 180
    && name.ends_with(".txt")
    && name.as_bytes()[0].is_ascii_alphanumeric()
    && name
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b"._- ".contains(&b))
}
fn directory(data: &Path, id: &str) -> Result<Option<PathBuf>> {
  let path = validated_game_directory(data, id)?.join("crash-reports");
  match fs::symlink_metadata(&path) {
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(e) => Err(e.into()),
    Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
      Err(Error::Conflict("invalid crash report directory".into()))
    }
    Ok(_) => Ok(Some(path)),
  }
}
impl ClientInstaller {
  pub async fn list_crash_reports(&self, name: &str) -> Result<ClientCrashReports> {
    let id = client_instance_id(name)?;
    let data = self.context.paths.data.clone();
    self
      .blocking(&CancellationToken::new(), move || {
        let mut reports = Vec::new();
        if let Some(path) = directory(&data, &id)? {
          for (index, entry) in fs::read_dir(path)?.enumerate() {
            if index >= MAX_SCAN {
              return Err(Error::Invalid(
                "崩溃报告目录条目过多，请整理后重试。".into(),
              ));
            }
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
              continue;
            };
            if !valid_filename(&name) {
              continue;
            }
            let meta = fs::symlink_metadata(entry.path())?;
            if meta.file_type().is_symlink() || !meta.is_file() {
              continue;
            }
            let modified_at = meta
              .modified()
              .ok()
              .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
              .and_then(|duration| u64::try_from(duration.as_millis()).ok());
            reports.push(ClientCrashReportEntry {
              name,
              size: meta.len(),
              modified_at,
            });
          }
        }
        reports.sort_by(|a, b| {
          b.modified_at
            .cmp(&a.modified_at)
            .then_with(|| b.name.cmp(&a.name))
        });
        let truncated = reports.len() > MAX_REPORTS;
        reports.truncate(MAX_REPORTS);
        Ok(ClientCrashReports {
          reports,
          truncated,
          max_report_bytes: MAX_REPORT_BYTES,
        })
      })
      .await
  }
  pub async fn read_crash_report(&self, name: &str, filename: &str) -> Result<ClientCrashReport> {
    let id = client_instance_id(name)?;
    if !valid_filename(filename) {
      return Err(Error::Invalid("崩溃报告文件名无效。".into()));
    }
    let data = self.context.paths.data.clone();
    let filename = filename.to_owned();
    self
      .blocking(&CancellationToken::new(), move || {
        let path = directory(&data, &id)?
          .ok_or_else(|| Error::NotFound("崩溃报告不存在，请刷新列表。".into()))?
          .join(&filename);
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
          return Err(Error::Invalid("只能读取常规崩溃报告文件。".into()));
        }
        let file = fs::File::open(path)?;
        let meta = file.metadata()?;
        if !meta.is_file() {
          return Err(Error::Invalid("崩溃报告文件无效。".into()));
        }
        if meta.len() > MAX_REPORT_BYTES {
          return Err(Error::Invalid(
            "报告超过 2 MiB，暂不支持在应用内查看。".into(),
          ));
        }
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        file.take(MAX_REPORT_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_REPORT_BYTES {
          return Err(Error::Invalid(
            "报告超过 2 MiB，请使用外部编辑器查看。".into(),
          ));
        }
        Ok(ClientCrashReport {
          name: filename,
          total_bytes: bytes.len() as u64,
          text: String::from_utf8_lossy(&bytes).into_owned(),
        })
      })
      .await
  }
}
