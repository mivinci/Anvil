use serde::Deserialize;
use std::{
  io::{BufRead, BufReader, Read, Write},
  net::TcpStream,
  process::{Child, Command, Stdio},
  thread,
  time::{Duration, Instant},
};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const ORIGIN: &str = "http://localhost:5173";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadyMessage {
  kind: String,
  port: u16,
  api_version: String,
}

#[test]
fn sidecar_binds_loopback_and_requires_token_and_allowed_origin() {
  let mut child = Command::new(env!("CARGO_BIN_EXE_anvil-desktop"))
    .env("ANVIL_HTTP_API_VERSION", "1.0.0")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .unwrap();
  let mut stdin = child.stdin.take().unwrap();
  stdin.write_all(format!("{TOKEN}\n").as_bytes()).unwrap();
  stdin.flush().unwrap();

  let stdout = child.stdout.take().unwrap();
  let (ready_tx, ready_rx) = std::sync::mpsc::channel();
  thread::spawn(move || {
    let mut line = String::new();
    let result = BufReader::new(stdout).read_line(&mut line);
    let _ = ready_tx.send(result.map(|_| line));
  });
  let ready_line = ready_rx
    .recv_timeout(Duration::from_secs(10))
    .unwrap()
    .unwrap();
  assert!(!ready_line.contains(TOKEN));
  let ready: ReadyMessage = serde_json::from_str(&ready_line).unwrap();
  assert_eq!(ready.kind, "ready");
  assert_eq!(ready.api_version, "1.0.0");

  let authorized = request(ready.port, "GET", "/health", Some(ORIGIN), Some(TOKEN));
  assert!(authorized.starts_with("HTTP/1.1 200 OK"), "{authorized}");
  assert!(authorized.contains("\"apiVersion\":\"1.0.0\""));
  assert!(authorized.contains(&format!("Access-Control-Allow-Origin: {ORIGIN}")));

  let unauthorized = request(ready.port, "GET", "/health", Some(ORIGIN), Some("wrong"));
  assert!(unauthorized.starts_with("HTTP/1.1 401 Unauthorized"));

  let forbidden_origin = request(
    ready.port,
    "GET",
    "/health",
    Some("https://untrusted.example"),
    Some(TOKEN),
  );
  assert!(forbidden_origin.starts_with("HTTP/1.1 403 Forbidden"));

  let preflight = request(ready.port, "OPTIONS", "/health", Some(ORIGIN), None);
  assert!(preflight.starts_with("HTTP/1.1 204 No Content"));
  assert!(preflight.contains("Access-Control-Allow-Headers: authorization"));

  stdin.write_all(b"shutdown\n").unwrap();
  stdin.flush().unwrap();
  drop(stdin);
  wait_for_exit(&mut child);
}

fn request(
  port: u16,
  method: &str,
  path: &str,
  origin: Option<&str>,
  token: Option<&str>,
) -> String {
  let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
  stream
    .set_read_timeout(Some(Duration::from_secs(3)))
    .unwrap();
  let mut request = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n");
  if let Some(origin) = origin {
    request.push_str(&format!("Origin: {origin}\r\n"));
  }
  if let Some(token) = token {
    request.push_str(&format!("Authorization: Bearer {token}\r\n"));
  }
  if method == "OPTIONS" {
    request.push_str(
      "Access-Control-Request-Method: GET\r\nAccess-Control-Request-Headers: authorization\r\n",
    );
  }
  request.push_str("Connection: close\r\n\r\n");
  stream.write_all(request.as_bytes()).unwrap();
  let mut response = String::new();
  stream.read_to_string(&mut response).unwrap();
  response
}

fn wait_for_exit(child: &mut Child) {
  let deadline = Instant::now() + Duration::from_secs(5);
  loop {
    if child.try_wait().unwrap().is_some() {
      return;
    }
    if Instant::now() >= deadline {
      let _ = child.kill();
      let _ = child.wait();
      panic!("sidecar did not stop after graceful shutdown");
    }
    thread::sleep(Duration::from_millis(25));
  }
}
