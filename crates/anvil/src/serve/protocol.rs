use serde::Serialize;
use std::io;
use tokio::{
  io::{AsyncRead, AsyncReadExt},
  net::TcpListener,
};

pub(super) const MAX_BOOTSTRAP_LINE_BYTES: usize = 128;
const ALLOWED_ORIGINS: &[&str] = &[
  "anvil-app://localhost",
  "http://anvil-app.localhost",
  "https://anvil-app.localhost",
  "http://localhost:5173",
  "http://127.0.0.1:5173",
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReadyMessage<'a> {
  pub kind: &'static str,
  pub port: u16,
  pub api_version: &'a str,
}

pub(super) async fn bind_loopback() -> io::Result<TcpListener> {
  TcpListener::bind(("127.0.0.1", 0)).await
}

pub(super) fn is_loopback_address(address: std::net::SocketAddr) -> bool {
  address.ip().is_loopback()
}

pub(super) fn is_allowed_origin(origin: &str) -> bool {
  ALLOWED_ORIGINS.contains(&origin)
}

pub(super) fn is_token(token: &str) -> bool {
  token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
  let mut difference = left.len() ^ right.len();
  let max_len = left.len().max(right.len());
  for index in 0..max_len {
    let left_byte = left.get(index).copied().unwrap_or_default();
    let right_byte = right.get(index).copied().unwrap_or_default();
    difference |= usize::from(left_byte ^ right_byte);
  }
  difference == 0
}

pub(super) async fn read_limited_line<R: AsyncRead + Unpin>(
  reader: &mut R,
  limit: usize,
) -> io::Result<Option<String>> {
  let mut bytes = Vec::with_capacity(limit.min(128));
  let mut byte = [0u8; 1];
  loop {
    match reader.read(&mut byte).await? {
      0 if bytes.is_empty() => return Ok(None),
      0 => {
        return Err(io::Error::new(
          io::ErrorKind::UnexpectedEof,
          "unterminated Runtime protocol line",
        ));
      }
      _ if byte[0] == b'\n' => {
        return String::from_utf8(bytes).map(Some).map_err(|_| {
          io::Error::new(
            io::ErrorKind::InvalidData,
            "Runtime protocol line is not UTF-8",
          )
        });
      }
      _ if bytes.len() == limit => {
        return Err(io::Error::new(
          io::ErrorKind::InvalidData,
          "Runtime protocol line is too large",
        ));
      }
      _ => bytes.push(byte[0]),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test(flavor = "current_thread")]
  async fn loopback_binding_uses_a_dynamic_local_port() {
    let listener = bind_loopback().await.unwrap();
    let address = listener.local_addr().unwrap();
    assert!(is_loopback_address(address));
    assert_ne!(address.port(), 0);
    assert!(!is_loopback_address("0.0.0.0:80".parse().unwrap()));
    assert!(!is_loopback_address("192.0.2.1:80".parse().unwrap()));
  }

  #[test]
  fn tokens_require_a_full_hex_credential_and_comparison_checks_lengths() {
    assert!(is_token(&"ab".repeat(32)));
    assert!(!is_token("short"));
    assert!(constant_time_eq(b"secret", b"secret"));
    assert!(!constant_time_eq(b"secret", b"secret!"));
  }
}
