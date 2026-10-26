//! Bounded, local-only play statistics for desktop accounts and instances.
//!
//! The Runtime records one session when a client launch reaches the running
//! phase and finishes it when the game exits or is stopped. Aggregates are
//! stored per account and instance; the UI only reads them.

use crate::{Error, Result, storage};
use serde::{Deserialize, Serialize};
use std::{
  collections::BTreeMap,
  path::{Path, PathBuf},
  sync::{Mutex, MutexGuard},
  time::{SystemTime, UNIX_EPOCH},
};

pub const SCHEMA_VERSION: u32 = 1;
pub const OFFLINE_ACCOUNT: &str = "offline";
pub const MAX_ACCOUNTS: usize = 32;
pub const MAX_INSTANCES_PER_ACCOUNT: usize = 64;
pub const MAX_SESSIONS_PER_ACCOUNT: usize = 256;
pub const MAX_TOTAL_SESSIONS: usize = 2_048;
pub const MAX_RECENT_SESSIONS: usize = 20;
pub const FILE_LIMIT_BYTES: usize = 512 * 1024;
pub const MIN_COUNTED_SESSION_MS: u64 = 10_000;
pub const MAX_SESSION_MS: u64 = 24 * 60 * 60 * 1000;

const FILE_NAME: &str = "play-stats.json";
const BACKUP_NAME: &str = "play-stats.corrupt.json";
const WEEK_MS: u64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Clone, Debug)]
pub struct PlaySession {
  pub account_id: String,
  pub instance_id: String,
  pub started_at: u64,
  pub ended_at: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredInstance {
  #[serde(default)]
  total_ms: u64,
  #[serde(default)]
  launches: u64,
  #[serde(default)]
  last_played_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSession {
  instance_id: String,
  started_at: u64,
  ended_at: u64,
  duration_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAccount {
  // Optional for schema-1 files written before lifetime totals were persisted.
  #[serde(default)]
  total_ms: Option<u64>,
  #[serde(default)]
  launches: Option<u64>,
  #[serde(default)]
  instances: BTreeMap<String, StoredInstance>,
  #[serde(default)]
  sessions: Vec<StoredSession>,
}

impl StoredAccount {
  fn totals(&self) -> (u64, u64) {
    let (time, launches) = self.instances.values().fold((0u64, 0u64), |totals, value| {
      (
        totals.0.saturating_add(value.total_ms),
        totals.1.saturating_add(value.launches),
      )
    });
    (
      self.total_ms.unwrap_or(time).max(time),
      self.launches.unwrap_or(launches).max(launches),
    )
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayStatsFile {
  schema_version: u32,
  #[serde(default)]
  accounts: BTreeMap<String, StoredAccount>,
}

impl Default for PlayStatsFile {
  fn default() -> Self {
    Self {
      schema_version: SCHEMA_VERSION,
      accounts: BTreeMap::new(),
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayStatsSnapshot {
  pub schema_version: u32,
  pub accounts: BTreeMap<String, AccountPlayStats>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountPlayStats {
  pub total_ms: u64,
  pub launches: u64,
  pub last_played_at: Option<u64>,
  pub last_instance_id: Option<String>,
  #[serde(rename = "last7dMs")]
  pub last_7d_ms: u64,
  pub instances: BTreeMap<String, InstancePlayStats>,
  pub recent_sessions: Vec<PlaySessionSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstancePlayStats {
  pub total_ms: u64,
  pub launches: u64,
  pub last_played_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaySessionSummary {
  pub instance_id: String,
  pub started_at: u64,
  pub ended_at: u64,
  pub duration_ms: u64,
}

pub struct PlayStatsStore {
  path: PathBuf,
  inner: Mutex<PlayStatsFile>,
}

impl PlayStatsStore {
  /// Open the store under the Runtime data directory. A missing file starts
  /// empty; an unreadable or unsupported file is moved aside as
  /// `play-stats.corrupt.json` before starting empty. A data directory that
  /// cannot be created leaves the store empty and surfaces errors on write.
  pub fn open(data: &Path) -> Self {
    let _ = std::fs::create_dir_all(data);
    let path = data.join(FILE_NAME);
    let file = match load(&path) {
      Ok(file) => file,
      Err(LoadError::Missing) => PlayStatsFile::default(),
      Err(LoadError::Invalid) => {
        backup(&path);
        PlayStatsFile::default()
      }
    };
    Self {
      path,
      inner: Mutex::new(file),
    }
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  /// Record one completed launch session. Sub-ten-second sessions count as a
  /// launch but not as play time. Durations are capped to 24 hours.
  pub fn record(&self, session: PlaySession) -> Result<()> {
    if !valid_account(&session.account_id) {
      return Err(Error::Invalid("invalid play statistics account".into()));
    }
    if !valid_instance(&session.instance_id) {
      return Err(Error::Invalid("invalid play statistics instance".into()));
    }
    let ended_at = session.ended_at.max(session.started_at);
    let duration_ms = ended_at
      .saturating_sub(session.started_at)
      .min(MAX_SESSION_MS);
    let counted_ms = if duration_ms < MIN_COUNTED_SESSION_MS {
      0
    } else {
      duration_ms
    };
    let mut inner = self.lock();
    let mut next = inner.clone();
    let account = next.accounts.entry(session.account_id).or_default();
    let (time, launches) = account.totals();
    account.total_ms = Some(time.saturating_add(counted_ms));
    account.launches = Some(launches.saturating_add(1));
    let instance = account
      .instances
      .entry(session.instance_id.clone())
      .or_default();
    instance.total_ms = instance.total_ms.saturating_add(counted_ms);
    instance.launches = instance.launches.saturating_add(1);
    instance.last_played_at = instance.last_played_at.max(ended_at);
    account.sessions.push(StoredSession {
      instance_id: session.instance_id,
      started_at: session.started_at,
      ended_at,
      duration_ms: counted_ms,
    });
    Pruner.apply(&mut next);
    self.persist(&next)?;
    *inner = next;
    Ok(())
  }

  pub fn snapshot(&self) -> PlayStatsSnapshot {
    let now = now_ms();
    let inner = self.lock();
    PlayStatsSnapshot {
      schema_version: SCHEMA_VERSION,
      accounts: inner
        .accounts
        .iter()
        .map(|(id, account)| (id.clone(), account_stats(account, now)))
        .collect(),
    }
  }

  pub fn clear(&self) -> Result<()> {
    let mut inner = self.lock();
    let next = PlayStatsFile::default();
    self.persist(&next)?;
    *inner = next;
    Ok(())
  }

  /// Explicitly clear one account, including an account no longer signed in.
  pub fn clear_account(&self, account_id: &str) -> Result<()> {
    if !valid_account(account_id) {
      return Err(Error::Invalid("invalid play statistics account".into()));
    }
    let mut inner = self.lock();
    let mut next = inner.clone();
    if next.accounts.remove(account_id).is_none() {
      return Ok(());
    }
    Pruner.apply(&mut next);
    self.persist(&next)?;
    *inner = next;
    Ok(())
  }

  fn lock(&self) -> MutexGuard<'_, PlayStatsFile> {
    self
      .inner
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
  }

  fn persist(&self, file: &PlayStatsFile) -> Result<()> {
    storage::replace_json(&self.path, file)
  }
}

fn account_stats(account: &StoredAccount, now: u64) -> AccountPlayStats {
  let (total_ms, launches) = account.totals();
  let mut last_played_at = 0u64;
  let mut last_instance_id: Option<String> = None;
  for (id, instance) in &account.instances {
    let newer = instance.last_played_at > last_played_at
      || (instance.last_played_at == last_played_at
        && instance.last_played_at > 0
        && last_instance_id
          .as_deref()
          .is_none_or(|current| id.as_str() < current));
    if newer {
      last_played_at = instance.last_played_at;
      last_instance_id = Some(id.clone());
    }
  }
  let cutoff = now.saturating_sub(WEEK_MS);
  let last_7d_ms = account
    .sessions
    .iter()
    .filter(|session| session.ended_at >= cutoff)
    .fold(0u64, |total, session| {
      total.saturating_add(session.duration_ms)
    });
  let mut recent_sessions: Vec<_> = account
    .sessions
    .iter()
    .map(PlaySessionSummary::from)
    .collect();
  recent_sessions.sort_by(|a, b| {
    b.started_at
      .cmp(&a.started_at)
      .then_with(|| b.ended_at.cmp(&a.ended_at))
  });
  recent_sessions.truncate(MAX_RECENT_SESSIONS);
  AccountPlayStats {
    total_ms,
    launches,
    last_played_at: (last_played_at > 0).then_some(last_played_at),
    last_instance_id,
    last_7d_ms,
    instances: account
      .instances
      .iter()
      .map(|(id, instance)| {
        (
          id.clone(),
          InstancePlayStats {
            total_ms: instance.total_ms,
            launches: instance.launches,
            last_played_at: instance.last_played_at,
          },
        )
      })
      .collect(),
    recent_sessions,
  }
}

impl From<&StoredSession> for PlaySessionSummary {
  fn from(session: &StoredSession) -> Self {
    Self {
      instance_id: session.instance_id.clone(),
      started_at: session.started_at,
      ended_at: session.ended_at,
      duration_ms: session.duration_ms,
    }
  }
}

#[derive(Default)]
struct Pruner;

impl Pruner {
  fn apply(&self, file: &mut PlayStatsFile) {
    for account in file.accounts.values_mut() {
      account
        .instances
        .retain(|id, instance| valid_instance(id) && instance.launches > 0);
      let (time, launches) = account.totals();
      account.total_ms = Some(time);
      account.launches = Some(launches);
      account.sessions.retain(|session| {
        valid_instance(&session.instance_id)
          && account.instances.contains_key(&session.instance_id)
          && session.started_at <= session.ended_at
      });
      for session in &mut account.sessions {
        session.ended_at = session.ended_at.max(session.started_at);
        session.duration_ms = session
          .duration_ms
          .min(session.ended_at.saturating_sub(session.started_at))
          .min(MAX_SESSION_MS);
      }
      account.sessions.sort_by_key(|session| session.started_at);
      if account.sessions.len() > MAX_SESSIONS_PER_ACCOUNT {
        let excess = account.sessions.len() - MAX_SESSIONS_PER_ACCOUNT;
        account.sessions.drain(0..excess);
      }
      if account.instances.len() > MAX_INSTANCES_PER_ACCOUNT {
        let mut ids: Vec<_> = account
          .instances
          .iter()
          .map(|(id, instance)| (id.clone(), instance.last_played_at))
          .collect();
        ids.sort_by_key(|(id, last)| (*last, id.clone()));
        let remove = ids.len() - MAX_INSTANCES_PER_ACCOUNT;
        for (id, _) in ids.into_iter().take(remove) {
          account.instances.remove(&id);
        }
        account
          .sessions
          .retain(|session| account.instances.contains_key(&session.instance_id));
      }
    }
    prune_total_sessions(file);
    file.accounts.retain(|account, value| {
      valid_account(account) && (!value.instances.is_empty() || !value.sessions.is_empty())
    });
    if file.accounts.len() > MAX_ACCOUNTS {
      let mut ids: Vec<_> = file
        .accounts
        .iter()
        .map(|(id, account)| (id.clone(), last_played(account)))
        .collect();
      ids.sort_by_key(|(id, last)| (*last, id.clone()));
      let remove = ids.len() - MAX_ACCOUNTS;
      for (id, _) in ids.into_iter().take(remove) {
        file.accounts.remove(&id);
      }
    }
  }
}

fn prune_total_sessions(file: &mut PlayStatsFile) {
  let mut total: usize = file
    .accounts
    .values()
    .map(|account| account.sessions.len())
    .sum();
  while total > MAX_TOTAL_SESSIONS {
    let victim = file
      .accounts
      .iter()
      .filter_map(|(id, account)| {
        account
          .sessions
          .first()
          .map(|session| (session.started_at, id.clone()))
      })
      .min();
    let Some((_, id)) = victim else {
      break;
    };
    let Some(account) = file.accounts.get_mut(&id) else {
      break;
    };
    account.sessions.remove(0);
    total -= 1;
  }
}

fn last_played(account: &StoredAccount) -> u64 {
  account
    .instances
    .values()
    .map(|instance| instance.last_played_at)
    .max()
    .unwrap_or(0)
}

fn normalize(mut file: PlayStatsFile) -> PlayStatsFile {
  for account in file.accounts.values_mut() {
    account.instances.retain(|id, _| valid_instance(id));
    account
      .sessions
      .retain(|session| valid_instance(&session.instance_id));
  }
  Pruner.apply(&mut file);
  file
}

fn valid_account(id: &str) -> bool {
  !id.is_empty()
    && id.len() <= 64
    && id
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn valid_instance(id: &str) -> bool {
  !id.is_empty()
    && id.len() <= 64
    && id
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|duration| duration.as_millis() as u64)
    .unwrap_or(0)
}

enum LoadError {
  Missing,
  Invalid,
}

fn load(path: &Path) -> std::result::Result<PlayStatsFile, LoadError> {
  match std::fs::symlink_metadata(path) {
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(LoadError::Missing),
    Err(_) => return Err(LoadError::Invalid),
    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
      return Err(LoadError::Invalid);
    }
    Ok(_) => {}
  }
  let file: PlayStatsFile =
    storage::read_json_bounded(path, FILE_LIMIT_BYTES).map_err(|_| LoadError::Invalid)?;
  if file.schema_version != SCHEMA_VERSION {
    return Err(LoadError::Invalid);
  }
  Ok(normalize(file))
}

fn backup(path: &Path) {
  let Ok(metadata) = std::fs::symlink_metadata(path) else {
    return;
  };
  if !metadata.file_type().is_file() {
    return;
  }
  let backup = path.with_file_name(BACKUP_NAME);
  let _ = std::fs::remove_file(&backup);
  if std::fs::rename(path, &backup).is_err()
    && let Ok(bytes) = std::fs::read(path)
  {
    let _ = std::fs::write(&backup, bytes);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn store() -> (tempfile::TempDir, PlayStatsStore) {
    let root = tempfile::tempdir().unwrap();
    let store = PlayStatsStore::open(root.path());
    (root, store)
  }

  fn session(account: &str, instance: &str, start: u64, duration: u64) -> PlaySession {
    PlaySession {
      account_id: account.into(),
      instance_id: instance.into(),
      started_at: start,
      ended_at: start + duration,
    }
  }

  #[test]
  fn records_per_account_and_instance() {
    let (_root, store) = store();
    store
      .record(session(
        "11111111-1111-1111-1111-111111111111",
        "survival",
        1_000,
        60_000,
      ))
      .unwrap();
    store
      .record(session(
        "11111111-1111-1111-1111-111111111111",
        "survival",
        2_000,
        30_000,
      ))
      .unwrap();
    store
      .record(session(
        "22222222-2222-2222-2222-222222222222",
        "creative",
        3_000,
        20_000,
      ))
      .unwrap();
    let snapshot = store.snapshot();
    let first = &snapshot.accounts["11111111-1111-1111-1111-111111111111"];
    assert_eq!(first.total_ms, 90_000);
    assert_eq!(first.launches, 2);
    assert_eq!(first.last_played_at, Some(61_000));
    assert_eq!(first.last_instance_id.as_deref(), Some("survival"));
    assert_eq!(first.instances["survival"].total_ms, 90_000);
    assert_eq!(first.recent_sessions.len(), 2);
    assert_eq!(first.recent_sessions[0].started_at, 2_000);
    let second = &snapshot.accounts["22222222-2222-2222-2222-222222222222"];
    assert_eq!(second.total_ms, 20_000);
    assert_eq!(second.instances["creative"].launches, 1);
  }

  #[test]
  fn short_sessions_count_as_launches_but_not_play_time() {
    let (_root, store) = store();
    store
      .record(session("account", "short", 1_000, 9_999))
      .unwrap();
    let account = &store.snapshot().accounts["account"];
    assert_eq!(account.total_ms, 0);
    assert_eq!(account.launches, 1);
    assert_eq!(account.recent_sessions[0].duration_ms, 0);
  }

  #[test]
  fn caps_duration_and_prunes_old_sessions() {
    let (_root, store) = store();
    store
      .record(session("account", "survival", 1_000, MAX_SESSION_MS * 2))
      .unwrap();
    for index in 0..(MAX_SESSIONS_PER_ACCOUNT + 5) {
      store
        .record(session(
          "account",
          "survival",
          10_000 + index as u64,
          20_000,
        ))
        .unwrap();
    }
    let snapshot = store.snapshot();
    let account = &snapshot.accounts["account"];
    assert_eq!(
      account.total_ms,
      MAX_SESSION_MS + 20_000 * (MAX_SESSIONS_PER_ACCOUNT as u64 + 5)
    );
    assert_eq!(account.recent_sessions.len(), MAX_RECENT_SESSIONS);
  }

  #[test]
  fn reloads_persisted_values_and_clears() {
    let (root, store) = store();
    store
      .record(session("account", "survival", 1_000, 60_000))
      .unwrap();
    drop(store);
    let reopened = PlayStatsStore::open(root.path());
    assert_eq!(reopened.snapshot().accounts["account"].total_ms, 60_000);
    reopened.clear().unwrap();
    assert!(reopened.snapshot().accounts.is_empty());
    assert_eq!(
      PlayStatsStore::open(root.path()).snapshot().accounts.len(),
      0
    );
  }

  #[test]
  fn clears_only_the_requested_account_and_preserves_other_history() {
    let (root, store) = store();
    store
      .record(session("one", "survival", 1_000, 60_000))
      .unwrap();
    store
      .record(session("two", "survival", 2_000, 60_000))
      .unwrap();
    store
      .record(session("two", "creative", 3_000, 60_000))
      .unwrap();
    store
      .record(session(OFFLINE_ACCOUNT, "survival", 4_000, 60_000))
      .unwrap();
    store.clear_account("one").unwrap();
    let snapshot = store.snapshot();
    assert!(!snapshot.accounts.contains_key("one"));
    assert_eq!(snapshot.accounts["two"].total_ms, 120_000);
    assert!(snapshot.accounts["two"].instances.contains_key("survival"));
    assert_eq!(snapshot.accounts[OFFLINE_ACCOUNT].launches, 1);
    assert_eq!(
      snapshot.accounts["two"].last_instance_id.as_deref(),
      Some("creative")
    );
    assert_eq!(PlayStatsStore::open(root.path()).snapshot(), snapshot);
    assert!(store.clear_account("").is_err());
    assert_eq!(store.snapshot(), snapshot);
  }

  #[test]
  fn lifetime_totals_survive_instance_history_limits_and_reload() {
    let (root, store) = store();
    for index in 0..=MAX_INSTANCES_PER_ACCOUNT {
      store
        .record(session(
          "account",
          &format!("world-{index}"),
          1_000 + index as u64,
          60_000,
        ))
        .unwrap();
    }
    let snapshot = PlayStatsStore::open(root.path()).snapshot();
    let account = &snapshot.accounts["account"];
    assert_eq!(account.instances.len(), MAX_INSTANCES_PER_ACCOUNT);
    assert_eq!(account.launches, MAX_INSTANCES_PER_ACCOUNT as u64 + 1);
    assert_eq!(
      account.total_ms,
      (MAX_INSTANCES_PER_ACCOUNT as u64 + 1) * 60_000
    );
  }

  #[test]
  fn reads_legacy_totals_and_persists_them_on_the_next_record() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
      root.path().join(FILE_NAME),
      serde_json::to_vec(&serde_json::json!({
        "schemaVersion": 1,
        "accounts": { "account": { "instances": {
          "survival": { "totalMs": 60_000, "launches": 1, "lastPlayedAt": 61_000 }
        }, "sessions": [] } }
      }))
      .unwrap(),
    )
    .unwrap();
    let store = PlayStatsStore::open(root.path());
    assert_eq!(store.snapshot().accounts["account"].total_ms, 60_000);
    store
      .record(session("account", "creative", 70_000, 60_000))
      .unwrap();
    let reloaded = PlayStatsStore::open(root.path()).snapshot();
    assert_eq!(reloaded.accounts["account"].total_ms, 120_000);
    assert_eq!(reloaded.accounts["account"].launches, 2);
  }

  #[test]
  fn corrupt_file_is_preserved_before_starting_empty() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(FILE_NAME), b"{not json").unwrap();
    let store = PlayStatsStore::open(root.path());
    assert!(store.snapshot().accounts.is_empty());
    assert_eq!(
      std::fs::read(root.path().join(BACKUP_NAME)).unwrap(),
      b"{not json"
    );
  }

  #[test]
  fn seven_day_window_is_rolling() {
    let (_root, store) = store();
    let now = now_ms();
    store
      .record(session(
        "account",
        "old",
        now - 8 * 24 * 60 * 60 * 1000,
        60_000,
      ))
      .unwrap();
    store
      .record(session("account", "recent", now - 60 * 60 * 1000, 60_000))
      .unwrap();
    assert_eq!(store.snapshot().accounts["account"].last_7d_ms, 60_000);
  }

  #[test]
  fn bounded_data_stays_below_the_read_limit() {
    let mut file = PlayStatsFile::default();
    for account_index in 0..MAX_ACCOUNTS {
      let account = file
        .accounts
        .entry(format!("account{account_index}"))
        .or_default();
      for instance_index in 0..MAX_INSTANCES_PER_ACCOUNT {
        let instance = format!("instance{instance_index}");
        account.instances.insert(
          instance.clone(),
          StoredInstance {
            total_ms: 60_000,
            launches: 1,
            last_played_at: 1_000,
          },
        );
        for session_index in 0..MAX_SESSIONS_PER_ACCOUNT {
          let started_at = (account_index * MAX_SESSIONS_PER_ACCOUNT + session_index) as u64;
          account.sessions.push(StoredSession {
            instance_id: instance.clone(),
            started_at,
            ended_at: started_at + 60_000,
            duration_ms: 60_000,
          });
        }
      }
    }
    Pruner.apply(&mut file);
    let bytes = serde_json::to_vec(&file).unwrap();
    assert!(bytes.len() < FILE_LIMIT_BYTES, "{} bytes", bytes.len());
    assert!(
      file
        .accounts
        .values()
        .map(|account| account.sessions.len())
        .sum::<usize>()
        <= MAX_TOTAL_SESSIONS
    );
  }

  #[test]
  fn rejects_invalid_identities() {
    let (_root, store) = store();
    assert!(matches!(
      store.record(session("../account", "survival", 1_000, 60_000)),
      Err(Error::Invalid(_))
    ));
    assert!(matches!(
      store.record(session("account", "../instance", 1_000, 60_000)),
      Err(Error::Invalid(_))
    ));
  }
}
