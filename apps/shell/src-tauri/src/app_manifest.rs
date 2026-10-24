use semver::{Version, VersionReq};
use serde::Deserialize;

const CURRENT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppManifest {
  pub schema_version: u32,
  pub app_id: String,
  pub version: String,
  pub target: String,
  pub page_entry: String,
  pub sidecar_entry: String,
  #[serde(default)]
  pub sidecar_args: Vec<String>,
  pub http_api_version: String,
  /// Semver requirement of runtime HTTP API versions the packaged UI accepts;
  /// the bundled seed's `http_api_version` must satisfy it, and so must any
  /// sidecar override the runtime publishes later.
  pub http_api_compatibility: String,
  pub shell_api_compatibility: String,
}

impl AppManifest {
  pub fn parse_and_validate(
    source: &[u8],
    expected_app_id: &str,
    expected_target: &str,
    shell_api_version: &str,
  ) -> Result<Self, String> {
    let manifest: Self = serde_json::from_slice(source)
      .map_err(|error| format!("could not parse JSON or required fields: {error}"))?;

    if manifest.schema_version != CURRENT_SCHEMA_VERSION {
      return Err(format!(
        "unsupported schema_version {}; expected {CURRENT_SCHEMA_VERSION}",
        manifest.schema_version
      ));
    }

    validate_app_id("app_id", &manifest.app_id)?;
    validate_app_id("ANVIL_DEFAULT_APP_ID", expected_app_id)?;
    if manifest.app_id != expected_app_id {
      return Err(format!(
        "app_id {} does not match configured default App ID {expected_app_id}",
        manifest.app_id
      ));
    }

    if manifest.target != expected_target {
      return Err(format!(
        "target {} does not match Cargo build target {expected_target}",
        manifest.target
      ));
    }

    Version::parse(&manifest.version)
      .map_err(|error| format!("version must be a semantic version: {error}"))?;
    let http_api_version = Version::parse(&manifest.http_api_version)
      .map_err(|error| format!("http_api_version must be a semantic version: {error}"))?;
    let http_api_compatibility =
      VersionReq::parse(&manifest.http_api_compatibility).map_err(|error| {
        format!("http_api_compatibility must be a semantic version requirement: {error}")
      })?;
    if !http_api_compatibility.matches(&http_api_version) {
      return Err(format!(
        "http_api_version {} is outside the manifest compatibility range {}",
        manifest.http_api_version, manifest.http_api_compatibility
      ));
    }
    let shell_api_version = Version::parse(shell_api_version)
      .map_err(|error| format!("ANVIL_SHELL_API_VERSION must be a semantic version: {error}"))?;
    let compatibility = VersionReq::parse(&manifest.shell_api_compatibility).map_err(|error| {
      format!("shell_api_compatibility must be a semantic version requirement: {error}")
    })?;
    if !compatibility.matches(&shell_api_version) {
      return Err(format!(
        "Shell API version {shell_api_version} is outside App compatibility range {}",
        manifest.shell_api_compatibility
      ));
    }

    validate_package_entry("page_entry", &manifest.page_entry)?;
    validate_package_entry("sidecar_entry", &manifest.sidecar_entry)?;
    if manifest.sidecar_args.len() > 16
      || manifest
        .sidecar_args
        .iter()
        .any(|arg| arg.len() > 1024 || arg.contains('\0'))
    {
      return Err(
        "sidecar_args must contain at most 16 arguments, each at most 1024 bytes without NUL"
          .into(),
      );
    }

    Ok(manifest)
  }
}

pub(crate) fn validate_app_id(field: &str, app_id: &str) -> Result<(), String> {
  let valid = app_id.split('.').count() >= 2
    && app_id.split('.').all(|part| {
      let first = part.chars().next();
      let last = part.chars().last();
      !part.is_empty()
        && first
          .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        && last
          .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        && part.chars().all(|character| {
          character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    });

  if valid {
    Ok(())
  } else {
    Err(format!(
      "{field} must be a dotted lowercase App ID, such as anvil.desktop"
    ))
  }
}

fn validate_package_entry(field: &str, entry: &str) -> Result<(), String> {
  let valid = !entry.is_empty()
    && !entry.starts_with('/')
    && !entry.contains('\\')
    && !entry.contains(':')
    && entry
      .split('/')
      .all(|component| !component.is_empty() && component != "." && component != "..");

  if valid {
    Ok(())
  } else {
    Err(format!(
      "{field} must be a normalized relative path inside the App package"
    ))
  }
}
