#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anvil_shell::app_protocol::AppPageProtocol;
use anvil_shell::{
  activation::AppActivation, runtime::RuntimeSupervisor, shell_updater::ShellUpdateService,
  updater::UpdateService,
};
#[cfg(not(dev))]
use anvil_shell::{
  app_storage::{ActivationFailureReason, AppStorage},
  bundled_app::install_bundled_app,
  runtime::{RuntimeFailure, RuntimeState},
};
use tauri::Manager;
#[cfg(target_os = "windows")]
mod windows_frame;

fn main() {
  let mut logging = Some(anvil_logging::init(
    anvil_logging::Process::Shell,
    cfg!(dev),
  ));
  let page_protocol = AppPageProtocol::for_current_user();
  let context = tauri::generate_context!();
  #[cfg(target_os = "windows")]
  let context = {
    let mut context = context;
    for window in &mut context.config_mut().app.windows {
      if window.label == "main" {
        window.decorations = true;
        window.transparent = true;
        window.window_effects = Some(tauri::utils::config::WindowEffectsConfig {
          effects: vec![tauri::window::Effect::Acrylic],
          ..Default::default()
        });
      }
    }
    context
  };

  let app = tauri::Builder::default()
    .manage(
      logging
        .as_ref()
        .expect("logging guard initialized")
        .status(),
    )
    .manage(RuntimeSupervisor::new())
    .manage(UpdateService::new())
    .manage(ShellUpdateService::new())
    .setup(|app| {
      #[cfg(target_os = "windows")]
      if let Some(window) = app.get_webview_window("main") {
        windows_frame::install(&window)?;
      }
      let runtime = app.state::<RuntimeSupervisor>();
      #[cfg(not(dev))]
      let updates = app.state::<UpdateService>();
      #[cfg(not(dev))]
      let shell_updates = app.state::<ShellUpdateService>();
      #[cfg(not(dev))]
      shell_updates.start_background_checks();
      let activation = AppActivation::for_current_user();
      app.manage(activation.clone());
      // The Vite page and sidecar must come from the same source build. Installed
      // versions are immutable and may expose an older API during development.
      #[cfg(dev)]
      {
        let source_package =
          std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../desktop/baseline");
        runtime.start_development_app(&source_package);
        Ok(())
      }
      #[cfg(not(dev))]
      {
        let resource_package = app
          .path()
          .resource_dir()
          .ok()
          .map(|resource_dir| resource_dir.join("baseline"));
        let source_package =
          std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../desktop/baseline");
        let bundled_package = resource_package.filter(|path| path.exists()).or_else(|| {
          (cfg!(debug_assertions) && source_package.join("manifest.json").is_file())
            .then_some(source_package)
        });

        if cfg!(debug_assertions) && bundled_package.is_none() {
          let activation_attempt = match activation.prepare_startup() {
            Ok(attempt) => attempt,
            Err(_) => {
              runtime.mark_startup_failure(RuntimeFailure::StorageUnavailable);
              false
            }
          };
          start_app_with_activation(
            app.handle().clone(),
            &runtime,
            activation,
            activation_attempt,
          );
          updates.start_background_checks();
          return Ok(());
        }

        let install_result = AppStorage::for_current_user()
          .map_err(|_| ())
          .and_then(|storage| {
            install_bundled_app(
              &storage,
              bundled_package.as_deref(),
              env!("ANVIL_DEFAULT_APP_ID"),
              env!("ANVIL_TARGET_PLATFORM"),
              env!("ANVIL_SHELL_API_VERSION"),
              env!("ANVIL_BUNDLED_APP_VERSION"),
            )
            .map(|_| ())
            .map_err(|_| ())
          });

        if install_result.is_ok() {
          let activation_attempt = match activation.prepare_startup() {
            Ok(attempt) => attempt,
            Err(_) => {
              runtime.mark_startup_failure(RuntimeFailure::StorageUnavailable);
              false
            }
          };
          start_app_with_activation(
            app.handle().clone(),
            &runtime,
            activation,
            activation_attempt,
          );
        } else {
          runtime.mark_startup_failure(RuntimeFailure::BundledAppInstallFailed);
        }
        updates.start_background_checks();
        Ok(())
      }
    })
    .invoke_handler(tauri::generate_handler![
      anvil_shell::shell_ipc::shell_get_app_info,
      anvil_shell::shell_ipc::shell_get_logging_status,
      anvil_shell::shell_ipc::shell_open_log_directory,
      anvil_shell::shell_ipc::shell_open_external_url,
      anvil_shell::shell_ipc::shell_get_update_status,
      anvil_shell::shell_ipc::shell_check_for_updates,
      anvil_shell::shell_ipc::shell_cancel_update,
      anvil_shell::shell_ipc::shell_restart_for_update,
      anvil_shell::shell_ipc::shell_get_runtime_status,
      anvil_shell::shell_ipc::shell_get_runtime_connection,
      anvil_shell::shell_ipc::shell_confirm_app_ready,
      anvil_shell::shell_ipc::shell_report_app_startup_failure
    ])
    .register_asynchronous_uri_scheme_protocol("anvil-app", move |_context, request, responder| {
      let page_protocol = page_protocol.clone();
      std::thread::spawn(move || {
        responder.respond(page_protocol.handle(request));
      });
    })
    .build(context)
    .expect("failed to run Anvil desktop application");
  app.run(move |app, event| {
    if matches!(event, tauri::RunEvent::Exit) {
      app.state::<RuntimeSupervisor>().shutdown();
      app.state::<UpdateService>().shutdown();
      app.state::<ShellUpdateService>().shutdown();
      tracing::info!(target: "anvil_shell", "Shell shutdown complete");
      // Tauri exits with process::exit after this callback, so RAII at the end
      // of main alone cannot guarantee flushing the asynchronous writer.
      drop(logging.take());
    }
  });
}

#[cfg(not(dev))]
fn start_app_with_activation(
  app: tauri::AppHandle,
  runtime: &RuntimeSupervisor,
  activation: AppActivation,
  activation_attempt: bool,
) {
  runtime.start_default_app();
  if !activation_attempt {
    return;
  }

  if runtime.status().state == RuntimeState::Ready {
    monitor_activation_health(app, activation);
  } else if activation
    .rollback(ActivationFailureReason::RuntimeStartup)
    .unwrap_or(false)
  {
    runtime.shutdown();
    runtime.start_default_app();
  }
}

#[cfg(not(dev))]
fn monitor_activation_health(app: tauri::AppHandle, activation: AppActivation) {
  std::thread::spawn(move || {
    if !activation.wait_for_timeout() {
      return;
    }

    let runtime = app.state::<RuntimeSupervisor>();
    if activation
      .rollback(ActivationFailureReason::PageTimeout)
      .unwrap_or(false)
    {
      runtime.shutdown();
      runtime.start_default_app();
      if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval("window.location.reload()");
      }
    }
  });
}
