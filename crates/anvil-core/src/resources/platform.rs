//! Bounded CurseForge plans, verified shared publication, and one instance transaction.
use super::transfer::{lookup, upstream_hash};
use super::*;
use crate::{
  CancellationToken,
  client::ClientInstallRecord,
  curseforge::{DependencyPreview, ModFile},
  fabric::ClientLoader,
};
use std::collections::{BTreeMap, VecDeque};
use tokio::sync::watch;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePlanFile {
  pub project_id: u32,
  pub file_id: u32,
  pub project_name: String,
  pub filename: String,
  pub size: u64,
  pub direct: bool,
  pub reuse: bool,
}
#[derive(Clone)]
struct Planned {
  file: ModFile,
  origin: CurseForgeOrigin,
}
// Private fields prevent callers from supplying untrusted URLs or forging the expected snapshot.
#[derive(Clone)]
pub struct ResourceInstallPlan {
  name: String,
  game: String,
  loader: ClientLoader,
  before: ResourceManifest,
  files: Vec<Planned>,
  summary: Vec<ResourcePlanFile>,
}
impl ResourceInstallPlan {
  pub fn files(&self) -> &[ResourcePlanFile] {
    &self.summary
  }
  pub fn name(&self) -> &str {
    &self.name
  }
  pub fn revision(&self) -> u64 {
    self.before.revision
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
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceInstallStage {
  Preparing,
  Downloading,
  Publishing,
  Completed,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInstallProgress {
  pub stage: ResourceInstallStage,
  pub completed_files: usize,
  pub total_files: usize,
  pub downloaded_bytes: u64,
  pub total_bytes: u64,
  pub received_bytes: u64,
}
impl ResourceInstallProgress {
  pub fn for_plan(plan: &ResourceInstallPlan) -> Self {
    Self {
      stage: ResourceInstallStage::Preparing,
      completed_files: 0,
      total_files: plan.files.len(),
      downloaded_bytes: 0,
      total_bytes: plan.total_bytes(),
      received_bytes: 0,
    }
  }
}
pub(super) fn validate_origin(origin: &CurseForgeOrigin) -> Result<()> {
  if origin.project_id == 0
    || origin.file_id == 0
    || origin.project_name.len() > 512
    || origin.sha1.len() != 40
    || !origin
      .sha1
      .bytes()
      .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    || origin.required_projects.len() > 128
    || origin.incompatible_projects.len() > 128
    || origin
      .required_projects
      .iter()
      .chain(&origin.incompatible_projects)
      .any(|id| *id == 0)
  {
    return Err(Error::Metadata(
      "invalid CurseForge resource identity".into(),
    ));
  }
  Ok(())
}
// Layout contains only enabled entries; validate field/ownership structure without assuming a complete graph.
pub(super) fn validate_layout_entries(manifest: &ResourceManifest) -> Result<()> {
  let mut copy = manifest.clone();
  for entry in &mut copy.mods {
    entry.origin = None;
    entry.source = ResourceSource::Local;
  }
  validate_manifest(&copy)?;
  let mut projects = HashSet::new();
  for entry in &manifest.mods {
    match (&entry.source, &entry.origin) {
      (ResourceSource::Local, None) => {}
      (ResourceSource::Curseforge, Some(origin)) => {
        validate_origin(origin)?;
        if !projects.insert(origin.project_id) {
          return Err(Error::Metadata("duplicate layout project".into()));
        }
      }
      _ => return Err(Error::Metadata("invalid layout source".into())),
    }
  }
  Ok(())
}
pub(super) fn validate_dependencies(manifest: &ResourceManifest) -> Result<()> {
  let projects: BTreeMap<_, _> = manifest
    .mods
    .iter()
    .filter_map(|entry| {
      entry
        .origin
        .as_ref()
        .map(|origin| (origin.project_id, entry))
    })
    .collect();
  for entry in &manifest.mods {
    let Some(origin) = &entry.origin else {
      continue;
    };
    for required in &origin.required_projects {
      let other = projects.get(required).ok_or_else(|| {
        Error::Conflict(format!("“{}”仍依赖该模组，不能移除。", origin.project_name))
      })?;
      if entry.enabled && !other.enabled {
        return Err(Error::Conflict(
          "请先启用必需依赖，或先停用依赖此模组的项目。".into(),
        ));
      }
    }
    if entry.enabled
      && origin
        .incompatible_projects
        .iter()
        .any(|id| projects.get(id).is_some_and(|entry| entry.enabled))
    {
      return Err(Error::Conflict(
        "已启用的模组间存在平台声明的不兼容关系。".into(),
      ));
    }
  }
  let mut pending: BTreeMap<_, _> = projects
    .iter()
    .map(|(id, entry)| {
      (
        *id,
        entry
          .origin
          .as_ref()
          .expect("indexed origin")
          .required_projects
          .len(),
      )
    })
    .collect();
  let mut dependents = BTreeMap::<u32, Vec<u32>>::new();
  for (id, entry) in &projects {
    for required in &entry
      .origin
      .as_ref()
      .expect("indexed origin")
      .required_projects
    {
      dependents.entry(*required).or_default().push(*id);
    }
  }
  let mut ready: VecDeque<_> = pending
    .iter()
    .filter(|(_, count)| **count == 0)
    .map(|(id, _)| *id)
    .collect();
  let mut completed = 0;
  while let Some(id) = ready.pop_front() {
    completed += 1;
    for dependent in dependents.get(&id).into_iter().flatten() {
      let count = pending.get_mut(dependent).expect("known project");
      *count -= 1;
      if *count == 0 {
        ready.push_back(*dependent);
      }
    }
  }
  if completed != projects.len() {
    return Err(Error::Conflict("平台模组依赖存在循环，不能提交。".into()));
  }
  Ok(())
}
fn installed(directory: &Path) -> Result<ClientInstallRecord> {
  storage::read_json_bounded(
    &directory.join("installed.json"),
    crate::client::MAX_INSTALL_RECORD_SIZE,
  )
}
fn checked_plan_state(directory: &Path, plan: &ResourceInstallPlan) -> Result<()> {
  let record = installed(directory)?;
  if record.version_id != plan.game
    || record.loader.as_ref() != Some(&plan.loader)
    || read_manifest(directory)? != plan.before
  {
    return Err(Error::Conflict(
      "实例或模组已变化，请重新生成安装计划。".into(),
    ));
  }
  Ok(())
}
fn merge(manifest: &mut ResourceManifest, file: &Planned, sha256: String) -> Result<()> {
  if let Some(entry) = manifest.mods.iter_mut().find(|entry| {
    entry
      .origin
      .as_ref()
      .is_some_and(|o| o.project_id == file.origin.project_id)
  }) {
    let origin = entry.origin.as_mut().expect("matched origin");
    if origin.file_id != file.origin.file_id
      || origin.sha1 != file.origin.sha1
      || entry.sha256 != sha256
      || entry.size != file.file.file_length
      || entry.filename != file.file.file_name
    {
      return Err(Error::Conflict(
        "项目已安装其他文件版本，暂不支持自动升级或替换。".into(),
      ));
    }
    let direct = origin.direct || file.origin.direct;
    *origin = file.origin.clone();
    origin.direct = direct;
    entry.enabled = true;
    return Ok(());
  }
  if manifest.mods.iter().any(|entry| {
    entry.sha256 == sha256 || entry.filename.eq_ignore_ascii_case(&file.file.file_name)
  }) {
    return Err(Error::Conflict(
      "模组与已有引用的文件内容或文件名冲突，请先手动处理。".into(),
    ));
  }
  manifest.mods.push(ResourceEntry {
    source: ResourceSource::Curseforge,
    origin: Some(file.origin.clone()),
    sha256,
    filename: file.file.file_name.clone(),
    size: file.file.file_length,
    enabled: true,
  });
  Ok(())
}
impl ResourceManager {
  pub async fn plan_curseforge(
    &self,
    name: &str,
    preview: DependencyPreview,
    revision: u64,
  ) -> Result<ResourceInstallPlan> {
    if preview.items.is_empty() || preview.items.len() > 32 {
      return Err(Error::Invalid("invalid resource plan size".into()));
    }
    let name = name.to_ascii_lowercase();
    self
      .access(&name.clone(), move |directory, data, fabric| {
        if !fabric {
          return Err(Error::Conflict("模组安装需要 Fabric 实例。".into()));
        }
        let record = installed(directory)?;
        let before = read_manifest(directory)?;
        check_revision(&before, revision)?;
        let mut after = before.clone();
        after.schema_version = 3;
        let mut files = Vec::new();
        let mut summary = Vec::new();
        let mut projects = HashSet::new();
        let mut names = HashSet::new();
        for item in preview.items {
          if !crate::curseforge::compatible(&item.file, &record.version_id)
            || !projects.insert(item.project.id)
            || !names.insert(item.file.file_name.to_ascii_lowercase())
            || item.project.id != item.file.mod_id
            || item.file.game_id != 432
            || item.project.allow_mod_distribution == Some(false)
          {
            return Err(Error::Metadata(
              "invalid or duplicate project in resource plan".into(),
            ));
          }
          let origin = CurseForgeOrigin {
            project_id: item.project.id,
            file_id: item.file.id,
            project_name: item.project.name,
            sha1: upstream_hash(&item.file)?,
            direct: item.direct,
            required_projects: item
              .file
              .dependencies
              .iter()
              .filter(|d| d.relation_type == 3)
              .map(|d| d.mod_id)
              .collect(),
            incompatible_projects: item
              .file
              .dependencies
              .iter()
              .filter(|d| d.relation_type == 5)
              .map(|d| d.mod_id)
              .collect(),
          };
          validate_origin(&origin)?;
          let file = Planned {
            file: item.file,
            origin,
          };
          let reusable = lookup(data, &file.file)?;
          // Preflight all identity/name/dependency conflicts before starting transfers; placeholder hash never persists.
          let sha256 = reusable
            .as_ref()
            .map(|(hash, _)| hash.clone())
            .or_else(|| {
              after
                .mods
                .iter()
                .find(|e| {
                  e.origin
                    .as_ref()
                    .is_some_and(|o| o.project_id == file.origin.project_id)
                })
                .map(|e| e.sha256.clone())
            })
            .unwrap_or_else(|| {
              hex(&Sha256::digest(
                format!("curseforge:{}", file.file.id).as_bytes(),
              ))
            });
          merge(&mut after, &file, sha256)?;
          summary.push(ResourcePlanFile {
            project_id: file.origin.project_id,
            file_id: file.origin.file_id,
            project_name: file.origin.project_name.clone(),
            filename: file.file.file_name.clone(),
            size: file.file.file_length,
            direct: file.origin.direct,
            reuse: reusable.is_some(),
          });
          files.push(file);
        }
        validate_manifest(&after)?;
        Ok(ResourceInstallPlan {
          name,
          game: record.version_id,
          loader: record
            .loader
            .ok_or_else(|| Error::Conflict("Fabric loader is missing".into()))?,
          before,
          files,
          summary,
        })
      })
      .await
  }
  /// Cancel the token and await completion before releasing operation ownership.
  pub async fn install_curseforge(
    &self,
    plan: ResourceInstallPlan,
    cancel: CancellationToken,
    progress: watch::Sender<ResourceInstallProgress>,
  ) -> Result<ResourceManifest> {
    let check = plan.clone();
    self
      .access(&plan.name, move |directory, _, _| {
        checked_plan_state(directory, &check)
      })
      .await?;
    let mut objects = Vec::new();
    for file in &plan.files {
      if cancel.is_cancelled() {
        return Err(Error::Cancelled);
      }
      let object = self.prepare_shared(&file.file, &cancel, &progress).await?;
      objects.push(object);
      progress.send_modify(|p| {
        p.completed_files += 1;
        p.downloaded_bytes = plan
          .files
          .iter()
          .take(p.completed_files)
          .map(|f| f.file.file_length)
          .sum();
      });
    }
    if cancel.is_cancelled() {
      return Err(Error::Cancelled);
    }
    progress.send_modify(|p| p.stage = ResourceInstallStage::Publishing);
    let name = plan.name.clone();
    self
      .access(&name, move |directory, data, fabric| {
        checked_plan_state(directory, &plan)?;
        if cancel.is_cancelled() {
          return Err(Error::Cancelled);
        }
        let mut after = plan.before.clone();
        after.schema_version = 3;
        for (file, sha256) in plan.files.iter().zip(objects) {
          verify_object(&object_path(data, &sha256), &sha256, file.file.file_length)?;
          merge(&mut after, file, sha256)?;
        }
        validate_manifest(&after)?;
        commit_cancel(directory, data, &mut after, fabric, &cancel)?;
        Ok(after)
      })
      .await
  }
}
