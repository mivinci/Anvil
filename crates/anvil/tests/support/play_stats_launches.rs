use super::*;
use axum::response::IntoResponse;

#[test]
fn clearing_stats_waits_for_all_launches_to_finish() {
  let root = tempfile::tempdir().unwrap();
  let stats = Arc::new(PlayStatsStore::open(root.path()));
  for account in ["account", OFFLINE_ACCOUNT] {
    stats
      .record(PlaySession {
        account_id: account.into(),
        instance_id: "world".into(),
        started_at: 1_000,
        ended_at: 61_000,
      })
      .unwrap();
  }
  let before = stats.snapshot();
  let launches = Launches::new(stats.clone());
  let (updates, _receiver) = watch::channel(Snapshot {
    id: 1,
    name: "world".into(),
    offline_test: false,
    started_at: None,
    phase: Phase::Preparing,
    cancel_requested: false,
    java_download_started: false,
    java_major: None,
    java_stage: None,
    java_downloaded_bytes: 0,
    java_total_bytes: None,
    exit_code: None,
    error_code: None,
    error: None,
  });
  launches.inner.lock().unwrap().operations.insert(
    1,
    Operation {
      console: Arc::new(Mutex::new(Console::default())),
      cancel: CancellationToken::new(),
      updates: updates.clone(),
    },
  );
  for phase in [
    Phase::Preparing,
    Phase::Java,
    Phase::Running,
    Phase::Stopping,
  ] {
    updates.send_modify(|value| value.phase = phase);
    for target in [None, Some("account")] {
      let error = launches.clear_play_stats(target).unwrap_err();
      assert_eq!(error.into_response().status(), StatusCode::CONFLICT);
      assert_eq!(stats.snapshot(), before);
    }
  }
  updates.send_modify(|value| value.phase = Phase::Exited);
  let cleared = launches.clear_play_stats(Some("account")).unwrap();
  assert!(!cleared.accounts.contains_key("account"));
  assert_eq!(cleared.accounts[OFFLINE_ACCOUNT].launches, 1);
  assert!(launches.clear_play_stats(None).unwrap().accounts.is_empty());
}
