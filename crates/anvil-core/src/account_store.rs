//! Persistent, non-secret account profiles shared by CLI and desktop clients.
use std::{
  fs::{self, File},
  io::Read,
  path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{Error, Result, account::AccountProfile};

const ACCOUNTS_FILE: &str = "accounts.json";
const ACCOUNTS_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountFile {
  schema_version: u32,
  accounts: Vec<AccountProfile>,
  selected_account: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AccountEntry {
  pub profile: AccountProfile,
  pub selected: bool,
}

impl AccountFile {
  pub fn entries(&self) -> Vec<AccountEntry> {
    self
      .accounts
      .iter()
      .cloned()
      .map(|profile| AccountEntry {
        selected: self
          .selected_account
          .as_deref()
          .is_some_and(|id| same_account_id(id, profile.id())),
        profile,
      })
      .collect()
  }

  pub fn selected(&self) -> Option<&AccountProfile> {
    let selected = self.selected_account.as_deref()?;
    self
      .accounts
      .iter()
      .find(|profile| same_account_id(profile.id(), selected))
  }

  pub fn find(&self, id: &str) -> Option<&AccountProfile> {
    self
      .accounts
      .iter()
      .find(|profile| same_account_id(profile.id(), id))
  }
}

/// Load the public account identities from the application data directory.
pub fn load(data: &Path) -> Result<AccountFile> {
  let path = data.join(ACCOUNTS_FILE);
  let metadata = match fs::symlink_metadata(&path) {
    Ok(metadata) => metadata,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
      return Ok(AccountFile {
        schema_version: 1,
        ..AccountFile::default()
      });
    }
    Err(error) => return Err(error.into()),
  };
  if metadata.file_type().is_symlink()
    || !metadata.is_file()
    || metadata.len() > ACCOUNTS_LIMIT as u64
  {
    return Err(Error::Conflict("account profile file is invalid".into()));
  }
  let mut bytes = Vec::with_capacity(metadata.len() as usize);
  File::open(path)?
    .take(ACCOUNTS_LIMIT as u64 + 1)
    .read_to_end(&mut bytes)?;
  if bytes.len() > ACCOUNTS_LIMIT {
    return Err(Error::Conflict(
      "account profile file exceeds its size limit".into(),
    ));
  }
  let file: AccountFile = serde_json::from_slice(&bytes)?;
  validate(&file)?;
  Ok(file)
}

/// Add or replace an account profile and make it the selected profile.
pub fn upsert(data: &Path, profile: AccountProfile) -> Result<AccountFile> {
  profile.validate()?;
  let mut file = load(data)?;
  if file.schema_version != 1 {
    return Err(Error::Conflict("unsupported account profile schema".into()));
  }
  file
    .accounts
    .retain(|existing| !same_account_id(existing.id(), profile.id()));
  file.accounts.push(profile.clone());
  file
    .accounts
    .sort_by(|left, right| left.id().cmp(right.id()));
  file.selected_account = Some(profile.id().to_owned());
  write(data, &file)?;
  Ok(file)
}

/// Select one of the profiles already present in the account file.
pub fn select(data: &Path, id: &str) -> Result<AccountFile> {
  let mut file = load(data)?;
  if file.schema_version != 1 {
    return Err(Error::Conflict("unsupported account profile schema".into()));
  }
  let profile = file
    .find(id)
    .ok_or_else(|| Error::NotFound("Minecraft account profile".into()))?;
  file.selected_account = Some(profile.id().to_owned());
  write(data, &file)?;
  Ok(file)
}

/// Remove a profile and select the first remaining profile when needed.
pub fn remove(data: &Path, id: &str) -> Result<AccountFile> {
  let mut file = load(data)?;
  if file.schema_version != 1 {
    return Err(Error::Conflict("unsupported account profile schema".into()));
  }
  file
    .accounts
    .retain(|profile| !same_account_id(profile.id(), id));
  if file
    .selected_account
    .as_deref()
    .is_some_and(|selected| same_account_id(selected, id))
  {
    file.selected_account = file.accounts.first().map(|profile| profile.id().to_owned());
  }
  write(data, &file)?;
  Ok(file)
}

fn validate(file: &AccountFile) -> Result<()> {
  if file.schema_version != 1 || file.accounts.len() > 10_000 {
    return Err(Error::Conflict(
      "unsupported or invalid account profile file".into(),
    ));
  }
  let mut ids = std::collections::HashSet::new();
  for profile in &file.accounts {
    profile.validate()?;
    let id = normalized_id(profile.id());
    if !ids.insert(id) {
      return Err(Error::Conflict(
        "duplicate Minecraft account profile".into(),
      ));
    }
  }
  if file.selected_account.as_deref().is_some_and(|id| {
    !file
      .accounts
      .iter()
      .any(|profile| same_account_id(profile.id(), id))
  }) {
    return Err(Error::Conflict(
      "selected Minecraft account does not exist".into(),
    ));
  }
  Ok(())
}

fn write(data: &Path, file: &AccountFile) -> Result<()> {
  fs::create_dir_all(data)?;
  if !fs::metadata(data)?.is_dir() {
    return Err(Error::Conflict(
      "application data root is not a directory".into(),
    ));
  }
  let path = data.join(ACCOUNTS_FILE);
  match fs::symlink_metadata(&path) {
    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
      return Err(Error::Conflict("account profile file is invalid".into()));
    }
    Ok(_) => {}
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => return Err(error.into()),
  }
  let mut temporary = tempfile::NamedTempFile::new_in(data)?;
  serde_json::to_writer_pretty(&mut temporary, file)?;
  use std::io::Write;
  temporary.write_all(b"\n")?;
  temporary.as_file().sync_all()?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    temporary
      .as_file()
      .set_permissions(fs::Permissions::from_mode(0o600))?;
  }
  temporary
    .persist(path)
    .map_err(|error| Error::Io(error.error))?;
  #[cfg(unix)]
  File::open(data)?.sync_all()?;
  Ok(())
}

fn same_account_id(left: &str, right: &str) -> bool {
  normalized_id(left) == normalized_id(right)
}

fn normalized_id(id: &str) -> String {
  id.bytes()
    .filter(|byte| *byte != b'-')
    .map(|byte| byte.to_ascii_lowercase() as char)
    .collect()
}

#[cfg(test)]
#[path = "../tests/account_store.rs"]
mod tests;
