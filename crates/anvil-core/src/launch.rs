//! Structured Minecraft client launch argument planning.
use std::{
  collections::HashMap,
  ffi::OsString,
  fmt, fs,
  path::{Path, PathBuf},
  time::SystemTime,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
  Error, Result,
  account::{AccountSession, MicrosoftClientId},
  client::ClientRuntimePreparation,
  client_plan::{ClientInstallPlan, ClientTarget, PlannedFile, evaluate_rules},
};

const MAX_LAUNCHER_NAME: usize = 128;
const MAX_LAUNCHER_VERSION: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
  pub width: u32,
  pub height: u32,
}

impl Resolution {
  pub fn new(width: u32, height: u32) -> Result<Self> {
    if !(320..=16_384).contains(&width) || !(240..=16_384).contains(&height) {
      return Err(Error::Invalid("unsupported Minecraft resolution".into()));
    }
    Ok(Self { width, height })
  }
}

#[derive(Clone, Debug)]
pub struct LaunchOptions {
  pub launcher_name: String,
  pub launcher_version: String,
  /// An Azure application ID registered for Anvil; never inferred from its name.
  pub microsoft_client_id: Option<MicrosoftClientId>,
  pub resolution: Option<Resolution>,
}

impl Default for LaunchOptions {
  fn default() -> Self {
    Self {
      launcher_name: "Anvil".into(),
      launcher_version: env!("CARGO_PKG_VERSION").into(),
      microsoft_client_id: None,
      resolution: None,
    }
  }
}

/// A ready-to-spawn process description. It is intentionally not serializable,
/// and Debug reports argument counts without printing values or credentials.
pub struct LaunchSpec {
  java_executable: PathBuf,
  jvm_arguments: Vec<String>,
  main_class: String,
  game_arguments: Vec<String>,
  working_directory: PathBuf,
  _runtime: ClientRuntimePreparation,
}

impl LaunchSpec {
  pub fn java_executable(&self) -> &Path {
    &self.java_executable
  }

  pub fn working_directory(&self) -> &Path {
    &self.working_directory
  }

  /// Arguments are separate process argv elements; callers must not join them
  /// into a shell command string.
  pub fn argv(&self) -> Vec<OsString> {
    self
      .jvm_arguments
      .iter()
      .chain(std::iter::once(&self.main_class))
      .chain(self.game_arguments.iter())
      .map(OsString::from)
      .collect()
  }
}

impl fmt::Debug for LaunchSpec {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("LaunchSpec")
      .field("java_executable", &self.java_executable)
      .field("jvm_argument_count", &self.jvm_arguments.len())
      .field("main_class", &self.main_class)
      .field("game_argument_count", &self.game_arguments.len())
      .field("working_directory", &self.working_directory)
      .finish()
  }
}

pub fn build_launch_spec(
  plan: &ClientInstallPlan,
  runtime: ClientRuntimePreparation,
  account: &AccountSession,
  game_directory: impl AsRef<Path>,
  options: &LaunchOptions,
) -> Result<LaunchSpec> {
  build_launch_spec_with_settings(
    plan,
    runtime,
    account,
    game_directory,
    options,
    &crate::client::ClientLaunchSettings::default(),
  )
}

pub fn build_launch_spec_with_settings(
  plan: &ClientInstallPlan,
  runtime: ClientRuntimePreparation,
  account: &AccountSession,
  game_directory: impl AsRef<Path>,
  options: &LaunchOptions,
  settings: &crate::client::ClientLaunchSettings,
) -> Result<LaunchSpec> {
  settings.validate()?;
  let mut effective = options.clone();
  effective.resolution = options.resolution.or(settings.resolution);
  let options = &effective;
  validate_launch_inputs(plan, &runtime, account, options)?;
  let game_directory = game_directory.as_ref();
  let profile = account.profile();
  let classpath = classpath(plan, &runtime)?;
  let mut values = HashMap::<&str, String>::from([
    ("auth_player_name", profile.username().to_owned()),
    (
      "auth_uuid",
      profile.id().replace('-', "").to_ascii_lowercase(),
    ),
    ("auth_access_token", account.access_token().to_owned()),
    // XUID is optional in the authenticated profile. Keep an empty argv value
    // when the provider omits it; authentication still uses the real session.
    ("auth_xuid", profile.xuid().unwrap_or_default().to_owned()),
    (
      "user_type",
      if account.is_offline_test() {
        "legacy"
      } else {
        "msa"
      }
      .into(),
    ),
    ("version_name", plan.version_id.clone()),
    ("version_type", plan.version_type.clone()),
    ("game_directory", path_value(game_directory)?),
    ("assets_root", path_value(&runtime.assets_root)?),
    ("assets_index_name", plan.asset_index_id.clone()),
    ("natives_directory", path_value(&runtime.natives_directory)?),
    ("classpath", classpath),
    ("launcher_name", options.launcher_name.clone()),
    ("launcher_version", options.launcher_version.clone()),
  ]);
  if account.is_offline_test() {
    values.insert("auth_xuid", "0".into());
    values.insert("clientid", "00000000-0000-0000-0000-000000000000".into());
  }
  if let Some(client_id) = &options.microsoft_client_id {
    values.insert("clientid", client_id.as_str().to_owned());
  }
  if let Some(resolution) = options.resolution {
    values.insert("resolution_width", resolution.width.to_string());
    values.insert("resolution_height", resolution.height.to_string());
  }

  let mut target = plan.target.clone();
  target
    .features
    .insert("is_demo_user".into(), account.is_demo_user());
  target
    .features
    .insert("has_custom_resolution".into(), options.resolution.is_some());
  target
    .features
    .insert("has_quick_plays_support".into(), false);
  target
    .features
    .insert("is_quick_play_singleplayer".into(), false);
  target
    .features
    .insert("is_quick_play_multiplayer".into(), false);
  target.features.insert("is_quick_play_realms".into(), false);

  let mut jvm_arguments = expand_arguments(&plan.arguments.jvm, &target, &values)?;
  // Instance heap settings take precedence over heap flags supplied by metadata.
  jvm_arguments.retain(|argument| !argument.starts_with("-Xmx") && !argument.starts_with("-Xms"));
  jvm_arguments.push(format!("-Xmx{}M", settings.max_memory_mib));
  if let Some(initial) = settings.initial_memory_mib {
    jvm_arguments.push(format!("-Xms{initial}M"));
  }
  if let Some(logging) = &plan.logging {
    let path = installed_artifact_path(&runtime, &logging.file.path, &logging.file)?;
    let logging_values = HashMap::from([("path", path_value(&path)?)]);
    let argument = expand_template(&logging.argument, &logging_values)?;
    jvm_arguments.push(argument);
  }
  let mut game_arguments = expand_arguments(&plan.arguments.game, &target, &values)?;
  if let Some(resolution) = options.resolution {
    for (flag, value) in [
      ("--width", resolution.width),
      ("--height", resolution.height),
    ] {
      if let Some(index) = game_arguments.iter().position(|argument| argument == flag) {
        let argument = game_arguments
          .get_mut(index + 1)
          .ok_or_else(|| Error::Invalid("invalid window size arguments".into()))?;
        *argument = value.to_string();
      } else {
        game_arguments.extend([flag.to_owned(), value.to_string()]);
      }
    }
  }

  Ok(LaunchSpec {
    java_executable: runtime.java.executable.clone(),
    jvm_arguments,
    main_class: plan.main_class.clone(),
    game_arguments,
    working_directory: game_directory.to_path_buf(),
    _runtime: runtime,
  })
}

fn validate_launch_inputs(
  plan: &ClientInstallPlan,
  runtime: &ClientRuntimePreparation,
  account: &AccountSession,
  options: &LaunchOptions,
) -> Result<()> {
  account.profile().validate()?;
  if account.expires_at() <= SystemTime::now() {
    return Err(Error::AccountRequired);
  }
  if runtime.installed.loader != plan.loader
    || runtime.installed.version_id != plan.version_id
    || runtime.installed.version_type != plan.version_type
    || runtime.installed.metadata_url != plan.metadata_url.as_str()
    || runtime.installed.metadata_sha1 != plan.metadata_sha1
    || runtime.installed.java_component != plan.java_component
    || runtime.installed.main_class != plan.main_class
    || runtime.installed.java_major != plan.java_major
    || runtime.installed.os != record_os(plan.target.os)
    || runtime.installed.architecture != record_architecture(plan.target.architecture)
    || runtime.installed.asset_index_id != plan.asset_index_id
    || runtime.installed.arguments != plan.arguments
    || runtime.java.major != plan.java_major
    || runtime.java.os != java_os(plan.target.os)
    || runtime.java.arch != java_architecture(plan.target.architecture)
    || runtime.asset_index != runtime.assets_root.join(&plan.asset_index.path)
  {
    return Err(Error::Conflict(
      "launch plan does not match the installed client snapshot".into(),
    ));
  }
  if options.launcher_name.is_empty()
    || options.launcher_name.len() > MAX_LAUNCHER_NAME
    || options.launcher_version.is_empty()
    || options.launcher_version.len() > MAX_LAUNCHER_VERSION
    || options
      .launcher_name
      .bytes()
      .chain(options.launcher_version.bytes())
      .any(|byte| byte.is_ascii_control())
  {
    return Err(Error::Invalid("invalid launcher identity".into()));
  }
  Ok(())
}

fn record_os(os: crate::client_plan::ClientOs) -> &'static str {
  match os {
    crate::client_plan::ClientOs::Windows => "windows",
    crate::client_plan::ClientOs::MacOs => "macos",
    crate::client_plan::ClientOs::Linux => "linux",
  }
}

fn java_os(os: crate::client_plan::ClientOs) -> &'static str {
  match os {
    crate::client_plan::ClientOs::Windows => "windows",
    crate::client_plan::ClientOs::MacOs => "mac",
    crate::client_plan::ClientOs::Linux => "linux",
  }
}

fn record_architecture(architecture: crate::client_plan::ClientArchitecture) -> &'static str {
  match architecture {
    crate::client_plan::ClientArchitecture::X86_64 => "x86_64",
    crate::client_plan::ClientArchitecture::Aarch64 => "aarch64",
  }
}

fn java_architecture(architecture: crate::client_plan::ClientArchitecture) -> &'static str {
  match architecture {
    crate::client_plan::ClientArchitecture::X86_64 => "x64",
    crate::client_plan::ClientArchitecture::Aarch64 => "aarch64",
  }
}

fn classpath(plan: &ClientInstallPlan, runtime: &ClientRuntimePreparation) -> Result<String> {
  let mut entries = Vec::with_capacity(plan.libraries.len() + 1);
  for library in &plan.libraries {
    let logical_path = format!("libraries/{}", library.artifact.path);
    entries.push(classpath_artifact_path(
      runtime,
      &logical_path,
      &library.artifact,
    )?);
  }
  entries.push(classpath_artifact_path(
    runtime,
    &plan.client_jar.path,
    &plan.client_jar,
  )?);
  std::env::join_paths(entries)
    .map_err(|_| Error::Invalid("client classpath contains a separator".into()))?
    .into_string()
    .map_err(|_| Error::Unsupported("non-UTF8 client classpath path".into()))
}

fn classpath_artifact_path(
  runtime: &ClientRuntimePreparation,
  logical_path: &str,
  planned: &PlannedFile,
) -> Result<PathBuf> {
  // Retain the record/plan identity check, but launch the private .jar view.
  let cached = installed_artifact_path(runtime, logical_path, planned)?;
  if runtime.installed.loader.is_none() {
    return Ok(cached);
  }
  let path = crate::client::classpath::jar_path(&runtime.session_directory, &planned.file.sha1);
  let metadata = fs::symlink_metadata(&path)?;
  if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != planned.file.size
  {
    return Err(Error::Conflict("session classpath JAR is invalid".into()));
  }
  Ok(path)
}

fn installed_artifact_path(
  runtime: &ClientRuntimePreparation,
  logical_path: &str,
  planned: &PlannedFile,
) -> Result<PathBuf> {
  let installed = runtime
    .installed
    .files
    .iter()
    .find(|file| file.path == logical_path)
    .ok_or_else(|| Error::Conflict(format!("installed file is missing: {logical_path}")))?;
  if installed.sha1 != planned.file.sha1 || installed.size != planned.file.size {
    return Err(Error::Conflict(format!(
      "installed file does not match launch plan: {logical_path}"
    )));
  }
  let path = runtime.cache_root.join(&installed.cache_path);
  match fs::symlink_metadata(&path) {
    Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(
      Error::Conflict(format!("installed file is not regular: {logical_path}")),
    ),
    Ok(_) => Ok(path),
    Err(error) => Err(error.into()),
  }
}

fn expand_arguments(
  arguments: &[Value],
  target: &ClientTarget,
  values: &HashMap<&str, String>,
) -> Result<Vec<String>> {
  let mut result = Vec::new();
  for argument in arguments {
    let (templates, allowed) = match argument {
      Value::String(value) => (vec![value.as_str()], true),
      Value::Object(object) => {
        if object
          .keys()
          .any(|key| !matches!(key.as_str(), "value" | "rules"))
        {
          return Err(Error::Unsupported(
            "unknown Minecraft argument field".into(),
          ));
        }
        let value = object
          .get("value")
          .ok_or_else(|| Error::Metadata("Minecraft argument has no value".into()))?;
        let templates = match value {
          Value::String(value) => vec![value.as_str()],
          Value::Array(values) => values
            .iter()
            .map(|value| {
              value
                .as_str()
                .ok_or_else(|| Error::Metadata("Minecraft argument value is not a string".into()))
            })
            .collect::<Result<Vec<_>>>()?,
          _ => {
            return Err(Error::Metadata(
              "Minecraft argument value is not a string or array".into(),
            ));
          }
        };
        let allowed = match object.get("rules") {
          Some(rules) => evaluate_rules(rules, target)?,
          None => true,
        };
        (templates, allowed)
      }
      _ => return Err(Error::Metadata("invalid Minecraft argument".into())),
    };
    if allowed {
      result.extend(
        templates
          .into_iter()
          .map(|template| expand_template(template, values))
          .collect::<Result<Vec<_>>>()?,
      );
    }
  }
  Ok(result)
}

pub(crate) fn expand_template(template: &str, values: &HashMap<&str, String>) -> Result<String> {
  let mut result = String::with_capacity(template.len());
  let mut remaining = template;
  while let Some(start) = remaining.find("${") {
    result.push_str(&remaining[..start]);
    let after = &remaining[start + 2..];
    let end = after
      .find('}')
      .ok_or_else(|| Error::Metadata("unterminated Minecraft argument placeholder".into()))?;
    let name = &after[..end];
    if name.is_empty() {
      return Err(Error::Metadata(
        "empty Minecraft argument placeholder".into(),
      ));
    }
    let value = values
      .get(name)
      .ok_or_else(|| Error::Unsupported(format!("missing Minecraft launch value {name}")))?;
    result.push_str(value);
    remaining = &after[end + 1..];
  }
  result.push_str(remaining);
  if result.bytes().any(|byte| byte.is_ascii_control()) {
    return Err(Error::Invalid(
      "Minecraft launch argument contains a control character".into(),
    ));
  }
  Ok(result)
}

fn path_value(path: &Path) -> Result<String> {
  let value = path
    .to_str()
    .ok_or_else(|| Error::Unsupported("non-UTF8 Minecraft launch path".into()))?;
  if value.bytes().any(|byte| byte.is_ascii_control()) {
    return Err(Error::Invalid(
      "launch path contains a control character".into(),
    ));
  }
  Ok(value.to_owned())
}
