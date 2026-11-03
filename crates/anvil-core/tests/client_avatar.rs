use super::*;
use crate::{Context, Paths, client::ClientInstance};

fn fixture() -> (tempfile::TempDir, ClientInstaller, PathBuf) {
  let root = tempfile::tempdir().unwrap();
  let context = Context::new(Paths {
    data: root.path().join("data"),
    cache: root.path().join("cache"),
  })
  .unwrap();
  let target = context.paths.data.join("instances/client-test");
  fs::create_dir_all(target.join("game")).unwrap();
  fs::write(target.join("game/keep"), b"world").unwrap();
  storage::write_json(
    &target.join("instance.json"),
    &ClientInstance {
      schema_version: 1,
      instance_type: "client".into(),
      id: "client-test".into(),
      name: "Test".into(),
    },
  )
  .unwrap();
  (root, ClientInstaller::new(context), target)
}
fn picture(format: ImageFormat, color: u8) -> Vec<u8> {
  let pixels = image::RgbImage::from_pixel(16, 8, image::Rgb([color, 20, 40]));
  let mut output = Cursor::new(Vec::new());
  DynamicImage::ImageRgb8(pixels)
    .write_to(&mut output, format)
    .unwrap();
  output.into_inner()
}
fn crc(bytes: &[u8]) -> u32 {
  let mut value = !0u32;
  for byte in bytes {
    value ^= u32::from(*byte);
    for _ in 0..8 {
      value = if value & 1 != 0 {
        (value >> 1) ^ 0xedb88320
      } else {
        value >> 1
      };
    }
  }
  !value
}
fn chunk(kind: &[u8; 4], bytes: &[u8]) -> Vec<u8> {
  let mut result = (bytes.len() as u32).to_be_bytes().to_vec();
  result.extend_from_slice(kind);
  result.extend_from_slice(bytes);
  result.extend_from_slice(&crc(&result[4..]).to_be_bytes());
  result
}
#[test]
fn normalizes_all_supported_formats_and_strips_metadata() {
  for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
    let avatar = normalize(&picture(format, 180)).unwrap();
    let decoded = decode_static(&avatar.png).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (256, 256));
    assert_eq!(avatar.revision.len(), 64);
    assert!(avatar.png.len() < MAX_STORED_BYTES);
    assert_eq!(normalize(&avatar.png).unwrap().revision, avatar.revision);
  }
}
#[test]
fn rejects_empty_wrong_format_truncated_and_oversized_images() {
  for bytes in [
    vec![],
    b"GIF89a".to_vec(),
    b"<svg/>".to_vec(),
    picture(ImageFormat::Png, 3)[..40].to_vec(),
    vec![0; MAX_AVATAR_INPUT_BYTES + 1],
  ] {
    assert!(normalize(&bytes).is_err());
  }
}
#[test]
fn rejects_pixel_bombs_and_animated_png_before_pixel_decode() {
  let mut bomb = picture(ImageFormat::Png, 100);
  bomb[16..20].copy_from_slice(&4001u32.to_be_bytes());
  bomb[20..24].copy_from_slice(&4000u32.to_be_bytes());
  let checksum = crc(&bomb[12..29]);
  bomb[29..33].copy_from_slice(&checksum.to_be_bytes());
  assert!(
    normalize(&bomb)
      .unwrap_err()
      .to_string()
      .contains("pixel budget")
  );
  let mut apng = picture(ImageFormat::Png, 100);
  apng.splice(33..33, chunk(b"acTL", &[0, 0, 0, 1, 0, 0, 0, 0]));
  assert!(
    normalize(&apng)
      .unwrap_err()
      .to_string()
      .contains("animated PNG")
  );
}
#[tokio::test]
async fn saves_replaces_reads_and_clears_only_avatar() {
  let (_root, installer, target) = fixture();
  assert!(installer.avatar("test").await.unwrap().is_none());
  let first = installer
    .save_avatar("test", picture(ImageFormat::Jpeg, 20))
    .await
    .unwrap();
  assert_eq!(
    installer.avatar("test").await.unwrap().unwrap().revision,
    first.revision
  );
  let second = installer
    .save_avatar("test", picture(ImageFormat::WebP, 200))
    .await
    .unwrap();
  assert_ne!(first.revision, second.revision);
  assert_eq!(fs::read(target.join("avatar.png")).unwrap(), second.png);
  installer.clear_avatar("test").await.unwrap();
  installer.clear_avatar("test").await.unwrap();
  assert!(installer.avatar("test").await.unwrap().is_none());
  assert_eq!(fs::read(target.join("game/keep")).unwrap(), b"world");
  assert!(target.join("instance.json").is_file());
}
#[tokio::test]
async fn rejected_replacement_preserves_old_image_and_has_no_temporary_files() {
  let (_root, installer, target) = fixture();
  let first = installer
    .save_avatar("test", picture(ImageFormat::Png, 20))
    .await
    .unwrap();
  assert!(
    installer
      .save_avatar("test", b"broken".to_vec())
      .await
      .is_err()
  );
  assert_eq!(fs::read(target.join("avatar.png")).unwrap(), first.png);
  assert_eq!(fs::read_dir(&target).unwrap().count(), 3);
}
#[tokio::test]
async fn corrupt_or_wrong_size_stored_images_fall_back_and_can_be_replaced() {
  let (_root, installer, target) = fixture();
  for bytes in [
    b"broken".to_vec(),
    picture(ImageFormat::Png, 10),
    vec![0; MAX_STORED_BYTES + 1],
  ] {
    fs::write(target.join("avatar.png"), bytes).unwrap();
    assert!(installer.avatar("test").await.unwrap().is_none());
  }
  installer
    .save_avatar("test", picture(ImageFormat::Png, 100))
    .await
    .unwrap();
  assert!(installer.avatar("test").await.unwrap().is_some());
}
#[tokio::test]
async fn reads_work_while_game_is_locked_but_writes_and_clear_refuse() {
  let (_root, installer, _target) = fixture();
  installer
    .save_avatar("test", picture(ImageFormat::Png, 100))
    .await
    .unwrap();
  let _running =
    storage::server_instance_lock(&installer.context.paths.data, "client-test").unwrap();
  assert!(installer.avatar("test").await.unwrap().is_some());
  assert!(matches!(
    installer
      .save_avatar("test", picture(ImageFormat::Png, 2))
      .await,
    Err(Error::Busy(_))
  ));
  assert!(matches!(
    installer.clear_avatar("test").await,
    Err(Error::Busy(_))
  ));
}
#[tokio::test]
async fn refuses_traversal_identity_changes_and_non_regular_targets() {
  let (_root, installer, target) = fixture();
  assert!(installer.avatar("../test").await.is_err());
  fs::create_dir(target.join("avatar.png")).unwrap();
  assert!(
    installer
      .save_avatar("test", picture(ImageFormat::Png, 1))
      .await
      .is_err()
  );
  assert!(installer.clear_avatar("test").await.is_err());
  assert!(installer.avatar("test").await.is_err());
  fs::write(
    target.join("instance.json"),
    br#"{"schema_version":1,"instance_type":"client","id":"client-other","name":"other"}"#,
  )
  .unwrap();
  assert!(installer.avatar("test").await.is_err());
}
#[cfg(unix)]
#[tokio::test]
async fn refuses_symlinks_and_hardlinks_without_touching_external_file() {
  let (root, installer, target) = fixture();
  let outside = root.path().join("outside.png");
  fs::write(&outside, picture(ImageFormat::Png, 100)).unwrap();
  for hard in [false, true] {
    let path = target.join("avatar.png");
    if hard {
      fs::hard_link(&outside, &path).unwrap();
    } else {
      std::os::unix::fs::symlink(&outside, &path).unwrap();
    }
    assert!(installer.avatar("test").await.is_err());
    assert!(
      installer
        .save_avatar("test", picture(ImageFormat::Png, 200))
        .await
        .is_err()
    );
    assert!(installer.clear_avatar("test").await.is_err());
    fs::remove_file(path).unwrap();
    assert_eq!(fs::read(&outside).unwrap(), picture(ImageFormat::Png, 100));
  }
  fs::rename(target.join("game"), target.join("original-game")).unwrap();
  std::os::unix::fs::symlink(target.join("original-game"), target.join("game")).unwrap();
  assert!(installer.avatar("test").await.is_err());
}

#[test]
fn rejects_animated_webp_and_applies_jpeg_orientation() {
  fn riff_chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut bytes = kind.to_vec();
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(body);
    if !body.len().is_multiple_of(2) {
      bytes.push(0);
    }
    bytes
  }
  let still = picture(ImageFormat::WebP, 60);
  let mut body = b"WEBP".to_vec();
  body.extend(riff_chunk(b"VP8X", &[2, 0, 0, 0, 15, 0, 0, 7, 0, 0]));
  body.extend(riff_chunk(b"ANIM", &[0; 6]));
  let mut frame = vec![0, 0, 0, 0, 0, 0, 15, 0, 0, 7, 0, 0, 1, 0, 0, 0];
  frame.extend_from_slice(&still[12..]);
  body.extend(riff_chunk(b"ANMF", &frame));
  let mut animated = b"RIFF".to_vec();
  animated.extend_from_slice(&(body.len() as u32).to_le_bytes());
  animated.extend(body);
  assert!(
    normalize(&animated)
      .unwrap_err()
      .to_string()
      .contains("animated WebP")
  );

  let mut jpeg = picture(ImageFormat::Jpeg, 60);
  // Minimal little-endian TIFF with Orientation=6 (90 degrees clockwise).
  let exif = [
    b"Exif\0\0".as_slice(),
    &[
      0x49, 0x49, 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 1, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
    ],
  ]
  .concat();
  let mut app1 = vec![0xff, 0xe1];
  app1.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
  app1.extend(exif);
  jpeg.splice(2..2, app1);
  let oriented = decode_static(&jpeg).unwrap();
  assert_eq!((oriented.width(), oriented.height()), (8, 16));
  assert!(normalize(&jpeg).is_ok());
}

#[tokio::test]
async fn settings_lock_serializes_avatar_operations() {
  let (_root, installer, _target) = fixture();
  let _settings =
    storage::client_settings_lock(&installer.context.paths.data, "client-test").unwrap();
  assert!(matches!(
    installer.avatar("test").await,
    Err(Error::Busy(_))
  ));
  assert!(matches!(
    installer
      .save_avatar("test", picture(ImageFormat::Png, 1))
      .await,
    Err(Error::Busy(_))
  ));
  assert!(matches!(
    installer.clear_avatar("test").await,
    Err(Error::Busy(_))
  ));
}
