use super::*;
use std::{
  collections::HashMap,
  io::{Read, Write},
  net::{TcpListener, TcpStream},
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  thread,
  time::Duration,
};

struct Fixture {
  manifest_url: Url,
  stop: Arc<AtomicBool>,
  worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
  fn new(manifest: Vec<u8>, document: Vec<u8>) -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let manifest_url = Url::parse(&format!("{base}manifest.json")).unwrap();
    let version_url = format!("{base}version.json");
    let manifest = String::from_utf8(manifest)
      .unwrap()
      .replace("http://127.0.0.1/version.json", &version_url)
      .into_bytes();
    let responses = HashMap::from([
      ("/manifest.json".to_owned(), manifest),
      ("/version.json".to_owned(), document),
    ]);
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let worker = thread::spawn(move || {
      while !thread_stop.load(Ordering::Relaxed) {
        match listener.accept() {
          Ok((mut stream, _)) => {
            if let Err(error) = respond(&mut stream, &responses)
              && error.kind() != std::io::ErrorKind::WouldBlock
            {
              panic!("metadata fixture failed: {error}");
            }
          }
          Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            thread::sleep(Duration::from_millis(1));
          }
          Err(error) => panic!("{error}"),
        }
      }
    });
    Self {
      manifest_url,
      stop,
      worker: Some(worker),
    }
  }
}
impl Drop for Fixture {
  fn drop(&mut self) {
    self.stop.store(true, Ordering::Relaxed);
    self.worker.take().unwrap().join().unwrap();
  }
}

fn respond(stream: &mut TcpStream, responses: &HashMap<String, Vec<u8>>) -> std::io::Result<()> {
  // Accepted sockets may inherit the listener's nonblocking mode on some platforms.
  stream.set_nonblocking(false)?;
  stream.set_read_timeout(Some(Duration::from_secs(15)))?;
  let mut request = Vec::new();
  let mut byte = [0u8; 1];
  while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
    if stream.read(&mut byte)? == 0 {
      return Ok(());
    }
    request.push(byte[0]);
  }
  let request = String::from_utf8_lossy(&request);
  let path = request
    .lines()
    .next()
    .unwrap_or("")
    .split_whitespace()
    .nth(1)
    .unwrap_or("/");
  if let Some(body) = responses.get(path) {
    write!(
      stream,
      "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
      body.len()
    )?;
    if path == "/version.json" && body.len() > VERSION_JSON_LIMIT as usize {
      return Ok(());
    }
    stream.write_all(body)?;
  } else {
    stream
      .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
  }
  Ok(())
}

fn valid_fixture(document: &[u8]) -> Vec<u8> {
  serde_json::to_vec(&serde_json::json!({
    "versions": [{
      "id": "1.21.1", "type": "release", "url": "http://127.0.0.1/version.json",
      "sha1": super::sha1_hex(document), "time": "2024-08-08T00:00:00+00:00",
      "releaseTime": "2024-08-08T00:00:00+00:00", "complianceLevel": 1
    }]
  }))
  .unwrap()
}
fn version_document(id: &str) -> Vec<u8> {
  serde_json::to_vec(&serde_json::json!({
    "id": id,
    "javaVersion": {"component":"java-runtime-gamma", "majorVersion":21},
    "downloads": {"server": {
      "sha1":"0123456789abcdef0123456789abcdef01234567",
      "size": 61283782,
      "url":"https://piston-data.mojang.com/v1/objects/0123456789abcdef0123456789abcdef01234567/server.jar"
    }}
  })).unwrap()
}
fn client_version_document(id: &str) -> Vec<u8> {
  serde_json::to_vec(&serde_json::json!({
    "id": id,
    "type":"release",
    "javaVersion": {"component":"java-runtime-delta", "majorVersion":21},
    "downloads": {"client": {
      "sha1":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "size": 123456789,
      "url":"https://piston-data.mojang.com/v1/objects/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/client.jar"
    }},
    "assetIndex": {
      "id":"17",
      "sha1":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "size": 42,
      "url":"https://piston-meta.mojang.com/v1/packages/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/17.json"
    },
    "mainClass":"net.minecraft.client.main.Main",
    "logging":{"client":{
      "argument":"-Dlog4j.configurationFile=${path}",
      "file":{"id":"client-1.12.xml","sha1":"dddddddddddddddddddddddddddddddddddddddd","size":888,
        "url":"https://piston-data.mojang.com/v1/objects/dddddddddddddddddddddddddddddddddddddddd/client-1.12.xml"},
      "type":"log4j2-xml"
    }},
    "libraries":[{"name":"org.lwjgl:lwjgl:3.3.3","downloads":{"artifact":{
      "path":"org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar",
      "url":"https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar",
      "size": 100,
      "sha1":"cccccccccccccccccccccccccccccccccccccccc"
    }}}],
    "arguments": {
      "jvm":["-Xmx2G", {"rules":[{"action":"allow","os":{"name":"osx"}}],"value":["-XstartOnFirstThread"]}],
      "game":["--username", {"rules":[{"action":"allow"}],"value":"${auth_player_name}"}]
    }
  })).unwrap()
}
fn executor() -> Executor {
  Executor::builder().build().unwrap()
}

#[tokio::test]
async fn lists_versions_from_the_official_manifest_shape() {
  let document = version_document("1.21.1");
  let mut manifest: serde_json::Value = serde_json::from_slice(&valid_fixture(&document)).unwrap();
  let mut snapshot = manifest["versions"][0].clone();
  snapshot["id"] = serde_json::Value::String("24w33a".into());
  snapshot["type"] = serde_json::Value::String("snapshot".into());
  snapshot["releaseTime"] = serde_json::Value::String("2024-08-14T12:00:00+00:00".into());
  manifest["versions"].as_array_mut().unwrap().push(snapshot);
  let fixture = Fixture::new(serde_json::to_vec(&manifest).unwrap(), document);

  let versions = list_versions_from(&executor(), fixture.manifest_url.clone())
    .await
    .unwrap();
  assert_eq!(
    versions,
    vec![
      GameVersion {
        id: "1.21.1".into(),
        version_type: "release".into(),
        release_time: "2024-08-08T00:00:00+00:00".into(),
      },
      GameVersion {
        id: "24w33a".into(),
        version_type: "snapshot".into(),
        release_time: "2024-08-14T12:00:00+00:00".into(),
      },
    ]
  );
}

#[tokio::test]
async fn resolves_server_and_java_requirement_through_verified_metadata_chain() {
  let document = version_document("1.21.1");
  let fixture = Fixture::new(valid_fixture(&document), document);
  let resolved = resolve_server_from(&executor(), "1.21.1", fixture.manifest_url.clone())
    .await
    .unwrap();
  assert_eq!(resolved.version_id, "1.21.1");
  assert_eq!(resolved.java_component, "java-runtime-gamma");
  assert_eq!(resolved.java_major, 21);
  assert_eq!(resolved.size, 61283782);
  assert_eq!(resolved.sha1, "0123456789abcdef0123456789abcdef01234567");
  assert_eq!(resolved.url.scheme(), "https");
}

#[tokio::test]
async fn rejects_a_version_json_that_does_not_match_the_manifest_hash() {
  let document = version_document("1.21.1");
  let mut manifest: serde_json::Value = serde_json::from_slice(&valid_fixture(&document)).unwrap();
  manifest["versions"][0]["sha1"] = serde_json::Value::String("f".repeat(40));
  let fixture = Fixture::new(serde_json::to_vec(&manifest).unwrap(), document);
  let result = resolve_server_from(&executor(), "1.21.1", fixture.manifest_url.clone()).await;
  assert!(
    matches!(&result, Err(Error::Metadata(message)) if message.contains("SHA-1")),
    "unexpected metadata result: {result:?}"
  );
}

#[tokio::test]
async fn rejects_missing_server_and_java_requirements() {
  for (downloads, java) in [
    (
      serde_json::json!({"downloads":{}}),
      serde_json::json!({"javaVersion":{"component":"java-runtime-gamma","majorVersion":21}}),
    ),
    (
      serde_json::json!({"downloads":{"server":{"url":"https://example.com/server.jar","size":12,"sha1":"0123456789abcdef0123456789abcdef01234567"}}}),
      serde_json::json!({}),
    ),
  ] {
    let document = serde_json::to_vec(&serde_json::json!({"id":"1.21.1", "javaVersion":java.get("javaVersion"), "downloads":downloads.get("downloads")})).unwrap();
    let fixture = Fixture::new(valid_fixture(&document), document);
    let result = resolve_server_from(&executor(), "1.21.1", fixture.manifest_url.clone()).await;
    assert!(
      matches!(&result, Err(Error::Metadata(_))),
      "unexpected metadata result: {result:?}"
    );
  }
}

#[tokio::test]
async fn rejects_unknown_version_invalid_id_and_untrusted_metadata_origin() {
  let document = version_document("1.21.1");
  let fixture = Fixture::new(valid_fixture(&document), document);
  let result = resolve_server_from(&executor(), "1.21.2", fixture.manifest_url.clone()).await;
  assert!(
    matches!(&result, Err(Error::NotFound(_))),
    "unexpected unknown-version result: {result:?}"
  );
  assert!(matches!(
    resolve_server_from(&executor(), "../server", fixture.manifest_url.clone()).await,
    Err(Error::Invalid(_))
  ));
  assert!(matches!(
    resolve_server_from(
      &executor(),
      "1.21.1",
      Url::parse("https://example.com/manifest.json").unwrap()
    )
    .await,
    Err(Error::Metadata(_))
  ));
}

#[tokio::test]
async fn rejects_invalid_artifact_metadata_before_returning_a_plan() {
  let bad = serde_json::to_vec(&serde_json::json!({
    "id":"1.21.1", "javaVersion":{"component":"java-runtime-gamma","majorVersion":21},
    "downloads":{"server":{"url":"http://example.com/server.jar","size":1,"sha1":"0123456789abcdef0123456789abcdef01234567"}}
  })).unwrap();
  let fixture = Fixture::new(valid_fixture(&bad), bad);
  let result = resolve_server_from(&executor(), "1.21.1", fixture.manifest_url.clone()).await;
  assert!(
    matches!(&result, Err(Error::Metadata(message)) if message.contains("HTTPS")),
    "unexpected metadata result: {result:?}"
  );
}

#[tokio::test]
async fn resolves_client_metadata_after_manifest_hash_verification() {
  let document = client_version_document("1.21.1");
  let fixture = Fixture::new(valid_fixture(&document), document.clone());
  let resolved = resolve_client_from(&executor(), "1.21.1", fixture.manifest_url.clone())
    .await
    .unwrap();

  assert_eq!(resolved.version_id, "1.21.1");
  assert_eq!(resolved.version_type, "release");
  assert_eq!(resolved.metadata_sha1, super::sha1_hex(&document));
  assert_eq!(resolved.java_component, "java-runtime-delta");
  assert_eq!(resolved.java_major, 21);
  assert_eq!(resolved.client_jar.size, 123456789);
  assert_eq!(resolved.client_jar.sha1, "a".repeat(40));
  assert_eq!(resolved.client_jar.url.scheme(), "https");
  assert_eq!(resolved.asset_index.id, "17");
  assert_eq!(resolved.asset_index.file.size, 42);
  assert_eq!(resolved.main_class, "net.minecraft.client.main.Main");
  let logging = resolved.logging.unwrap();
  assert_eq!(logging.kind, "log4j2-xml");
  assert_eq!(logging.file_id, "client-1.12.xml");
  assert_eq!(logging.file.size, 888);
  assert_eq!(resolved.libraries.len(), 1);
  assert_eq!(resolved.arguments.jvm.len(), 2);
  assert_eq!(resolved.arguments.game.len(), 2);
}

#[tokio::test]
async fn rejects_client_metadata_with_missing_or_invalid_required_fields() {
  let mut documents = Vec::new();

  let mut missing_java: serde_json::Value =
    serde_json::from_slice(&client_version_document("1.21.1")).unwrap();
  missing_java.as_object_mut().unwrap().remove("javaVersion");
  documents.push((
    "Java requirement",
    serde_json::to_vec(&missing_java).unwrap(),
  ));

  let mut missing_client: serde_json::Value =
    serde_json::from_slice(&client_version_document("1.21.1")).unwrap();
  missing_client["downloads"]
    .as_object_mut()
    .unwrap()
    .remove("client");
  documents.push(("client JAR", serde_json::to_vec(&missing_client).unwrap()));

  for (label, update) in [
    (
      "HTTP client URL",
      ("url", serde_json::json!("http://example.com/client.jar")),
    ),
    ("zero client size", ("size", serde_json::json!(0))),
    (
      "invalid client SHA-1",
      ("sha1", serde_json::json!("not-a-sha1")),
    ),
    (
      "oversized client JAR",
      ("size", serde_json::json!(MAX_CLIENT_JAR + 1)),
    ),
  ] {
    let mut document: serde_json::Value =
      serde_json::from_slice(&client_version_document("1.21.1")).unwrap();
    document["downloads"]["client"][update.0] = update.1;
    documents.push((label, serde_json::to_vec(&document).unwrap()));
  }

  for (label, update) in [
    (
      "HTTP asset index URL",
      ("url", serde_json::json!("http://example.com/assets.json")),
    ),
    (
      "oversized asset index",
      ("size", serde_json::json!(MAX_ASSET_INDEX + 1)),
    ),
    (
      "invalid asset index SHA-1",
      ("sha1", serde_json::json!("not-a-sha1")),
    ),
  ] {
    let mut document: serde_json::Value =
      serde_json::from_slice(&client_version_document("1.21.1")).unwrap();
    document["assetIndex"][update.0] = update.1;
    documents.push((label, serde_json::to_vec(&document).unwrap()));
  }

  let mut missing_index: serde_json::Value =
    serde_json::from_slice(&client_version_document("1.21.1")).unwrap();
  missing_index.as_object_mut().unwrap().remove("assetIndex");
  documents.push(("asset index", serde_json::to_vec(&missing_index).unwrap()));

  let mut missing_arguments: serde_json::Value =
    serde_json::from_slice(&client_version_document("1.21.1")).unwrap();
  missing_arguments
    .as_object_mut()
    .unwrap()
    .remove("arguments");
  documents.push(("arguments", serde_json::to_vec(&missing_arguments).unwrap()));

  for (label, document) in documents {
    let fixture = Fixture::new(valid_fixture(&document), document);
    let result = resolve_client_from(&executor(), "1.21.1", fixture.manifest_url.clone()).await;
    assert!(
      matches!(result, Err(Error::Metadata(_))),
      "{label} should be rejected, got {result:?}"
    );
  }
}

#[tokio::test]
async fn rejects_client_version_hash_id_and_untrusted_metadata_source() {
  let document = client_version_document("1.21.1");
  let mut manifest: serde_json::Value = serde_json::from_slice(&valid_fixture(&document)).unwrap();
  manifest["versions"][0]["sha1"] = serde_json::Value::String("f".repeat(40));
  let fixture = Fixture::new(serde_json::to_vec(&manifest).unwrap(), document.clone());
  let result = resolve_client_from(&executor(), "1.21.1", fixture.manifest_url.clone()).await;
  assert!(
    matches!(&result, Err(Error::Metadata(message)) if message.contains("SHA-1")),
    "unexpected hash result: {result:?}"
  );

  let wrong_id_document = client_version_document("1.21.2");
  let fixture = Fixture::new(valid_fixture(&wrong_id_document), wrong_id_document);
  let result = resolve_client_from(&executor(), "1.21.1", fixture.manifest_url.clone()).await;
  assert!(
    matches!(&result, Err(Error::Metadata(message)) if message.contains("ID")),
    "unexpected ID result: {result:?}"
  );

  assert!(matches!(
    resolve_client_from(
      &executor(),
      "1.21.1",
      Url::parse("https://example.com/manifest.json").unwrap()
    )
    .await,
    Err(Error::Metadata(_))
  ));
}

#[tokio::test]
async fn rejects_oversized_client_version_json() {
  let document = vec![b'x'; VERSION_JSON_LIMIT as usize + 1];
  let fixture = Fixture::new(valid_fixture(&document), document);
  let result = resolve_client_from(&executor(), "1.21.1", fixture.manifest_url.clone()).await;
  assert!(result.is_err(), "oversized version JSON should fail");
}

#[tokio::test]
#[ignore = "live Mojang metadata smoke; requires network access"]
async fn live_mojang_1_21_1_server_metadata_resolves() {
  let artifact = resolve_server(&executor(), "1.21.1").await.unwrap();
  assert_eq!(artifact.version_id, "1.21.1");
  assert_eq!(artifact.java_major, 21);
  assert!(artifact.size > 1_000_000);
  assert_eq!(artifact.sha1.len(), 40);
  assert_eq!(artifact.url.scheme(), "https");
}
