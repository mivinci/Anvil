//! Bounded public browse snapshots; installation resolves fresh metadata separately.
use super::*;
use crate::{
  Context,
  catalog_cache::{self, MODRINTH},
};
use std::time::SystemTime;
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiscoveryQuery {
  Search {
    game: Option<String>,
    text: String,
    index: u32,
  },
  Files {
    game: String,
    project: String,
    index: u32,
  },
  Project {
    project: String,
  },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "page", rename_all = "snake_case")]
pub enum DiscoveryPage {
  Search(Page<Project>),
  Files(Page<ModFile>),
  Project(Project),
}
pub struct CachedDiscovery {
  pub page: DiscoveryPage,
  pub fetched_at: SystemTime,
  pub persistence_error: Option<Error>,
}
impl DiscoveryQuery {
  pub fn identity(&self) -> Result<String> {
    match self {
      Self::Search { game, text, index } => {
        validate_search(game.as_deref(), text, *index)?;
      }
      Self::Files {
        game,
        project,
        index,
      } => {
        query(game, *index)?;
        id(project)?;
      }
      Self::Project { project } => id(project)?,
    }
    // The upstream version endpoint returns a complete filtered list. Share it
    // across UI pages instead of downloading it again for each page number.
    let identity = match self {
      Self::Files { game, project, .. } => Self::Files {
        game: game.clone(),
        project: project.clone(),
        index: 0,
      },
      other => other.clone(),
    };
    Ok(serde_json::to_string(&identity)?)
  }
  fn validate_page(&self, page: &DiscoveryPage) -> Result<()> {
    self.identity()?;
    let valid = match (self, page) {
      (Self::Search { index, .. }, DiscoveryPage::Search(p)) => {
        p.items.len() <= 20
          && p
            .next_index
            .is_none_or(|next| next > *index && next <= 10000)
          && p.items.iter().all(valid_project)
      }
      (Self::Project { project }, DiscoveryPage::Project(p)) => {
        p.id == *project && valid_project(p)
      }
      (Self::Files { project, .. }, DiscoveryPage::Files(p)) => {
        p.items.len() <= 2048
          && p.next_index.is_none()
          && p
            .items
            .iter()
            .all(|f| &f.mod_id == project && valid_file(f))
      }
      _ => false,
    };
    if !valid {
      return Err(Error::Metadata("invalid Modrinth browse snapshot".into()));
    }
    Ok(())
  }
  pub async fn load_cached(&self, context: &Context) -> Result<Option<CachedDiscovery>> {
    let key = self.identity()?;
    let Some(snapshot) = catalog_cache::load::<DiscoveryPage>(context, MODRINTH, &key).await?
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
  pub async fn refresh(&self, context: &Context, provider: &Modrinth) -> Result<CachedDiscovery> {
    let key = self.identity()?;
    let page = match self {
      Self::Search { game, text, index } => DiscoveryPage::Search(
        provider
          .search_catalog(game.as_deref(), text, *index)
          .await?,
      ),
      Self::Project { project } => DiscoveryPage::Project(provider.project(project).await?),
      Self::Files { game, project, .. } => {
        DiscoveryPage::Files(provider.browse_files(game, project).await?)
      }
    };
    self.validate_page(&page)?;
    let fetched_at = SystemTime::now();
    let persistence_error = catalog_cache::save(context, MODRINTH, &key, page.clone(), fetched_at)
      .await
      .err();
    Ok(CachedDiscovery {
      page,
      fetched_at,
      persistence_error,
    })
  }
}
