//! Export metadata contract. File collection and ZIP publication are separate.
use crate::{Error, Result, client::ClientInstallRecord, fabric::ClientLoader};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ExportSelection {
  pub mods: bool,
  pub resource_packs: bool,
  pub shader_packs: bool,
  pub config: bool,
  pub worlds: bool,
}
impl Default for ExportSelection {
  fn default() -> Self {
    Self {
      mods: true,
      resource_packs: true,
      shader_packs: true,
      config: true,
      worlds: false,
    }
  }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportOptions {
  pub name: String,
  pub version_id: String,
  #[serde(default)]
  pub selection: ExportSelection,
}
impl ExportOptions {
  pub fn validate(&self) -> Result<()> {
    if self.name.trim().is_empty()
      || self.name.len() > 256
      || self.name.chars().any(char::is_control)
    {
      return Err(Error::Invalid("请输入有效的整合包名称".into()));
    }
    if self.version_id.trim().is_empty()
      || self.version_id.len() > 128
      || self.version_id.chars().any(char::is_control)
    {
      return Err(Error::Invalid("请输入有效的整合包版本".into()));
    }
    let selected = &self.selection;
    if !(selected.mods
      || selected.resource_packs
      || selected.shader_packs
      || selected.config
      || selected.worlds)
    {
      return Err(Error::Invalid("请至少选择一种导出内容".into()));
    }
    Ok(())
  }
}

/// A self-contained pack places its selected files under `overrides/` instead
/// of publishing credentials, stale download URLs or inferred provider identities.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportIndex {
  format_version: u32,
  game: &'static str,
  version_id: String,
  name: String,
  files: Vec<()>,
  dependencies: BTreeMap<String, String>,
}

pub fn prepare_export_index(
  record: &ClientInstallRecord,
  options: &ExportOptions,
) -> Result<ExportIndex> {
  options.validate()?;
  crate::client::validate_install_record(record)?;
  let mut dependencies = BTreeMap::from([("minecraft".into(), record.version_id.clone())]);
  if let Some(ClientLoader::Fabric { version }) = &record.loader {
    dependencies.insert("fabric-loader".into(), version.clone());
  }
  Ok(ExportIndex {
    format_version: 1,
    game: "minecraft",
    version_id: options.version_id.clone(),
    name: options.name.clone(),
    files: Vec::new(),
    dependencies,
  })
}

#[cfg(test)]
#[path = "../../tests/modpack_export.rs"]
mod tests;
