use crate::app_manifest::AppManifest;
use crate::app_storage::{AppStorage, AppStorageError, PublishedAppVersion};
use percent_encoding::percent_decode_str;
use semver::Version;
use std::io::Read;
use std::path::Path;
use tauri::http::{Method, Request, Response, StatusCode, header};

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_PAGE_RESOURCE_BYTES: u64 = 64 * 1024 * 1024;
const RECOVERY_PAGE: &[u8] = include_bytes!("../../bootstrap/index.html");

#[derive(Debug, Clone)]
pub struct AppPageProtocol {
  storage: Result<AppStorage, String>,
  app_id: String,
  target: String,
  shell_api_version: String,
}

impl AppPageProtocol {
  pub fn for_current_user() -> Self {
    Self::new(
      AppStorage::for_current_user(),
      env!("ANVIL_DEFAULT_APP_ID"),
      env!("ANVIL_TARGET_PLATFORM"),
      env!("ANVIL_SHELL_API_VERSION"),
    )
  }

  pub fn new(
    storage: Result<AppStorage, AppStorageError>,
    app_id: impl Into<String>,
    target: impl Into<String>,
    shell_api_version: impl Into<String>,
  ) -> Self {
    Self {
      storage: storage.map_err(|error| error.to_string()),
      app_id: app_id.into(),
      target: target.into(),
      shell_api_version: shell_api_version.into(),
    }
  }

  /// Serve one local App resource or the embedded recovery page for the protocol root.
  pub fn handle(&self, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let method = request.method();
    if method != Method::GET && method != Method::HEAD {
      return response(
        StatusCode::METHOD_NOT_ALLOWED,
        "text/plain; charset=utf-8",
        b"method not allowed".to_vec(),
        false,
      );
    }

    let request_path = request.uri().path();
    let is_root = request_path.is_empty() || request_path == "/";
    let head_only = method == Method::HEAD;
    if !is_local_protocol_authority(request.uri()) {
      return response(
        StatusCode::BAD_REQUEST,
        "text/plain; charset=utf-8",
        b"invalid App protocol authority".to_vec(),
        head_only,
      );
    }
    let (version, page_root, page_entry) = match self.active_page() {
      Ok(page) => page,
      Err(_) if is_root => {
        return response(
          StatusCode::OK,
          "text/html; charset=utf-8",
          RECOVERY_PAGE.to_vec(),
          head_only,
        );
      }
      Err(_) => {
        return response(
          StatusCode::SERVICE_UNAVAILABLE,
          "text/plain; charset=utf-8",
          b"local App is unavailable".to_vec(),
          head_only,
        );
      }
    };

    let entry = match requested_entry(request_path, &page_root, &page_entry) {
      Ok(entry) => entry,
      Err(()) => {
        return response(
          StatusCode::BAD_REQUEST,
          "text/plain; charset=utf-8",
          b"invalid App resource path".to_vec(),
          head_only,
        );
      }
    };

    match read_page_resource(&version, &entry) {
      Ok(contents) => response(StatusCode::OK, content_type(&entry), contents, head_only),
      Err(_) if is_root => response(
        StatusCode::OK,
        "text/html; charset=utf-8",
        RECOVERY_PAGE.to_vec(),
        head_only,
      ),
      Err(_) => response(
        StatusCode::NOT_FOUND,
        "text/plain; charset=utf-8",
        b"App resource not found".to_vec(),
        head_only,
      ),
    }
  }

  fn active_page(&self) -> Result<(PublishedAppVersion, String, String), ()> {
    let storage = self.storage.as_ref().map_err(|_| ())?;
    let store = storage.app(&self.app_id).map_err(|_| ())?;
    let version = store.resolve_active().map_err(|_| ())?.ok_or(())?;

    let manifest_file = version.open_entry("manifest.json").map_err(|_| ())?;
    if manifest_file.metadata().map_err(|_| ())?.len() > MAX_MANIFEST_BYTES {
      return Err(());
    }
    let mut manifest_source = Vec::new();
    manifest_file
      .take(MAX_MANIFEST_BYTES + 1)
      .read_to_end(&mut manifest_source)
      .map_err(|_| ())?;
    let manifest = AppManifest::parse_and_validate(
      &manifest_source,
      &self.app_id,
      &self.target,
      &self.shell_api_version,
    )
    .map_err(|_| ())?;

    let version_number = Version::parse(&manifest.version).map_err(|_| ())?;
    if version_number != *version.version() {
      return Err(());
    }
    let Some(page_root) = manifest.page_entry.strip_suffix("/index.html") else {
      return Err(());
    };
    if page_root != "web" {
      return Err(());
    }

    // Resolve the entry before handing the package to the WebView. This turns an
    // absent or unsafe main page into the recovery screen on the initial request.
    version.open_entry(&manifest.page_entry).map_err(|_| ())?;
    Ok((version, page_root.to_owned(), manifest.page_entry))
  }
}

fn requested_entry(path: &str, page_root: &str, page_entry: &str) -> Result<String, ()> {
  if path.is_empty() || path == "/" {
    return Ok(page_entry.to_owned());
  }

  let raw_path = path.strip_prefix('/').ok_or(())?;
  if !has_valid_percent_escapes(raw_path) {
    return Err(());
  }
  let relative = percent_decode_str(raw_path).decode_utf8().map_err(|_| ())?;
  if relative.is_empty()
    || relative.contains('\0')
    || relative.contains('\\')
    || relative.contains(':')
    || relative
      .split('/')
      .any(|component| component.is_empty() || component == "." || component == "..")
  {
    return Err(());
  }

  Ok(format!("{page_root}/{relative}"))
}

fn has_valid_percent_escapes(path: &str) -> bool {
  let bytes = path.as_bytes();
  let mut index = 0;
  while index < bytes.len() {
    if bytes[index] == b'%' {
      let Some(hex) = bytes.get(index + 1..index + 3) else {
        return false;
      };
      if hex.len() != 2 || !hex.iter().all(u8::is_ascii_hexdigit) {
        return false;
      }
      index += 3;
    } else {
      index += 1;
    }
  }
  true
}

fn is_local_protocol_authority(uri: &tauri::http::Uri) -> bool {
  let Some(authority) = uri.authority() else {
    return false;
  };
  matches!(authority.host(), "localhost" | "anvil-app.localhost")
    && authority.port_u16().is_none()
    && !authority.as_str().contains('@')
}

fn read_page_resource(
  version: &PublishedAppVersion,
  entry: &str,
) -> Result<Vec<u8>, AppStorageError> {
  let file = version.open_entry(entry)?;
  if file.metadata()?.len() > MAX_PAGE_RESOURCE_BYTES {
    return Err(AppStorageError::InvalidEntry(entry.to_owned()));
  }
  let mut contents = Vec::new();
  file
    .take(MAX_PAGE_RESOURCE_BYTES + 1)
    .read_to_end(&mut contents)?;
  if contents.len() as u64 > MAX_PAGE_RESOURCE_BYTES {
    return Err(AppStorageError::InvalidEntry(entry.to_owned()));
  }
  Ok(contents)
}

fn content_type(entry: &str) -> &'static str {
  let extension = Path::new(entry)
    .extension()
    .and_then(|extension| extension.to_str())
    .unwrap_or("")
    .to_ascii_lowercase();
  match extension.as_str() {
    "html" | "htm" => "text/html; charset=utf-8",
    "css" => "text/css; charset=utf-8",
    "js" | "mjs" => "text/javascript; charset=utf-8",
    "json" | "map" | "webmanifest" => "application/json; charset=utf-8",
    "svg" => "image/svg+xml",
    "png" => "image/png",
    "jpg" | "jpeg" => "image/jpeg",
    "gif" => "image/gif",
    "webp" => "image/webp",
    "avif" => "image/avif",
    "ico" => "image/vnd.microsoft.icon",
    "woff" => "font/woff",
    "woff2" => "font/woff2",
    "ttf" => "font/ttf",
    "otf" => "font/otf",
    "wasm" => "application/wasm",
    "txt" => "text/plain; charset=utf-8",
    _ => "application/octet-stream",
  }
}

fn response(
  status: StatusCode,
  content_type: &'static str,
  body: Vec<u8>,
  head_only: bool,
) -> Response<Vec<u8>> {
  let content_length = body.len().to_string();
  Response::builder()
    .status(status)
    .header(header::CONTENT_TYPE, content_type)
    .header(header::CONTENT_LENGTH, content_length)
    .header(header::CACHE_CONTROL, "no-store")
    .header("x-content-type-options", "nosniff")
    .body(if head_only { Vec::new() } else { body })
    .expect("static App protocol response headers are valid")
}
