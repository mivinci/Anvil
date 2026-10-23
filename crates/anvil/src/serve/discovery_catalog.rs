//! Bounded public CurseForge discovery; installation resolution never consumes these stale pages.
use super::catalog::{Catalog, CatalogData, Source};
use anvil_core::{
  Context, Error,
  curseforge::{CurseForge, DiscoveryPage, DiscoveryQuery},
};
use std::{
  collections::VecDeque,
  sync::{Arc, Mutex},
  time::Duration,
};
struct Discovery {
  query: DiscoveryQuery,
  provider: Arc<CurseForge>,
}
impl Source for Discovery {
  type Value = DiscoveryPage;
  const NAME: &'static str = "curseforge";
  const TTL: Duration = Duration::from_secs(300);
  async fn load(&self, context: Context) -> Result<Option<CatalogData<Self::Value>>, Error> {
    Ok(
      self
        .query
        .load_cached(&context)
        .await?
        .map(|v| CatalogData {
          value: v.page,
          fetched_at: v.fetched_at,
          persistence_error: v.persistence_error,
        }),
    )
  }
  async fn fetch(&self, context: Context) -> Result<CatalogData<Self::Value>, Error> {
    let v = self.query.refresh(&context, &self.provider).await?;
    Ok(CatalogData {
      value: v.page,
      fetched_at: v.fetched_at,
      persistence_error: v.persistence_error,
    })
  }
  fn error(_: &Error) -> String {
    "无法更新 CurseForge 目录，请稍后重试。".into()
  }
}
#[derive(Default)]
pub(super) struct DiscoveryCatalog(Mutex<VecDeque<(String, Arc<Catalog<Discovery>>)>>);
impl DiscoveryCatalog {
  pub async fn get(
    &self,
    context: Context,
    provider: Arc<CurseForge>,
    query: DiscoveryQuery,
    force: bool,
    after: Option<u64>,
  ) -> Result<serde_json::Value, String> {
    let key = query.identity().map_err(|_| "CurseForge 查询无效。")?;
    let entry = {
      let mut entries = self
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      if let Some(index) = entries.iter().position(|(id, _)| id == &key) {
        let item = entries.remove(index).expect("entry exists");
        let entry = item.1.clone();
        entries.push_back(item);
        entry
      } else {
        if entries.len() >= 16 {
          let index = entries
            .iter()
            .position(|(_, entry)| Arc::strong_count(entry) == 1)
            .ok_or("CurseForge 目录正在更新，请稍后重试。")?;
          entries.remove(index);
        }
        let entry = Arc::new(Catalog::new(Discovery { query, provider }));
        entries.push_back((key, entry.clone()));
        entry
      }
    };
    let value = entry.get(context, force, after).await?;
    let mut page = match value.value {
      DiscoveryPage::Search(page) => serde_json::to_value(page),
      DiscoveryPage::Files(page) => serde_json::to_value(page),
    }
    .map_err(|_| "目录响应无法编码。")?;
    let fields = page.as_object_mut().ok_or("目录响应无效。")?;
    fields.insert("stale".into(), value.stale.into());
    fields.insert("revision".into(), value.revision.into());
    fields.insert("freshForMs".into(), value.fresh_for_ms.into());
    fields.insert("refreshError".into(), value.refresh_error.into());
    Ok(page)
  }
}
