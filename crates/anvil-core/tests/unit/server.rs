use super::*;
use sha1::{Digest, Sha1};
use std::{
  io::{Read, Write},
  net::{TcpListener, TcpStream},
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
  thread,
  time::Duration,
};

struct Fixture {
  url: Url,
  requests: Arc<AtomicUsize>,
  stop: Arc<AtomicBool>,
  worker: Option<thread::JoinHandle<()>>,
}

impl Fixture {
  fn new(data: Vec<u8>, send_bytes: usize) -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = Url::parse(&format!(
      "http://{}/server.jar",
      listener.local_addr().unwrap()
    ))
    .unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let thread_requests = requests.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let worker = thread::spawn(move || {
      while !thread_stop.load(Ordering::Relaxed) {
        match listener.accept() {
          Ok((mut stream, _)) => {
            thread_requests.fetch_add(1, Ordering::Relaxed);
            if let Err(error) = respond(&mut stream, &data, send_bytes)
              && error.kind() != std::io::ErrorKind::WouldBlock
            {
              panic!("server fixture failed: {error}");
            }
          }
          Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            thread::sleep(Duration::from_millis(1));
          }
          Err(error) => panic!("server fixture failed: {error}"),
        }
      }
    });
    Self {
      url,
      requests,
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

fn respond(stream: &mut TcpStream, data: &[u8], send_bytes: usize) -> std::io::Result<()> {
  stream.set_read_timeout(Some(Duration::from_secs(2)))?;
  stream.set_write_timeout(Some(Duration::from_secs(2)))?;
  let mut request = Vec::new();
  let mut byte = [0; 1];
  while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
    if stream.read(&mut byte)? == 0 {
      return Ok(());
    }
    request.push(byte[0]);
  }
  write!(
    stream,
    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
    data.len()
  )?;
  stream.write_all(&data[..send_bytes.min(data.len())])?;
  Ok(())
}

fn installer(root: &Path) -> ServerInstaller {
  let paths = crate::Paths {
    data: root.join("data"),
    cache: root.join("cache"),
  };
  let mut context = Context::new(paths).unwrap();
  context.http = q2q_http::Downloader::builder()
    .range_threshold(u64::MAX)
    .retries(0)
    .build()
    .unwrap();
  ServerInstaller::new(context)
}

fn plan(url: Url, data: &[u8]) -> ServerInstallPlan {
  let sha1 = Sha1::digest(data)
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect();
  make_plan(ServerArtifact {
    version_id: "1.21.1".into(),
    java_component: "java-runtime-gamma".into(),
    java_major: 21,
    url,
    size: data.len() as u64,
    sha1,
  })
  .unwrap()
}

fn progress() -> watch::Sender<ServerProgress> {
  watch::channel(ServerProgress::default()).0
}

fn payload() -> Vec<u8> {
  (0..128 * 1024)
    .map(|i| ((i * 31 + i / 7) % 251) as u8)
    .collect()
}

#[tokio::test]
async fn downloads_verified_server_jar_and_reuses_cached_content() {
  let root = tempfile::tempdir().unwrap();
  let bytes = payload();
  let fixture = Fixture::new(bytes.clone(), bytes.len());
  let installer = installer(root.path());
  let plan = plan(fixture.url.clone(), &bytes);
  assert_eq!(plan.cache_key().len(), 40);

  let first = installer
    .download(&plan, &CancellationToken::new(), &progress())
    .await
    .unwrap();
  assert!(!first.reused_cache);
  assert_eq!(fs::read(&first.cached_jar).unwrap(), bytes);
  assert_eq!(fixture.requests.load(Ordering::Relaxed), 1);

  let second = installer
    .download(&plan, &CancellationToken::new(), &progress())
    .await
    .unwrap();
  assert!(second.reused_cache);
  assert_eq!(second.cached_jar, first.cached_jar);
  assert_eq!(fixture.requests.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn checksum_failure_never_publishes_a_cache_artifact() {
  let root = tempfile::tempdir().unwrap();
  let bytes = payload();
  let fixture = Fixture::new(bytes.clone(), bytes.len());
  let installer = installer(root.path());
  let mut artifact_plan = plan(fixture.url.clone(), &bytes);
  artifact_plan.artifact.sha1 = "0".repeat(40);
  let expected_path = installer
    .context
    .paths
    .cache
    .join("servers/sha1")
    .join(format!("{}.jar", artifact_plan.cache_key()));

  let result = installer
    .download(&artifact_plan, &CancellationToken::new(), &progress())
    .await;
  assert!(matches!(
    result,
    Err(Error::Http(q2q_http::Error::Integrity))
  ));
  assert!(!expected_path.exists());
}

#[tokio::test]
async fn corrupted_published_cache_is_removed_and_downloaded_again() {
  let root = tempfile::tempdir().unwrap();
  let bytes = payload();
  let fixture = Fixture::new(bytes.clone(), bytes.len());
  let installer = installer(root.path());
  let plan = plan(fixture.url.clone(), &bytes);
  let first = installer
    .download(&plan, &CancellationToken::new(), &progress())
    .await
    .unwrap();
  assert!(!first.reused_cache);
  let mut corrupted = bytes.clone();
  corrupted[0] ^= 0xff;
  fs::write(&first.cached_jar, corrupted).unwrap();

  let repaired = installer
    .download(&plan, &CancellationToken::new(), &progress())
    .await
    .unwrap();
  assert!(!repaired.reused_cache);
  assert_eq!(fs::read(repaired.cached_jar).unwrap(), bytes);
  assert_eq!(fixture.requests.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn truncated_response_never_publishes_a_cache_artifact() {
  let root = tempfile::tempdir().unwrap();
  let bytes = payload();
  let fixture = Fixture::new(bytes.clone(), bytes.len() / 2);
  let installer = installer(root.path());
  let artifact_plan = plan(fixture.url.clone(), &bytes);
  let expected_path = installer
    .context
    .paths
    .cache
    .join("servers/sha1")
    .join(format!("{}.jar", artifact_plan.cache_key()));

  assert!(
    installer
      .download(&artifact_plan, &CancellationToken::new(), &progress())
      .await
      .is_err()
  );
  assert!(!expected_path.exists());
}

#[test]
fn install_plan_rejects_invalid_artifact_metadata() {
  let bytes = payload();
  let fixture = Fixture::new(bytes.clone(), bytes.len());
  let mut artifact_plan = plan(fixture.url.clone(), &bytes).artifact;
  artifact_plan.size = 0;
  assert!(matches!(make_plan(artifact_plan), Err(Error::Metadata(_))));
}

fn instance_material(
  root: &Path,
) -> (
  ServerInstanceManager,
  ServerInstallPlan,
  ServerInstallResult,
  Vec<u8>,
) {
  let installer = installer(root);
  let instances = ServerInstanceManager::new(installer.context.clone());
  let bytes = payload();
  let plan = plan(
    Url::parse("https://piston-data.mojang.com/server.jar").unwrap(),
    &bytes,
  );
  let cached_jar = installer
    .context
    .paths
    .cache
    .join("servers/sha1")
    .join(format!("{}.jar", plan.cache_key()));
  fs::create_dir_all(cached_jar.parent().unwrap()).unwrap();
  fs::write(&cached_jar, &bytes).unwrap();
  (
    instances,
    plan,
    ServerInstallResult {
      cached_jar,
      reused_cache: false,
    },
    bytes,
  )
}

#[cfg(all(unix, not(target_env = "musl")))]
fn install_test_java(data: &Path) {
  let platform = crate::platform::Platform::current().unwrap();
  let directory = data.join("runtimes/temurin-fixture");
  let executable = directory.join("jdk/bin/java");
  fs::create_dir_all(executable.parent().unwrap()).unwrap();
  let architecture = match platform.arch {
    "x64" => "amd64",
    "aarch64" => "aarch64",
    _ => unreachable!(),
  };
  fs::write(
    &executable,
    format!(
      "#!/bin/sh\nprintf '    java.runtime.version = 21.0.8+9\\n    os.arch = {architecture}\\n' >&2\n"
    ),
  )
  .unwrap();
  use std::os::unix::fs::PermissionsExt;
  fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
  fs::write(
    directory.join("anvil-runtime.json"),
    serde_json::to_vec_pretty(&serde_json::json!({
      "schema_version": 1,
      "id": "temurin-fixture",
      "vendor": "temurin",
      "version": "21.0.8+9",
      "major": 21,
      "image_type": "jre",
      "os": platform.os,
      "arch": platform.arch,
      "archive_sha256": "0".repeat(64),
      "home": "jdk"
    }))
    .unwrap(),
  )
  .unwrap();
}

async fn create_test_instance(root: &Path, name: &str) -> (ServerInstanceManager, ServerInstance) {
  let (manager, plan, download, _) = instance_material(root);
  let instance = manager
    .create(name, &plan, &download, &CancellationToken::new())
    .await
    .unwrap();
  (manager, instance)
}

#[tokio::test]
async fn create_publishes_a_complete_instance_and_list_reads_it() {
  let root = tempfile::tempdir().unwrap();
  let (manager, plan, download, bytes) = instance_material(root.path());
  let instance = manager
    .create("Survival", &plan, &download, &CancellationToken::new())
    .await
    .unwrap();
  assert_eq!(instance.id, "server-survival");
  assert_eq!(instance.name, "Survival");
  assert_eq!(instance.version_id, "1.21.1");
  assert!(!instance.eula_accepted);

  let directory = root.path().join("data/instances/server-survival");
  assert_eq!(fs::read(directory.join("server.jar")).unwrap(), bytes);
  assert!(directory.join("game").is_dir());
  assert_eq!(
    fs::read_to_string(directory.join("instance.json")).unwrap(),
    serde_json::to_string_pretty(&instance).unwrap()
  );
  assert_eq!(manager.list().await.unwrap(), vec![instance.clone()]);

  fs::write(directory.join("game/world.keep"), b"world data").unwrap();
  assert!(matches!(
    manager
      .create("survival", &plan, &download, &CancellationToken::new())
      .await,
    Err(Error::Conflict(_))
  ));
  assert_eq!(
    fs::read(directory.join("game/world.keep")).unwrap(),
    b"world data"
  );
  assert_eq!(manager.list().await.unwrap(), vec![instance]);
}

#[tokio::test]
async fn prepare_start_requires_eula_before_installing_java() {
  let root = tempfile::tempdir().unwrap();
  let (manager, instance) = create_test_instance(root.path(), "needs-eula").await;
  let error = manager
    .prepare_start(
      "needs-eula",
      false,
      &CancellationToken::new(),
      watch::channel(JavaProgress::default()).0,
    )
    .await
    .unwrap_err();

  assert_eq!(error.code(), "eula_required");
  assert_eq!(error.exit_code(), 3);
  assert!(!root.path().join("data/runtimes").exists());
  assert!(
    !root
      .path()
      .join("data/instances")
      .join(&instance.id)
      .join("game/eula.txt")
      .exists()
  );
  assert!(!manager.list().await.unwrap()[0].eula_accepted);
}

#[cfg(all(unix, not(target_env = "musl")))]
#[tokio::test]
async fn prepare_start_records_explicit_eula_and_reuses_valid_managed_java() {
  let root = tempfile::tempdir().unwrap();
  let (manager, instance) = create_test_instance(root.path(), "prepared").await;
  install_test_java(&root.path().join("data"));

  let prepared = manager
    .prepare_start(
      "prepared",
      true,
      &CancellationToken::new(),
      watch::channel(JavaProgress::default()).0,
    )
    .await
    .unwrap();
  assert_eq!(prepared.java.id, "temurin-fixture");
  assert_eq!(prepared.instance.id, instance.id);
  let game_directory = prepared.game_directory.clone();
  assert_eq!(
    fs::read_to_string(game_directory.join("eula.txt")).unwrap(),
    "# https://aka.ms/MinecraftEULA\neula=true\n"
  );
  let recorded = &manager.list().await.unwrap()[0];
  assert!(recorded.eula_accepted);
  assert_eq!(recorded.eula_url.as_deref(), Some(MINECRAFT_EULA_URL));

  assert!(matches!(
    manager
      .prepare_start(
        "prepared",
        false,
        &CancellationToken::new(),
        watch::channel(JavaProgress::default()).0,
      )
      .await,
    Err(Error::Busy(_))
  ));
  drop(prepared);

  let repeated = manager
    .prepare_start(
      "prepared",
      false,
      &CancellationToken::new(),
      watch::channel(JavaProgress::default()).0,
    )
    .await
    .unwrap();
  assert_eq!(repeated.java.id, "temurin-fixture");
}

#[cfg(all(unix, not(target_env = "musl")))]
#[tokio::test]
async fn an_eula_file_set_to_false_requires_a_new_explicit_acceptance() {
  let root = tempfile::tempdir().unwrap();
  let (manager, instance) = create_test_instance(root.path(), "revoked").await;
  install_test_java(&root.path().join("data"));
  let prepared = manager
    .prepare_start(
      "revoked",
      true,
      &CancellationToken::new(),
      watch::channel(JavaProgress::default()).0,
    )
    .await
    .unwrap();

  let eula = prepared.game_directory.join("eula.txt");
  drop(prepared);
  fs::write(&eula, "eula=false\n").unwrap();
  assert!(matches!(
    manager
      .prepare_start(
        "revoked",
        false,
        &CancellationToken::new(),
        watch::channel(JavaProgress::default()).0,
      )
      .await,
    Err(Error::EulaRequired)
  ));
  assert_eq!(fs::read_to_string(&eula).unwrap(), "eula=false\n");

  manager
    .prepare_start(
      "revoked",
      true,
      &CancellationToken::new(),
      watch::channel(JavaProgress::default()).0,
    )
    .await
    .unwrap();
  assert_eq!(
    fs::read_to_string(eula).unwrap(),
    "# https://aka.ms/MinecraftEULA\neula=true\n"
  );
  let recorded = manager.list().await.unwrap().remove(0);
  assert_eq!(recorded.id, instance.id);
  assert!(recorded.eula_accepted);
}

#[tokio::test]
async fn corrupt_cache_and_cancelled_create_never_publish_an_instance() {
  let root = tempfile::tempdir().unwrap();
  let (manager, plan, download, mut bytes) = instance_material(root.path());
  bytes[0] ^= 0xff;
  fs::write(&download.cached_jar, &bytes).unwrap();
  assert!(matches!(
    manager
      .create("corrupt", &plan, &download, &CancellationToken::new())
      .await,
    Err(Error::Http(q2q_http::Error::Integrity))
  ));
  assert!(!root.path().join("data/instances/server-corrupt").exists());
  assert_eq!(
    fs::read_dir(root.path().join("data/instances"))
      .unwrap()
      .count(),
    0
  );

  let cancel = CancellationToken::new();
  cancel.cancel();
  assert!(matches!(
    manager.create("cancelled", &plan, &download, &cancel).await,
    Err(Error::Cancelled)
  ));
  assert!(!root.path().join("data/instances/server-cancelled").exists());
}

#[test]
fn server_names_are_safe_and_case_insensitive() {
  assert_eq!(
    server_instance_id("My-World_1").unwrap(),
    "server-my-world_1"
  );
  for invalid in ["", "../outside", "two words", "trailing.", "a/b"] {
    assert!(matches!(
      server_instance_id(invalid),
      Err(Error::Invalid(_))
    ));
  }
  assert!(matches!(
    server_instance_id(&"a".repeat(49)),
    Err(Error::Invalid(_))
  ));
}

#[tokio::test]
async fn instance_lock_excludes_another_creator() {
  let root = tempfile::tempdir().unwrap();
  let (manager, plan, download, _) = instance_material(root.path());
  let _lock = storage::server_instance_lock(&root.path().join("data"), "server-locked").unwrap();
  assert!(matches!(
    manager
      .create("locked", &plan, &download, &CancellationToken::new())
      .await,
    Err(Error::Busy(_))
  ));
}

#[tokio::test]
async fn concurrent_create_has_only_one_published_instance() {
  let root = tempfile::tempdir().unwrap();
  let (manager, plan, download, _) = instance_material(root.path());
  let left_cancel = CancellationToken::new();
  let right_cancel = CancellationToken::new();
  let (left, right) = tokio::join!(
    manager.create("shared", &plan, &download, &left_cancel),
    manager.create("SHARED", &plan, &download, &right_cancel),
  );
  assert_ne!(left.is_ok(), right.is_ok());
  let error = if let Err(error) = left {
    error
  } else {
    right.unwrap_err()
  };
  assert!(matches!(error, Error::Busy(_) | Error::Conflict(_)));
  let servers = manager.list().await.unwrap();
  assert_eq!(servers.len(), 1);
  assert_eq!(servers[0].id, "server-shared");
}
