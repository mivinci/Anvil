//! Runtime self-update.
//!
//! The serve process checks a signed index of CLI releases and installs a
//! compatible newer binary into the Shell-provided sidecar store, so the
//! desktop App can advance its runtime without repackaging. Verification
//! matches the Shell's App update channel: an Ed25519 signature over the
//! index payload, HTTPS-only download URLs and a SHA-256 digest pinned by the
//! signed payload.
//!
//! The updater is compiled in only when `ANVIL_RUNTIME_UPDATE_MANIFEST_URL`
//! and `ANVIL_RUNTIME_UPDATE_PUBLIC_KEY` are set at build time, and it only
//! runs when the Shell passes `ANVIL_SIDECAR_STORE_DIR` and
//! `ANVIL_HTTP_API_COMPAT` in the sidecar environment. Any failure is
//! logged and retried no sooner than the next daily check.

use anvil_core::Context;
use semver::{Version, VersionReq};
use std::{
  collections::HashMap,
  path::{Path, PathBuf},
  time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

const MANIFEST_URL: Option<&str> = option_env!("ANVIL_RUNTIME_UPDATE_MANIFEST_URL");
const PUBLIC_KEY_HEX: Option<&str> = option_env!("ANVIL_RUNTIME_UPDATE_PUBLIC_KEY");

const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_INDEX_BYTES: u64 = 64 * 1024;
const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;
const GATE_TIMEOUT: Duration = Duration::from_secs(10);
const RETAINED_VERSIONS: usize = 3;

const POINTER_ENTRY: &str = "current.json";
const BLOCKED_ENTRY: &str = "blocked.json";
const CHECK_STATE_ENTRY: &str = "update-check.json";
const POINTER_SCHEMA_VERSION: u32 = 1;
const CHECK_STATE_SCHEMA_VERSION: u32 = 1;

/// Compiled-in settings; `None` means the release was built without an
/// update channel (local development) and the updater stays dormant.
pub(super) fn compiled_config() -> Option<CompiledConfig> {
  let url = MANIFEST_URL?;
  let public_key = decode_hex::<32>(PUBLIC_KEY_HEX.filter(|value| !value.is_empty())?)?;
  if url.is_empty() {
    return None;
  }
  Some(CompiledConfig {
    index_url: url.to_owned(),
    public_key,
  })
}

pub(super) struct CompiledConfig {
  index_url: String,
  public_key: [u8; 32],
}

/// Runtime settings the Shell passes to every sidecar launch.
#[derive(Debug, Clone)]
pub(super) struct Environment {
  pub store_dir: PathBuf,
  pub http_api_compatibility: VersionReq,
}

/// Read the Shell-provided update environment; `None` disables the updater.
pub(super) fn discover_environment() -> Option<Environment> {
  let store_dir = std::env::var_os("ANVIL_SIDECAR_STORE_DIR")?;
  let compatibility = std::env::var("ANVIL_HTTP_API_COMPAT").ok()?;
  let http_api_compatibility = VersionReq::parse(&compatibility).ok()?;
  let store_dir = PathBuf::from(store_dir);
  if store_dir.as_os_str().is_empty() {
    return None;
  }
  Some(Environment {
    store_dir,
    http_api_compatibility,
  })
}

/// Daily-limited check loop; runs until `cancel` fires.
pub(super) async fn run_loop(
  context: Context,
  compiled: CompiledConfig,
  environment: Environment,
  cancel: CancellationToken,
) {
  loop {
    if cancel.is_cancelled() {
      return;
    }
    let result = check_once(&context, &compiled, &environment, &cancel).await;
    match result {
      Ok(Some(version)) => tracing::info!(
        version = %version,
        "Runtime update installed; the Shell picks it up on next launch"
      ),
      Ok(None) => tracing::debug!("No compatible runtime update available"),
      Err(error) => tracing::warn!(%error, "Runtime update check failed"),
    }
    tokio::select! {
      _ = cancel.cancelled() => return,
      _ = tokio::time::sleep(CHECK_INTERVAL) => {}
    }
  }
}

/// Returns the installed version when an update was published.
pub(super) async fn check_once(
  context: &Context,
  compiled: &CompiledConfig,
  environment: &Environment,
  cancel: &CancellationToken,
) -> Result<Option<Version>, String> {
  if !record_check_window(environment)? {
    return Ok(None);
  }

  let index_bytes = fetch_index(context, &compiled.index_url, cancel)
    .await
    .map_err(|error| format!("could not fetch the update index: {error}"))?;
  let index = parse_signed_index(&index_bytes, &compiled.public_key)
    .map_err(|error| format!("the update index was rejected: {error}"))?;

  let Some(update) = plan_update(environment, &index)? else {
    return Ok(None);
  };
  install_update(context, environment, &index, &update, cancel).await?;
  Ok(Some(update.version))
}

// --- update planning ---

#[derive(Debug)]
pub(super) struct PlannedUpdate {
  version: Version,
  url: url::Url,
  sha256: String,
  size: u64,
}

/// Decide whether the index offers an installable update for this process.
pub(super) fn plan_update(
  environment: &Environment,
  index: &RuntimeIndex,
) -> Result<Option<PlannedUpdate>, String> {
  let current = Version::parse(env!("CARGO_PKG_VERSION"))
    .map_err(|_| "the runtime cannot parse its own version".to_owned())?;
  let candidate = Version::parse(&index.version)
    .map_err(|_| format!("index version {:?} is invalid", index.version))?;
  if candidate <= current {
    return Ok(None);
  }
  if !environment
    .http_api_compatibility
    .matches(&Version::parse(&index.http_api_version).map_err(|_| {
      format!(
        "index HTTP API version {:?} is invalid",
        index.http_api_version
      )
    })?)
  {
    tracing::warn!(
      candidate = %candidate,
      api = %index.http_api_version,
      accepted = %environment.http_api_compatibility,
      "Skipping a runtime update outside the accepted API range"
    );
    return Ok(None);
  }
  if blocked_versions(environment).contains(&index.version) {
    tracing::warn!(candidate = %candidate, "Skipping a blocked runtime version");
    return Ok(None);
  }
  if pointer_version(environment).as_deref() == Some(index.version.as_str()) {
    return Ok(None);
  }

  let target = current_target()
    .and_then(|target| index.targets.get(target))
    .ok_or_else(|| "the index has no entry for this platform".to_owned())?;
  Ok(Some(PlannedUpdate {
    version: candidate,
    url: url::Url::parse(&target.url)
      .map_err(|_| format!("invalid target URL {:?}", target.url))?,
    sha256: target.sha256.to_ascii_lowercase(),
    size: target.size,
  }))
}

// --- installation ---

async fn install_update(
  context: &Context,
  environment: &Environment,
  index: &RuntimeIndex,
  update: &PlannedUpdate,
  cancel: &CancellationToken,
) -> Result<(), String> {
  let staging_dir = environment
    .store_dir
    .join("staging")
    .join(format!("update-{}", update.version));
  std::fs::create_dir_all(&staging_dir)
    .map_err(|error| format!("could not create the staging directory: {error}"))?;
  let download_target = staging_dir.join(sidecar_binary_name());

  if cancel.is_cancelled() {
    return Err("the update was cancelled".to_owned());
  }
  context
    .download_file(
      &update.url,
      &download_target,
      Some(&update.sha256),
      Some(update.size),
      cancel,
    )
    .await
    .map_err(|error| format!("the runtime binary download failed: {error}"))?;

  make_executable(&download_target)
    .map_err(|error| format!("could not mark the runtime binary executable: {error}"))?;
  verify_candidate_binary(&download_target, &update.version, &index.http_api_version)
    .await
    .map_err(|error| format!("the downloaded runtime failed its gate check: {error}"))?;

  let version_dir = environment
    .store_dir
    .join("versions")
    .join(update.version.to_string());
  let published_binary = version_dir.join(sidecar_binary_name());
  if !published_binary.exists() {
    std::fs::create_dir_all(&version_dir)
      .map_err(|error| format!("could not create the version directory: {error}"))?;
    std::fs::rename(&download_target, &published_binary)
      .map_err(|error| format!("could not publish the runtime binary: {error}"))?;
  } else {
    std::fs::remove_file(&download_target).ok();
  }

  write_pointer(environment, &update.version, &index.http_api_version)?;
  clean_old_versions(environment, &update.version)?;
  Ok(())
}

/// The binary must report the version and HTTP API version the signed index
/// declares, proving the download is intact and built from the same tree.
async fn verify_candidate_binary(
  binary: &Path,
  version: &Version,
  http_api_version: &str,
) -> Result<(), String> {
  let report_version = gate_probe(binary, "--version").await?;
  if report_version != format!("anvil {version}") {
    return Err(format!("binary reports {report_version:?}"));
  }
  let report_api = gate_probe(binary, "--serve-api-version").await?;
  if report_api != http_api_version {
    return Err(format!(
      "binary reports HTTP API {report_api:?}, the index declares {http_api_version:?}"
    ));
  }
  Ok(())
}

async fn gate_probe(binary: &Path, argument: &str) -> Result<String, String> {
  let output = tokio::process::Command::new(binary).arg(argument).output();
  let output = tokio::time::timeout(GATE_TIMEOUT, output)
    .await
    .map_err(|_| format!("the probe with {argument:?} timed out"))?
    .map_err(|error| format!("the probe with {argument:?} failed to spawn: {error}"))?;
  if !output.status.success() {
    let status = output.status;
    return Err(format!("the probe with {argument:?} exited with {status}"));
  }
  let report = String::from_utf8_lossy(&output.stdout);
  Ok(String::from(
    report
      .lines()
      .next()
      .unwrap_or_default()
      .trim_end_matches(['\r', '\n']),
  ))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
  use std::os::unix::fs::PermissionsExt;
  std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
    .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn make_executable(_path: &Path) -> Result<(), String> {
  Ok(())
}

pub(super) fn sidecar_binary_name() -> &'static str {
  if cfg!(windows) { "anvil.exe" } else { "anvil" }
}

// --- index fetching and validation ---

async fn fetch_index(
  context: &Context,
  index_url: &str,
  cancel: &CancellationToken,
) -> Result<Vec<u8>, String> {
  let parsed = url::Url::parse(index_url)
    .map_err(|_| format!("the configured index URL {index_url:?} is invalid"))?;
  if cancel.is_cancelled() {
    return Err("the update was cancelled".to_owned());
  }
  context
    .fetch_bounded(&parsed, MAX_INDEX_BYTES)
    .await
    .map_err(|error| error.to_string())
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IndexEnvelope {
  payload: String,
  signature: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuntimeIndex {
  pub schema_version: u32,
  pub product: String,
  pub version: String,
  pub http_api_version: String,
  pub targets: HashMap<String, IndexTarget>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IndexTarget {
  pub binary: String,
  pub sha256: String,
  pub size: u64,
  pub url: String,
}

pub(super) fn parse_signed_index(
  source: &[u8],
  public_key: &[u8; 32],
) -> Result<RuntimeIndex, String> {
  if source.len() as u64 > MAX_INDEX_BYTES {
    return Err("the index exceeds its size limit".to_owned());
  }
  let envelope: IndexEnvelope = serde_json::from_slice(source)
    .map_err(|_| "the index is not a valid signed envelope".to_owned())?;
  if envelope.payload.len() as u64 > MAX_INDEX_BYTES * 2 {
    return Err("the index payload exceeds its size limit".to_owned());
  }
  let payload = decode_variable_hex(&envelope.payload)?;
  let signature = decode_hex::<64>(&envelope.signature)
    .ok_or_else(|| "the index signature is not a hex value".to_owned())?;
  ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public_key)
    .verify(&payload, &signature)
    .map_err(|_| "the index signature is invalid".to_owned())?;

  let index: RuntimeIndex = serde_json::from_slice(&payload)
    .map_err(|_| "the signed index payload is invalid".to_owned())?;
  if index.schema_version != 1 || index.product != "anvil" || index.targets.is_empty() {
    return Err("the index payload has unsupported fields".to_owned());
  }
  match Version::parse(&index.version) {
    Ok(version) if version.to_string() == index.version => {}
    _ => return Err("the index version is not a canonical semver".to_owned()),
  }
  if Version::parse(&index.http_api_version).is_err() {
    return Err("the index HTTP API version is invalid".to_owned());
  }
  for target in index.targets.values() {
    if target.size == 0 || target.size > MAX_BINARY_BYTES {
      return Err("a target size is out of bounds".to_owned());
    }
    if target.sha256.len() != 64 || !target.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
      return Err("a target digest is not a SHA-256 hex".to_owned());
    }
    if target.binary.is_empty()
      || target.binary.contains('/')
      || target.binary.contains('\\')
      || target.binary.contains(':')
    {
      return Err("a target binary name is not a plain file name".to_owned());
    }
    let url = url::Url::parse(&target.url).map_err(|_| "a target URL is invalid".to_owned())?;
    let trusted_transport = url.scheme() == "https";
    #[cfg(test)]
    let trusted_transport = trusted_transport
      || (url.scheme() == "http"
        && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")));
    if !trusted_transport
      || url.host_str().is_none()
      || !url.username().is_empty()
      || url.password().is_some()
      || url.fragment().is_some()
    {
      return Err("a target URL is not a plain HTTPS link".to_owned());
    }
  }
  Ok(index)
}

// --- sidecar store files ---

/// True when the 24-hour window allows a new check, and records the attempt.
fn record_check_window(environment: &Environment) -> Result<bool, String> {
  let path = environment.store_dir.join(CHECK_STATE_ENTRY);
  if let Ok(bytes) = read_bounded(&path)
    && let Ok(document) = serde_json::from_slice::<CheckStateDocument>(&bytes)
    && document.schema_version == CHECK_STATE_SCHEMA_VERSION
    && UNIX_EPOCH
      .checked_add(Duration::from_secs(document.last_check_unix_seconds))
      .and_then(|checked| SystemTime::now().duration_since(checked).ok())
      .is_some_and(|elapsed| elapsed < CHECK_INTERVAL)
  {
    return Ok(false);
  }
  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_err(|_| "the clock predates the Unix epoch".to_owned())?
    .as_secs();
  let document = CheckStateDocument {
    schema_version: CHECK_STATE_SCHEMA_VERSION,
    last_check_unix_seconds: now,
  };
  write_atomic(
    &path,
    &serde_json::to_vec(&document).map_err(|error| error.to_string())?,
  )
  .map(|()| true)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckStateDocument {
  schema_version: u32,
  last_check_unix_seconds: u64,
}

/// Pointer document the Shell reads; the schema matches its expectations.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PointerDocument {
  schema_version: u32,
  version: String,
  sidecar_entry: String,
  http_api_version: String,
}

fn write_pointer(
  environment: &Environment,
  version: &Version,
  http_api_version: &str,
) -> Result<(), String> {
  let document = PointerDocument {
    schema_version: POINTER_SCHEMA_VERSION,
    version: version.to_string(),
    sidecar_entry: format!("versions/{}/{}", version, sidecar_binary_name()),
    http_api_version: http_api_version.to_owned(),
  };
  write_atomic(
    &environment.store_dir.join(POINTER_ENTRY),
    &serde_json::to_vec(&document).map_err(|error| error.to_string())?,
  )
}

fn pointer_version(environment: &Environment) -> Option<String> {
  let bytes = read_bounded(&environment.store_dir.join(POINTER_ENTRY)).ok()?;
  let document = serde_json::from_slice::<PointerDocument>(&bytes).ok()?;
  (document.schema_version == POINTER_SCHEMA_VERSION).then_some(document.version)
}

fn blocked_versions(environment: &Environment) -> Vec<String> {
  #[derive(Debug, serde::Deserialize)]
  #[serde(deny_unknown_fields)]
  struct BlockedDocument {
    schema_version: u32,
    #[serde(default)]
    versions: Vec<String>,
  }
  read_bounded(&environment.store_dir.join(BLOCKED_ENTRY))
    .ok()
    .and_then(|bytes| serde_json::from_slice::<BlockedDocument>(&bytes).ok())
    .filter(|document| document.schema_version == 1)
    .map(|document| document.versions)
    .unwrap_or_default()
}

fn clean_old_versions(environment: &Environment, keep: &Version) -> Result<(), String> {
  let versions_dir = environment.store_dir.join("versions");
  let Ok(entries) = std::fs::read_dir(&versions_dir) else {
    return Ok(());
  };
  let mut versions: Vec<Version> = entries
    .flatten()
    .filter_map(|entry| entry.file_name().into_string().ok())
    .filter_map(|name| Version::parse(&name).ok())
    .collect();
  versions.sort_unstable();
  versions.reverse();
  versions.retain(|version| version != keep);
  for version in versions.iter().skip(RETAINED_VERSIONS - 1) {
    let path = versions_dir.join(version.to_string());
    if let Err(error) = std::fs::remove_dir_all(&path) {
      tracing::warn!(%error, version = %version, "Could not remove an old runtime version");
    }
  }
  Ok(())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
  let metadata = std::fs::symlink_metadata(path)
    .map_err(|error| format!("could not read {}: {error}", path.display()))?;
  if metadata.file_type().is_symlink() || !metadata.is_file() {
    return Err(format!("{} is not a regular file", path.display()));
  }
  if metadata.len() > MAX_INDEX_BYTES {
    return Err(format!("{} exceeds its size limit", path.display()));
  }
  std::fs::read(path).map_err(|error| error.to_string())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
  use std::io::Write;
  let parent = path
    .parent()
    .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
  std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
  let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
  temporary
    .write_all(bytes)
    .and_then(|_| temporary.as_file().sync_all())
    .map_err(|error| error.to_string())?;
  temporary
    .persist(path)
    .map(|_| ())
    .map_err(|error| error.error.to_string())
}

// --- helpers ---

/// Map this process's platform to the release target triple.
pub(super) fn current_target() -> Option<&'static str> {
  match (std::env::consts::OS, std::env::consts::ARCH) {
    ("macos", "aarch64") => Some("aarch64-apple-darwin"),
    ("macos", "x86_64") => Some("x86_64-apple-darwin"),
    ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
    ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
    _ => None,
  }
}

fn decode_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
  let bytes = decode_variable_hex(value).ok()?;
  if bytes.len() == N {
    Some(bytes.try_into().expect("length checked"))
  } else {
    None
  }
}

fn decode_variable_hex(value: &str) -> Result<Vec<u8>, String> {
  if !value.len().is_multiple_of(2) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    return Err("a hex field is malformed".to_owned());
  }
  (0..value.len())
    .step_by(2)
    .map(|index| {
      u8::from_str_radix(&value[index..index + 2], 16)
        .map_err(|_| "a hex field is malformed".to_owned())
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  const TEST_SEED: [u8; 32] = [0x42; 32];

  fn environment() -> (tempfile::TempDir, Environment) {
    let root = tempfile::tempdir().unwrap();
    let environment = Environment {
      store_dir: root.path().join("sidecar-store"),
      http_api_compatibility: VersionReq::parse("^1.0.0").unwrap(),
    };
    std::fs::create_dir_all(&environment.store_dir).unwrap();
    (root, environment)
  }

  fn test_key() -> ([u8; 32], ring::signature::Ed25519KeyPair) {
    use ring::signature::KeyPair as _;
    let key_pair = ring::signature::Ed25519KeyPair::from_seed_unchecked(&TEST_SEED).unwrap();
    let mut public_key = [0u8; 32];
    public_key.copy_from_slice(key_pair.public_key().as_ref());
    (public_key, key_pair)
  }

  fn sign_index(index: &RuntimeIndex, key_pair: &ring::signature::Ed25519KeyPair) -> Vec<u8> {
    let payload = serde_json::to_vec(index).unwrap();
    let signature = key_pair.sign(&payload);
    let envelope = IndexEnvelope {
      payload: payload.iter().map(|byte| format!("{byte:02x}")).collect(),
      signature: signature
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect(),
    };
    serde_json::to_vec(&envelope).unwrap()
  }

  fn test_index(version: &str, api: &str, url: &str, sha256: &str, size: u64) -> RuntimeIndex {
    let mut targets = HashMap::new();
    targets.insert(
      current_target()
        .expect("tests run on a released target")
        .to_owned(),
      IndexTarget {
        binary: format!("anvil-{version}-test"),
        sha256: sha256.to_owned(),
        size,
        url: url.to_owned(),
      },
    );
    RuntimeIndex {
      schema_version: 1,
      product: "anvil".to_owned(),
      version: version.to_owned(),
      http_api_version: api.to_owned(),
      targets,
    }
  }

  #[test]
  fn accepts_a_well_formed_signed_index() {
    let (public_key, key_pair) = test_key();
    let index = test_index(
      "99.0.0",
      "1.0.0",
      "https://example.test/anvil",
      &"ab".repeat(32),
      8,
    );
    let envelope = sign_index(&index, &key_pair);
    let parsed = parse_signed_index(&envelope, &public_key).unwrap();
    assert_eq!(parsed.version, "99.0.0");
  }

  #[test]
  fn rejects_a_tampered_signature() {
    let (public_key, key_pair) = test_key();
    let index = test_index(
      "99.0.0",
      "1.0.0",
      "https://example.test/anvil",
      &"ab".repeat(32),
      8,
    );
    let mut envelope = sign_index(&index, &key_pair);
    envelope[10] = if envelope[10] == b'0' { b'1' } else { b'0' };
    assert!(parse_signed_index(&envelope, &public_key).is_err());
  }

  #[test]
  fn rejects_insecure_and_malformed_payloads() {
    let (public_key, key_pair) = test_key();
    let sha = "ab".repeat(32);
    for index in [
      test_index("99.0.0", "1.0.0", "http://example.test/anvil", &sha, 8),
      test_index(
        "99.0.0",
        "1.0.0",
        "https://user@example.test/anvil",
        &sha,
        8,
      ),
      test_index("v99.0.0", "1.0.0", "https://example.test/anvil", &sha, 8),
      test_index("99.0.0", "1.x", "https://example.test/anvil", &sha, 8),
      test_index(
        "99.0.0",
        "1.0.0",
        "https://example.test/anvil",
        "tooshort",
        8,
      ),
    ] {
      let envelope = sign_index(&index, &key_pair);
      assert!(
        parse_signed_index(&envelope, &public_key).is_err(),
        "{:?}",
        index.version
      );
    }
  }

  #[test]
  fn plans_only_newer_compatible_unblocked_updates() {
    let (_root, environment) = environment();
    let index = test_index(
      "99.0.0",
      "1.0.0",
      "https://example.test/anvil",
      &"ab".repeat(32),
      8,
    );

    // A compatible newer version is planned.
    let planned = plan_update(&environment, &index).unwrap().unwrap();
    assert_eq!(planned.version, Version::parse("99.0.0").unwrap());

    // Outside the accepted API range.
    let mut incompatible = environment.clone();
    incompatible.http_api_compatibility = VersionReq::parse("^2.0.0").unwrap();
    assert!(plan_update(&incompatible, &index).unwrap().is_none());

    // Blocked by the Shell after a failed launch.
    let blocked = serde_json::json!({"schema_version": 1, "versions": ["99.0.0"]});
    std::fs::write(
      environment.store_dir.join(BLOCKED_ENTRY),
      blocked.to_string(),
    )
    .unwrap();
    assert!(plan_update(&environment, &index).unwrap().is_none());
  }

  #[test]
  fn skips_when_the_pointer_already_targets_the_candidate() {
    let (_root, environment) = environment();
    write_pointer(&environment, &Version::parse("99.0.0").unwrap(), "1.0.0").unwrap();
    let index = test_index(
      "99.0.0",
      "1.0.0",
      "https://example.test/anvil",
      &"ab".repeat(32),
      8,
    );
    assert!(plan_update(&environment, &index).unwrap().is_none());
  }

  #[cfg(unix)]
  #[test]
  fn retention_keeps_the_newest_versions() {
    let (_root, environment) = environment();
    let versions_dir = environment.store_dir.join("versions");
    for version in ["0.1.0", "0.2.0", "0.3.0", "0.4.0", "0.5.0"] {
      std::fs::create_dir_all(versions_dir.join(version)).unwrap();
    }
    clean_old_versions(&environment, &Version::parse("0.5.0").unwrap()).unwrap();
    for version in ["0.3.0", "0.4.0", "0.5.0"] {
      assert!(
        versions_dir.join(version).exists(),
        "{version} should be retained"
      );
    }
    for version in ["0.1.0", "0.2.0"] {
      assert!(
        !versions_dir.join(version).exists(),
        "{version} should be removed"
      );
    }
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn installs_a_signed_update_end_to_end() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let (_root, environment) = environment();
    let (public_key, key_pair) = test_key();

    // A stub "binary" that answers the gate probes.
    let root = tempfile::tempdir().unwrap();
    let binary = root.path().join("anvil-99.0.0-test");
    std::fs::write(
      &binary,
      "#!/bin/sh\ncase \"$1\" in\n--version) printf 'anvil 99.0.0\\n' ;;\n--serve-api-version) printf '1.0.0\\n' ;;\nesac\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let binary_bytes = std::fs::read(&binary).unwrap();
    let binary_len = binary_bytes.len() as u64;
    let digest = {
      let mut context = ring::digest::Context::new(&ring::digest::SHA256);
      context.update(&binary_bytes);
      context
        .finish()
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
    };

    // A loop-serving fixture for both the index and the binary; q2q-http may
    // issue several requests for one download.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let index = test_index(
      "99.0.0",
      "1.0.0",
      &format!("http://{address}/anvil-99.0.0-test"),
      &digest,
      binary_len,
    );
    let envelope = sign_index(&index, &key_pair);
    let responses = HashMap::from([
      ("/runtime-index.json".to_owned(), envelope),
      ("/anvil-99.0.0-test".to_owned(), binary_bytes),
    ]);
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = stop.clone();
    let server = std::thread::spawn(move || {
      while !server_stop.load(Ordering::Relaxed) {
        match listener.accept() {
          Ok((mut stream, _)) => respond(&mut stream, &responses),
          Err(_) => std::thread::sleep(Duration::from_millis(1)),
        }
      }
    });

    let context =
      Context::new(anvil_core::Paths::discover(Some(root.path().join("data"))).unwrap()).unwrap();
    let compiled = CompiledConfig {
      index_url: format!("http://{address}/runtime-index.json"),
      public_key,
    };
    let cancel = CancellationToken::new();
    let installed = check_once(&context, &compiled, &environment, &cancel)
      .await
      .unwrap();
    assert_eq!(installed, Some(Version::parse("99.0.0").unwrap()));

    let published = environment
      .store_dir
      .join("versions")
      .join("99.0.0")
      .join("anvil");
    assert!(published.exists());
    let pointer: serde_json::Value =
      serde_json::from_slice(&std::fs::read(environment.store_dir.join(POINTER_ENTRY)).unwrap())
        .unwrap();
    assert_eq!(pointer["version"], "99.0.0");
    assert_eq!(pointer["sidecar_entry"], "versions/99.0.0/anvil");
    assert_eq!(pointer["http_api_version"], "1.0.0");
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
  }

  #[cfg(unix)]
  fn respond(stream: &mut std::net::TcpStream, responses: &HashMap<String, Vec<u8>>) {
    use std::io::{Read as _, Write as _};
    stream.set_nonblocking(false).unwrap();
    stream
      .set_read_timeout(Some(Duration::from_secs(15)))
      .unwrap();
    let mut request = Vec::new();
    let mut byte = [0u8; 1];
    while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
      if stream.read(&mut byte).unwrap() == 0 {
        return;
      }
      request.push(byte[0]);
    }
    let request = String::from_utf8_lossy(&request);
    let path = request
      .lines()
      .next()
      .unwrap_or("")
      .split_whitespace()
      .nth(1)
      .unwrap_or("/");
    if let Some(body) = responses.get(path) {
      let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
      );
      let _ = stream.write_all(body);
    } else {
      let _ = stream
        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    }
  }
}
