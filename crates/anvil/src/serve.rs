//! Desktop runtime server for the Anvil desktop app, started by `anvil serve`.
//!
//! The first line on stdin is a short-lived bearer token. The process writes a
//! single JSON ready message to stdout after binding loopback, then accepts a
//! `shutdown` line (or stdin EOF) to stop gracefully.

use anvil_core::{Context, Paths};
use semver::Version;
use std::{error::Error, io, sync::Arc};
use tokio::{io::BufReader, sync::oneshot};

mod accounts;
mod api;
mod operations;
mod protocol;
mod versions;

pub(crate) async fn run_from_stdio_with_logging(
  logging: Option<anvil_logging::LoggingStatus>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
  tracing::info!("Runtime startup started");
  let api_version = std::env::var("ANVIL_HTTP_API_VERSION")?;
  let parsed_version = Version::parse(&api_version)?;
  if parsed_version.to_string() != api_version {
    return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid Runtime API version").into());
  }

  let stdin = tokio::io::stdin();
  let mut stdin = BufReader::new(stdin);
  let token = protocol::read_limited_line(&mut stdin, protocol::MAX_BOOTSTRAP_LINE_BYTES)
    .await?
    .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "missing Runtime credential"))?;
  let token = token.trim_end_matches(['\r', '\n']);
  if !protocol::is_token(token) {
    return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid Runtime credential").into());
  }

  let context = Context::new(Paths::discover(None)?)?;
  let runtime = Arc::new(operations::RuntimeState::new(context.clone()));
  let accounts = Arc::new(accounts::AccountState::new(context));
  let mut app_state = api::AppState::new(runtime, accounts, api_version.clone(), token.to_owned());
  app_state.logging = logging;
  let listener = protocol::bind_loopback().await?;
  let address = listener.local_addr()?;
  if !protocol::is_loopback_address(address) {
    return Err(
      io::Error::new(
        io::ErrorKind::AddrNotAvailable,
        "Runtime must bind loopback",
      )
      .into(),
    );
  }

  let ready = protocol::ReadyMessage {
    kind: "ready",
    port: address.port(),
    api_version: &api_version,
  };
  println!("{}", serde_json::to_string(&ready)?);
  use std::io::Write;
  std::io::stdout().flush()?;

  tracing::info!(api_version = %api_version, "Runtime API ready on loopback");

  let (shutdown_tx, shutdown_rx) = oneshot::channel();
  tokio::spawn(async move {
    let command = protocol::read_limited_line(&mut stdin, protocol::MAX_BOOTSTRAP_LINE_BYTES)
      .await
      .ok()
      .flatten();
    if command.is_none()
      || command
        .as_deref()
        .is_some_and(|line| line.trim() == "shutdown")
    {
      let _ = shutdown_tx.send(());
    }
  });

  api::serve(listener, app_state, shutdown_rx).await?;
  Ok(())
}
