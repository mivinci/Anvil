use std::io;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
  #[error("invalid argument: {0}")]
  Invalid(String),
  #[error("unsupported platform: {0}")]
  Unsupported(String),
  #[error("no matching Temurin release for {0}")]
  NotFound(String),
  #[error("runtime is busy: {0}")]
  Busy(String),
  #[error("installation conflict: {0}")]
  Conflict(String),
  #[error("invalid provider metadata: {0}")]
  Metadata(String),
  #[error("archive rejected: {0}")]
  Archive(String),
  #[error("Java validation failed: {0}")]
  Java(String),
  #[error("server EULA acceptance is required")]
  EulaRequired,
  #[error("could not start server process: {0}")]
  ServerLaunch(String),
  #[error("server process exited unsuccessfully: {details}")]
  ServerExit {
    details: String,
    process_exit_code: Option<i32>,
  },
  #[error("server process exited before reaching Ready (exit code {process_exit_code})")]
  ServerNotReady { process_exit_code: i32 },
  #[error("could not start Minecraft client process: {0}")]
  ClientLaunch(String),
  #[error("Minecraft client process exited unsuccessfully: {0}")]
  ClientExit(String),
  #[error("Minecraft account authorization is required")]
  AccountRequired,
  #[error("Minecraft account operation failed: {0}")]
  Authentication(String),
  #[error("operation cancelled")]
  Cancelled,
  #[error(transparent)]
  Http(#[from] q2q_http::Error),
  #[error("filesystem operation failed: {0}")]
  Io(#[from] io::Error),
  #[error("invalid JSON data: {0}")]
  Json(#[from] serde_json::Error),
  #[error("worker failed: {0}")]
  Worker(#[from] tokio::task::JoinError),
}

impl Error {
  pub fn code(&self) -> &'static str {
    match self {
      Self::Invalid(_) => "invalid_argument",
      Self::Unsupported(_) => "unsupported_platform",
      Self::NotFound(_) => "not_found",
      Self::Busy(_) => "busy",
      Self::Conflict(_) => "conflict",
      Self::Metadata(_) => "invalid_metadata",
      Self::Archive(_) => "invalid_archive",
      Self::Java(_) => "invalid_java",
      Self::EulaRequired => "eula_required",
      Self::ServerLaunch(_) => "server_launch_failed",
      Self::ServerExit { .. } => "server_exit",
      Self::ServerNotReady { .. } => "server_not_ready",
      Self::ClientLaunch(_) => "client_launch_failed",
      Self::ClientExit(_) => "client_exit",
      Self::AccountRequired => "account_required",
      Self::Authentication(_) => "authentication_failed",
      Self::Cancelled | Self::Http(q2q_http::Error::Cancelled) => "cancelled",
      Self::Http(q2q_http::Error::Busy) => "busy",
      Self::Http(q2q_http::Error::Integrity) => "integrity",
      Self::Http(_) => "http",
      Self::Io(_) => "io",
      Self::Json(_) => "invalid_json",
      Self::Worker(_) => "worker",
    }
  }

  pub fn exit_code(&self) -> u8 {
    match self.code() {
      "invalid_argument" => 2,
      "unsupported_platform"
      | "not_found"
      | "busy"
      | "conflict"
      | "eula_required"
      | "account_required" => 3,
      "http" => 4,
      "server_launch_failed"
      | "server_exit"
      | "server_not_ready"
      | "client_launch_failed"
      | "client_exit" => 5,
      "cancelled" => 130,
      _ => 6,
    }
  }
}

pub(crate) fn check_cancel(cancel: &tokio_util::sync::CancellationToken) -> Result<()> {
  if cancel.is_cancelled() {
    Err(Error::Cancelled)
  } else {
    Ok(())
  }
}
