use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
  let console = std::env::var("ANVIL_LOG_STDERR").is_ok_and(|value| value == "1");
  let _logging = anvil_logging::init(anvil_logging::Process::Runtime, console);
  match anvil_desktop_runtime::run_from_stdio_with_logging(Some(_logging.status())).await {
    Ok(()) => {
      tracing::info!(target: "anvil_desktop_runtime", "Runtime shutdown complete");
      ExitCode::SUCCESS
    }
    Err(error) => {
      tracing::error!(target: "anvil_desktop_runtime", error = %error, "Runtime startup or shutdown failed");
      // Return normally so the logging guard flushes before process exit.
      ExitCode::FAILURE
    }
  }
}
