use super::*;
use serde_json::{Value, json};

fn index() -> Value {
  json!({"formatVersion":1,"game":"minecraft","versionId":"1","name":"Example",
    "dependencies":{"minecraft":"1.21.1","fabric-loader":"0.16.0"},
    "files":[{"path":"mods/example.jar","hashes":{"sha1":"ab".repeat(20),"sha512":"cd".repeat(64)},
      "downloads":["https://cdn.modrinth.com/data/example.jar"],"fileSize":42}]})
}

fn parse(value: Value) -> Result<Plan> {
  parse_index(&serde_json::to_vec(&value).unwrap())
}

#[test]
fn fabric_and_vanilla_plans() {
  let plan = parse(index()).unwrap();
  assert_eq!(plan.fabric_loader.as_deref(), Some("0.16.0"));
  assert_eq!(plan.total_bytes, 42);
  let mut value = index();
  value["dependencies"]
    .as_object_mut()
    .unwrap()
    .remove("fabric-loader");
  assert!(parse(value).unwrap().fabric_loader.is_none());
}

#[test]
fn client_filtering_and_optional_default() {
  for (client, included) in [
    ("required", true),
    ("optional", true),
    ("unsupported", false),
  ] {
    let mut value = index();
    value["files"][0]["env"] = json!({"client":client,"server":"required"});
    let plan = parse(value).unwrap();
    assert_eq!(plan.files.len(), usize::from(included));
    assert_eq!(plan.skipped_files, usize::from(!included));
    assert_eq!(plan.total_bytes, if included { 42 } else { 0 });
  }
}

#[test]
fn rejects_traversal_and_nonportable_paths() {
  for path in [
    "../x",
    "/x",
    "C:/x",
    "mods\\x",
    "a//b",
    "a/./b",
    "a/NUL.txt",
    "mods/COM1.jar",
    "a/b.",
    "a/b ",
    "a/b:c",
  ] {
    let mut value = index();
    value["files"][0]["path"] = json!(path);
    assert!(parse(value).is_err(), "{path}");
  }
}

#[test]
fn rejects_collisions_including_skipped_files() {
  for path in ["MODS/EXAMPLE.JAR", "mods", "mods/example.jar/child"] {
    let mut value = index();
    let mut duplicate = value["files"][0].clone();
    duplicate["path"] = json!(path);
    duplicate["env"] = json!({"client":"unsupported","server":"required"});
    value["files"].as_array_mut().unwrap().push(duplicate);
    assert!(parse(value).is_err());
  }
}

#[test]
fn rejects_untrusted_downloads_hashes_and_unsupported_loaders() {
  for url in [
    "http://cdn.modrinth.com/file",
    "https://127.0.0.1/file",
    "https://github.com.evil.test/file",
    "https://user@github.com/file",
  ] {
    let mut value = index();
    value["files"][0]["downloads"] = json!([url]);
    assert!(parse(value).is_err());
  }
  let mut value = index();
  value["files"][0]["hashes"]["sha512"] = json!("bad");
  assert!(parse(value).is_err());
  let mut value = index();
  value["dependencies"]["forge"] = json!("1.0");
  assert!(parse(value).is_err());
}

#[test]
fn rejects_unknown_environment_and_oversized_inputs() {
  let mut value = index();
  value["files"][0]["env"] = json!({"client":"sometimes","server":"required"});
  assert!(parse(value).is_err());
  let mut value = index();
  value["files"][0]["fileSize"] = json!(MAX_FILE_BYTES + 1);
  assert!(parse(value).is_err());
  assert!(parse_index(&vec![b' '; MAX_INDEX_BYTES + 1]).is_err());
}

pub(super) fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
  use std::io::{Cursor, Write};
  let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
  for (name, bytes) in entries {
    zip
      .start_file(
        *name,
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
      )
      .unwrap();
    zip.write_all(bytes).unwrap();
  }
  zip.finish().unwrap().into_inner()
}

fn inspect(bytes: &[u8]) -> Result<ArchivePlan> {
  inspect_archive(
    std::io::Cursor::new(bytes),
    &crate::CancellationToken::new(),
  )
}

#[test]
fn archive_client_override_wins_regardless_of_zip_order() {
  let index = serde_json::to_vec(&index()).unwrap();
  let bytes = archive(&[
    ("client-overrides/config/example.json", b"client"),
    ("modrinth.index.json", &index),
    ("overrides/config/example.json", b"common"),
    ("server-overrides/config/server.json", b"server"),
  ]);
  let plan = inspect(&bytes).unwrap();
  assert_eq!(plan.overrides.len(), 1);
  assert_eq!(plan.overrides[0].entry_index, 0);
  assert_eq!(plan.overrides[0].path, "config/example.json");
  assert_eq!(plan.overrides[0].size, 6);
  assert_eq!(plan.archive_bytes, bytes.len() as u64);
  assert_eq!(plan.archive_sha256.len(), 64);
}

#[test]
fn archive_rejects_missing_index_and_unsafe_ignored_paths() {
  assert!(inspect(&archive(&[("overrides/config/a", b"x")])).is_err());
  let index = serde_json::to_vec(&index()).unwrap();
  for path in [
    "../escape",
    "server-overrides/../escape",
    "/absolute",
    "overrides/C:/escape",
  ] {
    assert!(inspect(&archive(&[("modrinth.index.json", &index), (path, b"x")])).is_err());
  }
}

#[test]
fn archive_rejects_layer_and_download_tree_conflicts() {
  let index = serde_json::to_vec(&index()).unwrap();
  for (first, second) in [
    ("overrides/config/a", "client-overrides/config/a/b"),
    ("overrides/config/a", "client-overrides/Config/a"),
    ("overrides/config/a", "overrides/Config/b"),
    ("overrides/mods", "client-overrides/config/a"),
    ("overrides/mods/example.jar/a", "client-overrides/config/a"),
  ] {
    assert!(
      inspect(&archive(&[
        ("modrinth.index.json", &index),
        (first, b"a"),
        (second, b"b")
      ]))
      .is_err(),
      "{first}: {second}"
    );
  }
}

#[test]
fn archive_rejects_symlinks_and_corrupt_payloads() {
  use std::io::{Cursor, Write};
  let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
  writer
    .start_file(
      "modrinth.index.json",
      zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
  writer
    .write_all(&serde_json::to_vec(&index()).unwrap())
    .unwrap();
  writer
    .add_symlink(
      "overrides/link",
      "../../escape",
      zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
  assert!(inspect(&writer.finish().unwrap().into_inner()).is_err());
  let index = serde_json::to_vec(&index()).unwrap();
  let mut bytes = archive(&[
    ("modrinth.index.json", &index),
    ("overrides/config/a", b"unique-payload"),
  ]);
  let position = bytes
    .windows(14)
    .position(|chunk| chunk == b"unique-payload")
    .unwrap();
  bytes[position] ^= 1;
  assert!(inspect(&bytes).is_err());
}

#[test]
fn archive_cancellation_and_directory_allocation_limit() {
  let bytes = archive(&[(
    "modrinth.index.json",
    &serde_json::to_vec(&index()).unwrap(),
  )]);
  let cancel = crate::CancellationToken::new();
  cancel.cancel();
  assert!(matches!(
    inspect_archive(std::io::Cursor::new(&bytes), &cancel),
    Err(Error::Cancelled)
  ));
  let mut malformed = bytes;
  let offset = malformed
    .windows(4)
    .rposition(|chunk| chunk == b"PK\x05\x06")
    .unwrap();
  malformed[offset + 8..offset + 10].copy_from_slice(&20_001u16.to_le_bytes());
  malformed[offset + 10..offset + 12].copy_from_slice(&20_001u16.to_le_bytes());
  assert!(inspect(&malformed).is_err());
}
