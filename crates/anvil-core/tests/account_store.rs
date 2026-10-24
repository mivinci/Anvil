use std::fs;

use super::*;
use crate::account::AccountProfile;

#[test]
fn profiles_are_upserted_selected_and_removed_without_tokens() {
  let root = tempfile::tempdir().unwrap();
  let first = AccountProfile::new("00000000-0000-0000-0000-000000000001", "First", None).unwrap();
  let second = AccountProfile::new("00000000-0000-0000-0000-000000000002", "Second", None).unwrap();

  let file = upsert(root.path(), first.clone()).unwrap();
  assert_eq!(file.selected().unwrap(), &first);
  let file = upsert(root.path(), second.clone()).unwrap();
  assert_eq!(file.selected().unwrap(), &second);
  assert_eq!(file.entries().len(), 2);
  assert!(file.entries().iter().any(|entry| entry.selected));

  let file = remove(root.path(), second.id()).unwrap();
  assert_eq!(file.entries().len(), 1);
  assert_eq!(file.selected().unwrap(), &first);
  let serialized = fs::read_to_string(root.path().join("accounts.json")).unwrap();
  assert!(!serialized.contains("refresh_token"));
}
