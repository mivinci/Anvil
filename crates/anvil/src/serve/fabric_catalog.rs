//! Per-game Fabric compatibility catalogues, with bounded shared refresh ownership.
use super::catalog::{Catalog, CatalogData, Source};
use anvil_core::{Context, Error, fabric};
use std::{
  collections::VecDeque,
  sync::{Arc, Mutex},
  time::Duration,
};
struct Fabric {
  game: String,
}
impl Source for Fabric {
  type Value = Vec<fabric::FabricVersion>;
  const NAME: &'static str = "fabric";
  const TTL: Duration = Duration::from_secs(1800);
  async fn load(&self, context: Context) -> Result<Option<CatalogData<Self::Value>>, Error> {
    Ok(
      fabric::load_cached_versions(&context, &self.game)
        .await?
        .map(|value| CatalogData {
          value: value.versions,
          fetched_at: value.fetched_at,
          persistence_error: value.persistence_error,
        }),
    )
  }
  async fn fetch(&self, context: Context) -> Result<CatalogData<Self::Value>, Error> {
    let value = fabric::refresh_versions(&context, &self.game).await?;
    Ok(CatalogData {
      value: value.versions,
      fetched_at: value.fetched_at,
      persistence_error: value.persistence_error,
    })
  }
  fn error(_: &Error) -> String {
    "无法更新 Fabric 版本目录，请稍后重试。".into()
  }
}
#[derive(Default)]
pub(super) struct FabricCatalog(Mutex<VecDeque<(String, Arc<Catalog<Fabric>>)>>);
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Response {
  versions: Vec<fabric::FabricVersion>,
  stale: bool,
  fresh_for_ms: u64,
  revision: u64,
  refresh_error: Option<String>,
}
impl FabricCatalog {
  pub async fn get(
    &self,
    context: Context,
    game: String,
    force: bool,
    after: Option<u64>,
  ) -> Result<Response, String> {
    if !anvil_core::metadata::is_valid_version_id(&game) {
      return Err("游戏版本无效。".into());
    }
    let entry = {
      let mut entries = self
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      if let Some(index) = entries.iter().position(|(key, _)| key == &game) {
        let item = entries.remove(index).expect("entry exists");
        let entry = item.1.clone();
        entries.push_back(item);
        entry
      } else {
        if entries.len() >= 8 {
          let index = entries
            .iter()
            .position(|(_, entry)| Arc::strong_count(entry) == 1)
            .ok_or("Fabric 目录正在更新，请稍后重试。")?;
          entries.remove(index);
        }
        let entry = Arc::new(Catalog::new(Fabric { game: game.clone() }));
        entries.push_back((game, entry.clone()));
        entry
      }
    };
    let value = entry.get(context, force, after).await?;
    Ok(Response {
      versions: value.value,
      stale: value.stale,
      fresh_for_ms: value.fresh_for_ms,
      revision: value.revision,
      refresh_error: value.refresh_error,
    })
  }
}
