//! Bounded, on-demand inspection of managed Fabric mod metadata. No game config is changed.
use super::*;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const MAX_METADATA: u64 = 256 * 1024;
const MAX_SCAN_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShaderEnvironmentStatus {
  RequiresFabric,
  MissingIris,
  IrisDisabled,
  MissingDependencies,
  Unconfirmed,
  Ready,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderMod {
  pub version: String,
  pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderEnvironment {
  pub status: ShaderEnvironmentStatus,
  pub iris: Option<ShaderMod>,
  pub sodium: Option<ShaderMod>,
  pub managed_only: bool,
  pub reason: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderPackCollection {
  pub revision: u64,
  pub applied_revision: Option<u64>,
  pub packs: Vec<ShaderPackEntry>,
  pub environment: ShaderEnvironment,
}
impl ResourceManager {
  /// References and environment are read under the same resource/instance locks.
  pub async fn shader_packs(&self, name: &str) -> Result<ShaderPackCollection> {
    self
      .access(name, |directory, data, _| {
        let manifest = read_manifest(directory)?;
        let record = storage::read_json_bounded(
          &directory.join("installed.json"),
          crate::client::MAX_INSTALL_RECORD_SIZE,
        )?;
        let environment = inspect_environment(directory, data, &manifest, &record, true);
        Ok(ShaderPackCollection {
          revision: manifest.revision,
          applied_revision: manifest.applied_revision,
          packs: manifest.shader_packs,
          environment,
        })
      })
      .await
  }
}
#[derive(Deserialize)]
struct Metadata {
  #[serde(rename = "schemaVersion")]
  schema: u32,
  id: String,
  version: String,
  #[serde(default)]
  environment: Option<String>,
  #[serde(default)]
  depends: BTreeMap<String, Value>,
  #[serde(default)]
  breaks: BTreeMap<String, Value>,
  #[serde(default)]
  provides: Vec<String>,
  #[serde(default)]
  jars: Vec<Value>,
}
fn valid_id(id: &str) -> bool {
  (2..=64).contains(&id.len())
    && id.as_bytes()[0].is_ascii_lowercase()
    && id
      .bytes()
      .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(&c))
}
fn object_bytes(data: &Path, entry: &ResourceEntry) -> Result<Vec<u8>> {
  let path = object_path(data, &entry.sha256);
  verify_object(&path, &entry.sha256, entry.size)?;
  let mut bytes = Vec::new();
  File::open(path)?
    .take(entry.size + 1)
    .read_to_end(&mut bytes)?;
  if bytes.len() as u64 != entry.size || hex(&Sha256::digest(&bytes)) != entry.sha256 {
    return Err(Error::Conflict("模组原件在识别期间发生变化。".into()));
  }
  Ok(bytes)
}
fn metadata(bytes: &[u8]) -> Result<Metadata> {
  let mut archive = open_zip(bytes)?;
  let mut index = None;
  for at in 0..archive.len() {
    let file = archive
      .by_index(at)
      .map_err(|_| Error::Metadata("无法读取模组归档。".into()))?;
    if file.name().eq_ignore_ascii_case("fabric.mod.json") {
      if index.is_some()
        || file.name() != "fabric.mod.json"
        || file.size() > MAX_METADATA
        || file.is_dir()
      {
        return Err(Error::Metadata(
          "模组元数据重复、路径不规范或超出大小限制。".into(),
        ));
      }
      packs::validate_zip_entry(&file)?;
      index = Some(at);
    }
  }
  let index = index.ok_or_else(|| Error::Metadata("受管 JAR 缺少 fabric.mod.json。".into()))?;
  let mut file = archive
    .by_index(index)
    .map_err(|_| Error::Metadata("无法读取模组元数据。".into()))?;
  let mut content = Vec::new();
  file
    .by_ref()
    .take(MAX_METADATA + 1)
    .read_to_end(&mut content)?;
  if content.len() as u64 > MAX_METADATA {
    return Err(Error::Metadata("模组元数据超出大小限制。".into()));
  }
  let meta: Metadata = serde_json::from_slice(&content)?;
  if meta.schema != 1
    || !valid_id(&meta.id)
    || meta.version.is_empty()
    || meta.version.len() > 128
    || meta.version.chars().any(char::is_control)
    || meta.depends.len() > 128
    || meta.breaks.len() > 128
    || meta.provides.len() > 128
    || meta.jars.len() > 128
    || !matches!(
      meta.environment.as_deref(),
      None | Some("*" | "client" | "server")
    )
    || meta
      .depends
      .keys()
      .chain(meta.breaks.keys())
      .any(|id| !valid_id(id))
    || meta.provides.iter().any(|id| !valid_id(id))
  {
    return Err(Error::Metadata("模组元数据格式无法确认。".into()));
  }
  Ok(meta)
}
// Nested payloads count against the same total byte budget as root JARs.
const MAX_NESTED_DEPTH: usize = 4;
const MAX_NESTED_JARS: usize = 128;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NestedJar {
  file: String,
}
struct Inspected {
  meta: Metadata,
  enabled: bool,
  nested: bool,
  raw_hash: String,
  content_hash: Option<String>,
}
struct ScanBudget {
  bytes: u64,
  expanded: u64,
  nested: usize,
}
// Ignore ZIP encoding differences only when every named entry has identical content.
// A separate aggregate expanded-byte budget bounds compressed library payloads.
fn nested_content_hash(bytes: &[u8], budget: &mut ScanBudget) -> Result<String> {
  let mut archive = open_zip(bytes)?;
  let mut entries = BTreeMap::new();
  for at in 0..archive.len() {
    let mut file = archive
      .by_index(at)
      .map_err(|_| Error::Metadata("无法读取内置库条目。".into()))?;
    packs::validate_zip_entry(&file)?;
    let name = file.name().to_string();
    let key = name.to_ascii_lowercase();
    if entries.contains_key(&key) {
      return Err(Error::Metadata("内置库存在重复条目。".into()));
    }
    budget.expanded = budget
      .expanded
      .checked_add(file.size())
      .ok_or_else(|| Error::Metadata("内置库解压大小溢出。".into()))?;
    if budget.expanded > MAX_SCAN_BYTES {
      return Err(Error::Metadata(
        "内置库解压内容超过合计 128 MiB 识别上限。".into(),
      ));
    }
    let size = file.size();
    let mut hash = Sha256::new();
    let mut read = 0_u64;
    let mut buffer = [0_u8; 8192];
    loop {
      let count = file.read(&mut buffer)?;
      if count == 0 {
        break;
      }
      read += count as u64;
      if read > size {
        return Err(Error::Metadata("内置库条目长度不一致。".into()));
      }
      hash.update(&buffer[..count]);
    }
    if read != size {
      return Err(Error::Metadata("内置库条目长度不一致。".into()));
    }
    entries.insert(key, (name, size, hash.finalize()));
  }
  let mut result = Sha256::new();
  for (_, (name, size, digest)) in entries {
    result.update((name.len() as u64).to_le_bytes());
    result.update(name.as_bytes());
    result.update(size.to_le_bytes());
    result.update(digest);
  }
  Ok(hex(&result.finalize()))
}

fn scan_jar(
  bytes: &[u8],
  parent_enabled: bool,
  depth: usize,
  budget: &mut ScanBudget,
  output: &mut Vec<Inspected>,
) -> Result<()> {
  let meta = metadata(bytes)?;
  let enabled = parent_enabled && meta.environment.as_deref() != Some("server");
  let declarations = if enabled {
    meta.jars.clone()
  } else {
    Vec::new()
  };
  output.push(Inspected {
    meta,
    enabled,
    nested: depth > 0,
    raw_hash: hex(&Sha256::digest(bytes)),
    content_hash: (depth > 0)
      .then(|| nested_content_hash(bytes, budget))
      .transpose()?,
  });
  if declarations.is_empty() {
    return Ok(());
  }
  if depth >= MAX_NESTED_DEPTH {
    return Err(Error::Metadata("嵌套模组超过 4 层识别上限。".into()));
  }
  let mut archive = open_zip(bytes)?;
  let mut paths = BTreeSet::new();
  for declaration in declarations {
    let declaration: NestedJar = serde_json::from_value(declaration)?;
    let path = declaration.file;
    if path.len() > 512
      || !path.to_ascii_lowercase().ends_with(".jar")
      || !paths.insert(path.to_ascii_lowercase())
    {
      return Err(Error::Metadata("嵌套模组路径重复或格式不支持。".into()));
    }
    let mut index = None;
    for at in 0..archive.len() {
      let file = archive
        .by_index(at)
        .map_err(|_| Error::Metadata("无法读取嵌套归档条目。".into()))?;
      if file.name().eq_ignore_ascii_case(&path) {
        if index.is_some() || file.name() != path || file.is_dir() {
          return Err(Error::Metadata("嵌套模组路径重复或大小写含糊。".into()));
        }
        packs::validate_zip_entry(&file)?;
        index = Some(at);
      }
    }
    let index = index.ok_or_else(|| Error::Metadata("声明的嵌套模组不存在。".into()))?;
    let mut file = archive
      .by_index(index)
      .map_err(|_| Error::Metadata("无法读取嵌套模组。".into()))?;
    budget.nested += 1;
    let size = file.size();
    budget.bytes = budget
      .bytes
      .checked_add(size)
      .ok_or_else(|| Error::Metadata("嵌套模组大小溢出。".into()))?;
    if budget.nested > MAX_NESTED_JARS
      || size == 0
      || size > MAX_IMPORT_BYTES as u64
      || budget.bytes > MAX_SCAN_BYTES
    {
      return Err(Error::Metadata(
        "嵌套模组超过 128 个、单份 64 MiB 或合计 128 MiB 识别上限。".into(),
      ));
    }
    let mut child = Vec::new();
    file.by_ref().take(size + 1).read_to_end(&mut child)?;
    if child.len() as u64 != size {
      return Err(Error::Metadata("嵌套模组长度不一致。".into()));
    }
    scan_jar(&child, enabled, depth + 1, budget, output)?;
  }
  Ok(())
}

pub(super) fn inspect_environment(
  directory: &Path,
  data: &Path,
  manifest: &ResourceManifest,
  record: &crate::client::ClientInstallRecord,
  verify_copies: bool,
) -> ShaderEnvironment {
  use ShaderEnvironmentStatus::*;
  let mut result = ShaderEnvironment {
    status: Unconfirmed,
    iris: None,
    sodium: None,
    managed_only: true,
    reason: String::new(),
  };
  let Some(crate::fabric::ClientLoader::Fabric { version: loader }) = &record.loader else {
    result.status = RequiresFabric;
    result.reason = "当前为原版实例，请新建 Fabric 实例以安装光影支持。".into();
    return result;
  };
  if manifest.mods.iter().map(|entry| entry.size).sum::<u64>() > MAX_SCAN_BYTES {
    result.reason = "受管模组总大小超出本次 128 MiB 识别预算，环境尚未确认。".into();
    return result;
  }
  let mut candidates = Vec::new();
  let mut issue = None;
  let mut budget = ScanBudget {
    bytes: manifest.mods.iter().map(|entry| entry.size).sum(),
    expanded: 0,
    nested: 0,
  };
  for entry in &manifest.mods {
    if entry.enabled
      && verify_copies
      && (ensure_directory(&directory.join("game/mods"), false).is_err()
        || verify_file(
          &directory.join("game/mods").join(&entry.filename),
          &entry.sha256,
          entry.size,
        )
        .is_err())
    {
      issue = Some(format!("受管模组副本缺失或已被修改：{}。", entry.filename));
    }
    let result = object_bytes(data, entry)
      .and_then(|bytes| scan_jar(&bytes, entry.enabled, 0, &mut budget, &mut candidates));
    if let Err(error) = result {
      issue = Some(format!(
        "无法确认受管 JAR 的元数据或完整性：{}：{error}",
        entry.filename
      ));
    }
  }
  let mut scanned = Vec::new();
  let mut ids = BTreeMap::<String, (String, Option<String>, bool)>::new();
  let mut nested_ids = Vec::new();
  for candidate in candidates {
    let Inspected {
      meta,
      enabled,
      nested,
      raw_hash,
      content_hash,
    } = candidate;
    if let Some((existing_raw, existing_content, existing_nested)) = ids.get(&meta.id) {
      // A root JAR may be bundled verbatim by another mod. Compare raw bytes first;
      // for nested libraries also allow different ZIP encodings of identical content.
      let identical = existing_raw == &raw_hash
        || (nested
          && *existing_nested
          && existing_content.is_some()
          && existing_content == &content_hash);
      if identical {
        continue;
      }
      issue = Some(format!("发现重复或版本冲突的受管模组 ID：{}。", meta.id));
    } else {
      ids.insert(meta.id.clone(), (raw_hash, content_hash, nested));
    }
    if matches!(meta.id.as_str(), "minecraft" | "fabricloader" | "java") {
      issue = Some("受管模组 ID 与运行环境标识冲突。".into());
    }
    let summary = ShaderMod {
      version: meta.version.clone(),
      enabled,
    };
    if meta.id == "iris" {
      result.iris = Some(summary.clone());
    }
    if meta.id == "sodium" {
      result.sodium = Some(summary);
    }
    if nested && enabled {
      nested_ids.push(meta.id.clone());
    }
    scanned.push((meta, enabled));
  }
  if let Some(reason) = issue {
    result.reason = reason;
    return result;
  }
  let Some(iris) = &result.iris else {
    result.status = MissingIris;
    result.reason = "未发现受管 Iris；识别范围仅限 Anvil 管理的模组。".into();
    return result;
  };
  if !iris.enabled {
    result.status = IrisDisabled;
    result.reason = "受管 Iris 尚未启用，请在模组管理中检查。".into();
    return result;
  }
  // Only validate the required dependency closure of Iris/Sodium, not a replacement for Fabric Loader.
  let mut active: BTreeMap<_, _> = scanned
    .iter()
    .filter(|(_, enabled)| *enabled)
    .map(|(meta, _)| (meta.id.as_str(), meta))
    .collect();
  for (meta, enabled) in &scanned {
    if !enabled {
      continue;
    }
    for alias in &meta.provides {
      if matches!(alias.as_str(), "minecraft" | "fabricloader" | "java")
        || ids.contains_key(alias)
        || active.insert(alias.as_str(), meta).is_some()
      {
        result.reason = format!("模组别名与已有 ID 或其他别名冲突：{alias}。");
        return result;
      }
    }
  }
  let mut versions: BTreeMap<&str, &str> = active
    .iter()
    .map(|(id, meta)| (*id, meta.version.as_str()))
    .collect();
  let java = record.java_major.to_string();
  versions.insert("minecraft", &record.version_id);
  versions.insert("fabricloader", loader);
  versions.insert("java", &java);
  let mut pending = vec!["iris"];
  pending.extend(nested_ids.iter().map(String::as_str));
  let mut visited = BTreeSet::new();
  while let Some(id) = pending.pop() {
    if !visited.insert(id) {
      continue;
    }
    let Some(meta) = active.get(id) else {
      result.status = MissingDependencies;
      result.reason = format!("缺少已启用的必需依赖：{id}。");
      return result;
    };
    for (dependency, requirement) in &meta.depends {
      let Some(version) = versions.get(dependency.as_str()) else {
        result.status = MissingDependencies;
        result.reason = format!("{id} 缺少已启用的必需依赖：{dependency}。");
        return result;
      };
      match matches_requirement(version, requirement) {
        Some(true) => {}
        Some(false) => {
          result.status = MissingDependencies;
          result.reason = format!("{id} 的依赖版本不匹配：{dependency}。");
          return result;
        }
        None => {
          result.reason = format!("无法确认 {id} 对 {dependency} 的版本约束。");
          return result;
        }
      }
      if active.contains_key(dependency.as_str()) {
        pending.push(dependency.as_str());
      }
    }
    for (other, requirement) in &meta.breaks {
      if let Some(version) = versions.get(other.as_str()) {
        match matches_requirement(version, requirement) {
          Some(false) => {}
          Some(true) => {
            result.reason = format!("{id} 声明与当前 {other} 不兼容。");
            return result;
          }
          None => {
            result.reason = format!("无法确认 {id} 与 {other} 的不兼容约束。");
            return result;
          }
        }
      }
    }
  }
  // An incompatibility naming an alias also targets its provider, and vice versa.
  for id in visited.clone() {
    let meta = active[id];
    visited.insert(meta.id.as_str());
    visited.extend(meta.provides.iter().map(String::as_str));
  }
  // Other enabled mods may also declare incompatibility with Iris or its dependencies.
  for (meta, enabled) in &scanned {
    if !enabled {
      continue;
    }
    for (id, requirement) in &meta.breaks {
      if visited.contains(id.as_str()) {
        match matches_requirement(versions[id.as_str()], requirement) {
          Some(false) => {}
          Some(true) => {
            result.reason = format!("{} 声明与当前 {id} 不兼容。", meta.id);
            return result;
          }
          None => {
            result.reason = format!("无法确认 {} 与 {id} 的不兼容约束。", meta.id);
            return result;
          }
        }
      }
    }
  }
  result.status = Ready;
  result.reason =
    "已识别启用的受管 Iris，声明的必需依赖检查通过；实际光影和显卡兼容性仍由游戏验证。".into();
  result
}

// Fabric has a wider version grammar than semver. Unsupported forms stay unknown.
fn stable_version(value: &str) -> Option<semver::Version> {
  let core = value.split('+').next()?;
  let parts: Vec<_> = core.split('.').collect();
  if parts.is_empty()
    || parts.len() > 3
    || parts
      .iter()
      .any(|part| part.is_empty() || !part.bytes().all(|c| c.is_ascii_digit()))
  {
    return None;
  }
  let mut padded = parts.iter().map(|v| v.to_string()).collect::<Vec<_>>();
  while padded.len() < 3 {
    padded.push("0".into());
  }
  semver::Version::parse(&padded.join(".")).ok()
}
fn matches_requirement(version: &str, requirement: &Value) -> Option<bool> {
  if let Some(options) = requirement.as_array() {
    if options.is_empty() || options.len() > 16 || options.iter().any(|option| !option.is_string())
    {
      return None;
    }
    let mut unknown = false;
    for option in options {
      match matches_requirement(version, option) {
        Some(true) => return Some(true),
        Some(false) => {}
        None => unknown = true,
      }
    }
    return if unknown { None } else { Some(false) };
  }
  let requirement = requirement.as_str()?;
  if requirement.is_empty() || requirement.len() > 512 {
    return None;
  }
  let mut unknown = false;
  let mut any = false;
  for token in requirement.split_whitespace() {
    any = true;
    if token == "*" {
      continue;
    }
    let value = token.trim_start_matches(['=', '>', '<', '~', '^']);
    let operator = &token[..token.len() - value.len()];
    if !matches!(operator, "" | "=" | ">" | ">=" | "<" | "<=" | "~" | "^") {
      return None;
    }
    if matches!(operator, "" | "=") && value == version {
      continue;
    }
    let Some(actual) = stable_version(version) else {
      unknown = true;
      continue;
    };
    // A stable installed version can be ordered against an ordinary prerelease lower
    // bound without accepting Fabric's wider snapshot grammar or prerelease selection.
    if let Some((base, prerelease)) = value.split_once('-')
      && matches!(operator, ">" | ">=" | "<" | "<=")
      && semver::Prerelease::new(prerelease).is_ok()
      && let Some(base) = stable_version(base)
    {
      let matches = if matches!(operator, ">" | ">=") {
        actual >= base
      } else {
        actual < base
      };
      if !matches {
        return Some(false);
      }
      continue;
    }
    let wanted = if value.split('.').any(|p| matches!(p, "x" | "X" | "*")) {
      if !matches!(operator, "" | "=") {
        unknown = true;
        continue;
      }
      format!("={}", value.replace(['x', 'X'], "*"))
    } else {
      let Some(expected) = stable_version(value) else {
        unknown = true;
        continue;
      };
      if matches!(operator, "~" | "^") {
        format!("{operator}{value}")
      } else {
        format!(
          "{}{expected}",
          if operator.is_empty() { "=" } else { operator }
        )
      }
    };
    let Ok(req) = semver::VersionReq::parse(&wanted) else {
      unknown = true;
      continue;
    };
    if !req.matches(&actual) {
      return Some(false);
    }
  }
  if !any || unknown { None } else { Some(true) }
}

#[cfg(test)]
#[path = "../../tests/shader_environment.rs"]
mod tests;
