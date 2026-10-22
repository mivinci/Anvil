use anvil_shell::{
  app_protocol::AppPageProtocol,
  app_storage::{AppStorage, AppVersionState},
};
use semver::Version;
use std::fs;
use tauri::http::{Method, Request, Response};
use tempfile::TempDir;

const APP_ID: &str = env!("ANVIL_DEFAULT_APP_ID");
const TARGET: &str = env!("ANVIL_TARGET_PLATFORM");
const SHELL_API_VERSION: &str = env!("ANVIL_SHELL_API_VERSION");
const APP_VERSION: &str = "0.1.0";

fn protocol(temp: &TempDir) -> AppPageProtocol {
  AppPageProtocol::new(
    AppStorage::open(temp.path()),
    APP_ID,
    TARGET,
    SHELL_API_VERSION,
  )
}

fn manifest(version: &str) -> String {
  format!(
    r#"{{
      "schema_version": 2,
      "app_id": "{APP_ID}",
      "version": "{version}",
      "target": "{TARGET}",
      "page_entry": "web/index.html",
      "sidecar_entry": "runtime/anvil-desktop",
      "http_api_version": "1.0.0",
      "http_api_compatibility": "^1.0.0",
      "shell_api_compatibility": "^{SHELL_API_VERSION}"
    }}"#
  )
}

fn publish_fixture(temp: &TempDir, manifest_source: &str, include_page: bool) {
  let storage = AppStorage::open(temp.path()).expect("open test storage");
  let store = storage.app(APP_ID).expect("open fixture App storage");
  let version = Version::parse(APP_VERSION).expect("valid fixture version");
  let staging = store.stage_version().expect("create staging directory");
  fs::write(staging.directory().join("manifest.json"), manifest_source)
    .expect("write fixture manifest");
  if include_page {
    let page_dir = staging.directory().join("web");
    let asset_dir = page_dir.join("_app");
    fs::create_dir_all(&asset_dir).expect("create fixture page directory");
    fs::write(
      page_dir.join("index.html"),
      "<!doctype html><html><script type=module src=\"/_app/app.js\"></script></html>",
    )
    .expect("write fixture page");
    fs::write(
      asset_dir.join("app.js"),
      "document.body.dataset.loaded = 'yes';",
    )
    .expect("write fixture script");
    fs::write(asset_dir.join("style.css"), "body { color: #fff; }").expect("write fixture style");
  }

  staging.publish(&version).expect("publish fixture App");
  store
    .write_state(&AppVersionState {
      active_version: Some(version),
      ..AppVersionState::default()
    })
    .expect("set fixture App active");
}

fn request(method: Method, uri: &str) -> Request<Vec<u8>> {
  Request::builder()
    .method(method)
    .uri(uri)
    .body(Vec::new())
    .expect("valid test request")
}

fn body(response: Response<Vec<u8>>) -> String {
  String::from_utf8(response.into_body()).expect("protocol response is UTF-8")
}

#[test]
fn active_fixture_app_and_local_assets_are_served_with_mime_types() {
  let temp = TempDir::new().expect("temporary data directory");
  publish_fixture(&temp, &manifest(APP_VERSION), true);
  let protocol = protocol(&temp);

  let page = protocol.handle(request(Method::GET, "anvil-app://localhost/"));
  assert_eq!(page.status(), 200);
  assert_eq!(page.headers()["content-type"], "text/html; charset=utf-8");
  assert!(body(page).contains("/_app/app.js"));

  let script = protocol.handle(request(
    Method::GET,
    "anvil-app://localhost/_app/app.js?cache=1",
  ));
  assert_eq!(script.status(), 200);
  assert_eq!(
    script.headers()["content-type"],
    "text/javascript; charset=utf-8"
  );
  assert!(body(script).contains("dataset.loaded"));

  let style = protocol.handle(request(Method::GET, "anvil-app://localhost/_app/style.css"));
  assert_eq!(style.status(), 200);
  assert_eq!(style.headers()["content-type"], "text/css; charset=utf-8");
}

#[test]
fn missing_or_corrupt_app_shows_embedded_recovery_entry() {
  let missing = TempDir::new().expect("temporary data directory");
  let missing_response = protocol(&missing).handle(request(Method::GET, "anvil-app://localhost/"));
  assert_eq!(missing_response.status(), 200);
  assert!(body(missing_response).contains("恢复入口"));

  let corrupt = TempDir::new().expect("temporary data directory");
  publish_fixture(&corrupt, "{invalid json", false);
  let corrupt_response = protocol(&corrupt).handle(request(Method::GET, "anvil-app://localhost/"));
  assert_eq!(corrupt_response.status(), 200);
  assert!(body(corrupt_response).contains("无法启动桌面 App"));
}

#[test]
fn missing_page_or_incompatible_manifest_shows_recovery_entry() {
  let missing_page = TempDir::new().expect("temporary data directory");
  publish_fixture(&missing_page, &manifest(APP_VERSION), false);
  let missing_response =
    protocol(&missing_page).handle(request(Method::GET, "anvil-app://localhost/"));
  assert!(body(missing_response).contains("无法启动桌面 App"));

  let incompatible = TempDir::new().expect("temporary data directory");
  let incompatible_manifest = manifest("0.2.0");
  publish_fixture(&incompatible, &incompatible_manifest, true);
  let incompatible_response =
    protocol(&incompatible).handle(request(Method::GET, "anvil-app://localhost/"));
  assert!(body(incompatible_response).contains("无法启动桌面 App"));
}

#[test]
fn traversal_absolute_and_malformed_encoded_paths_are_rejected() {
  let temp = TempDir::new().expect("temporary data directory");
  publish_fixture(&temp, &manifest(APP_VERSION), true);
  let protocol = protocol(&temp);

  for path in [
    "anvil-app://localhost/%2e%2e/secret.txt",
    "anvil-app://localhost//etc/passwd",
    "anvil-app://localhost/_app/%2f..%2fsecret.txt",
    "anvil-app://localhost/_app/%GG.js",
    "anvil-app://evil.example/_app/app.js",
  ] {
    let response = protocol.handle(request(Method::GET, path));
    assert_eq!(response.status(), 400, "request path {path}");
  }
}

#[test]
fn missing_resources_and_unsupported_methods_are_rejected() {
  let temp = TempDir::new().expect("temporary data directory");
  publish_fixture(&temp, &manifest(APP_VERSION), true);
  let protocol = protocol(&temp);

  let missing = protocol.handle(request(
    Method::GET,
    "anvil-app://localhost/_app/not-installed.js",
  ));
  assert_eq!(missing.status(), 404);

  let post = protocol.handle(request(Method::POST, "anvil-app://localhost/"));
  assert_eq!(post.status(), 405);
}

#[test]
fn head_returns_headers_without_page_body() {
  let temp = TempDir::new().expect("temporary data directory");
  publish_fixture(&temp, &manifest(APP_VERSION), true);
  let protocol = protocol(&temp);

  let response = protocol.handle(request(Method::HEAD, "anvil-app://localhost/"));
  assert_eq!(response.status(), 200);
  assert!(response.body().is_empty());
  assert!(
    response.headers()["content-length"]
      .to_str()
      .unwrap()
      .parse::<usize>()
      .unwrap()
      > 0
  );
}

#[cfg(unix)]
#[test]
fn protocol_never_follows_a_symlink_out_of_the_app_package() {
  use std::os::unix::fs::symlink;

  let temp = TempDir::new().expect("temporary data directory");
  publish_fixture(&temp, &manifest(APP_VERSION), true);
  let outside = temp.path().join("outside.js");
  fs::write(&outside, "outside secret").expect("write outside file");
  let app_web = temp
    .path()
    .join("apps")
    .join(APP_ID)
    .join("versions")
    .join(APP_VERSION)
    .join("web");
  symlink(&outside, app_web.join("outside.js")).expect("inject package symlink");

  let response = protocol(&temp).handle(request(Method::GET, "anvil-app://localhost/outside.js"));
  assert_eq!(response.status(), 404);
  assert!(!body(response).contains("outside secret"));
}
