//! Bounded instance-local image metadata; never writes into the game directory.
use super::{ClientInstaller, client_instance_id, validated_game_directory};
use crate::{CancellationToken, Error, Result, storage};
use image::{
  DynamicImage, ImageDecoder, ImageFormat, Limits,
  codecs::{jpeg::JpegDecoder, png::PngDecoder, webp::WebPDecoder},
};
use sha2::{Digest, Sha256};
use std::{
  fs,
  io::{Cursor, Read},
  path::{Path, PathBuf},
};

pub const MAX_AVATAR_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_AVATAR_PIXELS: u64 = 16_000_000;
const AVATAR_SIZE: u32 = 256;
const MAX_STORED_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub struct ClientAvatar {
  pub png: Vec<u8>,
  /// Content identity used for cache invalidation, not a persistent counter.
  pub revision: String,
}
impl ClientAvatar {
  fn new(png: Vec<u8>) -> Self {
    let revision = Sha256::digest(&png)
      .iter()
      .map(|byte| format!("{byte:02x}"))
      .collect();
    Self { png, revision }
  }
}
fn invalid(message: impl std::fmt::Display) -> Error {
  Error::Invalid(format!("invalid avatar image: {message}"))
}
fn limits() -> Limits {
  let mut limits = Limits::default();
  limits.max_image_width = Some(16_000);
  limits.max_image_height = Some(16_000);
  limits.max_alloc = Some(128 * 1024 * 1024);
  limits
}
fn decode<D: ImageDecoder>(mut decoder: D) -> Result<DynamicImage> {
  let (width, height) = decoder.dimensions();
  if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_AVATAR_PIXELS {
    return Err(invalid("pixel budget exceeded"));
  }
  decoder.set_limits(limits()).map_err(invalid)?;
  let orientation = decoder.orientation().map_err(invalid)?;
  let mut image = DynamicImage::from_decoder(decoder).map_err(invalid)?;
  image.apply_orientation(orientation);
  Ok(image)
}
fn decode_static(bytes: &[u8]) -> Result<DynamicImage> {
  if bytes.is_empty() || bytes.len() > MAX_AVATAR_INPUT_BYTES {
    return Err(invalid("file size exceeds limit or is empty"));
  }
  match image::guess_format(bytes).map_err(invalid)? {
    ImageFormat::Png => {
      let decoder = PngDecoder::with_limits(Cursor::new(bytes), limits()).map_err(invalid)?;
      if decoder.is_apng().map_err(invalid)? {
        return Err(invalid("animated PNG is not supported"));
      }
      decode(decoder)
    }
    ImageFormat::Jpeg => decode(JpegDecoder::new(Cursor::new(bytes)).map_err(invalid)?),
    ImageFormat::WebP => {
      let decoder = WebPDecoder::new(Cursor::new(bytes)).map_err(invalid)?;
      if decoder.has_animation() {
        return Err(invalid("animated WebP is not supported"));
      }
      decode(decoder)
    }
    _ => Err(invalid("only static PNG, JPEG and WebP are supported")),
  }
}
fn normalize(bytes: &[u8]) -> Result<ClientAvatar> {
  let image = decode_static(bytes)?;
  // UI supplies the chosen square; direct API callers receive a centered crop.
  let image = image
    .resize_to_fill(
      AVATAR_SIZE,
      AVATAR_SIZE,
      image::imageops::FilterType::Triangle,
    )
    .to_rgba8();
  let mut output = Cursor::new(Vec::new());
  DynamicImage::ImageRgba8(image)
    .write_to(&mut output, ImageFormat::Png)
    .map_err(invalid)?;
  let png = output.into_inner();
  if png.len() > MAX_STORED_BYTES {
    return Err(invalid("encoded avatar exceeds limit"));
  }
  Ok(ClientAvatar::new(png))
}
fn linked(metadata: &fs::Metadata) -> bool {
  if metadata.file_type().is_symlink() {
    return true;
  }
  #[cfg(windows)]
  {
    use std::os::windows::fs::MetadataExt;
    if metadata.file_attributes() & 0x400 != 0 {
      return true;
    }
  }
  false
}
fn avatar_path(data: &Path, id: &str) -> Result<PathBuf> {
  let metadata = fs::symlink_metadata(data)?;
  if linked(&metadata) || !metadata.is_dir() {
    return Err(Error::Conflict("invalid data directory".into()));
  }
  let game = validated_game_directory(data, id)?;
  let root = data.join("instances");
  let target = root.join(id);
  for path in [&root, &target, &game] {
    if linked(&fs::symlink_metadata(path)?) {
      return Err(Error::Conflict("linked avatar directory".into()));
    }
  }
  let canonical_root = fs::canonicalize(&root)?;
  if canonical_root.parent() != Some(fs::canonicalize(data)?.as_path())
    || fs::canonicalize(&target)?.parent() != Some(canonical_root.as_path())
  {
    return Err(Error::Conflict(
      "avatar directory escaped instance root".into(),
    ));
  }
  Ok(target.join("avatar.png"))
}
fn exists_regular(path: &Path) -> Result<bool> {
  match fs::symlink_metadata(path) {
    Ok(metadata) => {
      if linked(&metadata) || !metadata.is_file() {
        return Err(Error::Conflict("avatar is not a regular file".into()));
      }
      #[cfg(unix)]
      {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
          return Err(Error::Conflict("avatar has external hard links".into()));
        }
      }
      Ok(true)
    }
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
    Err(error) => Err(error.into()),
  }
}
fn read_avatar(path: &Path) -> Result<Option<ClientAvatar>> {
  if !exists_regular(path)? {
    return Ok(None);
  }
  let mut bytes = Vec::new();
  fs::File::open(path)?
    .take(MAX_STORED_BYTES as u64 + 1)
    .read_to_end(&mut bytes)?;
  if bytes.len() > MAX_STORED_BYTES || image::guess_format(&bytes).ok() != Some(ImageFormat::Png) {
    return Ok(None);
  }
  match decode_static(&bytes) {
    Ok(image) if image.width() == AVATAR_SIZE && image.height() == AVATAR_SIZE => {
      Ok(Some(ClientAvatar::new(bytes)))
    }
    _ => Ok(None),
  }
}
impl ClientInstaller {
  pub async fn avatar(&self, name: &str) -> Result<Option<ClientAvatar>> {
    let id = client_instance_id(name)?;
    let data = self.context.paths.data.clone();
    self
      .blocking(&CancellationToken::new(), move || {
        // Reads remain available while a game runs; deletion also takes this lock.
        let _settings = storage::client_settings_lock(&data, &id)?;
        read_avatar(&avatar_path(&data, &id)?)
      })
      .await
  }
  pub async fn save_avatar(&self, name: &str, bytes: Vec<u8>) -> Result<ClientAvatar> {
    if bytes.len() > MAX_AVATAR_INPUT_BYTES {
      return Err(invalid("file size exceeds limit"));
    }
    let id = client_instance_id(name)?;
    let data = self.context.paths.data.clone();
    self
      .blocking(&CancellationToken::new(), move || {
        let _instance = storage::server_instance_lock(&data, &id)?;
        let _settings = storage::client_settings_lock(&data, &id)?;
        let path = avatar_path(&data, &id)?;
        exists_regular(&path)?;
        let avatar = normalize(&bytes)?;
        storage::replace_file(&path, &avatar.png)?;
        Ok(avatar)
      })
      .await
  }
  pub async fn clear_avatar(&self, name: &str) -> Result<()> {
    let id = client_instance_id(name)?;
    let data = self.context.paths.data.clone();
    self
      .blocking(&CancellationToken::new(), move || {
        let _instance = storage::server_instance_lock(&data, &id)?;
        let _settings = storage::client_settings_lock(&data, &id)?;
        let path = avatar_path(&data, &id)?;
        if exists_regular(&path)? {
          fs::remove_file(&path)?;
          #[cfg(unix)]
          fs::File::open(path.parent().unwrap())?.sync_all()?;
        }
        Ok(())
      })
      .await
  }
}

#[cfg(test)]
#[path = "../../tests/client_avatar.rs"]
mod tests;
