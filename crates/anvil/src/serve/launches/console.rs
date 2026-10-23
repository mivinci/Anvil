//! Per-launch, memory-only console tails. Never forward raw child output to tracing.
use anvil_core::account::AccountSession;
use serde::Serialize;
use std::{
  collections::VecDeque,
  sync::{Arc, Mutex},
};
use tokio::io::{AsyncRead, AsyncReadExt};
const MAX_BYTES: usize = 256 * 1024;
const MAX_LINE: usize = 8192;
#[derive(Default)]
pub(super) struct Console {
  lines: VecDeque<String>,
  bytes: usize,
  truncated: bool,
  read_failed: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::serve) struct ConsoleSnapshot {
  pub exists: bool,
  pub launch_id: Option<u64>,
  pub text: String,
  pub total_bytes: usize,
  pub truncated: bool,
  pub read_failed: bool,
}
impl Console {
  pub fn snapshot(&self, id: u64) -> ConsoleSnapshot {
    ConsoleSnapshot {
      exists: true,
      launch_id: Some(id),
      text: self.lines.iter().map(String::as_str).collect(),
      total_bytes: self.bytes,
      truncated: self.truncated,
      read_failed: self.read_failed,
    }
  }
  fn push(&mut self, source: &str, line: &[u8], account: &AccountSession) {
    let value = account.redact_process_output(&String::from_utf8_lossy(line));
    let value = format!("[{source}] {value}\n");
    while self.bytes + value.len() > MAX_BYTES {
      if let Some(old) = self.lines.pop_front() {
        self.bytes -= old.len();
        self.truncated = true;
      } else {
        break;
      }
    }
    self.bytes += value.len();
    self.lines.push_back(value);
  }
}
pub(super) async fn capture(
  mut reader: impl AsyncRead + Unpin,
  source: &'static str,
  console: Arc<Mutex<Console>>,
  account: Arc<AccountSession>,
) {
  let mut chunk = [0_u8; 4096];
  let mut line = Vec::with_capacity(MAX_LINE);
  let mut skipping = false;
  loop {
    let count = match reader.read(&mut chunk).await {
      Ok(0) => break,
      Ok(count) => count,
      Err(_) => {
        console
          .lock()
          .unwrap_or_else(std::sync::PoisonError::into_inner)
          .read_failed = true;
        break;
      }
    };
    for byte in &chunk[..count] {
      if *byte == b'\n' {
        if !skipping {
          console
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(source, &line, &account);
        }
        line.clear();
        skipping = false;
      } else if !skipping {
        if line.len() == MAX_LINE {
          line.clear();
          skipping = true;
          let mut value = console
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
          value.truncated = true;
          value.push(source, "[已跳过超长输出行]".as_bytes(), &account);
        } else {
          line.push(*byte);
        }
      }
    }
  }
  if !skipping && !line.is_empty() {
    console
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .push(source, &line, &account);
  }
}
pub(super) async fn finish(mut tasks: tokio::task::JoinSet<()>, console: &Arc<Mutex<Console>>) {
  // Descendants can inherit pipe handles; cap draining after the direct child exits.
  let drain = async {
    let mut failed = false;
    while let Some(result) = tasks.join_next().await {
      failed |= result.is_err();
    }
    failed
  };
  let failed = match tokio::time::timeout(std::time::Duration::from_secs(2), drain).await {
    Ok(failed) => failed,
    Err(_) => {
      tasks.abort_all();
      while tasks.join_next().await.is_some() {}
      true
    }
  };
  if failed {
    console
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .read_failed = true;
  }
}
