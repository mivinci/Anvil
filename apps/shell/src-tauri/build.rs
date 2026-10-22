#[path = "src/app_manifest.rs"]
mod app_manifest;

use std::{env, fs, path::PathBuf};

fn main() {
  if let Err(error) = configure_build() {
    panic!("{error}");
  }

  tauri_build::build();
}

fn configure_build() -> Result<(), String> {
  for name in [
    "ANVIL_DEFAULT_APP_ID",
    "ANVIL_SHELL_API_VERSION",
    "ANVIL_APP_MANIFEST",
  ] {
    println!("cargo:rerun-if-env-changed={name}");
  }

  let target =
    env::var("TARGET").map_err(|error| format!("Cargo did not provide TARGET: {error}"))?;
  let default_app_id = env::var("ANVIL_DEFAULT_APP_ID").unwrap_or_else(|_| "anvil.desktop".into());
  let shell_api_version = env::var("ANVIL_SHELL_API_VERSION").unwrap_or_else(|_| "1.0.0".into());
  let manifest_path = env::var_os("ANVIL_APP_MANIFEST")
    .map(PathBuf::from)
    .unwrap_or_else(|| {
      PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo did not provide CARGO_MANIFEST_DIR"),
      )
      .join(format!("../../desktop/manifests/{target}.json"))
    });

  println!("cargo:rerun-if-changed={}", manifest_path.display());

  let manifest_source = fs::read(&manifest_path).map_err(|error| {
    format!(
      "cannot read App manifest {}: {error}",
      manifest_path.display()
    )
  })?;
  app_manifest::AppManifest::parse_and_validate(
    &manifest_source,
    &default_app_id,
    &target,
    &shell_api_version,
  )
  .map_err(|error| format!("invalid App manifest {}: {error}", manifest_path.display()))?;

  println!("cargo:rustc-env=ANVIL_DEFAULT_APP_ID={default_app_id}");
  println!("cargo:rustc-env=ANVIL_TARGET_PLATFORM={target}");
  println!("cargo:rustc-env=ANVIL_SHELL_API_VERSION={shell_api_version}");

  Ok(())
}
