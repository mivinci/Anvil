//! Instance-local launch preferences, independent of the running-game lock.
use super::{ClientInstaller, client_instance_id, read_json_regular, validated_game_directory};
use crate::{CancellationToken, Error, Result, launch::Resolution, storage};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientLaunchSettings {
  pub schema_version: u32,
  pub revision: u64,
  pub max_memory_mib: u32,
  pub initial_memory_mib: Option<u32>,
  pub resolution: Option<Resolution>,
}
impl Default for ClientLaunchSettings {
  fn default() -> Self {
    Self {
      schema_version: 1,
      revision: 0,
      max_memory_mib: 2048,
      initial_memory_mib: None,
      resolution: None,
    }
  }
}
impl ClientLaunchSettings {
  pub fn validate(&self) -> Result<()> {
    if self.schema_version != 1 {
      return Err(Error::Invalid("不支持此启动配置版本。".into()));
    }
    if !(512..=131072).contains(&self.max_memory_mib) {
      return Err(Error::Invalid("最大内存须为 512–131072 MiB。".into()));
    }
    if let Some(initial) = self.initial_memory_mib
      && (!(128..=self.max_memory_mib).contains(&initial))
    {
      return Err(Error::Invalid(
        "初始内存须至少 128 MiB，且不能大于最大内存。".into(),
      ));
    }
    if let Some(value) = self.resolution {
      Resolution::new(value.width, value.height)?;
    }
    Ok(())
  }
}
fn load(path: &Path) -> Result<ClientLaunchSettings> {
  match fs::symlink_metadata(path) {
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
      Ok(ClientLaunchSettings::default())
    }
    Err(error) => Err(error.into()),
    Ok(_) => {
      let settings: ClientLaunchSettings = read_json_regular(path, 8192)?;
      settings.validate()?;
      Ok(settings)
    }
  }
}
impl ClientInstaller {
  pub async fn launch_settings(&self, name: &str) -> Result<ClientLaunchSettings> {
    let id = client_instance_id(name)?;
    let data = self.context.paths.data.clone();
    self
      .blocking(&CancellationToken::new(), move || {
        let game = validated_game_directory(&data, &id)?;
        load(
          &game
            .parent()
            .ok_or_else(|| Error::Invalid("invalid instance directory".into()))?
            .join("launch-settings.json"),
        )
      })
      .await
  }
  pub async fn save_launch_settings(
    &self,
    name: &str,
    settings: ClientLaunchSettings,
  ) -> Result<ClientLaunchSettings> {
    settings.validate()?;
    let id = client_instance_id(name)?;
    let data = self.context.paths.data.clone();
    self
      .blocking(&CancellationToken::new(), move || {
        let _lock = storage::client_settings_lock(&data, &id)?;
        let game = validated_game_directory(&data, &id)?;
        let path = game
          .parent()
          .ok_or_else(|| Error::Invalid("invalid instance directory".into()))?
          .join("launch-settings.json");
        let current = load(&path)?;
        if current.revision != settings.revision {
          return Err(Error::Conflict(
            "设置已被其他操作修改，请重新读取后再保存。".into(),
          ));
        }
        let mut saved = settings;
        saved.revision = current
          .revision
          .checked_add(1)
          .ok_or_else(|| Error::Invalid("设置版本已达到上限。".into()))?;
        storage::replace_json(&path, &saved)?;
        Ok(saved)
      })
      .await
  }
}
