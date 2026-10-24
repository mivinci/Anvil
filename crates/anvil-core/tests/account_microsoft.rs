use super::*;
use std::{
  collections::HashMap,
  io::{Read, Write},
  net::{TcpListener, TcpStream},
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
  },
  thread,
  time::Duration,
};

const CLIENT_ID: &str = "01234567-89ab-cdef-0123-456789abcdef";
const ACCOUNT_ID: &str = "00112233445566778899aabbccddeeff";

#[derive(Default)]
struct MemoryCredentialStore(Mutex<HashMap<String, RefreshToken>>);

impl CredentialStore for MemoryCredentialStore {
  fn load_refresh_token(&self, account_id: &str) -> Result<Option<RefreshToken>> {
    self
      .0
      .lock()
      .unwrap()
      .get(account_id)
      .map(|token| RefreshToken::new(token.expose_secret()))
      .transpose()
  }

  fn store_refresh_token(&self, account_id: &str, token: RefreshToken) -> Result<()> {
    self.0.lock().unwrap().insert(account_id.into(), token);
    Ok(())
  }

  fn delete_refresh_token(&self, account_id: &str) -> Result<()> {
    self.0.lock().unwrap().remove(account_id);
    Ok(())
  }
}

struct FixtureRequest {
  method: String,
  path: String,
  headers: String,
  body: String,
}

struct HttpFixture {
  base_url: Url,
  stop: Arc<AtomicBool>,
  requests: Arc<Mutex<Vec<FixtureRequest>>>,
  worker: Option<thread::JoinHandle<()>>,
}

impl HttpFixture {
  fn new(device_polls_before_success: Option<usize>, decline_device_authorization: bool) -> Self {
    Self::with_xuid(
      device_polls_before_success,
      decline_device_authorization,
      Some("2533274799999999".into()),
    )
  }

  fn with_xuid(
    device_polls_before_success: Option<usize>,
    decline_device_authorization: bool,
    xuid: Option<String>,
  ) -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base_url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let thread_requests = requests.clone();
    let polls = Arc::new(Mutex::new(0usize));
    let thread_polls = polls.clone();
    let worker = thread::spawn(move || {
      while !thread_stop.load(Ordering::Relaxed) {
        match listener.accept() {
          Ok((mut stream, _)) => loop {
            stream.set_nonblocking(false).unwrap();
            let request = match read_request(&mut stream) {
              Ok(Some(request)) => request,
              Ok(None) => break,
              Err(error)
                if matches!(
                  error.kind(),
                  std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) && !thread_stop.load(Ordering::Relaxed) =>
              {
                continue;
              }
              Err(_) => break,
            };
            let response = respond(
              &request,
              &thread_polls,
              device_polls_before_success,
              decline_device_authorization,
              xuid.as_deref(),
            );
            thread_requests.lock().unwrap().push(request);
            if let Err(error) = write_response(&mut stream, response)
              && error.kind() != std::io::ErrorKind::BrokenPipe
            {
              panic!("authorization fixture failed: {error}");
            }
          },
          Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            thread::sleep(Duration::from_millis(1));
          }
          Err(error) => panic!("authorization fixture failed: {error}"),
        }
      }
    });
    Self {
      base_url,
      stop,
      requests,
      worker: Some(worker),
    }
  }
}

impl Drop for HttpFixture {
  fn drop(&mut self) {
    self.stop.store(true, Ordering::Relaxed);
    self.worker.take().unwrap().join().unwrap();
  }
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<FixtureRequest>> {
  stream.set_read_timeout(Some(Duration::from_millis(100)))?;
  let mut header = Vec::new();
  let mut byte = [0u8; 1];
  while !header.ends_with(b"\r\n\r\n") && header.len() <= 16 * 1024 {
    if stream.read(&mut byte)? == 0 {
      return Ok(None);
    }
    header.push(byte[0]);
  }
  let header_text = String::from_utf8_lossy(&header).into_owned();
  let mut first = header_text.lines().next().unwrap_or("").split_whitespace();
  let method = first.next().unwrap_or("").to_owned();
  let path = first.next().unwrap_or("/").to_owned();
  let content_length = header_text
    .lines()
    .find_map(|line| {
      let (name, value) = line.split_once(':')?;
      name
        .eq_ignore_ascii_case("content-length")
        .then(|| value.trim().parse::<usize>().ok())
        .flatten()
    })
    .unwrap_or(0);
  if content_length > 16 * 1024 {
    return Err(std::io::Error::new(
      std::io::ErrorKind::InvalidData,
      "fixture request body exceeded limit",
    ));
  }
  let mut body = vec![0; content_length];
  stream.read_exact(&mut body)?;
  Ok(Some(FixtureRequest {
    method,
    path,
    headers: header_text,
    body: String::from_utf8_lossy(&body).into_owned(),
  }))
}

fn respond(
  request: &FixtureRequest,
  polls: &Mutex<usize>,
  device_polls_before_success: Option<usize>,
  decline_device_authorization: bool,
  xuid: Option<&str>,
) -> (u16, Vec<u8>) {
  match (request.method.as_str(), request.path.as_str()) {
    ("POST", "/devicecode") => json_response(
      200,
      serde_json::json!({
        "device_code": "device-code-secret",
        "user_code": "ABCD-EFGH",
        "verification_uri": "https://microsoft.com/devicelogin",
        "expires_in": 300,
        "interval": 1
      }),
    ),
    ("POST", "/token") => {
      let form = form_urlencoded::parse(request.body.as_bytes())
        .into_owned()
        .collect::<HashMap<_, _>>();
      match form.get("grant_type").map(String::as_str) {
        Some("urn:ietf:params:oauth:grant-type:device_code") => {
          if decline_device_authorization {
            return json_response(400, serde_json::json!({"error":"authorization_declined"}));
          }
          let mut polls = polls.lock().unwrap();
          *polls += 1;
          if device_polls_before_success.is_none_or(|after| *polls < after) {
            json_response(400, serde_json::json!({"error":"authorization_pending"}))
          } else {
            json_response(
              200,
              serde_json::json!({
                "access_token":"microsoft-access-token",
                "refresh_token":"microsoft-refresh-token",
                "expires_in":3600
              }),
            )
          }
        }
        Some("refresh_token") => json_response(
          200,
          serde_json::json!({
            "access_token":"microsoft-refreshed-access-token",
            "refresh_token":"microsoft-rotated-refresh-token",
            "expires_in":3600
          }),
        ),
        _ => json_response(400, serde_json::json!({"error":"invalid_grant"})),
      }
    }
    ("POST", "/xbox/user") => json_response(
      200,
      serde_json::json!({
        "Token":"xbox-user-token",
        "DisplayClaims":{"xui":[{"uhs":"fixture-user-hash"}]}
      }),
    ),
    ("POST", "/xbox/xsts") => {
      let mut claims = serde_json::json!({"uhs":"fixture-user-hash"});
      if let Some(xuid) = xuid {
        claims["xid"] = serde_json::json!(xuid);
      }
      json_response(
        200,
        serde_json::json!({
          "Token":"xbox-xsts-token",
          "DisplayClaims":{"xui":[claims]}
        }),
      )
    }
    ("POST", "/minecraft/login") => json_response(
      200,
      serde_json::json!({"access_token":"minecraft-access-token","expires_in":3600}),
    ),
    ("GET", "/minecraft/profile") => {
      if !request
        .headers
        .to_ascii_lowercase()
        .contains("authorization: bearer minecraft-access-token")
        && !request
          .headers
          .to_ascii_lowercase()
          .contains("authorization: bearer minecraft-refreshed-access-token")
      {
        return json_response(401, serde_json::json!({"error":"invalid_token"}));
      }
      json_response(
        200,
        serde_json::json!({"id":ACCOUNT_ID,"name":"AnvilPlayer"}),
      )
    }
    _ => json_response(404, serde_json::json!({"error":"not_found"})),
  }
}

fn json_response(status: u16, value: serde_json::Value) -> (u16, Vec<u8>) {
  (status, serde_json::to_vec(&value).unwrap())
}

fn write_response(stream: &mut TcpStream, response: (u16, Vec<u8>)) -> std::io::Result<()> {
  let reason = if response.0 < 300 {
    "OK"
  } else if response.0 < 500 {
    "Bad Request"
  } else {
    "Not Found"
  };
  write!(
    stream,
    "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
    response.0,
    reason,
    response.1.len()
  )?;
  stream.write_all(&response.1)
}

fn service(
  fixture: &HttpFixture,
  store: Arc<MemoryCredentialStore>,
) -> super::super::AccountService<MicrosoftAccountProvider, MemoryCredentialStore> {
  super::super::AccountService::new(
    MicrosoftClientId::new(CLIENT_ID).unwrap(),
    Arc::new(MicrosoftAccountProvider::for_test(&fixture.base_url).unwrap()),
    store,
  )
}

#[tokio::test]
async fn microsoft_login_accepts_missing_optional_xuid() {
  let fixture = HttpFixture::with_xuid(Some(1), false, None);
  let session = service(&fixture, Arc::new(MemoryCredentialStore::default()))
    .authorize(
      CancellationToken::new(),
      tokio::sync::mpsc::unbounded_channel().0,
    )
    .await
    .unwrap();
  assert_eq!(session.profile().xuid(), None);
}

#[tokio::test]
async fn microsoft_login_rejects_invalid_xuid_without_storing_credentials() {
  let fixture = HttpFixture::with_xuid(Some(1), false, Some("not-a-numeric-xuid".into()));
  let credentials = Arc::new(MemoryCredentialStore::default());
  let result = service(&fixture, credentials.clone())
    .authorize(
      CancellationToken::new(),
      tokio::sync::mpsc::unbounded_channel().0,
    )
    .await;
  assert!(matches!(result, Err(Error::Authentication(_))));
  assert!(
    credentials
      .load_refresh_token(ACCOUNT_ID)
      .unwrap()
      .is_none()
  );
}

#[tokio::test]
async fn microsoft_device_flow_refreshes_through_xbox_and_minecraft_services() {
  let fixture = HttpFixture::new(Some(2), false);
  let credentials = Arc::new(MemoryCredentialStore::default());
  let service = service(&fixture, credentials.clone());
  let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
  let session = service
    .authorize(CancellationToken::new(), events)
    .await
    .unwrap();
  assert_eq!(session.profile().id(), ACCOUNT_ID);
  assert_eq!(session.profile().username(), "AnvilPlayer");
  assert_eq!(session.profile().xuid(), Some("2533274799999999"));
  assert!(!format!("{session:?}").contains("minecraft-access-token"));
  assert!(matches!(
    received.recv().await,
    Some(AuthorizationEvent::DeviceCode { ref user_code, .. }) if user_code == "ABCD-EFGH"
  ));
  assert_eq!(
    received.recv().await,
    Some(AuthorizationEvent::WaitingForApproval)
  );
  assert_eq!(
    credentials
      .load_refresh_token(ACCOUNT_ID)
      .unwrap()
      .unwrap()
      .expose_secret(),
    "microsoft-refresh-token"
  );

  let resumed = service
    .resume(
      session.profile(),
      CancellationToken::new(),
      tokio::sync::mpsc::unbounded_channel().0,
    )
    .await
    .unwrap();
  assert_eq!(resumed.profile(), session.profile());
  assert_eq!(
    credentials
      .load_refresh_token(ACCOUNT_ID)
      .unwrap()
      .unwrap()
      .expose_secret(),
    "microsoft-rotated-refresh-token"
  );

  let requests = fixture.requests.lock().unwrap();
  assert_eq!(requests[0].path, "/devicecode");
  assert_eq!(requests[1].path, "/token");
  assert_eq!(requests[2].path, "/token");
  assert!(requests[1].body.contains("device_code=device-code-secret"));
  let poll_form = form_urlencoded::parse(requests[2].body.as_bytes())
    .into_owned()
    .collect::<HashMap<_, _>>();
  assert_eq!(
    poll_form.get("grant_type").map(String::as_str),
    Some("urn:ietf:params:oauth:grant-type:device_code")
  );
  let login = requests
    .iter()
    .find(|request| request.path == "/minecraft/login")
    .unwrap();
  let login_json: serde_json::Value = serde_json::from_str(&login.body).unwrap();
  assert_eq!(
    login_json["identityToken"],
    "XBL3.0 x=fixture-user-hash;xbox-xsts-token"
  );
  assert!(
    requests
      .iter()
      .filter(|request| matches!(request.path.as_str(), "/xbox/user" | "/xbox/xsts"))
      .all(|request| {
        request
          .headers
          .to_ascii_lowercase()
          .contains("x-xbl-contract-version: 1")
      })
  );
  assert!(requests.iter().any(|request| {
    request.path == "/token" && request.body.contains("microsoft-refresh-token")
  }));
}

#[tokio::test]
async fn microsoft_device_flow_cancels_while_waiting_for_user_approval() {
  let fixture = HttpFixture::new(None, false);
  let service = service(&fixture, Arc::new(MemoryCredentialStore::default()));
  let cancel = CancellationToken::new();
  let task_cancel = cancel.clone();
  let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
  let task = tokio::spawn(async move { service.authorize(task_cancel, events).await });
  assert!(matches!(
    received.recv().await,
    Some(AuthorizationEvent::DeviceCode { .. })
  ));
  cancel.cancel();
  assert!(matches!(task.await.unwrap(), Err(Error::Cancelled)));
  assert!(
    fixture
      .requests
      .lock()
      .unwrap()
      .iter()
      .all(|request| request.path != "/xbox/user")
  );
}

#[tokio::test]
async fn microsoft_device_flow_reports_user_denial_without_continuing_authentication() {
  let fixture = HttpFixture::new(None, true);
  let service = service(&fixture, Arc::new(MemoryCredentialStore::default()));
  let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
  let result = service.authorize(CancellationToken::new(), events).await;
  assert!(matches!(
    result,
    Err(Error::Authentication(message)) if message.contains("declined")
  ));
  assert!(matches!(
    received.recv().await,
    Some(AuthorizationEvent::DeviceCode { .. })
  ));
  let requests = fixture.requests.lock().unwrap();
  assert!(requests.iter().any(|request| request.path == "/token"));
  assert!(requests.iter().all(|request| request.path != "/xbox/user"));
}
