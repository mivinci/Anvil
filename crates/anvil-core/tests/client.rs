use super::*;
use crate::client_plan::{ClientInstallPlan, ClientNativeArchive, PlannedFile};
use sha1::{Digest, Sha1};
use std::{
  io::{Cursor, Read, Write},
  net::{TcpListener, TcpStream},
  sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
  thread,
  time::Duration,
};

struct Fixture {
  url: Url,
  state: Arc<Mutex<Response>>,
  requests: Arc<AtomicUsize>,
  stop: Arc<AtomicBool>,
  worker: Option<thread::JoinHandle<()>>,
}

#[derive(Clone)]
struct Response {
  bytes: Vec<u8>,
  sent_bytes: usize,
  delay: Duration,
}

impl Fixture {
  fn new(bytes: Vec<u8>) -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = Url::parse(&format!(
      "http://{}/payload",
      listener.local_addr().unwrap()
    ))
    .unwrap();
    let state = Arc::new(Mutex::new(Response {
      sent_bytes: bytes.len(),
      bytes,
      delay: Duration::ZERO,
    }));
    let requests = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let thread_state = state.clone();
    let thread_requests = requests.clone();
    let thread_stop = stop.clone();
    let worker = thread::spawn(move || {
      while !thread_stop.load(Ordering::Relaxed) {
        match listener.accept() {
          Ok((mut stream, _)) => {
            stream.set_nonblocking(false).unwrap();
            if let Err(error) =
              serve_connection(&mut stream, &thread_state, &thread_requests, &thread_stop)
              && error.kind() != std::io::ErrorKind::WouldBlock
              && error.kind() != std::io::ErrorKind::BrokenPipe
              && error.kind() != std::io::ErrorKind::ConnectionReset
              && error.kind() != std::io::ErrorKind::ConnectionAborted
              // macOS sometimes reports EINVAL instead of EPIPE when the
              // fixture writes to a connection the client aborted mid-download.
              && error.kind() != std::io::ErrorKind::InvalidInput
            {
              panic!("client download fixture failed: {error}");
            }
          }
          Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            thread::sleep(Duration::from_millis(1));
          }
          Err(error) => panic!("client download fixture failed: {error}"),
        }
      }
    });
    Self {
      url,
      state,
      requests,
      stop,
      worker: Some(worker),
    }
  }

  fn set(&self, bytes: Vec<u8>, sent_bytes: usize) {
    *self.state.lock().unwrap() = Response {
      sent_bytes,
      bytes,
      delay: Duration::ZERO,
    };
  }

  fn delay(&self, delay: Duration) {
    self.state.lock().unwrap().delay = delay;
  }
}

impl Drop for Fixture {
  fn drop(&mut self) {
    self.stop.store(true, Ordering::Relaxed);
    self.worker.take().unwrap().join().unwrap();
  }
}

fn serve_connection(
  stream: &mut TcpStream,
  state: &Mutex<Response>,
  requests: &AtomicUsize,
  stop: &AtomicBool,
) -> std::io::Result<()> {
  loop {
    match respond(stream, state, requests) {
      Ok(Some(false)) => {}
      Ok(Some(true) | None) => return Ok(()),
      Err(error)
        if matches!(
          error.kind(),
          std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ) && !stop.load(Ordering::Relaxed) =>
      {
        continue;
      }
      Err(error)
        if matches!(
          error.kind(),
          std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ) =>
      {
        return Ok(());
      }
      Err(error) => return Err(error),
    }
  }
}

fn respond(
  stream: &mut TcpStream,
  state: &Mutex<Response>,
  requests: &AtomicUsize,
) -> std::io::Result<Option<bool>> {
  stream.set_read_timeout(Some(Duration::from_millis(100)))?;
  stream.set_write_timeout(Some(Duration::from_secs(10)))?;
  let mut request = Vec::new();
  let mut byte = [0; 1];
  while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
    if stream.read(&mut byte)? == 0 {
      return Ok(None);
    }
    request.push(byte[0]);
  }
  requests.fetch_add(1, Ordering::Relaxed);
  let response = state.lock().unwrap().clone();
  if !response.delay.is_zero() {
    thread::sleep(response.delay);
  }
  write!(
    stream,
    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n",
    response.bytes.len(),
    if response.sent_bytes < response.bytes.len() {
      "close"
    } else {
      "keep-alive"
    }
  )?;
  stream.write_all(&response.bytes[..response.sent_bytes.min(response.bytes.len())])?;
  Ok(Some(response.sent_bytes < response.bytes.len()))
}

fn installer(root: &Path) -> ClientInstaller {
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
  ClientInstaller::new(context)
}

fn progress() -> watch::Sender<ClientProgress> {
  watch::channel(ClientProgress::default()).0
}

fn plan(url: Url, bytes: &[u8]) -> ClientInstallPlan {
  let file = ClientFile {
    url,
    size: bytes.len() as u64,
    sha1: sha1_hex(bytes),
  };
  ClientInstallPlan {
    loader: None,
    version_id: "1.21.1".into(),
    version_type: "release".into(),
    metadata_url: Url::parse("https://piston-meta.mojang.com/v1/packages/test/1.21.1.json")
      .unwrap(),
    metadata_sha1: "1".repeat(40),
    target: ClientTarget::new(ClientOs::Linux, ClientArchitecture::X86_64),
    java_component: "java-runtime-gamma".into(),
    java_major: 21,
    client_jar: PlannedFile {
      path: "versions/1.21.1/1.21.1.jar".into(),
      file: file.clone(),
    },
    libraries: Vec::new(),
    native_archives: Vec::new(),
    asset_index_id: "1.21.1".into(),
    asset_index: PlannedFile {
      path: "indexes/1.21.1.json".into(),
      file,
    },
    assets: Vec::new(),
    logging: None,
    arguments: crate::metadata::ClientArguments::default(),
    main_class: "net.minecraft.client.main.Main".into(),
  }
}

fn payload() -> Vec<u8> {
  (0..128 * 1024)
    .map(|index| ((index * 31 + index / 7) % 251) as u8)
    .collect()
}

fn sha1_hex(bytes: &[u8]) -> String {
  Sha1::digest(bytes)
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect()
}

#[tokio::test]
async fn installs_atomic_instance_and_reuses_shared_content_cache() {
  let root = tempfile::tempdir().unwrap();
  let bytes = payload();
  let fixture = Fixture::new(bytes.clone());
  let installer = installer(root.path());
  let plan = plan(fixture.url.clone(), &bytes);

  let first = installer
    .install("Survival", &plan, &CancellationToken::new(), &progress())
    .await
    .unwrap();
  assert_eq!(first.downloaded_files, 1);
  assert_eq!(first.reused_files, 0);
  assert!(!first.already_installed);
  assert_eq!(first.installed.files.len(), 2);
  assert_eq!(
    installer
      .load_install_record(&first.instance)
      .await
      .unwrap(),
    first.installed
  );
  let repeated = installer
    .install("Survival", &plan, &CancellationToken::new(), &progress())
    .await
    .unwrap();
  assert!(repeated.already_installed);
  assert_eq!(repeated.downloaded_files, 0);
  assert_eq!(repeated.reused_files, 1);
  assert_eq!(
    installer.list().await.unwrap(),
    vec![first.instance.clone()]
  );
  assert_eq!(fixture.requests.load(Ordering::Relaxed), 1);

  let second_progress = progress();
  let second = installer
    .install(
      "Creative",
      &plan,
      &CancellationToken::new(),
      &second_progress,
    )
    .await
    .unwrap();
  assert_eq!(second.downloaded_files, 0);
  assert_eq!(second.reused_files, 1);
  assert_eq!(second_progress.borrow().received_bytes, 0);
  assert_eq!(
    second_progress.borrow().downloaded_bytes,
    bytes.len() as u64
  );
  assert!(!second.already_installed);
  assert_eq!(fixture.requests.load(Ordering::Relaxed), 1);
  assert!(
    root
      .path()
      .join("data/instances/client-survival/game")
      .is_dir()
  );
}

#[tokio::test]
async fn fabric_instance_downloads_only_added_library_and_reuses_base_game() {
  let root = tempfile::tempdir().unwrap();
  let base = payload();
  let base_server = Fixture::new(base.clone());
  let extra = b"local Fabric library fixture".to_vec();
  let extra_server = Fixture::new(extra.clone());
  let installer = installer(root.path());
  let base_plan = plan(base_server.url.clone(), &base);
  let first = installer
    .install(
      "Vanilla",
      &base_plan,
      &CancellationToken::new(),
      &progress(),
    )
    .await
    .unwrap();
  let mut fabric = base_plan;
  fabric.loader = Some(crate::fabric::ClientLoader::Fabric {
    version: "0.16.10".into(),
  });
  fabric.libraries.push(crate::client_plan::ClientLibrary {
    coordinate: "fixture:fabric:1".into(),
    artifact: PlannedFile {
      path: "fixture/fabric/1/fabric-1.jar".into(),
      file: ClientFile {
        url: extra_server.url.clone(),
        size: extra.len() as u64,
        sha1: sha1_hex(&extra),
      },
    },
    kind: crate::client_plan::ClientLibraryKind::Classpath,
  });
  let updates = progress();
  let second = installer
    .install("Fabric", &fabric, &CancellationToken::new(), &updates)
    .await
    .unwrap();
  assert_eq!(second.reused_files, 1);
  assert_eq!(second.downloaded_files, 1);
  assert_eq!(base_server.requests.load(Ordering::Relaxed), 1);
  assert_eq!(extra_server.requests.load(Ordering::Relaxed), 1);
  assert_eq!(updates.borrow().received_bytes, extra.len() as u64);
  assert_eq!(
    updates.borrow().downloaded_bytes,
    (base.len() + extra.len()) as u64
  );
  let jar = |r: &ClientInstallRecord| {
    r.files
      .iter()
      .find(|f| f.path.ends_with("1.21.1.jar"))
      .unwrap()
      .cache_path
      .clone()
  };
  assert_eq!(jar(&first.installed), jar(&second.installed));
}

#[tokio::test]
async fn repairs_corrupted_cached_content_before_publishing_update() {
  let root = tempfile::tempdir().unwrap();
  let bytes = payload();
  let fixture = Fixture::new(bytes.clone());
  let installer = installer(root.path());
  let plan = plan(fixture.url.clone(), &bytes);
  installer
    .install("Main", &plan, &CancellationToken::new(), &progress())
    .await
    .unwrap();
  let cache_file = root
    .path()
    .join("cache/clients/sha1")
    .join(sha1_hex(&bytes));
  fs::write(&cache_file, b"corrupt").unwrap();

  let repaired = installer
    .install("Main", &plan, &CancellationToken::new(), &progress())
    .await
    .unwrap();
  assert_eq!(repaired.downloaded_files, 1);
  assert_eq!(repaired.reused_files, 0);
  assert_eq!(fs::read(cache_file).unwrap(), bytes);
  assert_eq!(fixture.requests.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn integrity_failure_never_publishes_a_client_instance_and_can_retry() {
  let root = tempfile::tempdir().unwrap();
  let expected = payload();
  let fixture = Fixture::new(vec![7; expected.len()]);
  let installer = installer(root.path());
  let plan = plan(fixture.url.clone(), &expected);

  assert!(matches!(
    installer
      .install("Retry", &plan, &CancellationToken::new(), &progress())
      .await,
    Err(Error::Http(q2q_http::Error::Integrity))
  ));
  assert!(installer.list().await.unwrap().is_empty());
  assert!(!root.path().join("data/instances/client-retry").exists());

  fixture.set(expected.clone(), expected.len());
  let installed = installer
    .install("Retry", &plan, &CancellationToken::new(), &progress())
    .await
    .unwrap();
  assert_eq!(installed.downloaded_files, 1);
  assert_eq!(installer.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn truncated_file_is_not_installed_and_retry_resumes_or_restarts_cleanly() {
  let root = tempfile::tempdir().unwrap();
  let bytes = payload();
  let fixture = Fixture::new(bytes.clone());
  fixture.set(bytes.clone(), bytes.len() / 2);
  let installer = installer(root.path());
  let plan = plan(fixture.url.clone(), &bytes);

  assert!(
    installer
      .install("Retry", &plan, &CancellationToken::new(), &progress())
      .await
      .is_err()
  );
  assert!(installer.list().await.unwrap().is_empty());
  fixture.set(bytes.clone(), bytes.len());
  installer
    .install("Retry", &plan, &CancellationToken::new(), &progress())
    .await
    .unwrap();
  assert_eq!(
    fs::read(
      root
        .path()
        .join(format!("cache/clients/sha1/{}", sha1_hex(&bytes)))
    )
    .unwrap(),
    bytes
  );
}

#[tokio::test]
async fn simultaneous_installs_share_one_download_per_content_hash() {
  let root = tempfile::tempdir().unwrap();
  let bytes = payload();
  let fixture = Fixture::new(bytes.clone());
  let paths = crate::Paths {
    data: root.path().join("data"),
    cache: root.path().join("cache"),
  };
  let mut context = Context::new(paths).unwrap();
  context.http = q2q_http::Downloader::builder()
    .range_threshold(u64::MAX)
    .retries(0)
    .build()
    .unwrap();
  let installer = ClientInstaller::new(context);
  let plan = plan(fixture.url.clone(), &bytes);
  let left_cancel = CancellationToken::new();
  let right_cancel = CancellationToken::new();
  let left_progress = progress();
  let right_progress = progress();
  let left = installer.install("Left", &plan, &left_cancel, &left_progress);
  let right = installer.install("Right", &plan, &right_cancel, &right_progress);
  let (left, right) = tokio::join!(left, right);
  left.unwrap();
  right.unwrap();

  assert_eq!(fixture.requests.load(Ordering::Relaxed), 1);
  assert_eq!(installer.list().await.unwrap().len(), 2);
}

#[tokio::test]
async fn cancellation_leaves_no_published_instance_or_cache_target() {
  let root = tempfile::tempdir().unwrap();
  let bytes = payload();
  let fixture = Fixture::new(bytes.clone());
  fixture.delay(Duration::from_millis(200));
  let service = installer(root.path());
  let plan = plan(fixture.url.clone(), &bytes);
  let cancel = CancellationToken::new();
  let child = cancel.clone();
  let task = tokio::spawn(async move {
    service
      .install("Cancelled", &plan, &child, &progress())
      .await
  });
  tokio::time::sleep(Duration::from_millis(30)).await;
  cancel.cancel();
  assert!(matches!(task.await.unwrap(), Err(Error::Cancelled)));

  let service = installer(root.path());
  assert!(service.list().await.unwrap().is_empty());
  assert!(!root.path().join("data/instances/client-cancelled").exists());
  assert!(
    !root
      .path()
      .join(format!("cache/clients/sha1/{}", sha1_hex(&bytes)))
      .exists()
  );
}

#[test]
fn rejects_unsafe_install_paths_and_conflicting_hash_sizes() {
  let bytes = payload();
  let url = Url::parse("https://example.invalid/file").unwrap();
  let mut plan = plan(url.clone(), &bytes);
  plan.libraries.push(crate::client_plan::ClientLibrary {
    coordinate: "org.example:demo:1".into(),
    artifact: PlannedFile {
      path: "../escape.jar".into(),
      file: ClientFile {
        url: url.clone(),
        size: bytes.len() as u64,
        sha1: sha1_hex(&bytes),
      },
    },
    kind: crate::client_plan::ClientLibraryKind::Classpath,
  });
  assert!(collect_plan_files(&plan).is_err());

  plan.libraries[0].artifact.path = "libraries/org/example/demo.jar".into();
  plan.libraries[0].artifact.file.size += 1;
  assert!(collect_plan_files(&plan).is_err());
}

fn zip_files(files: &[(&str, &[u8])]) -> Vec<u8> {
  let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
  for (path, contents) in files {
    writer
      .start_file(*path, zip::write::SimpleFileOptions::default())
      .unwrap();
    writer.write_all(contents).unwrap();
  }
  writer.finish().unwrap().into_inner()
}

fn write_plan_cache(
  paths: &crate::Paths,
  plan: &ClientInstallPlan,
  installed: &ClientInstallRecord,
  base: &[u8],
  asset: &[u8],
  native: &[u8],
) {
  for file in &installed.files {
    let content = if file.sha1 == sha1_hex(base) {
      base
    } else if file.sha1 == sha1_hex(asset) {
      asset
    } else if !native.is_empty() && file.sha1 == sha1_hex(native) {
      native
    } else {
      panic!("test data missing for planned cache hash {}", file.sha1);
    };
    let target = paths.cache.join(&file.cache_path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, content).unwrap();
  }
  let _ = plan;
}

#[test]
fn prepares_verified_assets_and_concurrent_session_natives_with_cleanup() {
  let root = tempfile::tempdir().unwrap();
  let paths = crate::Paths {
    data: root.path().join("data"),
    cache: root.path().join("cache"),
  };
  let base = payload();
  let asset = b"small fixture asset";
  let native = zip_files(&[
    ("META-INF/MANIFEST.MF", b"manifest"),
    ("org/example/libfixture.so", b"native library"),
  ]);
  let url = Url::parse("https://example.invalid/payload").unwrap();
  let mut plan = plan(url.clone(), &base);
  plan.assets.push(ClientAssetObject {
    hash: sha1_hex(asset),
    size: asset.len() as u64,
    url: url.clone(),
    path: format!("objects/{}/{}", &sha1_hex(asset)[..2], sha1_hex(asset)),
  });
  plan.native_archives.push(ClientNativeArchive {
    coordinate: "org.example:fixture:1:natives-linux".into(),
    artifact: PlannedFile {
      path: "org/example/fixture/1/fixture-1-natives-linux.jar".into(),
      file: ClientFile {
        url,
        size: native.len() as u64,
        sha1: sha1_hex(&native),
      },
    },
    excludes: vec!["META-INF/".into()],
  });
  let installed = make_install_record(&plan, collect_plan_files(&plan).unwrap().0).unwrap();
  write_plan_cache(&paths, &plan, &installed, &base, asset, &native);
  verify_installed_cache(&paths, &installed, &CancellationToken::new()).unwrap();

  let asset_cache = paths.cache.join("clients/sha1").join(sha1_hex(asset));
  fs::write(&asset_cache, b"corrupt object").unwrap();
  assert!(verify_installed_cache(&paths, &installed, &CancellationToken::new()).is_err());
  fs::write(&asset_cache, asset).unwrap();

  let token = CancellationToken::new();
  let (left, right) = std::thread::scope(|scope| {
    let left =
      scope.spawn(|| prepare_client_files(&paths, "client-main", &installed, &plan, &token));
    let right =
      scope.spawn(|| prepare_client_files(&paths, "client-main", &installed, &plan, &token));
    (
      left.join().unwrap().unwrap(),
      right.join().unwrap().unwrap(),
    )
  });
  assert_ne!(left.session.path(), right.session.path());
  let shared_object = paths.cache.join("assets").join(format!(
    "objects/{}/{}",
    &sha1_hex(asset)[..2],
    sha1_hex(asset)
  ));
  assert_eq!(fs::read(shared_object).unwrap(), asset);
  for prepared in [&left, &right] {
    assert_eq!(
      fs::read(prepared.natives_directory.join("org/example/libfixture.so")).unwrap(),
      b"native library"
    );
    assert!(
      !prepared
        .natives_directory
        .join("META-INF/MANIFEST.MF")
        .exists()
    );
    assert!(prepared.asset_index.is_file());
  }
  let left_session = left.session.path().to_path_buf();
  let right_session = right.session.path().to_path_buf();
  drop(left);
  assert!(!left_session.exists());
  assert!(right_session.exists());
  drop(right);
  assert!(!right_session.exists());
}

#[test]
fn runtime_checks_reject_platform_and_java_mismatches() {
  let bytes = payload();
  let url = Url::parse("https://example.invalid/payload").unwrap();
  let mut plan = plan(url, &bytes);
  let current = ClientTarget::current().unwrap();
  plan.target = ClientTarget::new(
    match current.os {
      ClientOs::Windows => ClientOs::Linux,
      _ => ClientOs::Windows,
    },
    current.architecture,
  );
  assert!(matches!(
    validate_current_target(&plan.target),
    Err(Error::Unsupported(_))
  ));

  plan.target = ClientTarget::new(ClientOs::Linux, ClientArchitecture::X86_64);
  let java = InstalledJava {
    id: "fixture".into(),
    vendor: "temurin".into(),
    version: "21.0.1+12".into(),
    major: 17,
    image_type: "jre".into(),
    os: "linux".into(),
    arch: "aarch64".into(),
    java_home: PathBuf::from("java"),
    executable: PathBuf::from("java/bin/java"),
  };
  assert!(matches!(
    validate_java_for_plan(&java, &plan),
    Err(Error::Java(_))
  ));
}

#[test]
fn native_extraction_exclusions_do_not_bypass_path_safety() {
  let root = tempfile::tempdir().unwrap();
  let archive = root.path().join("unsafe.jar");
  fs::write(
    &archive,
    zip_files(&[("../escaped", b"no"), ("META-INF/MANIFEST.MF", b"skip")]),
  )
  .unwrap();
  let output = root.path().join("natives");
  fs::create_dir(&output).unwrap();
  assert!(
    extract::unpack_excluding(
      &archive,
      &output,
      true,
      &["META-INF/".into()],
      &CancellationToken::new()
    )
    .is_err()
  );
  assert!(!root.path().join("escaped").exists());
}

struct LaunchCredentialStore;

impl crate::account::CredentialStore for LaunchCredentialStore {
  fn load_refresh_token(
    &self,
    _account_id: &str,
  ) -> crate::Result<Option<crate::account::RefreshToken>> {
    Ok(None)
  }

  fn store_refresh_token(
    &self,
    _account_id: &str,
    _token: crate::account::RefreshToken,
  ) -> crate::Result<()> {
    Ok(())
  }

  fn delete_refresh_token(&self, _account_id: &str) -> crate::Result<()> {
    Ok(())
  }
}

struct LaunchAuthProvider;

impl crate::account::AccountProvider for LaunchAuthProvider {
  fn authorize(
    &self,
    _client_id: crate::account::MicrosoftClientId,
    _cancel: CancellationToken,
    _events: tokio::sync::mpsc::UnboundedSender<crate::account::AuthorizationEvent>,
  ) -> crate::account::AuthorizationFuture {
    Box::pin(async {
      crate::account::AuthenticatedAccount::new(
        crate::account::AccountProfile::new(
          "00112233445566778899aabbccddeeff",
          "FixturePlayer",
          Some("2533274799999999".into()),
        )?,
        crate::account::AccessToken::new("access-token-that-must-never-be-logged")?,
        crate::account::RefreshToken::new("fixture-refresh-token")?,
        std::time::SystemTime::now() + Duration::from_secs(3600),
        false,
      )
    })
  }

  fn refresh(
    &self,
    _client_id: crate::account::MicrosoftClientId,
    _account_id: String,
    _refresh_token: crate::account::RefreshToken,
    _cancel: CancellationToken,
    _events: tokio::sync::mpsc::UnboundedSender<crate::account::AuthorizationEvent>,
  ) -> crate::account::AuthorizationFuture {
    Box::pin(async { Err(Error::AccountRequired) })
  }
}

async fn launch_account() -> crate::account::AccountSession {
  crate::account::AccountService::new(
    crate::account::MicrosoftClientId::new("01234567-89ab-cdef-0123-456789abcdef").unwrap(),
    Arc::new(LaunchAuthProvider),
    Arc::new(LaunchCredentialStore),
  )
  .authorize(
    CancellationToken::new(),
    tokio::sync::mpsc::unbounded_channel().0,
  )
  .await
  .unwrap()
}

#[tokio::test]
async fn launch_spec_expands_rules_and_preserves_argument_boundaries() {
  let root = tempfile::tempdir().unwrap();
  let base = payload();
  let url = Url::parse("https://example.invalid/payload").unwrap();
  let mut plan = plan(url, &base);
  let metadata: serde_json::Value =
    serde_json::from_str(include_str!("fixtures/1.21.1.json")).unwrap();
  plan.arguments = serde_json::from_value(metadata["arguments"].clone()).unwrap();
  plan.logging = Some(crate::client_plan::ClientLoggingPlan {
    kind: "log4j2-xml".into(),
    argument: "-Dlog4j.configurationFile=${path}".into(),
    file: PlannedFile {
      path: "log_configs/client.xml".into(),
      file: plan.client_jar.file.clone(),
    },
  });
  let installed = make_install_record(&plan, collect_plan_files(&plan).unwrap().0).unwrap();
  let cache_root = root.path().join("cache with spaces");
  for file in &installed.files {
    let path = cache_root.join(&file.cache_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, &base).unwrap();
  }
  let session = tempfile::tempdir_in(root.path()).unwrap();
  let runtime = ClientRuntimePreparation {
    instance: ClientInstance {
      schema_version: 1,
      instance_type: "client".into(),
      id: "client-launch".into(),
      name: "Launch".into(),
    },
    installed,
    java: InstalledJava {
      id: "temurin-fixture".into(),
      vendor: "temurin".into(),
      version: "21.0.1+12".into(),
      major: 21,
      image_type: "jre".into(),
      os: "linux".into(),
      arch: "x64".into(),
      java_home: root.path().join("java"),
      executable: root.path().join("java/bin/java"),
    },
    cache_root: cache_root.clone(),
    assets_root: root.path().join("assets root"),
    asset_index: root.path().join("assets root/indexes/1.21.1.json"),
    natives_directory: root.path().join("session natives"),
    session_directory: session.path().to_path_buf(),
    _session: session,
    _instance_lock: storage::server_instance_lock(root.path(), "client-launch").unwrap(),
  };
  fs::create_dir_all(&runtime.assets_root).unwrap();
  fs::create_dir_all(&runtime.natives_directory).unwrap();
  let game_directory = root.path().join("game saves");
  fs::create_dir_all(&game_directory).unwrap();
  let account = launch_account().await;
  let options = crate::launch::LaunchOptions {
    launcher_name: "Anvil".into(),
    launcher_version: "0.1.0".into(),
    microsoft_client_id: Some(
      crate::account::MicrosoftClientId::new("01234567-89ab-cdef-0123-456789abcdef").unwrap(),
    ),
    resolution: Some(crate::launch::Resolution::new(1280, 720).unwrap()),
  };
  assert!(matches!(
    crate::launch::expand_template("${clientid}", &std::collections::HashMap::new()),
    Err(Error::Unsupported(message)) if message.contains("clientid")
  ));
  let expected_assets_root = runtime.assets_root.clone();
  let session_directory = runtime.session_directory.clone();
  let expected_client_jar_path = {
    let client_jar = runtime
      .installed
      .files
      .iter()
      .find(|file| file.path == plan.client_jar.path)
      .unwrap();
    cache_root.join(&client_jar.cache_path)
  };
  assert!(matches!(
    storage::server_instance_lock(root.path(), "client-launch"),
    Err(Error::Busy(_))
  ));
  let spec =
    crate::launch::build_launch_spec(&plan, runtime, &account, &game_directory, &options).unwrap();
  let args = spec
    .argv()
    .into_iter()
    .map(|argument| argument.into_string().unwrap())
    .collect::<Vec<_>>();
  let game_directory_arg = game_directory.to_str().unwrap();
  let game_dir_index = args.iter().position(|arg| arg == "--gameDir").unwrap();
  assert_eq!(args[game_dir_index + 1], game_directory_arg);
  let assets_index = args.iter().position(|arg| arg == "--assetsDir").unwrap();
  assert_eq!(
    args[assets_index + 1],
    expected_assets_root.to_str().unwrap()
  );
  let width = args.iter().position(|arg| arg == "--width").unwrap();
  assert_eq!(args[width + 1], "1280");
  let height = args.iter().position(|arg| arg == "--height").unwrap();
  assert_eq!(args[height + 1], "720");
  let classpath = args.iter().position(|arg| arg == "-cp").unwrap();
  assert_eq!(
    args[classpath + 1],
    expected_client_jar_path.to_str().unwrap()
  );
  assert!(
    args
      .iter()
      .any(|arg| arg.starts_with("-Dlog4j.configurationFile="))
  );
  assert!(!args.iter().any(|arg| arg == "WINDOWS_ONLY"));
  assert!(
    args
      .iter()
      .any(|arg| arg == "access-token-that-must-never-be-logged")
  );
  let debug = format!("{spec:?}");
  assert!(!debug.contains("access-token-that-must-never-be-logged"));
  assert!(args.contains(&"net.minecraft.client.main.Main".into()));
  assert!(!args.iter().any(|arg| arg.starts_with("${")));
  assert!(session_directory.exists());
  assert!(matches!(
    storage::server_instance_lock(root.path(), "client-launch"),
    Err(Error::Busy(_))
  ));
  drop(spec);
  assert!(!session_directory.exists());
  assert!(storage::server_instance_lock(root.path(), "client-launch").is_ok());
}
