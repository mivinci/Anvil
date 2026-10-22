/// The complete Tauri IPC surface exposed by the Shell.
///
/// This list is shared with `build.rs`, which generates one permission per command.
pub const SHELL_COMMANDS: &[&str] = &[
  "shell_get_app_info",
  "shell_get_logging_status",
  "shell_open_log_directory",
  "shell_open_external_url",
  "shell_get_update_status",
  "shell_check_for_updates",
  "shell_cancel_update",
  "shell_get_runtime_status",
  "shell_get_runtime_connection",
  "shell_confirm_app_ready",
  "shell_report_app_startup_failure",
];
