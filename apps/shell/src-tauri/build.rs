#[path = "src/app_manifest.rs"]
mod app_manifest;
#[path = "src/shell_commands.rs"]
mod shell_commands;

use std::{env, fs, path::PathBuf};
use tauri_build::{AppManifest, Attributes};

fn main() {
  if let Err(error) = configure_build() {
    panic!("{error}");
  }

  tauri_build::try_build(
    Attributes::new().app_manifest(AppManifest::new().commands(shell_commands::SHELL_COMMANDS)),
  )
  .unwrap_or_else(|error| panic!("failed to configure Tauri permissions: {error:#}"));
}

fn configure_build() -> Result<(), String> {
  for name in [
    "ANVIL_DEFAULT_APP_ID",
    "ANVIL_SHELL_API_VERSION",
    "ANVIL_APP_MANIFEST",
    "ANVIL_UPDATE_MANIFEST_URL",
    "ANVIL_UPDATE_PUBLIC_KEY",
  ] {
    println!("cargo:rerun-if-env-changed={name}");
  }

  let target =
    env::var("TARGET").map_err(|error| format!("Cargo did not provide TARGET: {error}"))?;
  let shell_manifest_dir = PathBuf::from(
    env::var_os("CARGO_MANIFEST_DIR").expect("Cargo did not provide CARGO_MANIFEST_DIR"),
  );
  let bundled_app_directory = shell_manifest_dir.join("../../desktop/baseline");
  fs::create_dir_all(&bundled_app_directory).map_err(|error| {
    format!(
      "cannot create bundled App package directory {}: {error}",
      bundled_app_directory.display()
    )
  })?;

  let default_app_id = env::var("ANVIL_DEFAULT_APP_ID").unwrap_or_else(|_| "anvil.desktop".into());
  let shell_api_version = env::var("ANVIL_SHELL_API_VERSION").unwrap_or_else(|_| "1.0.0".into());
  let manifest_path = env::var_os("ANVIL_APP_MANIFEST")
    .map(PathBuf::from)
    .unwrap_or_else(|| shell_manifest_dir.join(format!("../../desktop/manifests/{target}.json")));

  println!("cargo:rerun-if-changed={}", manifest_path.display());

  let manifest_source = fs::read(&manifest_path).map_err(|error| {
    format!(
      "cannot read App manifest {}: {error}",
      manifest_path.display()
    )
  })?;
  let app_manifest = app_manifest::AppManifest::parse_and_validate(
    &manifest_source,
    &default_app_id,
    &target,
    &shell_api_version,
  )
  .map_err(|error| format!("invalid App manifest {}: {error}", manifest_path.display()))?;

  println!("cargo:rustc-env=ANVIL_DEFAULT_APP_ID={default_app_id}");
  println!(
    "cargo:rustc-env=ANVIL_BUNDLED_APP_VERSION={}",
    app_manifest.version
  );
  println!("cargo:rustc-env=ANVIL_TARGET_PLATFORM={target}");
  println!("cargo:rustc-env=ANVIL_SHELL_API_VERSION={shell_api_version}");
  configure_update_source()?;

  Ok(())
}

fn configure_update_source() -> Result<(), String> {
  let manifest_url = env::var("ANVIL_UPDATE_MANIFEST_URL").ok();
  let public_key = env::var("ANVIL_UPDATE_PUBLIC_KEY").ok();
  match (manifest_url, public_key) {
    (None, None) => Ok(()),
    (Some(manifest_url), Some(public_key)) => {
      let parsed = url::Url::parse(&manifest_url)
        .map_err(|error| format!("invalid ANVIL_UPDATE_MANIFEST_URL: {error}"))?;
      if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
      {
        return Err(
          "ANVIL_UPDATE_MANIFEST_URL must be HTTPS without credentials or a fragment".into(),
        );
      }
      if public_key.len() != 64 || !public_key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("ANVIL_UPDATE_PUBLIC_KEY must be a 32-byte Ed25519 public key in hex".into());
      }
      println!("cargo:rustc-env=ANVIL_UPDATE_MANIFEST_URL={manifest_url}");
      println!("cargo:rustc-env=ANVIL_UPDATE_PUBLIC_KEY={public_key}");
      Ok(())
    }
    _ => Err(
      "ANVIL_UPDATE_MANIFEST_URL and ANVIL_UPDATE_PUBLIC_KEY must be configured together".into(),
    ),
  }
}
