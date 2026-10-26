//! Loopback play-statistics requests against a temporary Runtime data directory.
use crate::serve::{accounts::AccountState, api, operations::RuntimeState};
use anvil_core::{Context, Paths};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
  sync::oneshot,
};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const ORIGIN: &str = "http://localhost:5173";
const ACCOUNT: &str = "11111111-1111-1111-1111-111111111111";

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
    let state = api::AppState::new(runtime, accounts, "1.0.0".into(), TOKEN.to_owned());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop, rx) = oneshot::channel();
    let task = tokio::spawn(api::serve(listener, state, rx));
    Self { port, stop, task }
  }

  async fn request(&self, method: &str, path: &str, authorized: bool) -> (u16, String) {
    self.request_with_body(method, path, authorized, "").await
  }

  async fn request_with_body(
    &self,
    method: &str,
    path: &str,
    authorized: bool,
    body: &str,
  ) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", self.port)).await.unwrap();
    let auth = if authorized {
      format!("Authorization: Bearer {TOKEN}\r\n")
    } else {
      String::new()
    };
    let header = format!(
      "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: {ORIGIN}\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
      self.port,
      body.len()
    );
    stream.write_all(header.as_bytes()).await.unwrap();
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

fn now_ms() -> u64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_millis() as u64
}

#[tokio::test]
async fn play_stats_api_reads_persisted_sessions_and_clears() {
  let root = tempfile::tempdir().unwrap();
  let ended_at = now_ms();
  let started_at = ended_at - 60_000;
  std::fs::write(
    root.path().join("play-stats.json"),
    serde_json::to_vec(&json!({
      "schemaVersion": 1,
      "accounts": {
        ACCOUNT: {
          "instances": {
            "survival": { "totalMs": 60_000, "launches": 1, "lastPlayedAt": ended_at }
          },
          "sessions": [
            { "instanceId": "survival", "startedAt": started_at, "endedAt": ended_at, "durationMs": 60_000 }
          ]
        }
      }
    }))
    .unwrap(),
  )
  .unwrap();
  let server = Server::start(root.path()).await;

  let (status, body) = server.request("GET", "/play-stats", true).await;
  assert_eq!(status, 200, "{body}");
  let value: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(value["schemaVersion"], 1);
  assert_eq!(value["accounts"][ACCOUNT]["totalMs"], 60_000);
  assert_eq!(value["accounts"][ACCOUNT]["launches"], 1);
  assert_eq!(value["accounts"][ACCOUNT]["last7dMs"], 60_000);
  assert_eq!(value["accounts"][ACCOUNT]["lastInstanceId"], "survival");

  let (status, _) = server.request("GET", "/play-stats", false).await;
  assert_eq!(status, 401);

  let (status, body) = server.request("DELETE", "/play-stats", true).await;
  assert_eq!(status, 200, "{body}");
  let value: Value = serde_json::from_str(&body).unwrap();
  assert_eq!(value["accounts"], json!({}));

  let persisted: Value =
    serde_json::from_slice(&std::fs::read(root.path().join("play-stats.json")).unwrap()).unwrap();
  assert_eq!(persisted["accounts"], json!({}));
  server.shutdown().await;
}

#[tokio::test]
async fn deleting_an_instance_preserves_history_and_scoped_clear_is_isolated() {
  use anvil_core::play_stats::{OFFLINE_ACCOUNT, PlaySession, PlayStatsStore};
  const OTHER: &str = "22222222-2222-2222-2222-222222222222";
  let root = tempfile::tempdir().unwrap();
  let ended_at = now_ms();
  let store = PlayStatsStore::open(root.path());
  for account in [ACCOUNT, OTHER, OFFLINE_ACCOUNT] {
    store
      .record(PlaySession {
        account_id: account.into(),
        instance_id: "client-survival".into(),
        started_at: ended_at - 60_000,
        ended_at,
      })
      .unwrap();
  }
  let before = store.snapshot();
  let instance = root.path().join("instances/client-survival");
  std::fs::create_dir_all(instance.join("game/saves")).unwrap();
  std::fs::write(
    instance.join("instance.json"),
    serde_json::to_vec(&json!({
      "schema_version": 1, "instance_type": "client", "id": "client-survival", "name": "survival"
    }))
    .unwrap(),
  )
  .unwrap();
  let server = Server::start(root.path()).await;
  let (status, body) = server
    .request_with_body(
      "DELETE",
      "/clients/survival",
      true,
      r#"{"confirmation":"survival"}"#,
    )
    .await;
  assert_eq!(status, 200, "{body}");
  assert!(!instance.exists());
  assert_eq!(PlayStatsStore::open(root.path()).snapshot(), before);

  let path = format!("/play-stats/accounts/{ACCOUNT}");
  assert_eq!(server.request("DELETE", &path, false).await.0, 401);
  assert_eq!(
    server
      .request("DELETE", "/play-stats/accounts/bad!", true)
      .await
      .0,
    400
  );
  assert_eq!(PlayStatsStore::open(root.path()).snapshot(), before);
  let (status, body) = server.request("DELETE", &path, true).await;
  assert_eq!(status, 200, "{body}");
  let value: Value = serde_json::from_str(&body).unwrap();
  assert!(value["accounts"].get(ACCOUNT).is_none());
  assert_eq!(value["accounts"][OTHER]["totalMs"], 60_000);
  assert_eq!(value["accounts"][OFFLINE_ACCOUNT]["launches"], 1);
  assert_eq!(server.request("DELETE", &path, true).await.0, 200);
  assert_eq!(server.request("DELETE", "/play-stats", false).await.0, 401);
  assert_eq!(
    PlayStatsStore::open(root.path()).snapshot().accounts.len(),
    2
  );
  let (status, body) = server.request("DELETE", "/play-stats", true).await;
  assert_eq!(status, 200, "{body}");
  assert!(
    PlayStatsStore::open(root.path())
      .snapshot()
      .accounts
      .is_empty()
  );
  server.shutdown().await;
}
