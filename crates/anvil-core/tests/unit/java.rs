use super::*;

#[tokio::test]
async fn cancelled_install_never_queries_network_or_creates_directories() {
  let root = tempfile::tempdir().unwrap();
  let manager = JavaManager::new(
    Context::new(crate::Paths::discover(Some(root.path().into())).unwrap()).unwrap(),
  );
  let token = CancellationToken::new();
  token.cancel();
  let result = manager
    .install("21", &token, watch::channel(JavaProgress::default()).0)
    .await;
  assert!(matches!(result, Err(Error::Cancelled)));
  assert!(!root.path().join("locks").exists());
}

#[test]
fn manifest_rejects_unsupported_schema_and_home_traversal() {
  let root = tempfile::tempdir().unwrap();
  let mut m = Manifest {
    schema_version: 1,
    id: "temurin-test".into(),
    vendor: "temurin".into(),
    version: "21.0.8+9".into(),
    major: 21,
    image_type: "jre".into(),
    os: "linux".into(),
    arch: "x64".into(),
    archive_sha256: "0".repeat(64),
    home: "../outside".into(),
  };
  assert!(m.installed(root.path()).is_err());
  m.home = "jdk".into();
  m.schema_version = 2;
  assert!(m.installed(root.path()).is_err());
}

#[cfg(unix)]
mod pipeline {
  use super::*;
  use sha2::{Digest, Sha256};
  use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
      Arc, Mutex,
      atomic::{AtomicBool, Ordering},
    },
    thread,
  };

  struct Server {
    url: String,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    ranges: Arc<Mutex<Vec<u64>>>,
  }
  impl Server {
    fn new(data: Vec<u8>) -> Self {
      let listener = TcpListener::bind("127.0.0.1:0").unwrap();
      listener.set_nonblocking(true).unwrap();
      let url = format!("http://{}/java.tar.gz", listener.local_addr().unwrap());
      let stop = Arc::new(AtomicBool::new(false));
      let ranges = Arc::new(Mutex::new(Vec::new()));
      let thread_stop = stop.clone();
      let thread_ranges = ranges.clone();
      let handle = thread::spawn(move || {
        while !thread_stop.load(Ordering::Relaxed) {
          match listener.accept() {
            Ok((mut socket, _)) => {
              let _ = serve(&mut socket, &data, &thread_ranges);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
              thread::sleep(Duration::from_millis(1))
            }
            Err(e) => panic!("{e}"),
          }
        }
      });
      Self {
        url,
        stop,
        thread: Some(handle),
        ranges,
      }
    }
  }
  impl Drop for Server {
    fn drop(&mut self) {
      self.stop.store(true, Ordering::Relaxed);
      self.thread.take().unwrap().join().unwrap();
    }
  }
  fn serve(socket: &mut TcpStream, data: &[u8], ranges: &Mutex<Vec<u64>>) -> std::io::Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(15)))?;
    socket.set_write_timeout(Some(Duration::from_secs(15)))?;
    let mut request = Vec::new();
    let mut byte = [0u8; 1];
    while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
      if socket.read(&mut byte)? == 0 {
        return Ok(());
      }
      request.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&request).to_ascii_lowercase();
    if let Some(range) = text.lines().find_map(|l| l.strip_prefix("range: bytes=")) {
      let (start, end) = range.split_once('-').unwrap();
      let start: usize = start.parse().unwrap();
      let end: usize = end.parse().unwrap();
      ranges.lock().unwrap().push(start as u64);
      thread::sleep(Duration::from_millis(15));
      write!(
        socket,
        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{}\r\nContent-Length: {}\r\nETag: \"fixture-v1\"\r\nConnection: close\r\n\r\n",
        data.len(),
        end - start + 1
      )?;
      socket.write_all(&data[start..=end])?;
    } else {
      write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"fixture-v1\"\r\nConnection: close\r\n\r\n",
        data.len()
      )?;
      socket.write_all(data)?;
    }
    Ok(())
  }
  fn archive(valid_java: bool) -> Vec<u8> {
    let script = if valid_java {
      format!(
        "#!/bin/sh\nprintf '    java.runtime.version = 21.0.8+9-LTS\\n    os.arch = {}\\n' >&2\n",
        std::env::consts::ARCH
      )
    } else {
      "#!/bin/sh\nexit 1\n".into()
    };
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut tar = tar::Builder::new(encoder);
    for (name, bytes, mode) in [
      ("jdk/bin/java", script.into_bytes(), 0o755),
      ("jdk/lib/data", noise(), 0o644),
    ] {
      let mut h = tar::Header::new_gnu();
      h.set_size(bytes.len() as u64);
      h.set_mode(mode);
      h.set_cksum();
      tar.append_data(&mut h, name, bytes.as_slice()).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap()
  }
  fn noise() -> Vec<u8> {
    let mut state = 0x12345678u32;
    (0..1024 * 1024)
      .map(|_| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state as u8
      })
      .collect()
  }
  fn manager(root: &Path) -> JavaManager {
    let mut context = Context::new(crate::Paths::discover(Some(root.into())).unwrap()).unwrap();
    context.http = q2q_http::Downloader::builder()
      .range_threshold(1)
      .chunk_size(65536)
      .max_ranges_per_file(1)
      .retries(0)
      .build()
      .unwrap();
    JavaManager::new(context)
  }
  fn resolved(server: &Server, data: &[u8]) -> temurin::Resolved {
    temurin::Resolved {
      version: "21.0.8+9".into(),
      major: 21,
      image: "jre".into(),
      checksum: Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect(),
      url: server.url.clone(),
      size: data.len() as u64,
      zip: false,
    }
  }
  async fn install(manager: &JavaManager, server: &Server, data: &[u8]) -> Result<InstallResult> {
    manager
      .install_resolved(
        resolved(server, data),
        Platform::current()?,
        &CancellationToken::new(),
        watch::channel(JavaProgress::default()).0,
      )
      .await
  }
  #[tokio::test]
  async fn fresh_repeat_list_busy_and_uninstall_share_the_same_runtime() {
    let root = tempfile::tempdir().unwrap();
    let manager = manager(root.path());
    let data = archive(true);
    let server = Server::new(data.clone());
    let result = install(&manager, &server, &data).await.unwrap();
    assert!(!result.already_installed);
    assert!(result.runtime.executable.is_file());
    let requests = server.ranges.lock().unwrap().len();
    assert!(
      install(&manager, &server, &data)
        .await
        .unwrap()
        .already_installed
    );
    assert_eq!(server.ranges.lock().unwrap().len(), requests);
    assert_eq!(manager.list().await.unwrap().len(), 1);
    let lock = storage::lock(root.path(), &result.runtime.id).unwrap();
    assert!(matches!(
      install(&manager, &server, &data).await,
      Err(Error::Busy(_))
    ));
    assert!(matches!(
      manager.uninstall(&result.runtime.id).await,
      Err(Error::Busy(_))
    ));
    drop(lock);
    // Damaged executables must still be uninstallable.
    fs::remove_file(&result.runtime.executable).unwrap();
    manager.uninstall(&result.runtime.id).await.unwrap();
    assert!(manager.list().await.unwrap().is_empty());
  }
  #[tokio::test]
  async fn cancellation_resumes_without_fetching_committed_first_range() {
    let root = tempfile::tempdir().unwrap();
    let manager = manager(root.path());
    let data = archive(true);
    let server = Server::new(data.clone());
    let cancel = CancellationToken::new();
    let (sender, mut updates) = watch::channel(JavaProgress::default());
    let future = manager.install_resolved(
      resolved(&server, &data),
      Platform::current().unwrap(),
      &cancel,
      sender,
    );
    tokio::pin!(future);
    loop {
      tokio::select! {
        result = &mut future => { assert_eq!(result.err().unwrap().code(), "cancelled"); break; }
        result = updates.changed() => { result.unwrap(); if updates.borrow().downloaded_bytes >= 65536 { cancel.cancel(); } }
      }
    }
    assert!(manager.list().await.unwrap().is_empty());
    server.ranges.lock().unwrap().clear();
    install(&manager, &server, &data).await.unwrap();
    let ranges = server.ranges.lock().unwrap();
    assert!(!ranges.is_empty());
    assert!(
      !ranges.contains(&0),
      "committed first range was downloaded again: {ranges:?}"
    );
  }
  #[tokio::test]
  async fn failed_probe_leaves_no_published_runtime_or_staging() {
    let root = tempfile::tempdir().unwrap();
    let manager = manager(root.path());
    let data = archive(false);
    let server = Server::new(data.clone());
    assert!(matches!(
      install(&manager, &server, &data).await,
      Err(Error::Java(_))
    ));
    assert!(manager.list().await.unwrap().is_empty());
    assert_eq!(
      fs::read_dir(root.path().join("runtimes")).unwrap().count(),
      0
    );
  }
  #[tokio::test]
  async fn checksum_mismatch_never_publishes_a_runtime() {
    let root = tempfile::tempdir().unwrap();
    let manager = manager(root.path());
    let data = archive(true);
    let server = Server::new(data.clone());
    let mut spec = resolved(&server, &data);
    spec.checksum = "0".repeat(64);
    let result = manager
      .install_resolved(
        spec,
        Platform::current().unwrap(),
        &CancellationToken::new(),
        watch::channel(JavaProgress::default()).0,
      )
      .await;
    assert!(matches!(
      result,
      Err(Error::Http(q2q_http::Error::Integrity))
    ));
    assert!(manager.list().await.unwrap().is_empty());
  }
  #[tokio::test]
  async fn damaged_archive_fails_before_java_is_published() {
    let root = tempfile::tempdir().unwrap();
    let manager = manager(root.path());
    let data = b"not an archive".to_vec();
    let server = Server::new(data.clone());
    assert!(install(&manager, &server, &data).await.is_err());
    assert!(manager.list().await.unwrap().is_empty());
  }
}
