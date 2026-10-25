use anvil_shell::{
  activation::AppActivation,
  app_storage::{ActivationFailureReason, AppStorage, AppVersionState, AppVersionStore},
};
use semver::Version;
use std::{fs, time::Duration};
use tempfile::TempDir;

fn version(value: &str) -> Version {
  Version::parse(value).unwrap()
}

fn publish(store: &AppVersionStore, value: &str) {
  let staging = store.stage_version().unwrap();
  fs::write(staging.directory().join("version.txt"), value).unwrap();
  staging.publish(&version(value)).unwrap();
}

fn staged_update() -> (TempDir, AppVersionStore) {
  let root = TempDir::new().unwrap();
  let store = AppStorage::open(root.path().join("data"))
    .unwrap()
    .app("anvil.desktop")
    .unwrap();
  publish(&store, "1.0.0");
  publish(&store, "1.1.0");
  store
    .write_state(&AppVersionState {
      active_version: Some(version("1.0.0")),
      pending_version: Some(version("1.1.0")),
      ..AppVersionState::default()
    })
    .unwrap();
  (root, store)
}

#[test]
fn pending_version_becomes_active_only_after_page_health_confirmation() {
  let (_root, store) = staged_update();
  let activation = AppActivation::new(store.clone());

  assert!(activation.prepare_startup().unwrap());
  let attempted = store.state().unwrap();
  assert_eq!(attempted.active_version, Some(version("1.1.0")));
  assert_eq!(attempted.previous_healthy_version, Some(version("1.0.0")));
  assert!(attempted.activation_in_progress);
  assert_eq!(attempted.pending_version, None);

  assert!(activation.confirm_health().unwrap());
  let healthy = store.state().unwrap();
  assert_eq!(healthy.active_version, Some(version("1.1.0")));
  assert_eq!(healthy.previous_healthy_version, Some(version("1.0.0")));
  assert!(!healthy.activation_in_progress);
  assert_eq!(healthy.failed_version, None);
}

#[test]
fn a_failed_startup_rolls_back_and_the_same_version_is_not_retried() {
  let (_root, store) = staged_update();
  let activation = AppActivation::with_timeout(store.clone(), Duration::from_millis(10));

  assert!(activation.prepare_startup().unwrap());
  assert!(activation.wait_for_timeout());
  assert!(
    activation
      .rollback(ActivationFailureReason::PageTimeout)
      .unwrap()
  );

  let rolled_back = store.state().unwrap();
  assert_eq!(rolled_back.active_version, Some(version("1.0.0")));
  assert_eq!(rolled_back.previous_healthy_version, Some(version("1.0.0")));
  assert_eq!(rolled_back.failed_version, Some(version("1.1.0")));
  assert_eq!(
    rolled_back.activation_failure,
    Some(ActivationFailureReason::PageTimeout)
  );
  assert!(!rolled_back.activation_in_progress);
  assert!(!activation.prepare_startup().unwrap());
  assert_eq!(store.state().unwrap(), rolled_back);
}

#[test]
fn a_crash_during_activation_is_recovered_on_the_next_start() {
  let (_root, store) = staged_update();
  store
    .write_state(&AppVersionState {
      active_version: Some(version("1.1.0")),
      previous_healthy_version: Some(version("1.0.0")),
      activation_in_progress: true,
      ..AppVersionState::default()
    })
    .unwrap();
  let activation = AppActivation::new(store.clone());

  assert!(!activation.prepare_startup().unwrap());
  let recovered = store.state().unwrap();
  assert_eq!(recovered.active_version, Some(version("1.0.0")));
  assert_eq!(recovered.failed_version, Some(version("1.1.0")));
  assert_eq!(
    recovered.activation_failure,
    Some(ActivationFailureReason::Interrupted)
  );
  assert!(!recovered.activation_in_progress);
}

#[test]
fn an_interrupted_attempt_recovers_when_its_published_candidate_is_missing() {
  let (_root, store) = staged_update();
  let activation = AppActivation::new(store.clone());
  assert!(activation.prepare_startup().unwrap());
  fs::remove_dir_all(store.versions_dir().join("1.1.0")).unwrap();

  let restarted_shell = AppActivation::new(store.clone());
  assert!(!restarted_shell.prepare_startup().unwrap());
  let recovered = store.state().unwrap();
  assert_eq!(recovered.active_version, Some(version("1.0.0")));
  assert_eq!(recovered.failed_version, Some(version("1.1.0")));
  assert_eq!(
    recovered.activation_failure,
    Some(ActivationFailureReason::Interrupted)
  );
}

#[test]
fn a_newer_pending_version_survives_rollback_of_an_older_candidate() {
  let (_root, store) = staged_update();
  publish(&store, "1.2.0");
  store
    .write_state(&AppVersionState {
      active_version: Some(version("1.0.0")),
      pending_version: Some(version("1.1.0")),
      ..AppVersionState::default()
    })
    .unwrap();
  let activation = AppActivation::new(store.clone());
  assert!(activation.prepare_startup().unwrap());

  let mut state = store.state().unwrap();
  state.pending_version = Some(version("1.2.0"));
  store.write_state(&state).unwrap();
  assert!(
    activation
      .rollback(ActivationFailureReason::RuntimeStartup)
      .unwrap()
  );

  let rolled_back = store.state().unwrap();
  assert_eq!(rolled_back.active_version, Some(version("1.0.0")));
  assert_eq!(rolled_back.pending_version, Some(version("1.2.0")));
}
