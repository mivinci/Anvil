//! Loopback Runtime API tests seeded with fresh disk snapshots; no platform I/O.
use crate::serve::catalog::{Catalog, CatalogData, Source as CatalogSource};
use crate::serve::{accounts::AccountState, api, operations::RuntimeState};
use anvil_core::{Context, Paths, modrinth::DiscoveryQuery};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{
  path::Path as FsPath,
  sync::Arc,
  time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
  sync::oneshot,
};

struct OfflineSource {
  requests: Arc<AtomicUsize>,
  restored: Option<(u8, SystemTime)>,
  fail: bool,
}
impl CatalogSource for OfflineSource {
  type Value = u8;
  const TTL: std::time::Duration = std::time::Duration::from_secs(300);
  const NAME: &'static str = "offline-discovery-fixture";
  fn needs_refresh(&self, value: &u8) -> bool {
    *value == 0
  }
  async fn load(&self, _: Context) -> Result<Option<CatalogData<u8>>, anvil_core::Error> {
    Ok(self.restored.map(|(value, fetched_at)| CatalogData {
      value,
      fetched_at,
      persistence_error: None,
    }))
  }
  async fn fetch(&self, _: Context) -> Result<CatalogData<u8>, anvil_core::Error> {
    self.requests.fetch_add(1, Ordering::SeqCst);
    if self.fail {
      return Err(anvil_core::Error::Metadata(
        "offline fixture unavailable".into(),
      ));
    }
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    Ok(CatalogData {
      value: 2,
      fetched_at: SystemTime::now(),
      persistence_error: None,
    })
  }
  fn error(error: &anvil_core::Error) -> String {
    error.to_string()
  }
}

#[tokio::test]
async fn discovery_cache_coalesces_cold_reads_and_manual_refresh_bypasses_freshness() {
  let root = tempfile::tempdir().unwrap();
  let ctx = Context::new(Paths::discover(Some(root.path().to_owned())).unwrap()).unwrap();
  let requests = Arc::new(AtomicUsize::new(0));
  let cache = Arc::new(Catalog::new(OfflineSource {
    requests: requests.clone(),
    restored: None,
    fail: false,
  }));
  let (first, second) = tokio::join!(
    cache.get(ctx.clone(), false, None),
    cache.get(ctx.clone(), false, None)
  );
  assert_eq!(first.unwrap().value, 2);
  assert_eq!(second.unwrap().value, 2);
  assert_eq!(requests.load(Ordering::SeqCst), 1);
  assert!(!cache.get(ctx.clone(), false, None).await.unwrap().stale);
  assert_eq!(requests.load(Ordering::SeqCst), 1);
  assert_eq!(cache.get(ctx, true, None).await.unwrap().value, 2);
  assert_eq!(requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn discovery_cache_keeps_expired_snapshot_when_background_refresh_fails() {
  let root = tempfile::tempdir().unwrap();
  let ctx = Context::new(Paths::discover(Some(root.path().to_owned())).unwrap()).unwrap();
  let requests = Arc::new(AtomicUsize::new(0));
  let cache = Arc::new(Catalog::new(OfflineSource {
    requests: requests.clone(),
    restored: Some((1, SystemTime::now() - std::time::Duration::from_secs(600))),
    fail: true,
  }));
  let stale = cache.get(ctx.clone(), false, None).await.unwrap();
  assert_eq!(stale.value, 1);
  assert!(stale.stale);
  assert_eq!(stale.fresh_for_ms, 0);
  let failed = cache
    .get(ctx.clone(), true, Some(stale.revision))
    .await
    .unwrap();
  assert_eq!(failed.value, 1);
  assert!(failed.refresh_error.is_some());
  assert_eq!(requests.load(Ordering::SeqCst), 1);
  cache.get(ctx, true, None).await.unwrap();
  assert_eq!(requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn fresh_legacy_metadata_is_shown_then_upgraded_without_resetting_fetch_time() {
  for fail in [false, true] {
    let root = tempfile::tempdir().unwrap();
    let ctx = Context::new(Paths::discover(Some(root.path().to_owned())).unwrap()).unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let cache = Arc::new(Catalog::new(OfflineSource {
      requests: requests.clone(),
      restored: Some((0, SystemTime::now())),
      fail,
    }));
    let legacy = cache.get(ctx.clone(), false, None).await.unwrap();
    assert_eq!(legacy.value, 0);
    assert!(legacy.stale);
    assert_eq!(legacy.fresh_for_ms, 0);
    let upgraded = cache
      .get(ctx.clone(), true, Some(legacy.revision))
      .await
      .unwrap();
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    if fail {
      assert_eq!(upgraded.value, 0);
      assert!(upgraded.stale);
      assert!(upgraded.refresh_error.is_some());
    } else {
      assert_eq!(upgraded.value, 2);
      assert!(!upgraded.stale);
      assert!(upgraded.refresh_error.is_none());
    }
    cache.get(ctx, true, Some(upgraded.revision)).await.unwrap();
    assert_eq!(requests.load(Ordering::SeqCst), 1);
  }
}

struct Server {
  address: std::net::SocketAddr,
  token: String,
  stop: Option<oneshot::Sender<()>>,
  task: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
}
impl Server {
  async fn start(root: &FsPath) -> Self {
    let ctx = Context::new(Paths::discover(Some(root.to_owned())).unwrap()).unwrap();
    seed(&ctx);
    let runtime = Arc::new(RuntimeState::new(ctx.clone()));
    let accounts = Arc::new(AccountState::new(ctx));
    let token = format!("{:064x}", 42);
    let mut state = api::AppState::new(runtime, accounts, "1.0.0".into(), token.clone());
    state.curseforge = Arc::new(crate::serve::curseforge::Catalog::with_key(None));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, receiver) = oneshot::channel();
    let task = tokio::spawn(api::serve(listener, state, receiver));
    Self {
      address,
      token,
      stop: Some(stop),
      task: Some(task),
    }
  }
  async fn request(&self, path: &str, authenticated: bool) -> (u16, String) {
    let mut socket = TcpStream::connect(self.address).await.unwrap();
    let auth = if authenticated {
      format!("Authorization: Bearer {}\r\n", self.token)
    } else {
      String::new()
    };
    let request = format!(
      "GET {path} HTTP/1.1\r\nHost: localhost\r\nOrigin: anvil-app://localhost\r\n{auth}Connection: close\r\n\r\n"
    );
    socket.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(
      std::time::Duration::from_secs(5),
      socket.read_to_end(&mut response),
    )
    .await
    .unwrap()
    .unwrap();
    let response = String::from_utf8(response).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, body.to_owned())
  }
  async fn shutdown(mut self) {
    self.stop.take().unwrap().send(()).unwrap();
    self.task.take().unwrap().await.unwrap().unwrap();
  }
}
impl Drop for Server {
  fn drop(&mut self) {
    if let Some(task) = &self.task {
      task.abort();
    }
  }
}
fn seed(ctx: &Context) {
  let project = json!({"id":"abcdefgh","name":"Fixture","summary":"Public summary","authors":[],"links":{"websiteUrl":"https://modrinth.com/mod/abcdefgh"},"downloadCount":12500,"updatedAt":"2026-10-24T12:30:00Z"});
  let entries = [
    (
      DiscoveryQuery::Search {
        game: None,
        text: "fixture".into(),
        index: 0,
      },
      json!({"kind":"search","page":{"items":[project.clone()],"nextIndex":null}}),
    ),
    (
      DiscoveryQuery::Search {
        game: Some("1.21.1".into()),
        text: "fixture".into(),
        index: 0,
      },
      json!({"kind":"search","page":{"items":[project.clone()],"nextIndex":null}}),
    ),
    (
      DiscoveryQuery::Project {
        project: "abcdefgh".into(),
      },
      json!({"kind":"project","page":project}),
    ),
  ];
  let dir = ctx.paths.cache.join("metadata/modrinth");
  std::fs::create_dir_all(&dir).unwrap();
  for (query, payload) in entries {
    let key = query.identity().unwrap();
    let hash = ring::digest::digest(&ring::digest::SHA256, key.as_bytes());
    let filename: String = hash.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    std::fs::write(dir.join(format!("{filename}.json")), serde_json::to_vec(&json!({"schemaVersion":1,"fetchedAtSeconds":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),"payload":payload})).unwrap()).unwrap();
  }
}

#[tokio::test]
async fn global_browse_restores_search_and_summary_without_any_installed_instance() {
  let root = tempfile::tempdir().unwrap();
  let server = Server::start(root.path()).await;
  for path in [
    "/resources/modrinth/projects?q=fixture",
    "/resources/modrinth/projects?q=fixture&gameVersion=1.21.1&loader=fabric",
  ] {
    let (status, body) = server.request(path, true).await;
    assert_eq!(status, 200, "{body}");
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["items"][0]["id"], "abcdefgh");
    assert_eq!(body["items"][0]["downloadCount"], 12500);
    assert_eq!(body["items"][0]["updatedAt"], "2026-10-24T12:30:00Z");
    assert_eq!(body["stale"], false);
    assert!(body["freshForMs"].as_u64().unwrap() > 0);
  }
  let (status, body) = server
    .request("/resources/modrinth/projects/abcdefgh", true)
    .await;
  assert_eq!(status, 200, "{body}");
  let body: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(body["project"]["summary"], "Public summary");
  assert_eq!(body["project"]["downloadCount"], 12500);
  assert_eq!(body["project"]["updatedAt"], "2026-10-24T12:30:00Z");
  assert!(body.get("items").is_none());
  assert!(!root.path().join("instances").exists());
  assert!(!root.path().join("resources").exists());
  server.shutdown().await;
}

#[tokio::test]
async fn discovery_keeps_authentication_source_validation_and_credential_boundaries() {
  let root = tempfile::tempdir().unwrap();
  let server = Server::start(root.path()).await;
  assert_eq!(
    server
      .request("/resources/modrinth/projects?q=fixture", false)
      .await
      .0,
    401
  );
  for path in [
    "/resources/unknown/projects",
    "/resources/modrinth/projects?loader=forge",
    "/resources/modrinth/projects?gameVersion=",
    "/resources/modrinth/projects?q=%01",
    "/resources/modrinth/projects?index=9999",
    "/resources/modrinth/projects?unknown=1",
    "/resources/modrinth/projects/not-valid",
    "/resources/curseforge/projects/0",
  ] {
    assert_eq!(server.request(path, true).await.0, 400, "{path}");
  }
  assert_eq!(
    server
      .request("/resources/curseforge/projects?q=fixture", true)
      .await
      .0,
    503
  );
  assert_eq!(
    server
      .request("/resources/curseforge/projects/42", true)
      .await
      .0,
    503
  );
  assert_eq!(
    server
      .request("/clients/missing/modrinth/projects?q=fixture", true)
      .await
      .0,
    404
  );
  server.shutdown().await;
}
