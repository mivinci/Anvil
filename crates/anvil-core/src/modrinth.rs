//! Public Modrinth v2 metadata. No credential, transfer, or instance mutation.
mod catalog;
use crate::{Error, Result};
pub use catalog::{CachedDiscovery, DiscoveryPage, DiscoveryQuery};
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
  collections::{BTreeMap, BTreeSet, VecDeque},
  time::Duration,
};
use tokio::{sync::Mutex, time::Instant};

pub struct Modrinth {
  client: Client,
  next_request: Mutex<Instant>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
  pub id: String,
  pub name: String,
  pub summary: String,
  pub authors: Vec<Author>,
  pub links: Links,
  #[serde(default)]
  pub icon_url: Option<String>,
  #[serde(default)]
  pub download_count: Option<u64>,
  #[serde(default)]
  pub updated_at: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Author {
  pub name: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Links {
  pub website_url: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModFile {
  pub id: String,
  pub mod_id: String,
  pub display_name: String,
  pub file_name: String,
  pub file_length: u64,
  pub file_date: String,
  pub release_type: u32,
  pub sha1: String,
  #[serde(skip)]
  pub(crate) url: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
  pub items: Vec<T>,
  pub next_index: Option<u32>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewItem {
  pub project: Project,
  pub file: ModFile,
  pub direct: bool,
  #[serde(skip)]
  pub(crate) required: BTreeMap<String, String>,
  #[serde(skip)]
  pub(crate) incompatible: BTreeMap<String, Option<String>>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyPreview {
  pub items: Vec<PreviewItem>,
  pub total_bytes: u64,
  pub optional_projects: Vec<String>,
  pub warnings: Vec<String>,
}
#[derive(Deserialize)]
struct SearchResponse {
  hits: Vec<Hit>,
  total_hits: u32,
  offset: u32,
}
#[derive(Deserialize)]
struct Hit {
  project_id: String,
  title: String,
  description: String,
  author: String,
  project_type: String,
  client_side: String,
  #[serde(default)]
  icon_url: Option<String>,
  downloads: Option<u64>,
  date_modified: Option<String>,
}
#[derive(Deserialize)]
struct RawProject {
  id: String,
  title: String,
  description: String,
  project_type: String,
  client_side: String,
  #[serde(default)]
  icon_url: Option<String>,
  downloads: Option<u64>,
  updated: Option<String>,
}
#[derive(Clone, Deserialize)]
struct Version {
  id: String,
  project_id: String,
  name: String,
  date_published: String,
  version_type: String,
  status: String,
  game_versions: Vec<String>,
  loaders: Vec<String>,
  files: Vec<File>,
  dependencies: Vec<Dependency>,
}
#[derive(Clone, Deserialize)]
struct File {
  url: String,
  filename: String,
  primary: bool,
  size: u64,
  hashes: BTreeMap<String, String>,
}
#[derive(Clone, Deserialize)]
struct Dependency {
  project_id: Option<String>,
  version_id: Option<String>,
  dependency_type: String,
}
pub(crate) fn valid_id(id: &str) -> bool {
  id.len() == 8 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}
fn id(id: &str) -> Result<()> {
  if valid_id(id) {
    Ok(())
  } else {
    Err(Error::Invalid("invalid Modrinth ID".into()))
  }
}
fn query(game: &str, index: u32) -> Result<()> {
  if game.is_empty() || game.len() > 128 || game.chars().any(char::is_control) || index > 10000 {
    return Err(Error::Invalid("invalid Modrinth query".into()));
  }
  Ok(())
}
fn validate_search(game: Option<&str>, text: &str, index: u32) -> Result<()> {
  if game.is_some_and(|g| !crate::metadata::is_valid_version_id(g))
    || index > 10000
    || text.len() > 128
    || text.chars().any(char::is_control)
  {
    return Err(Error::Invalid("invalid Modrinth search query".into()));
  }
  Ok(())
}
fn valid_project(p: &Project) -> bool {
  valid_id(&p.id)
    && !p.name.is_empty()
    && p.name.len() <= 512
    && p.summary.len() <= 8192
    && p.authors.len() <= 128
    && p.authors.iter().all(|a| a.name.len() <= 512)
    && p.links.website_url == format!("https://modrinth.com/mod/{}", p.id)
    && p.icon_url.as_deref().is_none_or(valid_icon_url)
    && p.updated_at.as_ref().is_none_or(|value| value.len() <= 64)
}
fn valid_icon_url(value: &str) -> bool {
  crate::catalog_cache::valid_image_url(value, &["cdn.modrinth.com"])
}
pub(crate) fn valid_file(file: &ModFile) -> bool {
  valid_id(&file.id)
    && valid_id(&file.mod_id)
    && !file.display_name.is_empty()
    && file.display_name.len() <= 512
    && file.file_name.len() <= 128
    && file.file_name.ends_with(".jar")
    && !file.file_name.contains(['/', '\\', ':'])
    && file.file_length > 0
    && file.file_length <= crate::resources::MAX_IMPORT_BYTES as u64
    && file.sha1.len() == 40
    && file
      .sha1
      .bytes()
      .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn project(
  id: String,
  name: String,
  summary: String,
  authors: Vec<Author>,
  icon_url: Option<String>,
  download_count: Option<u64>,
  updated_at: Option<String>,
) -> Result<Project> {
  if !valid_id(&id)
    || name.is_empty()
    || name.len() > 512
    || summary.len() > 8192
    || authors.len() > 128
  {
    return Err(Error::Metadata("invalid Modrinth project".into()));
  }
  Ok(Project {
    links: Links {
      website_url: format!("https://modrinth.com/mod/{id}"),
    },
    id,
    name,
    summary,
    authors,
    icon_url: icon_url.filter(|value| valid_icon_url(value)),
    download_count,
    updated_at: updated_at.filter(|value| value.len() <= 64),
  })
}
impl Version {
  fn file(&self, game: &str) -> Result<ModFile> {
    id(&self.id)?;
    id(&self.project_id)?;
    if self.status != "listed"
      || !self.loaders.iter().any(|v| v == "fabric")
      || !self.game_versions.iter().any(|v| v == game)
      || self.files.len() > 64
      || self.dependencies.len() > 128
    {
      return Err(Error::Metadata(
        "Modrinth 版本不适用于此 Minecraft/Fabric 实例。".into(),
      ));
    }
    let jars: Vec<_> = self
      .files
      .iter()
      .filter(|f| f.filename.ends_with(".jar"))
      .collect();
    let primary: Vec<_> = jars.iter().copied().filter(|f| f.primary).collect();
    let file = match (primary.as_slice(), jars.as_slice()) {
      ([file], _) | ([], [file]) => *file,
      _ => {
        return Err(Error::Metadata(
          "Modrinth 版本没有唯一的主 JAR 文件。".into(),
        ));
      }
    };
    let url = url::Url::parse(&file.url)
      .map_err(|_| Error::Metadata("invalid Modrinth file URL".into()))?;
    if url.scheme() != "https"
      || url.host_str() != Some("cdn.modrinth.com")
      || !url.username().is_empty()
      || url.password().is_some()
      || url.port().is_some()
    {
      return Err(Error::Metadata("invalid Modrinth CDN URL".into()));
    }
    let file = ModFile {
      id: self.id.clone(),
      mod_id: self.project_id.clone(),
      display_name: self.name.clone(),
      file_name: file.filename.clone(),
      file_length: file.size,
      file_date: self.date_published.clone(),
      release_type: match self.version_type.as_str() {
        "release" => 1,
        "beta" => 2,
        "alpha" => 3,
        _ => return Err(Error::Metadata("unknown Modrinth release type".into())),
      },
      sha1: file
        .hashes
        .get("sha1")
        .cloned()
        .unwrap_or_default()
        .to_ascii_lowercase(),
      url: file.url.clone(),
    };
    if !valid_file(&file) {
      return Err(Error::Metadata("invalid Modrinth file".into()));
    }
    Ok(file)
  }
}
impl Modrinth {
  pub fn new() -> Result<Self> {
    let client = Client::builder()
      .user_agent(concat!(
        "mivinci/Anvil/",
        env!("CARGO_PKG_VERSION"),
        " (https://github.com/mivinci/Anvil)"
      ))
      .redirect(reqwest::redirect::Policy::none())
      .connect_timeout(Duration::from_secs(5))
      .timeout(Duration::from_secs(12))
      .build()
      .map_err(|_| Error::Metadata("Modrinth HTTP initialization failed".into()))?;
    Ok(Self {
      client,
      next_request: Mutex::new(Instant::now()),
    })
  }
  async fn get<T: DeserializeOwned>(&self, path: &str, params: &[(&str, String)]) -> Result<T> {
    let mut url =
      url::Url::parse(&format!("https://api.modrinth.com/v2/{path}")).expect("fixed URL");
    url
      .query_pairs_mut()
      .extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str())));
    // Bound request rate across search, version browsing, and dependency resolution.
    let mut next = self.next_request.lock().await;
    if *next > Instant::now() + Duration::from_secs(2) {
      return Err(Error::Metadata("Modrinth 请求受限，请稍后重试。".into()));
    }
    tokio::time::sleep_until(*next).await;
    *next = Instant::now() + Duration::from_millis(250);
    let mut response = self
      .client
      .get(url)
      .send()
      .await
      .map_err(|_| Error::Metadata("无法连接 Modrinth，请稍后重试。".into()))?;
    if response.status().as_u16() == 429 {
      let delay = response
        .headers()
        .get("retry-after")
        .or_else(|| response.headers().get("x-ratelimit-reset"))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60)
        .clamp(1, 300);
      *next = Instant::now() + Duration::from_secs(delay);
    }
    if !response.status().is_success() {
      return Err(Error::Metadata(
        match response.status().as_u16() {
          404 => "Modrinth 项目或版本不存在。",
          429 => "Modrinth 请求过于频繁，请稍后重试。",
          _ => "Modrinth 服务暂时不可用。",
        }
        .into(),
      ));
    }
    drop(next);
    let mut bytes = Vec::new();
    while let Some(chunk) = response
      .chunk()
      .await
      .map_err(|_| Error::Metadata("Modrinth 响应中断。".into()))?
    {
      if bytes.len().saturating_add(chunk.len()) > 4 * 1024 * 1024 {
        return Err(Error::Metadata("Modrinth response exceeds limit".into()));
      }
      bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| Error::Metadata("Modrinth 响应格式无效。".into()))
  }
  pub async fn search(&self, game: &str, text: &str, index: u32) -> Result<Page<Project>> {
    self.search_catalog(Some(game), text, index).await
  }
  pub async fn search_catalog(
    &self,
    game: Option<&str>,
    text: &str,
    index: u32,
  ) -> Result<Page<Project>> {
    validate_search(game, text, index)?;
    let mut facets = vec![
      vec!["project_type:mod".to_owned()],
      vec!["categories:fabric".to_owned()],
      vec![
        "client_side:required".to_owned(),
        "client_side:optional".to_owned(),
      ],
    ];
    if let Some(game) = game {
      facets.push(vec![format!("versions:{game}")]);
    }
    let facets = serde_json::to_string(&facets)?;
    let result: SearchResponse = self
      .get(
        "search",
        &[
          ("query", text.into()),
          ("facets", facets),
          ("index", "downloads".into()),
          ("offset", index.to_string()),
          ("limit", "20".into()),
        ],
      )
      .await?;
    if result.hits.len() > 20 || result.offset != index {
      return Err(Error::Metadata("invalid Modrinth page".into()));
    }
    let count = result.hits.len() as u32;
    let mut items = Vec::new();
    for h in result.hits {
      if h.project_type != "mod" || h.client_side == "unsupported" {
        return Err(Error::Metadata("invalid Modrinth search match".into()));
      }
      items.push(project(
        h.project_id,
        h.title,
        h.description,
        vec![Author { name: h.author }],
        h.icon_url,
        h.downloads,
        h.date_modified,
      )?);
    }
    let next = index.saturating_add(count);
    Ok(Page {
      items,
      next_index: (count > 0 && next < result.total_hits && next <= 10000).then_some(next),
    })
  }
  async fn versions(&self, game: &str, project: &str) -> Result<Vec<Version>> {
    query(game, 0)?;
    id(project)?;
    let versions: Vec<Version> = self
      .get(
        &format!("project/{project}/version"),
        &[
          ("loaders", "[\"fabric\"]".into()),
          ("game_versions", serde_json::to_string(&[game])?),
        ],
      )
      .await?;
    if versions.len() > 2048 {
      return Err(Error::Metadata("too many Modrinth versions".into()));
    }
    if versions.iter().any(|v| v.project_id != project) {
      return Err(Error::Metadata("Modrinth project identity mismatch".into()));
    }
    Ok(versions)
  }
  pub async fn files(&self, game: &str, project: &str, index: u32) -> Result<Page<ModFile>> {
    query(game, index)?;
    let mut page = self.browse_files(game, project).await?;
    page.items = page
      .items
      .into_iter()
      .skip(index as usize)
      .take(21)
      .collect();
    page.next_index = (page.items.len() > 20).then_some(index + 20);
    page.items.truncate(20);
    Ok(page)
  }
  async fn browse_files(&self, game: &str, project: &str) -> Result<Page<ModFile>> {
    let mut versions = self.versions(game, project).await?;
    versions.sort_by(|a, b| {
      b.date_published
        .cmp(&a.date_published)
        .then(a.id.cmp(&b.id))
    });
    let items: Vec<_> = versions.iter().filter_map(|v| v.file(game).ok()).collect();
    Ok(Page {
      items,
      next_index: None,
    })
  }
  async fn version(&self, version: &str) -> Result<Version> {
    id(version)?;
    let result: Version = self.get(&format!("version/{version}"), &[]).await?;
    if result.id != version {
      return Err(Error::Metadata("Modrinth version identity mismatch".into()));
    }
    Ok(result)
  }
  pub async fn project(&self, project_id: &str) -> Result<Project> {
    id(project_id)?;
    let p: RawProject = self.get(&format!("project/{project_id}"), &[]).await?;
    if p.id != project_id || p.project_type != "mod" || p.client_side == "unsupported" {
      return Err(Error::Metadata(
        "Modrinth 项目不支持客户端模组安装。".into(),
      ));
    }
    project(
      p.id,
      p.title,
      p.description,
      Vec::new(),
      p.icon_url,
      p.downloads,
      p.updated,
    )
  }
  pub async fn preview(
    &self,
    game: &str,
    project: &str,
    version: &str,
    existing: &BTreeMap<String, String>,
  ) -> Result<DependencyPreview> {
    query(game, 0)?;
    id(project)?;
    let first = self.version(version).await?;
    if first.project_id != project {
      return Err(Error::Metadata(
        "Modrinth version belongs to another project".into(),
      ));
    }
    let mut queue = VecDeque::from([(first, true)]);
    let mut selected = BTreeMap::<String, PreviewItem>::new();
    let mut optional = BTreeSet::new();
    while let Some((version, direct)) = queue.pop_front() {
      if let Some(chosen) = selected.get(&version.project_id) {
        if chosen.file.id != version.id {
          return Err(Error::Conflict(
            "Modrinth 必需依赖要求不同版本，请手动处理。".into(),
          ));
        }
        continue;
      }
      if selected.len() >= 32 {
        return Err(Error::Metadata("Modrinth 依赖超过 32 个项目上限。".into()));
      }
      let file = version.file(game)?;
      let project = self.project(&version.project_id).await?;
      let mut required = BTreeMap::new();
      let mut incompatible = BTreeMap::new();
      for dep in version.dependencies {
        if dep.dependency_type == "embedded" {
          continue;
        }
        if dep.dependency_type == "optional" {
          if let Some(p) = dep.project_id {
            id(&p)?;
            optional.insert(p);
          } else if let Some(v) = dep.version_id {
            optional.insert(self.version(&v).await?.project_id);
          }
          if optional.len() > 128 {
            return Err(Error::Metadata(
              "too many optional Modrinth dependencies".into(),
            ));
          }
          continue;
        }
        if dep.dependency_type != "required" && dep.dependency_type != "incompatible" {
          return Err(Error::Metadata("unknown Modrinth dependency type".into()));
        }
        if dep.dependency_type == "incompatible" {
          let pinned = if let Some(v) = &dep.version_id {
            Some(self.version(v).await?)
          } else {
            None
          };
          let p = if let Some(p) = dep.project_id {
            id(&p)?;
            if pinned.as_ref().is_some_and(|v| v.project_id != p) {
              return Err(Error::Metadata(
                "incompatible dependency identity mismatch".into(),
              ));
            }
            p
          } else if let Some(v) = pinned {
            v.project_id
          } else {
            return Err(Error::Metadata("unresolved incompatible dependency".into()));
          };
          if let Some(v) = &dep.version_id {
            id(v)?;
          }
          if incompatible.insert(p, dep.version_id).is_some() {
            return Err(Error::Metadata("duplicate incompatible dependency".into()));
          }
          continue;
        }
        let target = if let Some(v) = &dep.version_id {
          self.version(v).await?
        } else if let Some(p) = &dep.project_id {
          id(p)?;
          if let Some(chosen) = selected.get(p) {
            self.version(&chosen.file.id).await?
          } else if let Some(v) = existing.get(p) {
            self.version(v).await?
          } else {
            let mut versions = self.versions(game, p).await?;
            versions.sort_by(|a, b| {
              b.date_published
                .cmp(&a.date_published)
                .then(a.id.cmp(&b.id))
            });
            versions
              .into_iter()
              .find(|v| v.file(game).is_ok())
              .ok_or_else(|| Error::Metadata("必需依赖没有兼容的 Fabric 版本。".into()))?
          }
        } else {
          return Err(Error::Metadata(
            "Modrinth 必需依赖缺少项目或版本标识。".into(),
          ));
        };
        if dep
          .project_id
          .as_ref()
          .is_some_and(|p| p != &target.project_id)
        {
          return Err(Error::Metadata(
            "dependency project identity mismatch".into(),
          ));
        }
        target.file(game)?;
        if existing
          .get(&target.project_id)
          .is_some_and(|v| v != &target.id)
        {
          return Err(Error::Conflict(
            "已安装的 Modrinth 依赖版本不符合要求，暂不自动替换。".into(),
          ));
        }
        if required
          .insert(target.project_id.clone(), target.id.clone())
          .is_some()
        {
          return Err(Error::Metadata("duplicate required dependency".into()));
        }
        if queue.len() >= 128 {
          return Err(Error::Metadata(
            "Modrinth dependency queue exceeds limit".into(),
          ));
        }
        queue.push_back((target, false));
      }
      selected.insert(
        project.id.clone(),
        PreviewItem {
          project,
          file,
          direct,
          required,
          incompatible,
        },
      );
    }
    let items: Vec<_> = selected.into_values().collect();
    Ok(DependencyPreview {
      total_bytes: items.iter().map(|i| i.file.file_length).sum(),
      items,
      optional_projects: optional.into_iter().collect(),
      warnings: vec!["仅安装兼容的 Fabric 主 JAR 和必需依赖；不自动升级或替换已有项目。".into()],
    })
  }
}
