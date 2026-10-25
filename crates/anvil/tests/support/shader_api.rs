//! Actual loopback requests against the Runtime router, using only temporary instances.
use crate::serve::{accounts::AccountState, api, operations::RuntimeState};
use anvil_core::{Context, Paths};
use serde_json::{Value, json};
use std::{
  io::{Cursor, Write},
  path::Path,
  sync::Arc,
};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
  sync::oneshot,
};

struct Server {
  port: u16,
  stop: oneshot::Sender<()>,
  task: tokio::task::JoinHandle<std::io::Result<()>>,
}
impl Server {
  async fn start(root: &Path) -> Self {
    let context = Context::new(Paths {
      data: root.to_path_buf(),
      cache: root.join("cache"),
    })
    .unwrap();
    let runtime = Arc::new(RuntimeState::new(context.clone()));
    let accounts = Arc::new(AccountState::new(context));
    let state = api::AppState::new(runtime, accounts, "1.0.0".into(), "a".repeat(64));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop, rx) = oneshot::channel();
    let task = tokio::spawn(api::serve(listener, state, rx));
    Self { port, stop, task }
  }
  async fn request(
    &self,
    method: &str,
    path: &str,
    body: &[u8],
    authorized: bool,
    origin: &str,
    declared_length: Option<usize>,
  ) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", self.port)).await.unwrap();
    let auth = if authorized {
      format!("Authorization: Bearer {}\r\n", "a".repeat(64))
    } else {
      String::new()
    };
    let header = format!(
      "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: {origin}\r\n{auth}Content-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
      self.port,
      declared_length.unwrap_or(body.len())
    );
    stream.write_all(header.as_bytes()).await.unwrap();
    stream.write_all(body).await.unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(
      std::time::Duration::from_secs(5),
      stream.read_to_end(&mut response),
    )
    .await
    .unwrap()
    .unwrap();
    let response = String::from_utf8(response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (
      head.split_whitespace().nth(1).unwrap().parse().unwrap(),
      body.to_owned(),
    )
  }
  async fn shutdown(self) {
    self.stop.send(()).unwrap();
    self.task.await.unwrap().unwrap();
  }
}
fn fixture(root: &Path) {
  let dir = root.join("instances/client-test");
  std::fs::create_dir_all(dir.join("game")).unwrap();
  std::fs::write(
    dir.join("instance.json"),
    serde_json::to_vec(&json!({
      "schema_version": 1, "instance_type": "client", "id": "client-test", "name": "test"
    }))
    .unwrap(),
  )
  .unwrap();
  let sha = "a".repeat(40);
  let files = ["versions/1.21.1/1.21.1.jar", "assets/indexes/17.json"].map(|path| {
    json!({
      "path": path, "sha1": sha, "size": 0, "cache_path": format!("clients/sha1/{sha}")
    })
  });
  std::fs::write(dir.join("installed.json"), serde_json::to_vec(&json!({
    "schema_version": 1, "version_id": "1.21.1", "version_type": "release",
    "metadata_url": "https://piston-meta.mojang.com/fixture.json", "metadata_sha1": sha,
    "java_component": "java-runtime-delta", "java_major": 21, "os": "macos",
    "architecture": "aarch64", "asset_index_id": "17", "main_class": "net.minecraft.client.main.Main",
    "arguments": {"game": [], "jvm": []}, "files": files
  })).unwrap()).unwrap();
}
fn shader() -> Vec<u8> {
  let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
  writer
    .start_file(
      "shaders/composite.fsh",
      zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
  writer.write_all(b"#version 150\nvoid main() {}").unwrap();
  writer.finish().unwrap().into_inner()
}
const ORIGIN: &str = "http://localhost:5173";
// Production deliberately shares one resource-request permit across server states.
static TEST_SERVER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn shader_api_auth_import_revision_and_removal() {
  let _serial = TEST_SERVER.lock().await;
  let root = tempfile::tempdir().unwrap();
  fixture(root.path());
  let server = Server::start(root.path()).await;
  let path = "/clients/test/shader-packs";
  assert_eq!(
    server
      .request("GET", path, b"", false, ORIGIN, None)
      .await
      .0,
    401
  );
  assert_eq!(
    server
      .request("GET", path, b"", true, "https://untrusted.example", None)
      .await
      .0,
    403
  );
  let permit = super::RESOURCE_REQUESTS.acquire().await.unwrap();
  assert_eq!(
    server.request("GET", path, b"", true, ORIGIN, None).await.0,
    429
  );
  drop(permit);
  let (status, body) = server.request("GET", path, b"", true, ORIGIN, None).await;
  assert_eq!(status, 200, "{body}");
  let body: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(body["environment"]["status"], "requires_fabric");
  assert_eq!(body["packs"], json!([]));
  let (status, body) = server
    .request(
      "POST",
      &format!("{path}?filename=Light.zip&revision=0"),
      &shader(),
      true,
      ORIGIN,
      None,
    )
    .await;
  assert_eq!(status, 200, "{body}");
  let changed: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(changed["revision"], 1);
  assert!(changed.get("environment").is_none());
  let hash = changed["packs"][0]["sha256"].as_str().unwrap();
  let delete = format!("{path}/{hash}");
  assert_eq!(
    server
      .request(
        "DELETE",
        &format!("{delete}?revision=1"),
        b"",
        false,
        ORIGIN,
        None
      )
      .await
      .0,
    401
  );
  assert_eq!(
    server
      .request(
        "DELETE",
        &format!("{delete}?revision=0"),
        b"",
        true,
        ORIGIN,
        None
      )
      .await
      .0,
    409
  );
  let (status, body) = server.request("GET", path, b"", true, ORIGIN, None).await;
  assert_eq!(status, 200);
  let current: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(current["packs"], changed["packs"]);
  assert_eq!(current["revision"], changed["revision"]);
  assert_eq!(
    server
      .request(
        "DELETE",
        &format!("{delete}?revision=1"),
        b"",
        true,
        ORIGIN,
        None
      )
      .await
      .0,
    200
  );
  assert!(
    !root
      .path()
      .join("instances/client-test/game/shaderpacks/Light.zip")
      .exists()
  );
  server.shutdown().await;
}

#[tokio::test]
async fn shader_api_rejects_invalid_uploads_paths_and_oversized_requests() {
  let _serial = TEST_SERVER.lock().await;
  let root = tempfile::tempdir().unwrap();
  fixture(root.path());
  let server = Server::start(root.path()).await;
  let path = "/clients/test/shader-packs";
  assert_eq!(
    server
      .request(
        "POST",
        &format!("{path}?filename=Light.zip&revision=0"),
        &shader(),
        false,
        ORIGIN,
        None
      )
      .await
      .0,
    401
  );
  assert_eq!(
    server
      .request(
        "POST",
        &format!("{path}?filename=Light.zip&revision=0"),
        b"broken zip",
        true,
        ORIGIN,
        None
      )
      .await
      .0,
    400
  );
  assert_eq!(
    server
      .request(
        "POST",
        &format!("{path}?filename=..%2FLight.zip&revision=0"),
        &shader(),
        true,
        ORIGIN,
        None
      )
      .await
      .0,
    400
  );
  assert_eq!(
    server
      .request(
        "POST",
        &format!("{path}?filename=Light.zip&revision=0"),
        &vec![0; anvil_core::resources::MAX_IMPORT_BYTES + 1],
        true,
        ORIGIN,
        None
      )
      .await
      .0,
    413
  );
  assert_eq!(
    server
      .request(
        "GET",
        "/clients/missing/shader-packs",
        b"",
        true,
        ORIGIN,
        None
      )
      .await
      .0,
    404
  );
  assert!(
    !root
      .path()
      .join("instances/client-test/resources.json")
      .exists()
  );
  server.shutdown().await;
}
