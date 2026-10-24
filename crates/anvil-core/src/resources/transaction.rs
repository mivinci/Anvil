//! Recoverable, instance-private mod and resource-pack materialization.
use super::*;

const JOURNAL: &str = "resource-transaction.json";
const WORK: &str = ".anvil-resource-work";
const LAYOUT: &str = "resource-layout.json";
const MAX_JOURNAL_BYTES: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Layout {
  pub(super) mods: Vec<ResourceEntry>,
  #[serde(
    default,
    rename = "resourcePacks",
    skip_serializing_if = "Vec::is_empty"
  )]
  pub(super) resource_packs: Vec<ResourcePackEntry>,
}
#[derive(
  Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
enum Kind {
  #[default]
  Mod,
  ResourcePack,
}
impl Kind {
  fn directory(self, root: &Path) -> PathBuf {
    root.join(match self {
      Self::Mod => "game/mods",
      Self::ResourcePack => "game/resourcepacks",
    })
  }
  fn validate_filename(self, filename: &str) -> Result<()> {
    match self {
      Self::Mod => validate_filename(filename),
      Self::ResourcePack => validate_resource_filename(filename, "zip"),
    }
  }
}
// Untagged entries preserve the old mod-only journal representation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
enum Entry {
  Mod(ResourceEntry),
  Pack(ResourcePackEntry),
}
impl Entry {
  fn kind(&self) -> Kind {
    match self {
      Self::Mod(_) => Kind::Mod,
      Self::Pack(_) => Kind::ResourcePack,
    }
  }
  fn filename(&self) -> &str {
    match self {
      Self::Mod(e) => &e.filename,
      Self::Pack(e) => &e.filename,
    }
  }
  fn hash(&self) -> &str {
    match self {
      Self::Mod(e) => &e.sha256,
      Self::Pack(e) => &e.sha256,
    }
  }
  fn size(&self) -> u64 {
    match self {
      Self::Mod(e) => e.size,
      Self::Pack(e) => e.size,
    }
  }
}
impl Layout {
  fn entries(&self, kind: Kind) -> Vec<Entry> {
    match kind {
      Kind::Mod => self.mods.iter().cloned().map(Entry::Mod).collect(),
      Kind::ResourcePack => self
        .resource_packs
        .iter()
        .cloned()
        .map(Entry::Pack)
        .collect(),
    }
  }
  fn entry(&self, kind: Kind, filename: &str) -> Option<Entry> {
    match kind {
      Kind::Mod => self
        .mods
        .iter()
        .find(|e| e.filename == filename)
        .cloned()
        .map(Entry::Mod),
      Kind::ResourcePack => self
        .resource_packs
        .iter()
        .find(|e| e.filename == filename)
        .cloned()
        .map(Entry::Pack),
    }
  }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
  #[serde(default)]
  kind: Kind,
  filename: String,
  before: Option<Entry>,
  after: Option<Entry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
  schema_version: u32,
  before: ResourceManifest,
  after: ResourceManifest,
  old_layout: Layout,
  new_layout: Layout,
  changes: Vec<Change>,
  #[serde(default)]
  committed: bool,
}
fn exists(path: &Path) -> Result<bool> {
  match fs::symlink_metadata(path) {
    Ok(_) => Ok(true),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
    Err(error) => Err(error.into()),
  }
}
fn sync_directory(_path: &Path) -> Result<()> {
  #[cfg(unix)]
  File::open(_path)?.sync_all()?;
  Ok(())
}
fn validate_layout(layout: &Layout) -> Result<()> {
  let manifest = ResourceManifest {
    schema_version: 4,
    revision: 0,
    applied_revision: None,
    mods: layout.mods.clone(),
    resource_packs: layout.resource_packs.clone(),
  };
  super::platform::validate_layout_entries(&manifest)?;
  if layout.mods.iter().any(|entry| !entry.enabled) {
    return Err(Error::Conflict(
      "disabled entry in managed mod layout".into(),
    ));
  }
  Ok(())
}
pub(super) fn read_layout(directory: &Path) -> Result<Layout> {
  let path = directory.join(LAYOUT);
  if !exists(&path)? {
    return Ok(Layout::default());
  }
  regular_file(&path, MAX_RECORD_BYTES as u64)?;
  let layout = storage::read_json_bounded(&path, MAX_RECORD_BYTES)?;
  validate_layout(&layout)?;
  Ok(layout)
}
fn desired(manifest: &ResourceManifest) -> Layout {
  Layout {
    mods: manifest
      .mods
      .iter()
      .filter(|entry| entry.enabled)
      .cloned()
      .collect(),
    resource_packs: manifest.resource_packs.clone(),
  }
}

/// Caller holds the same instance lock that remains held for the game session.
pub(crate) fn sync_for_launch(
  directory: &Path,
  data: &Path,
  fabric: bool,
  cancel: &crate::CancellationToken,
) -> Result<()> {
  let _objects = storage::client_cache_lock(data, "resources-store")?;
  recover(directory)?;
  let mut manifest = read_manifest(directory)?;
  let layout = read_layout(directory)?;
  if !fabric && (manifest.mods.iter().any(|entry| entry.enabled) || !layout.mods.is_empty()) {
    return Err(Error::Conflict(
      "enabled managed mods require Fabric; disable them before launching vanilla".into(),
    ));
  }
  // Vanilla instances without managed game files need no new resource record.
  if !fabric && manifest.resource_packs.is_empty() && layout.resource_packs.is_empty() {
    return Ok(());
  }
  // Reconcile and verify even when the revision was previously applied: local
  // files can have been removed or replaced since the last resource operation.
  let before = manifest.clone();
  manifest.applied_revision = Some(manifest.revision);
  transact(directory, data, &before, &manifest, cancel)
}

pub(super) fn transact(
  directory: &Path,
  data: &Path,
  before: &ResourceManifest,
  after: &ResourceManifest,
  cancel: &crate::CancellationToken,
) -> Result<()> {
  validate_manifest(before)?;
  validate_manifest(after)?;
  let old_layout = read_layout(directory)?;
  let new_layout = desired(after);
  ensure_directory(&directory.join("game"), false)?;
  let mut changes = Vec::new();
  for kind in [Kind::Mod, Kind::ResourcePack] {
    let old_entries = old_layout.entries(kind);
    let new_entries = new_layout.entries(kind);
    let folder = kind.directory(directory);
    if !old_entries.is_empty() || !new_entries.is_empty() || exists(&folder)? {
      ensure_directory(&folder, true)?;
      sync_directory(&directory.join("game"))?;
    }
    let mut names = std::collections::BTreeSet::new();
    let mut folded = std::collections::HashMap::new();
    for entry in old_entries.iter().chain(&new_entries) {
      if let Some(previous) = folded.insert(entry.filename().to_ascii_lowercase(), entry.filename())
        && previous != entry.filename()
      {
        return Err(Error::Conflict(
          "managed resource filenames differ only by case".into(),
        ));
      }
      names.insert(entry.filename().to_owned());
    }
    for filename in names {
      crate::error::check_cancel(cancel)?;
      let old = old_entries.iter().find(|e| e.filename() == filename);
      let new = new_entries.iter().find(|e| e.filename() == filename);
      let target = folder.join(&filename);
      let present = exists(&target)?;
      if present {
        let owned = old.ok_or_else(|| {
          Error::Conflict(
            "an external resource already uses the requested filename; it was not overwritten"
              .into(),
          )
        })?;
        verify_file(&target, owned.hash(), owned.size())?;
      }
      if let Some(entry) = new {
        verify_object(&object_path(data, entry.hash()), entry.hash(), entry.size())?;
      }
      if (old != new || (new.is_some() && !present)) && (present || new.is_some()) {
        changes.push(Change {
          kind,
          filename,
          before: old.filter(|_| present).cloned(),
          after: new.cloned(),
        });
      }
    }
  }
  crate::error::check_cancel(cancel)?;
  if changes.is_empty() && old_layout == new_layout {
    if before != after {
      storage::replace_json(&directory.join("resources.json"), after)?;
    }
    return Ok(());
  }
  if exists(&directory.join(WORK))? || exists(&directory.join(JOURNAL))? {
    return Err(Error::Conflict(
      "resource transaction needs recovery before making changes".into(),
    ));
  }
  let mut journal = Journal {
    schema_version: 2,
    before: before.clone(),
    after: after.clone(),
    old_layout,
    new_layout,
    changes,
    committed: false,
  };
  if serde_json::to_vec_pretty(&journal)?.len() > MAX_JOURNAL_BYTES {
    return Err(Error::Conflict(
      "resource transaction exceeds journal limit".into(),
    ));
  }
  storage::replace_json(&directory.join(JOURNAL), &journal)?;
  let result = apply_transaction(directory, data, &mut journal, cancel);
  match result {
    Ok(()) => cleanup(directory),
    Err(error) => {
      recover(directory).map_err(|_| Error::Conflict("resource recovery could not complete; preserve transaction files and retry after resolving the conflict".into()))?;
      Err(error)
    }
  }
}
fn apply_transaction(
  directory: &Path,
  data: &Path,
  journal: &mut Journal,
  cancel: &crate::CancellationToken,
) -> Result<()> {
  let work = directory.join(WORK);
  fs::create_dir(&work)?;
  fs::create_dir(work.join("new"))?;
  fs::create_dir(work.join("backup"))?;
  for (index, change) in journal.changes.iter().enumerate() {
    crate::error::check_cancel(cancel)?;
    if let Some(entry) = &change.after {
      let source = object_path(data, entry.hash());
      verify_object(&source, entry.hash(), entry.size())?;
      let staged = work.join("new").join(index.to_string());
      // Independent copies isolate writes from the shared resource original.
      let mut input = File::open(&source)?;
      let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)?;
      let copied = std::io::copy(
        &mut std::io::Read::by_ref(&mut input).take(entry.size() + 1),
        &mut output,
      )?;
      if copied != entry.size() {
        return Err(Error::Conflict(
          "resource changed while preparing private copy".into(),
        ));
      }
      output.sync_all()?;
      verify_file(&staged, entry.hash(), entry.size())?;
    }
  }
  sync_directory(&work.join("new"))?;
  sync_directory(&work)?;
  sync_directory(directory)?;
  for (index, change) in journal.changes.iter().enumerate() {
    crate::error::check_cancel(cancel)?;
    let target = change.kind.directory(directory).join(&change.filename);
    if let Some(entry) = &change.before {
      verify_file(&target, entry.hash(), entry.size())?;
      File::open(&target)?.sync_all()?;
      fs::rename(&target, work.join("backup").join(index.to_string()))?;
      sync_directory(&work.join("backup"))?;
      sync_directory(&change.kind.directory(directory))?;
    }
    if change.after.is_some() {
      if exists(&target)? {
        return Err(Error::Conflict(
          "resource target changed during transaction".into(),
        ));
      }
      let staged = work.join("new").join(index.to_string());
      publish_private(&staged, &target)?;
      sync_directory(&change.kind.directory(directory))?;
      sync_directory(&work.join("new"))?;
    }
  }
  storage::replace_json(&directory.join(LAYOUT), &journal.new_layout)?;
  // Commit point: files and ownership layout precede the instance manifest.
  storage::replace_json(&directory.join("resources.json"), &journal.after)?;
  // A file repair can have identical before/after manifests. Persist its commit
  // decision separately so recovery never mistakes a half-applied repair for success.
  journal.committed = true;
  storage::replace_json(&directory.join(JOURNAL), journal)?;
  Ok(())
}
fn validate_journal(journal: &Journal) -> Result<()> {
  if !matches!(journal.schema_version, 1 | 2) || journal.changes.len() > 2 * MAX_ENTRIES {
    return Err(Error::Conflict("invalid resource transaction".into()));
  }
  if journal.schema_version == 1
    && (!journal.before.resource_packs.is_empty()
      || !journal.after.resource_packs.is_empty()
      || !journal.old_layout.resource_packs.is_empty()
      || !journal.new_layout.resource_packs.is_empty()
      || journal
        .changes
        .iter()
        .any(|change| change.kind != Kind::Mod))
  {
    return Err(Error::Conflict(
      "invalid legacy resource transaction".into(),
    ));
  }
  validate_manifest(&journal.before)?;
  validate_manifest(&journal.after)?;
  validate_layout(&journal.old_layout)?;
  validate_layout(&journal.new_layout)?;
  if journal.new_layout != desired(&journal.after)
    || journal.after.applied_revision != Some(journal.after.revision)
  {
    return Err(Error::Conflict(
      "resource transaction layout does not match its manifest".into(),
    ));
  }
  let mut names = HashSet::new();
  for change in &journal.changes {
    change.kind.validate_filename(&change.filename)?;
    if !names.insert((change.kind, change.filename.to_ascii_lowercase()))
      || (change.before.is_none() && change.after.is_none())
      || change
        .before
        .iter()
        .chain(change.after.iter())
        .any(|entry| entry.kind() != change.kind || entry.filename() != change.filename)
      || change.before.as_ref().is_some_and(|entry| {
        journal
          .old_layout
          .entry(change.kind, &change.filename)
          .as_ref()
          != Some(entry)
      })
      || journal
        .new_layout
        .entry(change.kind, &change.filename)
        .as_ref()
        != change.after.as_ref()
    {
      return Err(Error::Conflict("invalid resource transaction entry".into()));
    }
  }
  Ok(())
}
pub(super) fn recover(directory: &Path) -> Result<()> {
  let path = directory.join(JOURNAL);
  if !exists(&path)? {
    return Ok(());
  }
  regular_file(&path, MAX_JOURNAL_BYTES as u64)?;
  let journal: Journal = storage::read_json_bounded(&path, MAX_JOURNAL_BYTES)?;
  validate_journal(&journal)?;
  ensure_directory(&directory.join("game"), false)?;
  for kind in [Kind::Mod, Kind::ResourcePack] {
    let folder = kind.directory(directory);
    if !journal.old_layout.entries(kind).is_empty()
      || !journal.new_layout.entries(kind).is_empty()
      || journal.changes.iter().any(|change| change.kind == kind)
      || exists(&folder)?
    {
      ensure_directory(&folder, false)?;
    }
  }
  let work = directory.join(WORK);
  if exists(&work)? {
    ensure_directory(&work, false)?;
    for part in ["new", "backup"] {
      if exists(&work.join(part))? {
        ensure_directory(&work.join(part), false)?;
      }
    }
  }
  let current = read_manifest(directory)?;
  if journal.committed && current != journal.after {
    return Err(Error::Conflict(
      "committed resource manifest changed outside the transaction".into(),
    ));
  }
  if current == journal.after
    && (current != journal.before || journal.schema_version == 1 || journal.committed)
  {
    // Already committed. Do not roll back a successful manifest on restart.
    for change in &journal.changes {
      let target = change.kind.directory(directory).join(&change.filename);
      if let Some(entry) = &change.after {
        verify_file(&target, entry.hash(), entry.size())?;
      } else if exists(&target)? {
        return Err(Error::Conflict(
          "external file appeared in committed resource layout".into(),
        ));
      }
    }
    storage::replace_json(&directory.join(LAYOUT), &journal.new_layout)?;
  } else if current == journal.before {
    for (index, change) in journal.changes.iter().enumerate().rev() {
      let target = change.kind.directory(directory).join(&change.filename);
      let backup = work.join("backup").join(index.to_string());
      if exists(&backup)? {
        let entry = change
          .before
          .as_ref()
          .ok_or_else(|| Error::Conflict("unexpected resource backup".into()))?;
        verify_file(&backup, entry.hash(), entry.size())?;
        if exists(&target)? && verify_file(&target, entry.hash(), entry.size()).is_ok() {
          // No-clobber publication may have restored the target before a crash
          // without removing its backup. Completing that unlink is idempotent.
          fs::remove_file(&backup)?;
          sync_directory(&change.kind.directory(directory))?;
          sync_directory(&work.join("backup"))?;
          continue;
        }
        if exists(&target)? {
          let new = change
            .after
            .as_ref()
            .ok_or_else(|| Error::Conflict("external file blocks resource recovery".into()))?;
          verify_file(&target, new.hash(), new.size())?;
          fs::remove_file(&target)?;
        }
        publish_private(&backup, &target)?;
      } else if let Some(entry) = &change.before {
        verify_file(&target, entry.hash(), entry.size())?;
      } else if exists(&target)? {
        let entry = change
          .after
          .as_ref()
          .ok_or_else(|| Error::Conflict("external file blocks resource recovery".into()))?;
        verify_file(&target, entry.hash(), entry.size())?;
        fs::remove_file(&target)?;
      }
      sync_directory(&change.kind.directory(directory))?;
      if exists(&work.join("backup"))? {
        sync_directory(&work.join("backup"))?;
      }
    }
    storage::replace_json(&directory.join(LAYOUT), &journal.old_layout)?;
  } else {
    return Err(Error::Conflict(
      "resource manifest changed outside the pending transaction".into(),
    ));
  }
  cleanup(directory)
}
fn cleanup(directory: &Path) -> Result<()> {
  let work = directory.join(WORK);
  if exists(&work)? {
    ensure_directory(&work, false)?;
    fs::remove_dir_all(&work)?;
  }
  sync_directory(directory)?;
  fs::remove_file(directory.join(JOURNAL))?;
  sync_directory(directory)
}

// Reuse tempfile's cross-platform no-clobber publication, retaining the source
// on errors so recovery never loses its only backup. Source files are private
// copies; no link is ever created to a shared object.
fn publish_private(source: &Path, target: &Path) -> Result<()> {
  match tempfile::TempPath::try_from_path(source.to_path_buf())?.persist_noclobber(target) {
    Ok(()) => Ok(()),
    Err(error) => {
      let io_error = error.error;
      if let Err(keep_error) = error.path.keep() {
        std::mem::forget(keep_error.path);
      }
      Err(Error::Io(io_error))
    }
  }
}
