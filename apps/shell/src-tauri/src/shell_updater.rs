use crate::app_storage::AppStorage;
use q2q_http::Downloader;
use ring::signature;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
  sync::{Arc, Mutex},
  time::{Duration, SystemTime},
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

const UPDATE_MANIFEST_URL: Option<&str> = option_env!("ANVIL_SHELL_UPDATE_MANIFEST_URL");
const UPDATE_PUBLIC_KEY: Option<&str> = option_env!("ANVIL_SHELL_UPDATE_PUBLIC_KEY");
const MAX_MANIFEST_BYTES: usize = 128 * 1024;
const UPDATE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellUpdateState {
  NotConfigured,
  Idle,
  Checking,
  Available,
  UpToDate,
  Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellUpdateFailure {
  StorageUnavailable,
  Network,
  InvalidSignature,
  InvalidRelease,
  Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellUpdateStatus {
  pub state: ShellUpdateState,
  pub failure: Option<ShellUpdateFailure>,
  pub current_version: String,
  pub available_version: Option<String>,
  pub release_url: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellUpdateCheckOutcome {
  Started,
  AlreadyRunning,
  RecentlyChecked,
  NotConfigured,
  Failed,
}

#[derive(Debug, Clone)]
struct ShellUpdateConfig {
  manifest_url: String,
  public_key: [u8; 32],
  target: String,
  current_version: Version,
}

#[derive(Debug)]
struct ShellUpdateInner {
  config: Option<ShellUpdateConfig>,
  status: Mutex<ShellUpdateStatus>,
  cancellation: Mutex<Option<CancellationToken>>,
}

#[derive(Debug, Clone)]
pub struct ShellUpdateService {
  inner: Arc<ShellUpdateInner>,
}

impl ShellUpdateService {
  pub fn new() -> Self {
    let current_version = Version::parse(env!("ANVIL_SHELL_VERSION"))
      .expect("build script validates the Shell version");
    let config = build_update_config(current_version.clone());
    let state = if config.is_some() {
      ShellUpdateState::Idle
    } else {
      ShellUpdateState::NotConfigured
    };
    Self {
      inner: Arc::new(ShellUpdateInner {
        config,
        status: Mutex::new(ShellUpdateStatus {
          state,
          failure: None,
          current_version: current_version.to_string(),
          available_version: None,
          release_url: None,
        }),
        cancellation: Mutex::new(None),
      }),
    }
  }

  pub fn status(&self) -> ShellUpdateStatus {
    self
      .inner
      .status
      .lock()
      .map(|status| status.clone())
      .unwrap_or_else(|_| ShellUpdateStatus {
        state: ShellUpdateState::Failed,
        failure: Some(ShellUpdateFailure::StorageUnavailable),
        current_version: env!("ANVIL_SHELL_VERSION").to_owned(),
        available_version: None,
        release_url: None,
      })
  }

  pub fn check_for_updates(&self) -> ShellUpdateCheckOutcome {
    self.start_update_check(false)
  }

  pub fn check_for_updates_now(&self) -> ShellUpdateCheckOutcome {
    self.start_update_check(true)
  }

  fn start_update_check(&self, force: bool) -> ShellUpdateCheckOutcome {
    let Some(config) = self.inner.config.clone() else {
      return ShellUpdateCheckOutcome::NotConfigured;
    };
    let mut status = match self.inner.status.lock() {
      Ok(status) => status,
      Err(_) => return ShellUpdateCheckOutcome::Failed,
    };
    if status.state == ShellUpdateState::Checking {
      return ShellUpdateCheckOutcome::AlreadyRunning;
    }

    let store = match AppStorage::for_current_user()
      .and_then(|storage| storage.app(env!("ANVIL_DEFAULT_APP_ID")))
    {
      Ok(store) => store,
      Err(_) => {
        status.state = ShellUpdateState::Failed;
        status.failure = Some(ShellUpdateFailure::StorageUnavailable);
        tracing::warn!(failure = ?status.failure, "Shell update check could not access storage");
        return ShellUpdateCheckOutcome::Failed;
      }
    };
    let now = SystemTime::now();
    if !force {
      match store.last_shell_update_check() {
        Ok(last_check) if should_skip_update_check(last_check, now, false) => {
          return ShellUpdateCheckOutcome::RecentlyChecked;
        }
        Ok(_) => {}
        Err(_) => {
          status.state = ShellUpdateState::Failed;
          status.failure = Some(ShellUpdateFailure::StorageUnavailable);
          tracing::warn!(failure = ?status.failure, "Shell update check could not read its timestamp");
          return ShellUpdateCheckOutcome::Failed;
        }
      }
    }
    if store.record_shell_update_check(now).is_err() {
      status.state = ShellUpdateState::Failed;
      status.failure = Some(ShellUpdateFailure::StorageUnavailable);
      tracing::warn!(failure = ?status.failure, "Shell update check could not save its timestamp");
      return ShellUpdateCheckOutcome::Failed;
    }

    status.state = ShellUpdateState::Checking;
    status.failure = None;
    status.available_version = None;
    status.release_url = None;
    let cancellation = CancellationToken::new();
    if let Ok(mut slot) = self.inner.cancellation.lock() {
      *slot = Some(cancellation.clone());
    }
    drop(status);

    let inner = self.inner.clone();
    tauri::async_runtime::spawn(async move {
      let result = fetch_update(&config, &cancellation).await;
      match result {
        Ok(release) if should_offer_update(&config.current_version, &release.version) => {
          update_status(&inner, |status| {
            status.state = ShellUpdateState::Available;
            status.failure = None;
            status.available_version = Some(release.version.to_string());
            status.release_url = Some(release.release_url);
          });
          tracing::info!(version = %release.version, "Shell update is available");
        }
        Ok(_) => {
          update_status(&inner, |status| {
            status.state = ShellUpdateState::UpToDate;
            status.failure = None;
          });
        }
        Err(error) => {
          update_status(&inner, |status| {
            status.state = ShellUpdateState::Failed;
            status.failure = Some(error.failure());
          });
          tracing::warn!(failure = ?error.failure(), "Shell update check failed");
        }
      }
      if let Ok(mut slot) = inner.cancellation.lock() {
        *slot = None;
      }
    });
    ShellUpdateCheckOutcome::Started
  }

  pub fn start_background_checks(&self) {
    if self.inner.config.is_none() {
      return;
    }
    // Availability is not staged locally, so every Shell launch needs a fresh
    // result; otherwise a recent persisted hourly check could hide an update.
    let _ = self.check_for_updates_now();
    let service = self.clone();
    tauri::async_runtime::spawn(async move {
      loop {
        tokio::time::sleep(CHECK_INTERVAL).await;
        let _ = service.check_for_updates();
      }
    });
  }

  pub fn shutdown(&self) {
    if let Ok(slot) = self.inner.cancellation.lock() {
      if let Some(cancellation) = slot.as_ref() {
        cancellation.cancel();
      }
    }
  }
}

impl Default for ShellUpdateService {
  fn default() -> Self {
    Self::new()
  }
}

fn build_update_config(current_version: Version) -> Option<ShellUpdateConfig> {
  let manifest_url = UPDATE_MANIFEST_URL?;
  let public_key = decode_hex::<32>(UPDATE_PUBLIC_KEY?).ok()?;
  Some(ShellUpdateConfig {
    manifest_url: manifest_url.to_owned(),
    public_key,
    target: env!("ANVIL_TARGET_PLATFORM").to_owned(),
    current_version,
  })
}

fn should_skip_update_check(last_check: Option<SystemTime>, now: SystemTime, force: bool) -> bool {
  !force
    && last_check.is_some_and(|last_check| {
      now.duration_since(last_check).unwrap_or_default() < UPDATE_INTERVAL
    })
}

async fn fetch_update(
  config: &ShellUpdateConfig,
  cancellation: &CancellationToken,
) -> Result<ShellRelease, ShellUpdateError> {
  let downloader = Downloader::builder()
    .max_active_files(1)
    .max_requests(2)
    .max_requests_per_host(1)
    .max_disk_tasks(1)
    .build()
    .map_err(|_| ShellUpdateError::Network)?;
  let response = tokio::select! {
    _ = cancellation.cancelled() => return Err(ShellUpdateError::Cancelled),
    response = downloader.executor().get(&config.manifest_url).max_body_size(MAX_MANIFEST_BYTES as u64).send() => {
      response.map_err(|_| ShellUpdateError::Network)?
    }
  };
  let bytes = tokio::select! {
    _ = cancellation.cancelled() => return Err(ShellUpdateError::Cancelled),
    response = response.collect_bounded(MAX_MANIFEST_BYTES) => response.map_err(|_| ShellUpdateError::Network)?,
  };
  parse_signed_release(&bytes, &config.public_key, &config.target)
}

#[derive(Debug, Clone)]
struct ShellRelease {
  version: Version,
  release_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedReleaseEnvelope {
  payload: String,
  signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedRelease {
  schema_version: u32,
  channel: String,
  product_id: String,
  version: String,
  target: String,
  release_url: String,
}

fn parse_signed_release(
  source: &[u8],
  public_key: &[u8; 32],
  expected_target: &str,
) -> Result<ShellRelease, ShellUpdateError> {
  if source.len() > MAX_MANIFEST_BYTES {
    return Err(ShellUpdateError::InvalidRelease);
  }
  let envelope: SignedReleaseEnvelope =
    serde_json::from_slice(source).map_err(|_| ShellUpdateError::InvalidRelease)?;
  if envelope.payload.len() > MAX_MANIFEST_BYTES * 2 {
    return Err(ShellUpdateError::InvalidRelease);
  }
  let payload = decode_variable_hex(&envelope.payload)?;
  let signature_bytes = decode_hex::<64>(&envelope.signature)?;
  signature::UnparsedPublicKey::new(&signature::ED25519, public_key)
    .verify(&payload, &signature_bytes)
    .map_err(|_| ShellUpdateError::InvalidSignature)?;

  let release: SignedRelease =
    serde_json::from_slice(&payload).map_err(|_| ShellUpdateError::InvalidRelease)?;
  if release.schema_version != 1
    || release.channel != "stable"
    || release.product_id != "anvil-shell"
    || release.target != expected_target
  {
    return Err(ShellUpdateError::InvalidRelease);
  }
  let version = Version::parse(&release.version).map_err(|_| ShellUpdateError::InvalidRelease)?;
  if version.to_string() != release.version {
    return Err(ShellUpdateError::InvalidRelease);
  }
  let url = url::Url::parse(&release.release_url).map_err(|_| ShellUpdateError::InvalidRelease)?;
  let release_path = url.path();
  let expected_release_path = format!("/mivinci/Anvil/releases/tag/desktop/v{version}");
  if url.scheme() != "https"
    || url.host_str() != Some("github.com")
    || !url.username().is_empty()
    || url.password().is_some()
    || url.query().is_some()
    || url.fragment().is_some()
    || release_path != expected_release_path
  {
    return Err(ShellUpdateError::InvalidRelease);
  }
  Ok(ShellRelease {
    version,
    release_url: release.release_url,
  })
}

fn should_offer_update(current: &Version, candidate: &Version) -> bool {
  candidate > current && (!current.pre.is_empty() || candidate.pre.is_empty())
}

fn decode_variable_hex(source: &str) -> Result<Vec<u8>, ShellUpdateError> {
  if source.len() % 2 != 0 || !source.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    return Err(ShellUpdateError::InvalidRelease);
  }
  let mut decoded = Vec::with_capacity(source.len() / 2);
  for pair in source.as_bytes().chunks_exact(2) {
    let high = decode_nibble(pair[0]).ok_or(ShellUpdateError::InvalidRelease)?;
    let low = decode_nibble(pair[1]).ok_or(ShellUpdateError::InvalidRelease)?;
    decoded.push((high << 4) | low);
  }
  Ok(decoded)
}

fn decode_hex<const N: usize>(source: &str) -> Result<[u8; N], ShellUpdateError> {
  if source.len() != N * 2 {
    return Err(ShellUpdateError::InvalidRelease);
  }
  let decoded = decode_variable_hex(source)?;
  decoded
    .try_into()
    .map_err(|_| ShellUpdateError::InvalidRelease)
}

fn decode_nibble(byte: u8) -> Option<u8> {
  match byte {
    b'0'..=b'9' => Some(byte - b'0'),
    b'a'..=b'f' => Some(byte - b'a' + 10),
    b'A'..=b'F' => Some(byte - b'A' + 10),
    _ => None,
  }
}

fn update_status(inner: &ShellUpdateInner, update: impl FnOnce(&mut ShellUpdateStatus)) {
  if let Ok(mut status) = inner.status.lock() {
    update(&mut status);
  }
}

#[derive(Debug, Error)]
enum ShellUpdateError {
  #[error("shell update source could not be reached")]
  Network,
  #[error("shell update signature is invalid")]
  InvalidSignature,
  #[error("shell update manifest is invalid")]
  InvalidRelease,
  #[error("shell update check was cancelled")]
  Cancelled,
}

impl ShellUpdateError {
  fn failure(&self) -> ShellUpdateFailure {
    match self {
      Self::Network => ShellUpdateFailure::Network,
      Self::InvalidSignature => ShellUpdateFailure::InvalidSignature,
      Self::InvalidRelease => ShellUpdateFailure::InvalidRelease,
      Self::Cancelled => ShellUpdateFailure::Cancelled,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use ring::signature::KeyPair;

  const TEST_SEED: [u8; 32] = [9; 32];

  fn signed_manifest(payload: &[u8]) -> (Vec<u8>, [u8; 32]) {
    let key_pair = signature::Ed25519KeyPair::from_seed_unchecked(&TEST_SEED).unwrap();
    let signature = key_pair.sign(payload);
    let envelope = serde_json::json!({
      "payload": payload.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
      "signature": signature.as_ref().iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    });
    let mut public_key = [0; 32];
    public_key.copy_from_slice(key_pair.public_key().as_ref());
    (serde_json::to_vec(&envelope).unwrap(), public_key)
  }

  fn release_payload(version: &str, target: &str, release_url: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
      "schema_version": 1,
      "channel": "stable",
      "product_id": "anvil-shell",
      "version": version,
      "target": target,
      "release_url": release_url
    }))
    .unwrap()
  }

  #[test]
  fn accepts_a_signed_shell_release_for_the_expected_target() {
    let target = env!("ANVIL_TARGET_PLATFORM");
    let payload = release_payload(
      "0.2.0",
      target,
      "https://github.com/mivinci/Anvil/releases/tag/desktop/v0.2.0",
    );
    let (manifest, public_key) = signed_manifest(&payload);
    let release = parse_signed_release(&manifest, &public_key, target).unwrap();
    assert_eq!(release.version, Version::parse("0.2.0").unwrap());
    assert_eq!(
      release.release_url,
      "https://github.com/mivinci/Anvil/releases/tag/desktop/v0.2.0"
    );
  }

  #[test]
  fn accepts_a_semver_prerelease_on_the_single_release_feed() {
    let target = env!("ANVIL_TARGET_PLATFORM");
    let payload = release_payload(
      "0.2.0-alpha.1",
      target,
      "https://github.com/mivinci/Anvil/releases/tag/desktop/v0.2.0-alpha.1",
    );
    let (manifest, public_key) = signed_manifest(&payload);
    assert!(parse_signed_release(&manifest, &public_key, target).is_ok());
  }

  #[test]
  fn stable_shells_skip_prereleases_and_prereleases_follow_semver() {
    let parse = |version: &str| Version::parse(version).unwrap();
    assert!(!should_offer_update(
      &parse("0.1.0"),
      &parse("0.2.0-alpha.1")
    ));
    assert!(should_offer_update(
      &parse("0.2.0-alpha.1"),
      &parse("0.2.0-alpha.2")
    ));
    assert!(should_offer_update(
      &parse("0.2.0-alpha.2"),
      &parse("0.2.0")
    ));
    assert!(!should_offer_update(
      &parse("0.2.0-alpha.2"),
      &parse("0.2.0-alpha.1")
    ));
  }

  #[test]
  fn rejects_wrong_channel_and_product_identity() {
    for (field, value) in [("channel", "beta"), ("product_id", "other-app")] {
      let mut payload: serde_json::Value = serde_json::from_slice(&release_payload(
        "0.2.0",
        env!("ANVIL_TARGET_PLATFORM"),
        "https://github.com/mivinci/Anvil/releases/tag/desktop/v0.2.0",
      ))
      .unwrap();
      payload[field] = serde_json::Value::String(value.to_owned());
      let (manifest, public_key) = signed_manifest(&serde_json::to_vec(&payload).unwrap());
      assert!(matches!(
        parse_signed_release(&manifest, &public_key, env!("ANVIL_TARGET_PLATFORM")),
        Err(ShellUpdateError::InvalidRelease)
      ));
    }
  }

  #[test]
  fn rejects_wrong_target_and_unsafe_release_urls() {
    let payload = release_payload(
      "0.2.0",
      "another-target",
      "https://github.com/mivinci/Anvil/releases/tag/desktop/v0.2.0",
    );
    let (manifest, public_key) = signed_manifest(&payload);
    assert!(matches!(
      parse_signed_release(&manifest, &public_key, env!("ANVIL_TARGET_PLATFORM")),
      Err(ShellUpdateError::InvalidRelease)
    ));

    for unsafe_url in [
      "https://example.com/mivinci/Anvil/releases/tag/desktop/v0.2.0",
      "https://github.com/attacker/mivinci/Anvil/releases/tag/desktop/v0.2.0",
      "https://github.com/mivinci/Anvil/releases/tag/desktop/v0.2.1",
      "https://github.com/mivinci/Anvil/releases/tag/desktop-test/v0.2.0",
      "https://github.com/mivinci/Anvil/releases/tag/desktop/v0.2.0?download=1",
      "http://github.com/mivinci/Anvil/releases/tag/desktop/v0.2.0",
    ] {
      let payload = release_payload("0.2.0", env!("ANVIL_TARGET_PLATFORM"), unsafe_url);
      let (manifest, public_key) = signed_manifest(&payload);
      assert!(matches!(
        parse_signed_release(&manifest, &public_key, env!("ANVIL_TARGET_PLATFORM")),
        Err(ShellUpdateError::InvalidRelease)
      ));
    }
  }

  #[test]
  fn rejects_a_tampered_shell_manifest_signature() {
    let payload = release_payload(
      "0.2.0",
      env!("ANVIL_TARGET_PLATFORM"),
      "https://github.com/mivinci/Anvil/releases/tag/desktop/v0.2.0",
    );
    let (manifest, public_key) = signed_manifest(&payload);
    let mut envelope: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    let mut signature = envelope["signature"].as_str().unwrap().as_bytes().to_vec();
    signature[0] = if signature[0] == b'0' { b'1' } else { b'0' };
    envelope["signature"] = serde_json::Value::String(String::from_utf8(signature).unwrap());
    let manifest = serde_json::to_vec(&envelope).unwrap();
    assert!(matches!(
      parse_signed_release(&manifest, &public_key, env!("ANVIL_TARGET_PLATFORM")),
      Err(ShellUpdateError::InvalidSignature)
    ));
  }

  #[test]
  fn update_check_interval_can_be_forced_and_is_limited_to_one_day() {
    let now = SystemTime::now();
    assert!(should_skip_update_check(Some(now), now, false));
    assert!(!should_skip_update_check(Some(now), now, true));
    assert!(!should_skip_update_check(None, now, false));
    assert!(!should_skip_update_check(
      Some(now - UPDATE_INTERVAL),
      now,
      false
    ));
  }
}
