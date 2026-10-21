mod backend;

use tokio::sync::mpsc;

use backend::GuiCommand;
use tokio_util::sync::CancellationToken;

slint::include_modules!();

const MICROSOFT_CLIENT_ID: &str = "b374e556-655b-41b1-aee9-45821ad631fe";

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let window = AnvilWindow::new()?;
  let (cmd_tx, cmd_rx) = mpsc::channel(16);
  let shutdown = CancellationToken::new();
  let worker = std::thread::Builder::new()
    .name("anvil-gui-worker".into())
    .spawn({
      let shutdown = shutdown.clone();
      let window = window.as_weak();
      move || backend::run(window, cmd_rx, shutdown)
    })?;

  let commands = cmd_tx.clone();
  window.on_refresh_data(move || {
    let _ = commands.try_send(GuiCommand::Refresh);
  });
  let commands = cmd_tx.clone();
  window.on_install_client(move |name, version| {
    let _ = commands.try_send(GuiCommand::Install(name.to_string(), version.to_string()));
  });
  let commands = cmd_tx.clone();
  window.on_login(move || {
    let _ = commands.try_send(GuiCommand::Login);
  });
  let commands = cmd_tx.clone();
  window.on_cancel(move || {
    let _ = commands.try_send(GuiCommand::Cancel);
  });
  let commands = cmd_tx.clone();
  window.on_launch_client(move |name, account_id| {
    let _ = commands.try_send(GuiCommand::Launch(name.to_string(), account_id.to_string()));
  });
  let commands = cmd_tx.clone();
  window.on_select_account(move |account_id| {
    let _ = commands.try_send(GuiCommand::SelectAccount(account_id.to_string()));
  });
  let commands = cmd_tx.clone();
  window.on_logout_account(move |account_id| {
    let _ = commands.try_send(GuiCommand::Logout(account_id.to_string()));
  });
  let _ = cmd_tx.try_send(GuiCommand::Refresh);

  let ui_result = window.run();
  shutdown.cancel();
  drop(cmd_tx);
  let worker_result = worker
    .join()
    .map_err(|_| std::io::Error::other("Anvil background worker panicked"))?;
  ui_result?;
  worker_result.map_err(std::io::Error::other)?;
  Ok(())
}
