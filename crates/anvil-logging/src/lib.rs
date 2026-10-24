//! Shared desktop diagnostics. Entry points own the guard; Core owns no outputs.
#[cfg(target_os = "macos")]
use directories::BaseDirs;
#[cfg(not(target_os = "macos"))]
use directories::ProjectDirs;
use rolling_file::{BasicRollingFileAppender, RollingConditionBasic};
use std::{
  fs::{self, File, OpenOptions},
  io::{self, Write},
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
};
use tracing_appender::non_blocking::{ErrorCounter, NonBlockingBuilder, WorkerGuard};
use tracing_subscriber::{
  EnvFilter, filter::filter_fn, layer::SubscriberExt, util::SubscriberInitExt,
};

const FILE_BYTES: u64 = 10 * 1024 * 1024;
const HISTORY_FILES: usize = 4;
const QUEUE_LINES: usize = 1024;

#[derive(Clone, Copy, Debug)]
pub enum Process {
  Shell,
  Runtime,
}
impl Process {
  fn name(self) -> &'static str {
    match self {
      Self::Shell => "shell",
      Self::Runtime => "runtime",
    }
  }
}

pub fn log_directory() -> io::Result<PathBuf> {
  // The Shell passes its resolved directory into the sanitized child environment.
  if let Some(path) = std::env::var_os("ANVIL_LOG_DIRECTORY") {
    let path = PathBuf::from(path);
    if !path.is_absolute() {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "log directory must be absolute",
      ));
    }
    return Ok(path);
  }
  #[cfg(target_os = "macos")]
  {
    let dirs =
      BaseDirs::new().ok_or_else(|| io::Error::other("application log directory unavailable"))?;
    Ok(dirs.home_dir().join("Library/Logs/Anvil"))
  }
  #[cfg(not(target_os = "macos"))]
  {
    let dirs = ProjectDirs::from("", "", "Anvil")
      .ok_or_else(|| io::Error::other("application log directory unavailable"))?;
    Ok(
      dirs
        .state_dir()
        .unwrap_or_else(|| dirs.data_local_dir())
        .join("logs"),
    )
  }
}

/// A read-only view of this process's writer, including asynchronous failures.
#[derive(Clone)]
pub struct LoggingStatus {
  directory: Option<PathBuf>,
  available: Arc<AtomicBool>,
}
impl LoggingStatus {
  pub fn directory(&self) -> Option<&std::path::Path> {
    self.directory.as_deref()
  }
  pub fn file_logging_available(&self) -> bool {
    self.available.load(Ordering::Acquire)
  }
}

pub struct LoggingGuard {
  worker: Option<WorkerGuard>,
  // Keep the lock until the worker has drained and closed its writer.
  _lock: Option<File>,
  available: Arc<AtomicBool>,
  dropped: ErrorCounter,
  directory: Option<PathBuf>,
}
impl LoggingGuard {
  pub fn status(&self) -> LoggingStatus {
    LoggingStatus {
      directory: self.directory.clone(),
      available: self.available.clone(),
    }
  }
  pub fn file_logging_available(&self) -> bool {
    self.available.load(Ordering::Acquire)
  }
}
impl Drop for LoggingGuard {
  fn drop(&mut self) {
    let dropped = self.dropped.dropped_lines();
    if dropped > 0 {
      eprintln!("Anvil logging: dropped {dropped} records because the bounded queue was full");
    }
    drop(self.worker.take());
  }
}

/// Install a process-wide subscriber. Never writes to stdout or prevents startup.
pub fn init(process: Process, console: bool) -> LoggingGuard {
  let directory = log_directory().ok();
  let (file, lock) = match open_file(process) {
    Ok((file, lock)) => (Some(file), Some(lock)),
    Err((stage, error)) => {
      report_file_failure(stage, &error);
      (None, None)
    }
  };
  let available = Arc::new(AtomicBool::new(file.is_some()));
  let writer = Output {
    file,
    console,
    available: available.clone(),
  };
  let (writer, worker) = NonBlockingBuilder::default()
    .buffered_lines_limit(QUEUE_LINES)
    .lossy(true)
    .thread_name(match process {
      Process::Shell => "anvil-shell-logs",
      Process::Runtime => "anvil-runtime-logs",
    })
    .finish(writer);
  let dropped = writer.error_counter();
  let fallback = "warn,anvil_shell=info,anvil=info,anvil_core=info,anvil_logging=info";
  // from_env() accepts a missing variable as an empty filter (ERROR only).
  // Select our application defaults explicitly for missing or empty values.
  let directives = std::env::var("RUST_LOG")
    .ok()
    .filter(|value| !value.trim().is_empty());
  let filter = EnvFilter::builder()
    .with_regex(false)
    .parse(directives.as_deref().unwrap_or(fallback))
    .unwrap_or_else(|_| EnvFilter::new(fallback));
  let result = tracing_subscriber::registry()
    // HTTP libraries must never emit verbose request/header diagnostics, even
    // when an application module is configured for trace-level diagnostics.
    .with(filter_fn(|metadata| {
      metadata.target().starts_with("anvil_") || *metadata.level() <= tracing::Level::WARN
    }))
    .with(filter)
    .with(
      tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(writer),
    )
    .try_init();
  if result.is_err() {
    available.store(false, Ordering::Release);
    eprintln!(
      "Anvil logging: subscriber initialization failed; existing diagnostics remain active"
    );
  } else {
    tracing::info!(target: "anvil_logging", process = process.name(), file_output = available.load(Ordering::Acquire), "logging initialized");
  }
  LoggingGuard {
    worker: Some(worker),
    _lock: lock,
    available,
    dropped,
    directory,
  }
}

fn open_file(
  process: Process,
) -> Result<(BasicRollingFileAppender, File), (&'static str, io::Error)> {
  let directory = log_directory().map_err(|error| ("resolve directory", error))?;
  fs::create_dir_all(&directory).map_err(|error| ("create directory", error))?;
  if fs::symlink_metadata(&directory)
    .map_err(|error| ("inspect directory", error))?
    .file_type()
    .is_symlink()
  {
    return Err((
      "validate directory",
      io::Error::other("log directory must not be a symbolic link"),
    ));
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
      .map_err(|error| ("set directory permissions", error))?;
  }
  // Rotation is not multi-process safe: a second Shell/Runtime falls back to
  // stderr instead of racing another process over the same historical files.
  let stem = format!("{}.log", process.name());
  for suffix in ["", ".1", ".2", ".3", ".4", ".lock"] {
    match fs::symlink_metadata(directory.join(format!("{stem}{suffix}"))) {
      Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
        return Err((
          "validate log entry",
          io::Error::other("invalid log file entry"),
        ));
      }
      Ok(_) => {}
      Err(error) if error.kind() == io::ErrorKind::NotFound => {}
      Err(error) => return Err(("inspect log entry", error)),
    }
  }
  let lock = OpenOptions::new()
    .read(true)
    .write(true)
    .create(true)
    .truncate(false)
    .open(directory.join(format!("{stem}.lock")))
    .map_err(|error| ("open lock file", error))?;
  lock.try_lock().map_err(|error| match error {
    std::fs::TryLockError::WouldBlock => (
      "lock file",
      io::Error::new(io::ErrorKind::WouldBlock, "another process owns this log"),
    ),
    std::fs::TryLockError::Error(error) => ("lock file", error),
  })?;
  let file = BasicRollingFileAppender::new(
    directory.join(stem),
    RollingConditionBasic::new().max_size(FILE_BYTES),
    HISTORY_FILES,
  )
  .map_err(|error| ("open rolling log file", error))?;
  Ok((file, lock))
}

fn report_file_failure(stage: &str, error: &io::Error) {
  eprintln!(
    "Anvil logging: file output unavailable (stage={stage}, kind={:?}, os_error={:?}, detail={error}); falling back to stderr",
    error.kind(),
    error.raw_os_error()
  );
}

struct Output {
  file: Option<BasicRollingFileAppender>,
  console: bool,
  available: Arc<AtomicBool>,
}
impl Output {
  fn failed(&mut self, stage: &str, error: io::Error) {
    self.file = None;
    self.available.store(false, Ordering::Release);
    report_file_failure(stage, &error);
  }
}
impl Write for Output {
  fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
    if let Some(file) = self.file.as_mut()
      && let Err(error) = file.write_all(bytes)
    {
      self.failed("write log file", error);
    }
    if self.console || self.file.is_none() {
      let _ = io::stderr().write_all(bytes);
    }
    Ok(bytes.len())
  }
  fn flush(&mut self) -> io::Result<()> {
    if let Some(file) = self.file.as_mut()
      && let Err(error) = file.flush()
    {
      self.failed("flush log file", error);
    }
    if self.console || self.file.is_none() {
      let _ = io::stderr().flush();
    }
    Ok(())
  }
}
