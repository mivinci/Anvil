//! Modrinth identities and plans reuse shared transfers, locks and resource transactions.
use super::transfer::{DownloadFile, lookup};
use super::*;
use crate::{
  CancellationToken, client::ClientInstallRecord, fabric::ClientLoader, modrinth::DependencyPreview,
};
use std::collections::{BTreeMap, BTreeSet};
use tokio::sync::watch;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModrinthOrigin {
  pub project_id: String,
  pub version_id: String,
  pub project_name: String,
  pub sha1: String,
  pub direct: bool,
  pub required_versions: BTreeMap<String, String>,
  pub incompatible_versions: BTreeMap<String, Option<String>>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthPlanFile {
  pub project_id: String,
  pub file_id: String,
  pub project_name: String,
  pub filename: String,
  pub size: u64,
  pub direct: bool,
  pub reuse: bool,
}
#[derive(Clone)]
struct Planned {
  origin: ModrinthOrigin,
  file: DownloadFile,
}
#[derive(Clone)]
pub struct ModrinthInstallPlan {
  name: String,
  game: String,
  loader: ClientLoader,
  before: ResourceManifest,
  files: Vec<Planned>,
  summary: Vec<ModrinthPlanFile>,
}
impl ModrinthInstallPlan {
  pub fn name(&self) -> &str {
    &self.name
  }
  pub fn revision(&self) -> u64 {
    self.before.revision
  }
  pub fn files(&self) -> &[ModrinthPlanFile] {
    &self.summary
  }
  pub fn total_bytes(&self) -> u64 {
    self.summary.iter().map(|f| f.size).sum()
  }
  pub fn download_bytes(&self) -> u64 {
    self
      .summary
      .iter()
      .filter(|f| !f.reuse)
      .map(|f| f.size)
      .sum()
  }
}
pub(super) fn validate_entries(manifest: &ResourceManifest, graph: bool) -> Result<()> {
  let mut projects = BTreeMap::new();
  for entry in &manifest.mods {
    if entry.source != ResourceSource::Modrinth {
      if entry.modrinth_origin.is_some() {
        return Err(Error::Metadata("unexpected Modrinth origin".into()));
      }
      continue;
    }
    let origin = entry
      .modrinth_origin
      .as_ref()
      .ok_or_else(|| Error::Metadata("missing Modrinth origin".into()))?;
    if manifest.schema_version < 4
      || entry.origin.is_some()
      || !crate::modrinth::valid_id(&origin.project_id)
      || !crate::modrinth::valid_id(&origin.version_id)
      || origin.project_name.is_empty()
      || origin.project_name.len() > 512
      || origin.sha1.len() != 40
      || !origin
        .sha1
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
      || origin.required_versions.len() > 128
      || origin.incompatible_versions.len() > 128
      || origin
        .required_versions
        .iter()
        .any(|(p, v)| !crate::modrinth::valid_id(p) || !crate::modrinth::valid_id(v))
      || origin.incompatible_versions.iter().any(|(p, v)| {
        !crate::modrinth::valid_id(p) || v.as_ref().is_some_and(|v| !crate::modrinth::valid_id(v))
      })
      || projects
        .insert(origin.project_id.clone(), (entry, origin))
        .is_some()
    {
      return Err(Error::Metadata(
        "invalid or duplicate Modrinth origin".into(),
      ));
    }
  }
  if !graph {
    return Ok(());
  }
  for (entry, origin) in projects.values() {
    for (p, v) in &origin.required_versions {
      let (dependency, other) = projects.get(p).ok_or_else(|| {
        Error::Conflict(format!(
          "“{}”仍需要此 Modrinth 依赖，不能移除。",
          origin.project_name
        ))
      })?;
      if &other.version_id != v || (entry.enabled && !dependency.enabled) {
        return Err(Error::Conflict(
          "Modrinth 必需依赖版本不符或已停用。".into(),
        ));
      }
    }
    if entry.enabled
      && origin.incompatible_versions.iter().any(|(p, v)| {
        projects
          .get(p)
          .is_some_and(|(e, o)| e.enabled && v.as_ref().is_none_or(|v| v == &o.version_id))
      })
    {
      return Err(Error::Conflict(
        "已启用的 Modrinth 模组声明互不兼容。".into(),
      ));
    }
  }
  // Topological elimination rejects cycles without recursive stack growth.
  let mut remaining: BTreeSet<_> = projects.keys().cloned().collect();
  while !remaining.is_empty() {
    let ready: Vec<_> = remaining
      .iter()
      .filter(|p| {
        projects[*p]
          .1
          .required_versions
          .keys()
          .all(|d| !remaining.contains(d))
      })
      .cloned()
      .collect();
    if ready.is_empty() {
      return Err(Error::Conflict("Modrinth 依赖存在循环，不能提交。".into()));
    }
    for p in ready {
      remaining.remove(&p);
    }
  }
  Ok(())
}
fn record(directory: &Path) -> Result<ClientInstallRecord> {
  storage::read_json_bounded(
    &directory.join("installed.json"),
    crate::client::MAX_INSTALL_RECORD_SIZE,
  )
}
fn check(directory: &Path, plan: &ModrinthInstallPlan) -> Result<()> {
  let r = record(directory)?;
  if r.version_id != plan.game
    || r.loader.as_ref() != Some(&plan.loader)
    || read_manifest(directory)? != plan.before
  {
    return Err(Error::Conflict(
      "实例或资源已变化，请重新生成 Modrinth 安装计划。".into(),
    ));
  }
  Ok(())
}
fn merge(manifest: &mut ResourceManifest, file: &Planned, hash: String) -> Result<()> {
  if let Some(entry) = manifest.mods.iter_mut().find(|e| {
    e.modrinth_origin
      .as_ref()
      .is_some_and(|o| o.project_id == file.origin.project_id)
  }) {
    let origin = entry.modrinth_origin.as_mut().expect("matched origin");
    if origin.version_id != file.origin.version_id
      || origin.sha1 != file.origin.sha1
      || entry.sha256 != hash
      || entry.filename != file.file.filename
      || entry.size != file.file.size
    {
      return Err(Error::Conflict(
        "Modrinth 项目已安装其他版本，暂不自动升级或替换。".into(),
      ));
    }
    let direct = origin.direct || file.origin.direct;
    **origin = file.origin.clone();
    origin.direct = direct;
    entry.enabled = true;
    return Ok(());
  }
  if manifest
    .mods
    .iter()
    .any(|e| e.sha256 == hash || e.filename.eq_ignore_ascii_case(&file.file.filename))
  {
    return Err(Error::Conflict(
      "模组与已有文件内容或名称冲突，请先手动处理。".into(),
    ));
  }
  manifest.mods.push(ResourceEntry {
    source: ResourceSource::Modrinth,
    origin: None,
    modrinth_origin: Some(Box::new(file.origin.clone())),
    sha256: hash,
    filename: file.file.filename.clone(),
    size: file.file.size,
    enabled: true,
  });
  Ok(())
}
impl ResourceManager {
  pub async fn plan_modrinth(
    &self,
    name: &str,
    preview: DependencyPreview,
    revision: u64,
  ) -> Result<ModrinthInstallPlan> {
    if preview.items.is_empty()
      || preview.items.len() > 32
      || preview.items.iter().filter(|i| i.direct).count() != 1
    {
      return Err(Error::Invalid("invalid Modrinth plan size".into()));
    }
    let name = name.to_ascii_lowercase();
    self
      .access(&name.clone(), move |directory, data, fabric| {
        if !fabric {
          return Err(Error::Conflict("模组安装需要 Fabric 实例。".into()));
        }
        let r = record(directory)?;
        let before = read_manifest(directory)?;
        check_revision(&before, revision)?;
        let mut after = before.clone();
        after.schema_version = 4;
        let mut files = Vec::new();
        let mut summary = Vec::new();
        let mut projects = HashSet::new();
        for item in preview.items {
          if !crate::modrinth::valid_file(&item.file)
            || item.project.id != item.file.mod_id
            || !projects.insert(item.project.id.clone())
          {
            return Err(Error::Metadata("invalid Modrinth plan identity".into()));
          }
          let file = Planned {
            file: DownloadFile {
              filename: item.file.file_name,
              size: item.file.file_length,
              sha1: item.file.sha1.clone(),
              url: item.file.url,
            },
            origin: ModrinthOrigin {
              project_id: item.project.id,
              version_id: item.file.id,
              project_name: item.project.name,
              sha1: item.file.sha1,
              direct: item.direct,
              required_versions: item.required,
              incompatible_versions: item.incompatible,
            },
          };
          let reusable = lookup(data, &file.file)?;
          let hash = reusable
            .as_ref()
            .map(|(h, _)| h.clone())
            .or_else(|| {
              after
                .mods
                .iter()
                .find(|e| {
                  e.modrinth_origin
                    .as_ref()
                    .is_some_and(|o| o.project_id == file.origin.project_id)
                })
                .map(|e| e.sha256.clone())
            })
            .unwrap_or_else(|| {
              hex(&Sha256::digest(
                format!("modrinth:{}", file.origin.version_id).as_bytes(),
              ))
            });
          merge(&mut after, &file, hash)?;
          summary.push(ModrinthPlanFile {
            project_id: file.origin.project_id.clone(),
            file_id: file.origin.version_id.clone(),
            project_name: file.origin.project_name.clone(),
            filename: file.file.filename.clone(),
            size: file.file.size,
            direct: file.origin.direct,
            reuse: reusable.is_some(),
          });
          files.push(file);
        }
        validate_manifest(&after)?;
        Ok(ModrinthInstallPlan {
          name,
          game: r.version_id,
          loader: r
            .loader
            .ok_or_else(|| Error::Conflict("Fabric loader is missing".into()))?,
          before,
          files,
          summary,
        })
      })
      .await
  }
  pub async fn install_modrinth(
    &self,
    plan: ModrinthInstallPlan,
    cancel: CancellationToken,
    progress: watch::Sender<ResourceInstallProgress>,
  ) -> Result<ResourceManifest> {
    let initial = plan.clone();
    self
      .access(&plan.name, move |directory, _, _| {
        check(directory, &initial)
      })
      .await?;
    let mut objects = Vec::new();
    for file in &plan.files {
      if cancel.is_cancelled() {
        return Err(Error::Cancelled);
      }
      objects.push(self.prepare_shared(&file.file, &cancel, &progress).await?);
      progress.send_modify(|p| {
        p.completed_files += 1;
        p.downloaded_bytes = plan
          .files
          .iter()
          .take(p.completed_files)
          .map(|f| f.file.size)
          .sum();
      });
    }
    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    progress.send_modify(|p| p.stage = ResourceInstallStage::Publishing);
    self
      .access(&plan.name.clone(), move |directory, data, fabric| {
        check(directory, &plan)?;
        if cancel.is_cancelled() {
          return Err(Error::Cancelled);
        }
        let mut after = plan.before.clone();
        after.schema_version = 4;
        for (file, hash) in plan.files.iter().zip(objects) {
          verify_object(&object_path(data, &hash), &hash, file.file.size)?;
          merge(&mut after, file, hash)?;
        }
        validate_manifest(&after)?;
        commit_cancel(directory, data, &mut after, fabric, &cancel)?;
        Ok(after)
      })
      .await
  }
}
