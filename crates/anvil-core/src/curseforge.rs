//! Read-only CurseForge discovery and bounded dependency previews. No file transfer or mutation.
mod catalog;
use crate::{Error, Result};
pub use catalog::{CachedDiscovery, DiscoveryPage, DiscoveryQuery};
use reqwest::{Client, header::HeaderValue};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
  collections::{BTreeMap, HashSet, VecDeque},
  time::{Duration, Instant},
};
use tokio::sync::Mutex;
use url::Url;

const LIMIT: usize = 2 * 1024 * 1024;
const PAGE: u32 = 20;
const MAX_NODES: usize = 32;

// Deliberately no Debug: the provider owns a credential.
pub struct CurseForge {
  client: Client,
  key: Option<HeaderValue>,
  cache: Mutex<VecDeque<(String, Instant, Vec<u8>)>>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
  pub id: u32,
  pub game_id: u32,
  pub class_id: Option<u32>,
  pub name: String,
  pub summary: String,
  pub authors: Vec<Author>,
  pub links: ProjectLinks,
  pub allow_mod_distribution: Option<bool>,
  #[serde(default)]
  pub logo: Option<ProjectLogo>,
  #[serde(default)]
  pub download_count: Option<u64>,
  #[serde(default)]
  pub date_modified: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLogo {
  pub thumbnail_url: Option<String>,
  pub url: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLinks {
  pub website_url: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Author {
  pub name: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModFile {
  pub id: u32,
  pub mod_id: u32,
  pub game_id: u32,
  pub display_name: String,
  pub file_name: String,
  pub file_length: u64,
  pub file_date: String,
  pub release_type: u32,
  pub is_available: bool,
  #[serde(default)]
  pub is_server_pack: Option<bool>,
  #[serde(default)]
  pub is_early_access_content: Option<bool>,
  pub game_versions: Vec<String>,
  pub hashes: Vec<FileHash>,
  pub dependencies: Vec<Dependency>,
  // Never expose transfer URLs to the UI; absence can reflect distribution restrictions.
  #[serde(default, skip_serializing)]
  pub(crate) download_url: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct FileHash {
  pub algo: u32,
  pub value: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
  pub mod_id: u32,
  pub relation_type: u32,
}
#[derive(Deserialize)]
struct Envelope<T> {
  data: T,
}
#[derive(Deserialize)]
struct UpstreamPage<T> {
  data: Vec<T>,
  pagination: Pagination,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pagination {
  index: u32,
  result_count: u32,
  total_count: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPage<T> {
  pub items: Vec<T>,
  pub next_index: Option<u32>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewItem {
  pub project: Project,
  pub file: ModFile,
  pub direct: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyPreview {
  pub items: Vec<PreviewItem>,
  pub total_bytes: u64,
  pub optional_projects: Vec<u32>,
  pub warnings: Vec<String>,
}
impl CurseForge {
  pub fn new(key: Option<String>) -> Result<Self> {
    let key = key
      .filter(|key| !key.trim().is_empty())
      .map(|key| {
        if key.len() > 1024 {
          return Err(Error::Invalid("CurseForge credential is invalid".into()));
        }
        let mut value = HeaderValue::from_str(&key)
          .map_err(|_| Error::Invalid("CurseForge credential is invalid".into()))?;
        value.set_sensitive(true);
        Ok(value)
      })
      .transpose()?;
    let client = Client::builder()
      .redirect(reqwest::redirect::Policy::none())
      .timeout(Duration::from_secs(12))
      .connect_timeout(Duration::from_secs(5))
      .build()
      .map_err(|_| Error::Metadata("CurseForge HTTP initialization failed".into()))?;
    Ok(Self {
      client,
      key,
      cache: Mutex::new(VecDeque::new()),
    })
  }
  pub fn configured(&self) -> bool {
    self.key.is_some()
  }
  async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
    self.get_with_refresh(path, query, false).await
  }
  async fn get_with_refresh<T: DeserializeOwned>(
    &self,
    path: &str,
    query: &[(&str, String)],
    force: bool,
  ) -> Result<T> {
    let key = self
      .key
      .as_ref()
      .ok_or_else(|| Error::Invalid("CurseForge API Key 尚未配置，正在等待申请。".into()))?;
    let mut url =
      Url::parse(&format!("https://api.curseforge.com/v1/{path}")).expect("fixed API URL");
    url
      .query_pairs_mut()
      .extend_pairs(query.iter().map(|(key, value)| (*key, value.as_str())));
    let identity = url.as_str().to_owned();
    // Serialize cache misses; the Runtime bounds callers before they enter this provider.
    let mut cache = self.cache.lock().await;
    cache.retain(|(_, at, _)| at.elapsed() < Duration::from_secs(300));
    if !force && let Some((_, _, bytes)) = cache.iter().find(|(id, _, _)| id == &identity) {
      return serde_json::from_slice(bytes)
        .map_err(|_| Error::Metadata("CurseForge response is invalid".into()));
    }
    let mut response = self
      .client
      .get(url)
      .header("x-api-key", key.clone())
      .send()
      .await
      .map_err(|_| Error::Metadata("无法连接 CurseForge，请稍后重试。".into()))?;
    if !response.status().is_success() {
      let message = match response.status().as_u16() {
        401 | 403 => "CurseForge 拒绝了 API 凭据，请检查申请与配置。",
        404 => "CurseForge 项目或文件不存在。",
        429 => "CurseForge 请求过于频繁，请稍后重试。",
        _ => "CurseForge 服务暂时不可用。",
      };
      return Err(Error::Metadata(message.into()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
      .chunk()
      .await
      .map_err(|_| Error::Metadata("CurseForge response interrupted".into()))?
    {
      if bytes.len().saturating_add(chunk.len()) > LIMIT {
        return Err(Error::Metadata("CurseForge response exceeds limit".into()));
      }
      bytes.extend_from_slice(&chunk);
    }
    let value = serde_json::from_slice(&bytes)
      .map_err(|_| Error::Metadata("CurseForge response is invalid".into()))?;
    cache.retain(|(id, _, _)| id != &identity);
    while cache.len() >= 16 {
      cache.pop_front();
    }
    cache.push_back((identity, Instant::now(), bytes));
    Ok(value)
  }
  pub async fn search(&self, game: &str, text: &str, index: u32) -> Result<CatalogPage<Project>> {
    self
      .search_with_refresh(Some(game), text, index, false)
      .await
  }
  async fn search_with_refresh(
    &self,
    game: Option<&str>,
    text: &str,
    index: u32,
    force: bool,
  ) -> Result<CatalogPage<Project>> {
    validate_search(game, text, index)?;
    let mut params = vec![
      ("gameId", "432".into()),
      ("classId", "6".into()),
      ("modLoaderType", "4".into()),
      ("searchFilter", text.into()),
      ("sortField", "2".into()),
      ("sortOrder", "desc".into()),
      ("pageSize", PAGE.to_string()),
      ("index", index.to_string()),
    ];
    if let Some(game) = game {
      params.push(("gameVersion", game.into()));
    }
    let mut page: UpstreamPage<Project> =
      self.get_with_refresh("mods/search", &params, force).await?;
    for project in &mut page.data {
      sanitize_logo(project);
    }
    let next_index = next(&page, index)?;
    if page.data.iter().any(|p| !valid_project(p)) {
      return Err(Error::Metadata(
        "CurseForge project identity mismatch".into(),
      ));
    }
    Ok(CatalogPage {
      items: page.data,
      next_index,
    })
  }
  pub async fn project(&self, project: u32, force: bool) -> Result<Project> {
    validate_id(project)?;
    let mut response: Envelope<Project> = self
      .get_with_refresh(&format!("mods/{project}"), &[], force)
      .await?;
    sanitize_logo(&mut response.data);
    if response.data.id != project || !valid_project(&response.data) {
      return Err(Error::Metadata(
        "CurseForge project identity mismatch".into(),
      ));
    }
    Ok(response.data)
  }
  pub async fn files(&self, game: &str, project: u32, index: u32) -> Result<CatalogPage<ModFile>> {
    self.files_with_refresh(game, project, index, false).await
  }
  async fn files_with_refresh(
    &self,
    game: &str,
    project: u32,
    index: u32,
    force: bool,
  ) -> Result<CatalogPage<ModFile>> {
    validate_query(game, index)?;
    validate_id(project)?;
    let page: UpstreamPage<ModFile> = self
      .get_with_refresh(
        &format!("mods/{project}/files"),
        &[
          ("gameVersion", game.into()),
          ("modLoaderType", "4".into()),
          ("pageSize", PAGE.to_string()),
          ("index", index.to_string()),
        ],
        force,
      )
      .await?;
    let next_index = next(&page, index)?;
    if page.data.iter().any(|file| {
      file.mod_id != project
        || file.game_id != 432
        || file.dependencies.len() > 128
        || file.hashes.len() > 8
    }) {
      return Err(Error::Metadata("CurseForge file identity mismatch".into()));
    }
    let items = page
      .data
      .into_iter()
      .filter(|file| compatible(file, game))
      .collect();
    Ok(CatalogPage { items, next_index })
  }
  pub async fn preview(&self, game: &str, project: u32, file: u32) -> Result<DependencyPreview> {
    self
      .preview_with_existing(game, project, file, &BTreeMap::new())
      .await
  }
  pub async fn preview_with_existing(
    &self,
    game: &str,
    project: u32,
    file: u32,
    existing: &BTreeMap<u32, u32>,
  ) -> Result<DependencyPreview> {
    validate_query(game, 0)?;
    validate_id(project)?;
    validate_id(file)?;
    let root: Envelope<ModFile> = self
      .get(&format!("mods/{project}/files/{file}"), &[])
      .await?;
    if root.data.id != file || root.data.mod_id != project || !compatible(&root.data, game) {
      return Err(Error::Conflict(
        "所选文件不适用于当前 Minecraft / Fabric 实例。".into(),
      ));
    }
    let mut queue = VecDeque::from([(project, root.data, true)]);
    let mut selected = BTreeMap::new();
    let mut scheduled = HashSet::from([project]);
    let mut optional = HashSet::new();
    while let Some((id, file, direct)) = queue.pop_front() {
      let entry: Envelope<Project> = self.get(&format!("mods/{id}"), &[]).await?;
      let entry = entry.data;
      if entry.id != id || entry.game_id != 432 || entry.class_id != Some(6) {
        return Err(Error::Metadata(
          "CurseForge project identity mismatch".into(),
        ));
      }
      if entry.allow_mod_distribution == Some(false) || !usable(&file) {
        return Err(Error::Conflict(format!(
          "项目 {id} 的文件不可通过第三方下载，或缺少受支持的文件校验信息。"
        )));
      }
      if file.dependencies.len() > 128 {
        return Err(Error::Metadata(
          "CurseForge dependency limit exceeded".into(),
        ));
      }
      for dep in &file.dependencies {
        validate_id(dep.mod_id)?;
        match dep.relation_type {
          3 => {
            if scheduled.insert(dep.mod_id) {
              if scheduled.len() > MAX_NODES {
                return Err(Error::Conflict(
                  "依赖超过 32 个项目，请缩小安装范围。".into(),
                ));
              }
              // Only stable releases are automatically selected; users choose the root file explicitly.
              let candidate = if let Some(pinned) = existing.get(&dep.mod_id) {
                let pinned_id = *pinned;
                let pinned: Envelope<ModFile> = self
                  .get(&format!("mods/{}/files/{pinned}", dep.mod_id), &[])
                  .await?;
                if pinned.data.id != pinned_id
                  || pinned.data.mod_id != dep.mod_id
                  || !compatible(&pinned.data, game)
                  || !usable(&pinned.data)
                {
                  return Err(Error::Conflict(
                    "已安装依赖不再满足平台标签，需先手动处理。".into(),
                  ));
                }
                pinned.data
              } else {
                let candidates = self.files(game, dep.mod_id, 0).await?;
                candidates
                  .items
                  .into_iter()
                  .filter(|f| f.release_type == 1 && usable(f))
                  .max_by(|a, b| a.file_date.cmp(&b.file_date).then(a.id.cmp(&b.id)))
                  .ok_or_else(|| {
                    Error::Conflict(format!(
                      "依赖项目 {} 的首批版本中没有可用的兼容正式版，需要手动处理。",
                      dep.mod_id
                    ))
                  })?
              };
              queue.push_back((dep.mod_id, candidate, false));
            }
          }
          2 => {
            optional.insert(dep.mod_id);
          }
          1 | 4 | 5 => {}
          6 => {
            return Err(Error::Conflict(
              "依赖含尚未支持的 Include 关系，需要手动处理。".into(),
            ));
          }
          _ => {
            return Err(Error::Metadata(
              "unknown CurseForge dependency relation".into(),
            ));
          }
        }
      }
      selected.insert(
        id,
        PreviewItem {
          project: entry,
          file,
          direct,
        },
      );
    }
    // Reject cycles and declared incompatibilities instead of silently claiming a valid solution.
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for id in selected.keys() {
      visit(*id, &selected, &mut visiting, &mut visited)?;
    }
    let mut total_bytes = 0u64;
    for item in selected.values() {
      total_bytes = total_bytes
        .checked_add(item.file.file_length)
        .ok_or_else(|| Error::Metadata("resource size overflow".into()))?;
      if item
        .file
        .dependencies
        .iter()
        .any(|dep| dep.relation_type == 5 && selected.contains_key(&dep.mod_id))
      {
        return Err(Error::Conflict(
          "依赖集合中存在平台声明的不兼容项目。".into(),
        ));
      }
    }
    let mut optional_projects: Vec<_> = optional
      .into_iter()
      .filter(|id| !selected.contains_key(id))
      .collect();
    optional_projects.sort_unstable();
    let mut items: Vec<_> = selected.into_values().collect();
    items.sort_by_key(|i| (!i.direct, i.project.id));
    Ok(DependencyPreview {
      items,
      total_bytes,
      optional_projects,
      warnings: vec![
        "这是只读依赖预览，尚未下载或修改实例。".into(),
        "平台标签不包含完整的模组版本约束与客户端侧要求；尚未与已导入的本地模组求解兼容性。".into(),
        "可选依赖不会自动加入；工具和内嵌依赖不单独下载。".into(),
      ],
    })
  }
}
fn visit(
  id: u32,
  items: &BTreeMap<u32, PreviewItem>,
  visiting: &mut HashSet<u32>,
  visited: &mut HashSet<u32>,
) -> Result<()> {
  if visited.contains(&id) {
    return Ok(());
  }
  if !visiting.insert(id) {
    return Err(Error::Conflict("必需依赖存在循环，需要手动处理。".into()));
  }
  for dep in &items[&id].file.dependencies {
    if dep.relation_type == 3 {
      visit(dep.mod_id, items, visiting, visited)?;
    }
  }
  visiting.remove(&id);
  visited.insert(id);
  Ok(())
}
pub(crate) fn compatible(file: &ModFile, game: &str) -> bool {
  file.game_id == 432
    && file.id > 0
    && file.is_available
    && file.is_server_pack != Some(true)
    && file.is_early_access_content != Some(true)
    && file.game_versions.iter().any(|v| v == game)
    && file.game_versions.iter().any(|v| v == "Fabric")
}
pub(crate) fn usable(file: &ModFile) -> bool {
  let safe_url = file
    .download_url
    .as_ref()
    .and_then(|url| Url::parse(url).ok())
    .is_some_and(|url| {
      url.query().is_none()
        && url.fragment().is_none()
        && url.as_str().len() <= 2048
        && url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && matches!(
          url.host_str(),
          Some("edge.forgecdn.net" | "mediafilez.forgecdn.net")
        )
    });
  file.dependencies.len() <= 128
    && file.hashes.len() <= 8
    && file.game_versions.len() <= 256
    && file.game_versions.iter().all(|v| v.len() <= 128)
    && file.display_name.len() <= 512
    && file.file_date.len() <= 64
    && file.file_length > 0
    && file.file_length <= crate::resources::MAX_IMPORT_BYTES as u64
    && crate::resources::validate_filename(&file.file_name).is_ok()
    && file.hashes.iter().any(|hash| {
      hash.algo == 1 && hash.value.len() == 40 && hash.value.bytes().all(|b| b.is_ascii_hexdigit())
    })
    && safe_url
}
fn validate_id(id: u32) -> Result<()> {
  if id == 0 {
    return Err(Error::Invalid("invalid CurseForge ID".into()));
  }
  Ok(())
}
fn validate_search(game: Option<&str>, text: &str, index: u32) -> Result<()> {
  if game.is_some_and(|g| !crate::metadata::is_valid_version_id(g))
    || index > 9980
    || text.len() > 128
    || text.chars().any(char::is_control)
  {
    return Err(Error::Invalid("invalid CurseForge search query".into()));
  }
  Ok(())
}
fn valid_project(p: &Project) -> bool {
  p.id > 0
    && p.game_id == 432
    && p.class_id == Some(6)
    && !p.name.is_empty()
    && p.name.len() <= 512
    && p.summary.len() <= 8192
    && p.authors.len() <= 128
    && p.authors.iter().all(|a| a.name.len() <= 512)
    && p
      .date_modified
      .as_ref()
      .is_none_or(|value| value.len() <= 64)
    && p.logo.as_ref().is_none_or(|logo| {
      logo.thumbnail_url.as_deref().is_none_or(valid_icon_url)
        && logo.url.as_deref().is_none_or(valid_icon_url)
    })
    && p.links.website_url.as_ref().is_none_or(|s| {
      s.len() <= 2048
        && Url::parse(s).ok().is_some_and(|u| {
          u.scheme() == "https"
            && matches!(u.host_str(), Some("www.curseforge.com" | "curseforge.com"))
            && u.username().is_empty()
            && u.password().is_none()
            && u.port().is_none()
        })
    })
}
fn valid_icon_url(value: &str) -> bool {
  crate::catalog_cache::valid_image_url(value, &["media.forgecdn.net", "edge.forgecdn.net"])
}
fn sanitize_logo(project: &mut Project) {
  if let Some(logo) = &mut project.logo {
    logo.thumbnail_url = logo
      .thumbnail_url
      .take()
      .filter(|value| valid_icon_url(value));
    logo.url = logo.url.take().filter(|value| valid_icon_url(value));
    if logo.thumbnail_url.is_none() && logo.url.is_none() {
      project.logo = None;
    }
  }
}
fn validate_query(game: &str, index: u32) -> Result<()> {
  if !crate::metadata::is_valid_version_id(game) || index > 9980 {
    return Err(Error::Invalid("invalid catalog query".into()));
  }
  Ok(())
}
fn next<T>(page: &UpstreamPage<T>, index: u32) -> Result<Option<u32>> {
  if page.data.len() > PAGE as usize
    || page.pagination.index != index
    || page.pagination.result_count as usize != page.data.len()
  {
    return Err(Error::Metadata("invalid CurseForge pagination".into()));
  }
  let end = index + page.pagination.result_count;
  Ok(
    (end > index && end < page.pagination.total_count.min(10_000) as u32 && end <= 9980)
      .then_some(end),
  )
}
