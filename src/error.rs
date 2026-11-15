use crate::config::CommandNotFoundError;

use std::path::PathBuf;
use std::process::ExitStatus;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error(transparent)]
  CommandNotFound(#[from] CommandNotFoundError),

  /// An I/O operation failed; `context` says which one.
  #[error("{context}: {source}")]
  Io {
    context: String,
    source: std::io::Error,
  },

  /// A helper tool ran but reported failure.
  #[error("{tool} failed: {stderr}")]
  CommandFailed { tool: &'static str, stderr: String },

  /// Only constructed when listing iOS simulators.
  #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
  #[error("Failed to parse simctl JSON: {0}")]
  ParseSimctl(#[from] serde_json::Error),

  #[cfg(target_os = "macos")]
  #[error("Failed to boot simulator: {0}")]
  BootFailed(String),

  /// Only constructed on platforms without iOS simulators.
  #[cfg_attr(target_os = "macos", allow(dead_code))]
  #[error("iOS simulators are only available on macOS")]
  IosUnsupported,

  #[error("Cannot determine AVD directory")]
  NoAvdDirectory,

  #[error("No AVDs found in {}", .0.display())]
  NoAvds(PathBuf),

  #[error("Android emulator '{0}' is already running")]
  AlreadyRunning(String),

  #[error("Emulator '{avd}' exited immediately ({status}):\n{log_tail}\n(full log: {})", .log_path.display())]
  ExitedEarly {
    avd: String,
    status: ExitStatus,
    log_tail: String,
    log_path: PathBuf,
  },

  #[error("Emulator '{0}' not found")]
  NotFound(String),

  #[error("'{query}' matches {count} devices; open one by its id instead:\n{choices}")]
  Ambiguous {
    query: String,
    count: usize,
    choices: String,
  },
}

impl Error {
  /// Adapter for `map_err` that attaches a description to an I/O error.
  pub fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> Error {
    let context = context.into();
    move |source| Error::Io { context, source }
  }
}
