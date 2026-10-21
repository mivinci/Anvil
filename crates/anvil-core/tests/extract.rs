use super::*;
use std::io::Write;
fn tar_file(path: &Path, name: &str, data: &[u8]) {
  let encoder =
    flate2::write::GzEncoder::new(File::create(path).unwrap(), flate2::Compression::fast());
  let mut tar = tar::Builder::new(encoder);
  let mut header = tar::Header::new_gnu();
  header.set_size(data.len() as u64);
  header.set_mode(0o755);
  header.set_cksum();
  tar.append_data(&mut header, name, data).unwrap();
  tar.into_inner().unwrap().finish().unwrap();
}
#[test]
fn rejects_nonportable_and_traversing_paths() {
  for name in [
    "../outside",
    "/absolute",
    "C:/java",
    "a\\..\\outside",
    "NUL",
    "dir/CON.txt",
    "bad.",
  ] {
    assert!(relative(Path::new(name)).is_err(), "{name}");
  }
}
#[test]
fn tar_roundtrip_preserves_executable_and_cancel_does_not_extract() {
  let root = tempfile::tempdir().unwrap();
  let archive = root.path().join("java.tar.gz");
  tar_file(&archive, "jdk/bin/java", b"java");
  let out = root.path().join("out");
  fs::create_dir(&out).unwrap();
  unpack(&archive, &out, false, &CancellationToken::new()).unwrap();
  assert_eq!(fs::read(out.join("jdk/bin/java")).unwrap(), b"java");
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    assert_ne!(
      fs::metadata(out.join("jdk/bin/java"))
        .unwrap()
        .permissions()
        .mode()
        & 0o111,
      0
    );
  }
  let token = CancellationToken::new();
  token.cancel();
  let out = root.path().join("cancelled");
  fs::create_dir(&out).unwrap();
  assert!(matches!(
    unpack(&archive, &out, false, &token),
    Err(Error::Cancelled)
  ));
  assert_eq!(fs::read_dir(out).unwrap().count(), 0);
}
#[test]
fn zip_roundtrip_and_zip_traversal() {
  let root = tempfile::tempdir().unwrap();
  for name in ["jdk/bin/java.exe", "../escaped"] {
    let archive = root.path().join("java.zip");
    let mut writer = zip::ZipWriter::new(File::create(&archive).unwrap());
    writer
      .start_file(name, zip::write::SimpleFileOptions::default())
      .unwrap();
    writer.write_all(b"java").unwrap();
    writer.finish().unwrap();
    let out = tempfile::tempdir_in(root.path()).unwrap();
    let result = unpack(&archive, out.path(), true, &CancellationToken::new());
    if name.starts_with("..") {
      assert!(result.is_err());
      assert!(!root.path().join("escaped").exists());
    } else {
      result.unwrap();
      assert_eq!(fs::read(out.path().join(name)).unwrap(), b"java");
    }
  }
}
#[test]
fn malformed_archive_and_expansion_limit_are_rejected() {
  let root = tempfile::tempdir().unwrap();
  let archive = root.path().join("bad.tar.gz");
  fs::write(&archive, b"not gzip").unwrap();
  assert!(unpack(&archive, root.path(), false, &CancellationToken::new()).is_err());
  let token = CancellationToken::new();
  let mut extractor = Extractor {
    root: root.path(),
    cancel: &token,
    paths: HashSet::new(),
    bytes: 0,
    links: Vec::new(),
  };
  assert!(
    extractor
      .register(Path::new("huge"), MAX_BYTES + 1)
      .is_err()
  );
}
#[test]
fn duplicate_entries_and_cyclic_links_are_rejected() {
  let root = tempfile::tempdir().unwrap();
  let token = CancellationToken::new();
  let mut extractor = Extractor {
    root: root.path(),
    cancel: &token,
    paths: HashSet::new(),
    bytes: 0,
    links: Vec::new(),
  };
  extractor.register(Path::new("same"), 0).unwrap();
  assert!(extractor.register(Path::new("same"), 0).is_err());
  extractor.links = vec![("a".into(), "b".into()), ("b".into(), "a".into())];
  assert!(extractor.links().is_err());
}
#[test]
fn tar_link_escape_is_rejected_and_internal_file_link_is_copied() {
  let root = tempfile::tempdir().unwrap();
  for target in ["java", "../../../outside"] {
    let archive = root.path().join("links.tar.gz");
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
      File::create(&archive).unwrap(),
      flate2::Compression::fast(),
    ));
    let mut h = tar::Header::new_gnu();
    h.set_size(4);
    h.set_mode(0o755);
    h.set_cksum();
    tar
      .append_data(&mut h, "jdk/bin/java", &b"java"[..])
      .unwrap();
    let mut h = tar::Header::new_gnu();
    h.set_size(0);
    h.set_mode(0o777);
    h.set_entry_type(tar::EntryType::Symlink);
    h.set_link_name(target).unwrap();
    h.set_cksum();
    tar
      .append_data(&mut h, "jdk/bin/link", io::empty())
      .unwrap();
    tar.into_inner().unwrap().finish().unwrap();
    let out = tempfile::tempdir_in(root.path()).unwrap();
    let result = unpack(&archive, out.path(), false, &CancellationToken::new());
    if target == "java" {
      result.unwrap();
      assert!(
        fs::symlink_metadata(out.path().join("jdk/bin/link"))
          .unwrap()
          .is_file()
      );
    } else {
      assert!(result.is_err());
    }
  }
}
