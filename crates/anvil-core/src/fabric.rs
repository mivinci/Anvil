//! Fabric client profile resolution. Binary transfer remains in ClientInstaller/q2q-http.
use crate::{
  CancellationToken, Context, Error, Result,
  client_plan::{ClientInstallPlan, ClientLibrary, ClientLibraryKind, PlannedFile},
  metadata::{ClientArguments, ClientFile},
};
use serde::{Deserialize, Serialize};
use url::Url;
const META_LIMIT: u64 = 4 * 1024 * 1024;
const MAVEN: &str = "https://maven.fabricmc.net/";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientLoader {
  Fabric { version: String },
}
impl ClientLoader {
  pub fn validate(&self) -> Result<()> {
    match self {
      Self::Fabric { version } => validate_version(version),
    }
  }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FabricVersion {
  pub version: String,
  pub stable: bool,
}
#[derive(Deserialize)]
struct LoaderEntry {
  loader: FabricVersion,
  #[serde(rename = "launcherMeta")]
  launcher_meta: Option<LauncherMeta>,
}
#[derive(Deserialize)]
struct LauncherMeta {
  #[serde(default)]
  min_java_version: Option<u32>,
}
#[derive(Deserialize)]
struct Profile {
  #[serde(rename = "inheritsFrom")]
  inherits_from: String,
  #[serde(rename = "mainClass")]
  main_class: String,
  #[serde(default)]
  arguments: ClientArguments,
  libraries: Vec<Library>,
}
#[derive(Deserialize)]
struct Library {
  name: String,
  url: String,
  sha1: Option<String>,
  size: Option<u64>,
}

pub fn validate_version(version: &str) -> Result<()> {
  if version.is_empty()
    || version.len() > 64
    || !version.as_bytes()[0].is_ascii_alphanumeric()
    || !version
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
    || version.ends_with('.')
  {
    return Err(Error::Invalid("invalid Fabric loader version".into()));
  }
  Ok(())
}
fn meta_url(game: &str, loader: Option<&str>, profile: bool) -> Result<Url> {
  if !crate::metadata::is_valid_version_id(game) {
    return Err(Error::Invalid("invalid game version".into()));
  }
  let mut url = Url::parse("https://meta.fabricmc.net/v2/versions/loader/").expect("static URL");
  {
    let mut parts = url.path_segments_mut().expect("hierarchical URL");
    parts.pop_if_empty().push(game);
    if let Some(loader) = loader {
      validate_version(loader)?;
      parts.push(loader);
    }
    if profile {
      parts.push("profile").push("json");
    }
  }
  Ok(url)
}
async fn json<T: serde::de::DeserializeOwned>(context: &Context, url: &Url) -> Result<T> {
  let response = context
    .http
    .executor()
    .get(url.as_str())
    .max_body_size(META_LIMIT)
    .send()
    .await?;
  let bytes = response.collect_bounded(META_LIMIT as usize).await?;
  Ok(serde_json::from_slice(&bytes)?)
}
pub async fn versions(context: &Context, game: &str) -> Result<Vec<FabricVersion>> {
  let entries: Vec<LoaderEntry> = json(context, &meta_url(game, None, false)?).await?;
  if entries.len() > 1024 {
    return Err(Error::Metadata("too many Fabric loader versions".into()));
  }
  let mut seen = std::collections::HashSet::new();
  let mut result = Vec::new();
  for entry in entries {
    validate_version(&entry.loader.version)?;
    if seen.insert(entry.loader.version.clone()) {
      result.push(entry.loader);
    }
  }
  // Keep upstream order, preferring stable releases without inventing a version.
  result.sort_by_key(|version| !version.stable);
  Ok(result)
}
pub struct CachedFabricVersions {
  pub versions: Vec<FabricVersion>,
  pub fetched_at: std::time::SystemTime,
  pub persistence_error: Option<Error>,
}
fn validate_versions(values: &[FabricVersion]) -> Result<()> {
  if values.len() > 1024 {
    return Err(Error::Metadata("too many Fabric loader versions".into()));
  }
  let mut seen = std::collections::HashSet::new();
  for value in values {
    validate_version(&value.version)?;
    if !seen.insert(&value.version) {
      return Err(Error::Metadata("duplicate Fabric loader version".into()));
    }
  }
  Ok(())
}
pub async fn load_cached_versions(
  context: &Context,
  game: &str,
) -> Result<Option<CachedFabricVersions>> {
  meta_url(game, None, false)?;
  let Some(snapshot) =
    crate::catalog_cache::load::<Vec<FabricVersion>>(context, crate::catalog_cache::FABRIC, game)
      .await?
  else {
    return Ok(None);
  };
  validate_versions(&snapshot.value)?;
  Ok(Some(CachedFabricVersions {
    versions: snapshot.value,
    fetched_at: snapshot.fetched_at,
    persistence_error: None,
  }))
}
pub async fn refresh_versions(context: &Context, game: &str) -> Result<CachedFabricVersions> {
  let versions = versions(context, game).await?;
  validate_versions(&versions)?;
  let fetched_at = std::time::SystemTime::now();
  let persistence_error = crate::catalog_cache::save(
    context,
    crate::catalog_cache::FABRIC,
    game,
    versions.clone(),
    fetched_at,
  )
  .await
  .err();
  Ok(CachedFabricVersions {
    versions,
    fetched_at,
    persistence_error,
  })
}
pub async fn apply(
  context: &Context,
  plan: &mut ClientInstallPlan,
  loader: &ClientLoader,
  cancel: &CancellationToken,
) -> Result<()> {
  loader.validate()?;
  let ClientLoader::Fabric { version } = loader;
  let resolved = tokio::select! {
    _ = cancel.cancelled() => return Err(Error::Cancelled),
    result = resolve(context, plan, version) => result?,
  };
  let (profile, libraries, min_java) = resolved;
  for library in libraries {
    let identity = library
      .coordinate
      .rsplit_once(':')
      .expect("validated coordinate")
      .0;
    plan
      .libraries
      .retain(|existing| existing.coordinate.rsplit_once(':').map(|(id, _)| id) != Some(identity));
    plan.libraries.push(library);
  }
  plan.main_class = profile.main_class;
  plan.arguments.jvm.extend(profile.arguments.jvm);
  plan.arguments.game.extend(profile.arguments.game);
  plan.java_major = plan.java_major.max(min_java);
  plan.loader = Some(loader.clone());
  Ok(())
}
async fn resolve(
  context: &Context,
  plan: &ClientInstallPlan,
  version: &str,
) -> Result<(Profile, Vec<ClientLibrary>, u32)> {
  let entry: LoaderEntry =
    json(context, &meta_url(&plan.version_id, Some(version), false)?).await?;
  if entry.loader.version != version {
    return Err(Error::Metadata("Fabric loader identity mismatch".into()));
  }
  let min_java = entry
    .launcher_meta
    .and_then(|meta| meta.min_java_version)
    .unwrap_or(8);
  if !(8..=99).contains(&min_java) {
    return Err(Error::Metadata("invalid Fabric Java requirement".into()));
  }
  let profile: Profile = json(context, &meta_url(&plan.version_id, Some(version), true)?).await?;
  if profile.inherits_from != plan.version_id
    || profile.libraries.is_empty()
    || profile.libraries.len() > 128
    || !matches!(
      profile.main_class.as_str(),
      "net.fabricmc.loader.impl.launch.knot.KnotClient"
        | "net.fabricmc.loader.launch.knot.KnotClient"
    )
    || profile.arguments.jvm.len() > 128
    || profile.arguments.game.len() > 128
    || !crate::metadata::valid_argument_list(&profile.arguments.jvm)
    || !crate::metadata::valid_argument_list(&profile.arguments.game)
  {
    return Err(Error::Metadata(
      "invalid or unsupported Fabric client profile".into(),
    ));
  }
  let head = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .timeout(std::time::Duration::from_secs(15))
    .build()
    .map_err(|_| Error::Metadata("could not create Fabric metadata client".into()))?;
  let mut libraries = Vec::new();
  let mut identities = std::collections::HashSet::new();
  for library in &profile.libraries {
    if library.url != MAVEN {
      return Err(Error::Metadata(
        "untrusted Fabric library repository".into(),
      ));
    }
    let (path, identity) = maven_path(&library.name)?;
    if !identities.insert(identity) {
      return Err(Error::Metadata("duplicate Fabric library identity".into()));
    }
    let url = Url::parse(MAVEN)
      .expect("static URL")
      .join(&path)
      .map_err(|_| Error::Metadata("invalid Fabric artifact URL".into()))?;
    let sha1 = match &library.sha1 {
      Some(hash) => hash.clone(),
      None => {
        let response = context
          .http
          .executor()
          .get(format!("{url}.sha1"))
          .max_body_size(128)
          .send()
          .await?;
        let bytes = response.collect_bounded(128).await?;
        String::from_utf8(bytes.to_vec())
          .map_err(|_| Error::Metadata("invalid Fabric checksum encoding".into()))?
          .trim()
          .to_owned()
      }
    };
    if sha1.len() != 40 || !sha1.bytes().all(|byte| byte.is_ascii_hexdigit()) {
      return Err(Error::Metadata("invalid Fabric artifact checksum".into()));
    }
    // HEAD is only a bounded metadata probe; JAR downloads use q2q-http below.
    let size = match library.size {
      Some(size) => size,
      None => {
        let response = head
          .head(url.clone())
          .send()
          .await
          .map_err(|_| Error::Metadata("Fabric size probe failed".into()))?;
        if !response.status().is_success() {
          return Err(Error::Metadata("Fabric size probe rejected".into()));
        }
        response
          .headers()
          .get(reqwest::header::CONTENT_LENGTH)
          .and_then(|value| value.to_str().ok())
          .and_then(|value| value.parse::<u64>().ok())
          .ok_or_else(|| Error::Metadata("missing Fabric artifact length".into()))?
      }
    };
    if size == 0 || size > 64 * 1024 * 1024 {
      return Err(Error::Metadata("Fabric artifact exceeds limit".into()));
    }
    libraries.push(ClientLibrary {
      coordinate: library.name.clone(),
      artifact: PlannedFile {
        path,
        file: ClientFile {
          url,
          sha1: sha1.to_ascii_lowercase(),
          size,
        },
      },
      kind: ClientLibraryKind::Classpath,
    });
  }
  let expected_loader = format!("net.fabricmc:fabric-loader:{version}");
  let expected_mapping = format!("net.fabricmc:intermediary:{}", plan.version_id);
  if !libraries
    .iter()
    .any(|lib| lib.coordinate == expected_loader)
    || libraries.iter().any(|lib| {
      lib.coordinate.starts_with("net.fabricmc:intermediary:") && lib.coordinate != expected_mapping
    })
  {
    return Err(Error::Metadata(
      "Fabric profile has missing loader or mismatched mappings".into(),
    ));
  }
  Ok((profile, libraries, min_java))
}
fn maven_path(coordinate: &str) -> Result<(String, String)> {
  let parts: Vec<_> = coordinate.split(':').collect();
  if parts.len() != 3
    || parts.iter().any(|part| validate_version(part).is_err())
    || parts[0]
      .split('.')
      .any(|part| part.is_empty() || part == "." || part == "..")
  {
    return Err(Error::Metadata("invalid Fabric Maven coordinate".into()));
  }
  Ok((
    format!(
      "{}/{}/{}/{}-{}.jar",
      parts[0].replace('.', "/"),
      parts[1],
      parts[2],
      parts[1],
      parts[2]
    ),
    format!("{}:{}", parts[0], parts[1]),
  ))
}
