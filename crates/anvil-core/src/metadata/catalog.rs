//! Minecraft catalogue adapter: validation is shared with the network metadata parser.
use super::{GameVersion, VersionManifest, version_manifest_url, versions_from_manifest};
use crate::{
  Context, Error, Result,
  catalog_cache::{self, MINECRAFT},
};
use std::time::SystemTime;
pub struct CachedVersionCatalog {
  pub versions: Vec<GameVersion>,
  pub fetched_at: SystemTime,
  pub persistence_error: Option<Error>,
}
pub async fn load_cached_version_catalog(
  context: &Context,
) -> Result<Option<CachedVersionCatalog>> {
  let Some(snapshot) =
    catalog_cache::load::<VersionManifest>(context, MINECRAFT, "versions").await?
  else {
    return Ok(None);
  };
  Ok(Some(CachedVersionCatalog {
    versions: versions_from_manifest(&snapshot.value)?,
    fetched_at: snapshot.fetched_at,
    persistence_error: None,
  }))
}
pub async fn refresh_version_catalog(context: &Context) -> Result<CachedVersionCatalog> {
  let manifest =
    super::fetch_version_manifest(&context.http.executor(), version_manifest_url()?).await?;
  let versions = versions_from_manifest(&manifest)?;
  let fetched_at = SystemTime::now();
  let persistence_error = catalog_cache::save(context, MINECRAFT, "versions", manifest, fetched_at)
    .await
    .err();
  Ok(CachedVersionCatalog {
    versions,
    fetched_at,
    persistence_error,
  })
}
