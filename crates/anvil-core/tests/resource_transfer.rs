use super::*;

#[test]
fn canonical_download_destination_is_accepted() {
  let root = tempfile::tempdir().unwrap();
  let target = root.path().join("resource.jar");
  fs::write(&target, b"download fixture").unwrap();
  let returned = fs::canonicalize(root.path()).unwrap().join("resource.jar");
  #[cfg(windows)]
  assert_ne!(
    returned, target,
    "exercise Windows verbatim versus ordinary paths"
  );
  assert!(same_download_location(&returned, &target).unwrap());
}

#[test]
fn unexpected_or_missing_download_destination_is_refused() {
  let root = tempfile::tempdir().unwrap();
  let target = root.path().join("expected.jar");
  let other = root.path().join("other.jar");
  fs::write(&target, b"same bytes").unwrap();
  fs::write(&other, b"same bytes").unwrap();
  assert!(!same_download_location(&other, &target).unwrap());
  assert!(!same_download_location(root.path(), &target).unwrap());
  assert!(!same_download_location(&root.path().join("missing"), &target).unwrap());
}

#[cfg(unix)]
#[test]
fn linked_download_destination_is_refused() {
  let root = tempfile::tempdir().unwrap();
  let target = root.path().join("expected.jar");
  let link = root.path().join("link.jar");
  fs::write(&target, b"fixture").unwrap();
  std::os::unix::fs::symlink(&target, &link).unwrap();
  assert!(!same_download_location(&link, &target).unwrap());
}

use crate::Paths;
use std::{
  io::{Cursor, Write},
  sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
  },
  time::Duration,
};
use tokio::{
  io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
  net::TcpListener,
  task::JoinSet,
  time::{sleep, timeout},
};

const CHUNK: usize = 64 * 1024;

async fn wait_until(mut ready: impl FnMut() -> bool) {
  timeout(Duration::from_secs(5), async {
    while !ready() {
      sleep(Duration::from_millis(5)).await;
    }
  })
  .await
  .expect("fixture condition timed out");
}

#[tokio::test]
async fn interrupted_resource_transfer_drains_and_resumes_from_verified_range() {
  shared_transfer_fixture(false).await;
}

#[tokio::test]
async fn cancelling_first_consumer_leaves_shared_download_running() {
  shared_transfer_fixture(true).await;
}

async fn shared_transfer_fixture(complete_second: bool) {
  let root = tempfile::tempdir().unwrap();
  let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
  archive
    .start_file(
      "fixture.bin",
      zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
  archive.write_all(&vec![42; 3 * CHUNK]).unwrap();
  let body = Arc::new(archive.finish().unwrap().into_inner());

  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let url = format!("http://{}/resource", listener.local_addr().unwrap());
  let ranges = Arc::new(Mutex::new(Vec::new()));
  let sent = Arc::new(AtomicUsize::new(0));
  let waiting = Arc::new(AtomicUsize::new(0));
  let disconnected = Arc::new(AtomicUsize::new(0));
  let (release, release_rx) = watch::channel(false);
  let mut server = JoinSet::new();
  server.spawn({
    let (body, ranges, sent, waiting, disconnected) = (
      body.clone(),
      ranges.clone(),
      sent.clone(),
      waiting.clone(),
      disconnected.clone(),
    );
    async move {
      let mut connections = JoinSet::new();
      loop {
        tokio::select! {
          accepted = listener.accept() => {
            let (socket, _) = accepted.unwrap();
            let (body, ranges, sent, waiting, disconnected, mut release) = (
              body.clone(), ranges.clone(), sent.clone(), waiting.clone(),
              disconnected.clone(), release_rx.clone(),
            );
            connections.spawn(async move {
              let mut socket = BufReader::new(socket);
              let mut line = String::new();
              let mut range = None;
              let mut if_range = None;
              loop {
                line.clear();
                if socket.read_line(&mut line).await.unwrap() == 0 { return; }
                if line == "\r\n" { break; }
                if let Some((key, value)) = line.split_once(':') {
                  if key.eq_ignore_ascii_case("range") { range = Some(value.trim().to_owned()); }
                  if key.eq_ignore_ascii_case("if-range") { if_range = Some(value.trim().to_owned()); }
                }
              }
              let range = range.expect("range request");
              let (start, end) = range.strip_prefix("bytes=").unwrap().split_once('-').unwrap();
              let (start, end) = (start.parse::<usize>().unwrap(), end.parse::<usize>().unwrap());
              ranges.lock().unwrap().push((start, if_range));
              let bytes = &body[start..=end];
              let header = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{}\r\nContent-Length: {}\r\nETag: \"fixture\"\r\nConnection: close\r\n\r\n",
                body.len(), bytes.len(),
              );
              if socket.get_mut().write_all(header.as_bytes()).await.is_err() { return; }
              let first = if start == 0 || *release.borrow() { bytes.len() } else { bytes.len().min(512) };
              if socket.get_mut().write_all(&bytes[..first]).await.is_err() { return; }
              sent.fetch_add(first, Ordering::SeqCst);
              if first < bytes.len() {
                waiting.fetch_add(1, Ordering::SeqCst);
                let mut extra = [0; 1];
                tokio::select! {
                  _ = socket.read(&mut extra) => {
                    disconnected.fetch_add(1, Ordering::SeqCst);
                    return;
                  }
                  result = release.wait_for(|ready| *ready) => { result.unwrap(); }
                }
                socket.get_mut().write_all(&bytes[first..]).await.unwrap();
                sent.fetch_add(bytes.len() - first, Ordering::SeqCst);
              }
            });
          }
          Some(result) = connections.join_next(), if !connections.is_empty() => { result.unwrap(); }
        }
      }
    }
  });

  let mut context = Context::new(Paths {
    data: root.path().join("data"),
    cache: root.path().join("cache"),
  })
  .unwrap();
  context.http = q2q_http::Downloader::builder()
    .range_threshold(1)
    .chunk_size(CHUNK as u64)
    .build()
    .unwrap();
  let manager = Arc::new(ResourceManager::new(context));
  let file = DownloadFile {
    filename: "fixture.jar".into(),
    size: body.len() as u64,
    sha1: hex(&sha1::Sha1::digest(body.as_ref())),
    url,
    extension: "jar",
  };
  assert!(
    file.validate().is_err(),
    "the production HTTPS gate stays in force"
  );
  let initial = ResourceInstallProgress {
    stage: ResourceInstallStage::Preparing,
    completed_files: 0,
    total_files: 1,
    downloaded_bytes: 0,
    total_bytes: file.size,
    received_bytes: 0,
  };
  let (progress, _) = watch::channel(initial.clone());
  let cancel = CancellationToken::new();
  let mut transfer = tokio::spawn({
    let (manager, file, cancel, progress) = (
      manager.clone(),
      file.clone(),
      cancel.clone(),
      progress.clone(),
    );
    async move {
      manager
        .prepare_validated_shared(&file, &cancel, &progress)
        .await
    }
  });
  tokio::select! {
    result = &mut transfer => panic!("transfer ended before pause: {result:?}"),
    _ = wait_until(|| waiting.load(Ordering::SeqCst) > 0) => {}
  }
  let second_cancel = CancellationToken::new();
  let (second_progress, _) = watch::channel(ResourceInstallProgress {
    received_bytes: 5,
    ..initial.clone()
  });
  let second = tokio::spawn({
    let manager = ResourceManager::new(manager.context.clone());
    let (file, token, progress) = (file.clone(), second_cancel.clone(), second_progress.clone());
    async move {
      manager
        .prepare_validated_shared(&file, &token, &progress)
        .await
    }
  });
  wait_until(|| second_progress.borrow().downloaded_bytes >= CHUNK as u64).await;
  cancel.cancel();
  let result = timeout(Duration::from_secs(5), transfer)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(result.unwrap_err().code(), "cancelled");
  let first_progress = progress.borrow().received_bytes;
  assert_eq!(disconnected.load(Ordering::SeqCst), 0);
  assert!(!second.is_finished());
  if complete_second {
    release.send_replace(true);
    let hash = timeout(Duration::from_secs(5), second)
      .await
      .unwrap()
      .unwrap()
      .unwrap();
    let saved = fs::read(object_path(&manager.context.paths.data, &hash)).unwrap();
    assert_eq!(saved, *body);
    assert_eq!(progress.borrow().received_bytes, first_progress);
    assert_eq!(second_progress.borrow().received_bytes, file.size + 5);
    let mut starts: Vec<_> = ranges
      .lock()
      .unwrap()
      .iter()
      .map(|(start, _)| *start)
      .collect();
    starts.sort_unstable();
    assert_eq!(starts, (0..body.len()).step_by(CHUNK).collect::<Vec<_>>());
    return;
  }
  second_cancel.cancel();
  let second_result = timeout(Duration::from_secs(5), second)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(second_result.unwrap_err().code(), "cancelled");
  wait_until(|| disconnected.load(Ordering::SeqCst) == waiting.load(Ordering::SeqCst)).await;
  let paused_bytes = sent.load(Ordering::SeqCst);
  let paused_progress = progress.borrow().received_bytes;
  sleep(Duration::from_millis(100)).await;
  assert_eq!(sent.load(Ordering::SeqCst), paused_bytes);
  assert_eq!(progress.borrow().received_bytes, paused_progress);
  let requests_before_resume = ranges.lock().unwrap().len();

  release.send_replace(true);
  progress.send_replace(initial);
  let hash = timeout(
    Duration::from_secs(5),
    manager.prepare_validated_shared(&file, &CancellationToken::new(), &progress),
  )
  .await
  .unwrap()
  .unwrap();
  let ranges = ranges.lock().unwrap();
  assert!(ranges.len() > requests_before_resume);
  assert_eq!(
    ranges[requests_before_resume],
    (CHUNK, Some("\"fixture\"".into()))
  );
  assert!(
    ranges[requests_before_resume..]
      .iter()
      .all(|(start, _)| *start >= CHUNK)
  );
  let saved = fs::read(object_path(&manager.context.paths.data, &hash)).unwrap();
  assert_eq!(saved, *body);
}
