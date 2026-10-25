//! Public Minecraft news RSS adapter; refresh ownership and cache semantics are shared.
use super::catalog::{Catalog, CatalogData, Source};
use anvil_core::{Context, Error, news};
use std::time::Duration;

#[derive(Default)]
pub(super) struct MinecraftNews;
pub(super) type NewsCatalog = Catalog<MinecraftNews>;

impl Source for MinecraftNews {
  type Value = Vec<news::NewsItem>;
  const NAME: &'static str = "minecraft-news";
  const TTL: Duration = Duration::from_secs(6 * 60 * 60);

  async fn load(&self, context: Context) -> Result<Option<CatalogData<Self::Value>>, Error> {
    Ok(
      news::load_cached_news(&context)
        .await?
        .map(|value| CatalogData {
          value: value.items,
          fetched_at: value.fetched_at,
          persistence_error: value.persistence_error,
        }),
    )
  }

  async fn fetch(&self, context: Context) -> Result<CatalogData<Self::Value>, Error> {
    let value = news::refresh_news(&context).await?;
    Ok(CatalogData {
      value: value.items,
      fetched_at: value.fetched_at,
      persistence_error: value.persistence_error,
    })
  }

  fn error(error: &Error) -> String {
    // RSS failures are mapped to a generic UI message, but the upstream adapter
    // only stores bounded, non-sensitive metadata (for example HTTP status).
    tracing::warn!(reason = %error, "Minecraft news feed request failed");
    "暂时无法读取 Minecraft 官方新闻，请检查网络后重试。".into()
  }
}
