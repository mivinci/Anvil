//! Loopback avatar API tests with a temporary instance and a synthetic PNG.
use crate::serve::{accounts::AccountState, api, operations::RuntimeState};
use anvil_core::{Context, Paths};
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path, sync::Arc};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
  sync::oneshot,
};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const ORIGIN: &str = "http://localhost:5173";
const PNG: &[u8] = &[
  137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4, 0, 0,
  0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 100, 248, 15, 0, 1, 5, 1, 1, 39,
  24, 227, 102, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

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
  async fn request(
    &self,
    method: &str,
    path: &str,
    body: &[u8],
    content_type: &str,
    authorized: bool,
    extra: &[(&str, &str)],
  ) -> (u16, HashMap<String, String>, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", self.port)).await.unwrap();
    let auth = if authorized {
      format!("Authorization: Bearer {TOKEN}\r\n")
    } else {
      String::new()
    };
    let extra = extra
      .iter()
      .map(|(name, value)| format!("{name}: {value}\r\n"))
      .collect::<String>();
    let header = format!(
      "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: {ORIGIN}\r\n{auth}Content-Type: {content_type}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
      self.port,
      body.len()
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
    let split = response
      .windows(4)
      .position(|window| window == b"\r\n\r\n")
      .unwrap();
    let head = String::from_utf8(response[..split].to_vec()).unwrap();
    let body = response[split + 4..].to_vec();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let headers = head
      .lines()
      .skip(1)
      .filter_map(|line| line.split_once(':'))
      .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
      .collect();
    (status, headers, body)
  }
  async fn shutdown(self) {
    self.stop.send(()).unwrap();
    self.task.await.unwrap().unwrap();
  }
}

fn fixture(root: &Path) {
  let directory = root.join("instances/client-test");
  std::fs::create_dir_all(directory.join("game")).unwrap();
  std::fs::write(
    directory.join("instance.json"),
    serde_json::to_vec(&json!({
      "schema_version": 1, "instance_type": "client", "id": "client-test", "name": "test"
    }))
    .unwrap(),
  )
  .unwrap();
}

#[tokio::test]
async fn avatar_api_authenticates_and_round_trips_a_bounded_image() {
  let root = tempfile::tempdir().unwrap();
  fixture(root.path());
  let server = Server::start(root.path()).await;
  let path = "/clients/test/avatar";
  assert_eq!(
    server
      .request("GET", path, b"", "application/octet-stream", false, &[])
      .await
      .0,
    401
  );
  let (status, _, body) = server
    .request("GET", path, b"", "application/octet-stream", true, &[])
    .await;
  assert_eq!(status, 404, "{}", String::from_utf8_lossy(&body));
  let (status, _, _) = server
    .request("PUT", path, b"not an image", "image/png", true, &[])
    .await;
  assert_eq!(status, 400);
  let (status, _, body) = server
    .request("PUT", path, PNG, "image/png", true, &[])
    .await;
  assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
  let changed: Value = serde_json::from_slice(&body).unwrap();
  let revision = changed["revision"].as_str().unwrap();
  assert_eq!(revision.len(), 64);
  let (status, headers, body) = server
    .request("GET", path, b"", "application/octet-stream", true, &[])
    .await;
  assert_eq!(status, 200);
  assert_eq!(headers["content-type"], "image/png");
  assert_eq!(headers["etag"], format!("\"{revision}\""));
  assert_eq!(&body[..8], b"\x89PNG\r\n\x1a\n");
  let (status, _, body) = server
    .request(
      "GET",
      path,
      b"",
      "application/octet-stream",
      true,
      &[("If-None-Match", &format!("\"{revision}\""))],
    )
    .await;
  assert_eq!(status, 304);
  assert!(body.is_empty());
  assert_eq!(
    server
      .request("DELETE", path, b"", "application/octet-stream", true, &[])
      .await
      .0,
    204
  );
  assert_eq!(
    server
      .request("GET", path, b"", "application/octet-stream", true, &[])
      .await
      .0,
    404
  );
  assert_eq!(
    server
      .request("DELETE", path, b"", "application/octet-stream", true, &[])
      .await
      .0,
    204
  );
  server.shutdown().await;
}
