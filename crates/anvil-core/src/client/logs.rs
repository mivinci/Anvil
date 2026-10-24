//! Read-only, bounded snapshots of the game's own latest.log; never accept a file path.
use super::{ClientInstaller, client_instance_id, validated_game_directory};
use crate::{CancellationToken, Error, Result};
use serde::Serialize;
use std::{
  fs,
  io::{Read, Seek, SeekFrom},
};

const MAX_TAIL_BYTES: u64 = 256 * 1024;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientGameLog {
  pub exists: bool,
  pub text: String,
  pub total_bytes: u64,
  pub truncated: bool,
}
impl ClientInstaller {
  /// Reads at most 256 KiB, without taking the lock held by a running game.
  pub async fn read_game_log(&self, name: &str) -> Result<ClientGameLog> {
    let id = client_instance_id(name)?;
    let data = self.context.paths.data.clone();
    self
      .blocking(&CancellationToken::new(), move || {
        let game = validated_game_directory(&data, &id)?;
        let empty = || ClientGameLog {
          exists: false,
          text: String::new(),
          total_bytes: 0,
          truncated: false,
        };
        let logs = game.join("logs");
        let path = logs.join("latest.log");
        for (entry, is_directory) in [(&logs, true), (&path, false)] {
          let metadata = match fs::symlink_metadata(entry) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(empty()),
            Err(error) => return Err(error.into()),
          };
          if metadata.file_type().is_symlink()
            || (if is_directory {
              !metadata.is_dir()
            } else {
              !metadata.is_file()
            })
          {
            return Err(Error::Conflict(
              "game log must be a regular file in the instance".into(),
            ));
          }
        }
        let mut file = fs::File::open(&path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
          return Err(Error::Conflict("invalid game log file".into()));
        }
        let size = metadata.len();
        let start = size.saturating_sub(MAX_TAIL_BYTES);
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = Vec::with_capacity((size - start) as usize);
        file.take(size - start).read_to_end(&mut bytes)?;
        // Omit an incomplete leading line and UTF-8 sequence when taking a tail.
        if start > 0
          && let Some(newline) = bytes.iter().position(|byte| *byte == b'\n')
        {
          bytes.drain(..=newline);
        }
        Ok(ClientGameLog {
          exists: true,
          text: String::from_utf8_lossy(&bytes).into_owned(),
          total_bytes: size,
          truncated: start > 0,
        })
      })
      .await
  }
}
