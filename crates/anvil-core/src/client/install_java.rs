//! Shared Java preparation for CLI and desktop client-install workflows.
use super::*;

impl ClientInstaller {
  /// Prepare the shared managed Java before publishing a playable client instance.
  pub async fn prepare_install_java(
    &self,
    plan: &ClientInstallPlan,
    cancel: &CancellationToken,
    progress: &watch::Sender<ClientProgress>,
  ) -> Result<()> {
    let cancel = cancel.child_token();
    let _cancel_on_drop = CancelOnDrop(cancel.clone());
    let (sender, mut updates) = watch::channel(JavaProgress::default());
    progress.send_replace(ClientProgress {
      stage: ClientInstallStage::Java,
      java_major: Some(plan.java_major),
      java_stage: Some(crate::java::InstallStage::Resolving),
      ..ClientProgress::default()
    });
    let manager = JavaManager::new(self.context.clone());
    let task = manager.ensure_major(plan.java_major, &cancel, sender);
    tokio::pin!(task);
    let mut watching = true;
    loop {
      tokio::select! {
        result = &mut task => {
          let result = result?;
          validate_java_for_plan(&result.runtime, plan)?;
          progress.send_modify(|state| state.java_stage = Some(crate::java::InstallStage::Completed));
          return Ok(());
        }
        changed = updates.changed(), if watching => {
          if changed.is_err() { watching = false; continue; }
          let java = updates.borrow_and_update().clone();
          progress.send_modify(|state| {
            state.java_stage = Some(java.stage);
            state.downloaded_bytes = java.downloaded_bytes;
            state.total_bytes = java.total_bytes;
          });
        }
      }
    }
  }
}
