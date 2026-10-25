//! Bounded public shader browse snapshots; installation resolves fresh metadata separately.
use super::*;
use crate::{
  Context,
  catalog_cache::{self, MODRINTH_SHADER},
};
use std::time::SystemTime;
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ShaderDiscoveryQuery {
  Search {
    game: Option<String>,
    text: String,
    index: u32,
  },
  Files {
    game: String,
    project: String,
  },
  Project {
    project: String,
  },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "page", rename_all = "snake_case")]
pub enum ShaderDiscoveryPage {
  Search(Page<Project>),
  Files(Page<ModFile>),
  Project(Project),
}
pub struct CachedShaderDiscovery {
  pub page: ShaderDiscoveryPage,
  pub fetched_at: SystemTime,
  pub persistence_error: Option<Error>,
}
impl ShaderDiscoveryQuery {
  pub fn identity(&self) -> Result<String> {
    match self {
      Self::Search { game, text, index } => {
        validate_search(game.as_deref(), text, *index)?;
      }
      Self::Files { game, project } => {
        query(game, 0)?;
        id(project)?;
      }
      Self::Project { project } => id(project)?,
    }
    Ok(serde_json::to_string(self)?)
  }
  fn validate_page(&self, page: &ShaderDiscoveryPage) -> Result<()> {
    self.identity()?;
    let valid = match (self, page) {
      (Self::Search { index, .. }, ShaderDiscoveryPage::Search(p)) => {
        p.items.len() <= 20
          && p
            .next_index
            .is_none_or(|next| next > *index && next <= 10000)
          && p
            .items
            .iter()
            .all(|item| valid_project(item, ProjectKind::Shader))
      }
      (Self::Project { project }, ShaderDiscoveryPage::Project(p)) => {
        p.id == *project && valid_project(p, ProjectKind::Shader)
      }
      (Self::Files { project, .. }, ShaderDiscoveryPage::Files(p)) => {
        p.items.len() <= 2048
          && p.next_index.is_none()
          && p
            .items
            .iter()
            .all(|f| &f.mod_id == project && valid_shader_file(f))
      }
      _ => false,
    };
    if !valid {
      return Err(Error::Metadata(
        "invalid Modrinth shader browse snapshot".into(),
      ));
    }
    Ok(())
  }
  pub async fn load_cached(&self, context: &Context) -> Result<Option<CachedShaderDiscovery>> {
    let key = self.identity()?;
    let Some(snapshot) =
      catalog_cache::load::<ShaderDiscoveryPage>(context, MODRINTH_SHADER, &key).await?
    else {
      return Ok(None);
    };
    self.validate_page(&snapshot.value)?;
    Ok(Some(CachedShaderDiscovery {
      page: snapshot.value,
      fetched_at: snapshot.fetched_at,
      persistence_error: None,
    }))
  }
  pub async fn refresh(
    &self,
    context: &Context,
    provider: &Modrinth,
  ) -> Result<CachedShaderDiscovery> {
    let key = self.identity()?;
    let page = match self {
      Self::Search { game, text, index } => ShaderDiscoveryPage::Search(
        provider
          .search_shaders(game.as_deref(), text, *index)
          .await?,
      ),
      Self::Project { project } => {
        ShaderDiscoveryPage::Project(provider.shader_project(project).await?)
      }
      Self::Files { game, project } => {
        ShaderDiscoveryPage::Files(provider.shader_files(game, project).await?)
      }
    };
    self.validate_page(&page)?;
    let fetched_at = SystemTime::now();
    let persistence_error =
      catalog_cache::save(context, MODRINTH_SHADER, &key, page.clone(), fetched_at)
        .await
        .err();
    Ok(CachedShaderDiscovery {
      page,
      fetched_at,
      persistence_error,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::Paths;

  #[tokio::test]
  async fn shader_search_snapshot_round_trips_through_its_own_cache() {
    let root = tempfile::tempdir().unwrap();
    let context = Context::new(Paths {
      data: root.path().join("data"),
      cache: root.path().join("cache"),
    })
    .unwrap();
    let project = project(
      ProjectKind::Shader,
      ProjectFields {
        id: "abcd1234".into(),
        name: "Shader".into(),
        summary: "summary".into(),
        authors: Vec::new(),
        icon_url: None,
        download_count: None,
        updated_at: None,
      },
    )
    .unwrap();
    let query = ShaderDiscoveryQuery::Search {
      game: Some("1.21.1".into()),
      text: "complementary".into(),
      index: 0,
    };
    let page = ShaderDiscoveryPage::Search(Page {
      items: vec![project.clone()],
      next_index: None,
    });
    query.validate_page(&page).unwrap();
    catalog_cache::save(
      &context,
      MODRINTH_SHADER,
      &query.identity().unwrap(),
      page,
      SystemTime::now(),
    )
    .await
    .unwrap();
    let cached = query.load_cached(&context).await.unwrap().unwrap();
    match cached.page {
      ShaderDiscoveryPage::Search(page) => {
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, project.id);
      }
      _ => panic!("unexpected shader cache page"),
    }
  }
}
