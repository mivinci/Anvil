//! Minecraft adapter; disk validation stays in Core, coordination is shared with other catalogues.
use super::catalog::{Catalog, CatalogData, Source};
use anvil_core::{Context, Error, metadata};
use std::time::Duration;
#[derive(Default)]
pub(super) struct Minecraft;
pub(super) type VersionCatalog = Catalog<Minecraft>;
impl Source for Minecraft {
  type Value = Vec<metadata::GameVersion>;
  const NAME: &'static str = "minecraft";
  const TTL: Duration = Duration::from_secs(1800);
  async fn load(&self, context: Context) -> Result<Option<CatalogData<Self::Value>>, Error> {
    Ok(
      metadata::load_cached_version_catalog(&context)
        .await?
        .map(|value| CatalogData {
          value: value.versions,
          fetched_at: value.fetched_at,
          persistence_error: value.persistence_error,
        }),
    )
  }
  async fn fetch(&self, context: Context) -> Result<CatalogData<Self::Value>, Error> {
    let value = metadata::refresh_version_catalog(&context).await?;
    Ok(CatalogData {
      value: value.versions,
      fetched_at: value.fetched_at,
      persistence_error: value.persistence_error,
    })
  }
  fn error(error: &Error) -> String {
    catalog_error(error)
  }
}

// Classify only public catalogue failures. Never include proxy URLs or raw request errors.
fn catalog_error(error: &anvil_core::Error) -> String {
  use anvil_core::{Error, error::HttpError};
  match error {
    Error::Http(HttpError::Timeout) => {
      "连接 Mojang 版本目录超时，请检查网络或代理后重试。（http_timeout）".into()
    }
    Error::Http(HttpError::Http(cause)) if cause.is_timeout() => {
      "读取 Mojang 版本目录超时，请稍后重试。（http_timeout）".into()
    }
    Error::Http(HttpError::Http(cause)) if cause.is_connect() => {
      "无法连接 Mojang 版本目录，请检查 DNS、代理或 HTTPS 连接后重试。（http_connect）".into()
    }
    Error::Http(HttpError::Status { code, .. }) => {
      format!("Mojang 版本目录返回 HTTP {code}，请稍后重试。")
    }
    Error::Http(_) => "读取 Mojang 版本目录响应失败，请检查网络或代理后重试。（http）".into(),
    Error::Metadata(_) => "官方版本目录校验失败，请重试或反馈此错误。（invalid_metadata）".into(),
    Error::Json(_) => {
      "官方版本目录不是有效的 JSON，可能收到代理错误页，请检查网络后重试。（invalid_json）".into()
    }
    _ => format!("无法更新版本目录，请稍后重试。（{}）", error.code()),
  }
}
