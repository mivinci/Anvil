//! Persist only validated public discovery DTOs. Transfer grants and credentials stay out of snapshots.
use super::*;
use crate::{
  Context,
  catalog_cache::{self, CURSEFORGE},
};
use std::time::SystemTime;
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiscoveryQuery {
  Search {
    game: String,
    text: String,
    index: u32,
  },
  Files {
    game: String,
    project: u32,
    index: u32,
  },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "page", rename_all = "snake_case")]
pub enum DiscoveryPage {
  Search(CatalogPage<Project>),
  Files(CatalogPage<ModFile>),
}
pub struct CachedDiscovery {
  pub page: DiscoveryPage,
  pub fetched_at: SystemTime,
  pub persistence_error: Option<Error>,
}
impl DiscoveryQuery {
  pub fn identity(&self) -> Result<String> {
    self.validate()?;
    Ok(serde_json::to_string(self)?)
  }
  fn validate(&self) -> Result<()> {
    match self {
      Self::Search { game, text, index } => {
        validate_query(game, *index)?;
        if text.len() > 128 || text.chars().any(char::is_control) {
          return Err(Error::Invalid("invalid search query".into()));
        }
      }
      Self::Files {
        game,
        project,
        index,
      } => {
        validate_query(game, *index)?;
        validate_id(*project)?;
      }
    }
    Ok(())
  }
  fn validate_page(&self, page: &DiscoveryPage) -> Result<()> {
    self.validate()?;
    let valid = match (self, page) {
      (Self::Search { index, .. }, DiscoveryPage::Search(page)) => {
        page.items.len() <= PAGE as usize
          && page
            .next_index
            .is_none_or(|next| next > *index && next <= 10000)
          && page.items.iter().all(|p| {
            p.id > 0 && p.game_id == 432 && p.class_id == Some(6) && p.authors.len() <= 128
          })
      }
      (
        Self::Files {
          game,
          project,
          index,
        },
        DiscoveryPage::Files(page),
      ) => {
        page.items.len() <= PAGE as usize
          && page
            .next_index
            .is_none_or(|next| next > *index && next <= 10000)
          && page.items.iter().all(|f| {
            f.mod_id == *project
              && f.game_id == 432
              && f.dependencies.len() <= 128
              && f.hashes.len() <= 8
              && compatible(f, game)
          })
      }
      _ => false,
    };
    if !valid {
      return Err(Error::Metadata(
        "invalid cached CurseForge catalogue".into(),
      ));
    }
    Ok(())
  }
  pub async fn load_cached(&self, context: &Context) -> Result<Option<CachedDiscovery>> {
    let key = self.identity()?;
    let Some(snapshot) = catalog_cache::load::<DiscoveryPage>(context, CURSEFORGE, &key).await?
    else {
      return Ok(None);
    };
    self.validate_page(&snapshot.value)?;
    Ok(Some(CachedDiscovery {
      page: snapshot.value,
      fetched_at: snapshot.fetched_at,
      persistence_error: None,
    }))
  }
  pub async fn refresh(&self, context: &Context, provider: &CurseForge) -> Result<CachedDiscovery> {
    let key = self.identity()?;
    let page = match self {
      Self::Search { game, text, index } => DiscoveryPage::Search(
        provider
          .search_with_refresh(game, text, *index, true)
          .await?,
      ),
      Self::Files {
        game,
        project,
        index,
      } => DiscoveryPage::Files(
        provider
          .files_with_refresh(game, *project, *index, true)
          .await?,
      ),
    };
    self.validate_page(&page)?;
    let fetched_at = SystemTime::now();
    let persistence_error =
      catalog_cache::save(context, CURSEFORGE, &key, page.clone(), fetched_at)
        .await
        .err();
    Ok(CachedDiscovery {
      page,
      fetched_at,
      persistence_error,
    })
  }
}
