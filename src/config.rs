use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Config {
  #[serde(default)]
  pub android_emulator_cmd: Option<String>,
  #[serde(default)]
  pub adb_cmd: Option<String>,
  #[serde(default)]
  pub xcrun_cmd: Option<String>,
}

#[derive(Debug)]
pub enum CommandNotFoundError {
  AndroidEmulator { suggestion: String },
  Adb { suggestion: String },
  Xcrun { suggestion: String },
}

impl std::fmt::Display for CommandNotFoundError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      CommandNotFoundError::AndroidEmulator { suggestion } => {
        write!(f, "Android emulator command not found. {}\n\nPlease configure it in your config file:\n{}\n\nOr set the ANDROID_EMULATOR_CMD environment variable.",
                       suggestion, get_config_paths_display())
      }
      CommandNotFoundError::Adb { suggestion } => {
        write!(f, "ADB command not found. {}\n\nPlease configure it in your config file:\n{}\n\nOr set the ADB_CMD environment variable.",
                       suggestion, get_config_paths_display())
      }
      CommandNotFoundError::Xcrun { suggestion } => {
        write!(f, "xcrun command not found. {}\n\nPlease configure it in your config file:\n{}\n\nOr set the XCRUN_CMD environment variable.",
                       suggestion, get_config_paths_display())
      }
    }
  }
}

impl std::error::Error for CommandNotFoundError {}

pub fn get_config_paths() -> Vec<PathBuf> {
  let mut paths = Vec::new();

  // Try XDG config directory first
  if let Some(config_dir) = dirs::config_dir() {
    paths.push(config_dir.join("emulators").join("config.toml"));
  }

  // Fallback to home directory
  if let Some(home_dir) = dirs::home_dir() {
    paths.push(home_dir.join(".emulators").join("config.toml"));
  }

  paths
}

fn get_config_paths_display() -> String {
  let paths = get_config_paths();
  paths
    .iter()
    .map(|p| format!("  {}", p.display()))
    .collect::<Vec<_>>()
    .join("\n")
}

pub fn load_config() -> Option<Config> {
  for path in get_config_paths() {
    if path.exists() {
      let contents = std::fs::read_to_string(&path).ok()?;
      return toml::from_str(&contents).ok();
    }
  }
  None
}

fn command_exists(cmd: &str) -> bool {
  #[cfg(target_os = "windows")]
  {
    std::process::Command::new("where")
      .arg(cmd)
      .output()
      .map(|o| o.status.success())
      .unwrap_or(false)
  }

  #[cfg(not(target_os = "windows"))]
  {
    std::process::Command::new("which")
      .arg(cmd)
      .output()
      .map(|o| o.status.success())
      .unwrap_or(false)
  }
}

fn file_exists(path: &str) -> bool {
  PathBuf::from(path).exists()
}

pub fn get_android_emulator_cmd() -> Result<String, CommandNotFoundError> {
  // Check config first
  if let Some(config) = load_config() {
    if let Some(ref cmd) = config.android_emulator_cmd {
      if command_exists(cmd) || file_exists(cmd) {
        return Ok(cmd.clone());
      }
    }
  }

  // Check environment variable
  if let Ok(cmd) = std::env::var("ANDROID_EMULATOR_CMD") {
    if command_exists(&cmd) || file_exists(&cmd) {
      return Ok(cmd);
    }
  }

  // Try common platform-specific paths
  let home = dirs::home_dir().ok_or(CommandNotFoundError::AndroidEmulator {
    suggestion: "Could not determine home directory".to_string(),
  })?;

  #[cfg(target_os = "macos")]
  let common_paths = vec![home.join("Library/Android/sdk/emulator/emulator")];

  #[cfg(target_os = "linux")]
  let common_paths = vec![home.join("Android/Sdk/emulator/emulator")];

  #[cfg(target_os = "windows")]
  let common_paths = vec![home.join("AppData/Local/Android/Sdk/emulator/emulator.exe")];

  #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
  let common_paths: Vec<PathBuf> = vec![];

  for path in &common_paths {
    if path.exists() {
      return Ok(path.to_string_lossy().to_string());
    }
  }

  // Fall back to simple command name
  if command_exists("emulator") {
    return Ok("emulator".to_string());
  }

  let suggestion = "Install Android SDK or add it to PATH.\nCommon locations:\n  macOS: ~/Library/Android/sdk/emulator/emulator\n  Linux: ~/Android/Sdk/emulator/emulator\n  Windows: %LOCALAPPDATA%\\Android\\Sdk\\emulator\\emulator.exe".to_string();

  Err(CommandNotFoundError::AndroidEmulator { suggestion })
}

pub fn get_adb_cmd() -> Result<String, CommandNotFoundError> {
  // Check config first
  if let Some(config) = load_config() {
    if let Some(ref cmd) = config.adb_cmd {
      if command_exists(cmd) || file_exists(cmd) {
        return Ok(cmd.clone());
      }
    }
  }

  // Check environment variable
  if let Ok(cmd) = std::env::var("ADB_CMD") {
    if command_exists(&cmd) || file_exists(&cmd) {
      return Ok(cmd);
    }
  }

  // Try common platform-specific paths
  let home = dirs::home_dir().ok_or(CommandNotFoundError::Adb {
    suggestion: "Could not determine home directory".to_string(),
  })?;

  #[cfg(target_os = "macos")]
  let common_paths = vec![home.join("Library/Android/sdk/platform-tools/adb")];

  #[cfg(target_os = "linux")]
  let common_paths = vec![home.join("Android/Sdk/platform-tools/adb")];

  #[cfg(target_os = "windows")]
  let common_paths = vec![home.join("AppData/Local/Android/Sdk/platform-tools/adb.exe")];

  #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
  let common_paths: Vec<PathBuf> = vec![];

  for path in &common_paths {
    if path.exists() {
      return Ok(path.to_string_lossy().to_string());
    }
  }

  // Fall back to simple command name
  if command_exists("adb") {
    return Ok("adb".to_string());
  }

  let suggestion = "Install Android SDK Platform-Tools or add it to PATH.\nCommon locations:\n  macOS: ~/Library/Android/sdk/platform-tools/adb\n  Linux: ~/Android/Sdk/platform-tools/adb\n  Windows: %LOCALAPPDATA%\\Android\\Sdk\\platform-tools\\adb.exe".to_string();

  Err(CommandNotFoundError::Adb { suggestion })
}

#[cfg(target_os = "macos")]
pub fn get_xcrun_cmd() -> Result<String, CommandNotFoundError> {
  // Check config first
  if let Some(config) = load_config() {
    if let Some(ref cmd) = config.xcrun_cmd {
      if command_exists(cmd) || file_exists(cmd) {
        return Ok(cmd.clone());
      }
    }
  }

  // Check environment variable
  if let Ok(cmd) = std::env::var("XCRUN_CMD") {
    if command_exists(&cmd) || file_exists(&cmd) {
      return Ok(cmd);
    }
  }

  // Fall back to simple command name
  if command_exists("xcrun") {
    return Ok("xcrun".to_string());
  }

  let suggestion = "Install Xcode Command Line Tools: xcode-select --install".to_string();

  Err(CommandNotFoundError::Xcrun { suggestion })
}

#[cfg(not(target_os = "macos"))]
pub fn get_xcrun_cmd() -> Result<String, CommandNotFoundError> {
  // Non-macOS: still return a default, but it won't work
  // This is for consistency - iOS functions won't be called anyway
  Ok(
    std::env::var("XCRUN_CMD")
      .or_else(|_| load_config().and_then(|c| c.xcrun_cmd).ok_or(()))
      .unwrap_or_else(|_| "xcrun".to_string()),
  )
}
