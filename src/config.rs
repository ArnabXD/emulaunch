use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Config {
  #[serde(default)]
  pub android_emulator_cmd: Option<String>,
  #[serde(default)]
  pub adb_cmd: Option<String>,
  #[serde(default)]
  pub xcrun_cmd: Option<String>,
  #[serde(default)]
  pub theme: Option<String>,
  #[serde(default)]
  pub theme_overrides: Option<ThemeOverrides>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ThemeOverrides {
  #[serde(default)]
  pub header_fg: Option<String>,
  #[serde(default)]
  pub name_fg: Option<String>,
  #[serde(default)]
  pub state_booted_fg: Option<String>,
  #[serde(default)]
  pub state_shutdown_fg: Option<String>,
  #[serde(default)]
  pub state_unknown_fg: Option<String>,
  #[serde(default)]
  pub meta_fg: Option<String>,
  #[serde(default)]
  pub filter_placeholder_fg: Option<String>,
  #[serde(default)]
  pub filter_active_fg: Option<String>,
  #[serde(default)]
  pub selection_bg: Option<String>,
  #[serde(default)]
  pub help_key_fg: Option<String>,
  #[serde(default)]
  pub help_text_fg: Option<String>,
}

#[derive(Debug)]
pub enum CommandNotFoundError {
  AndroidEmulator {
    suggestion: String,
  },
  Adb {
    suggestion: String,
  },
  #[cfg(target_os = "macos")]
  Xcrun {
    suggestion: String,
  },
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
      #[cfg(target_os = "macos")]
      CommandNotFoundError::Xcrun { suggestion } => {
        write!(f, "xcrun command not found. {}\n\nPlease configure it in your config file:\n{}\n\nOr set the XCRUN_CMD environment variable.",
                       suggestion, get_config_paths_display())
      }
    }
  }
}

impl std::error::Error for CommandNotFoundError {}

// Error message constants
const SUGGESTION_ANDROID_SDK: &str = "Install Android SDK or add it to PATH.\n\
Common locations:\n  macOS: ~/Library/Android/sdk/emulator/emulator\n  Linux: ~/Android/Sdk/emulator/emulator\n  Windows: %LOCALAPPDATA%\\Android\\Sdk\\emulator\\emulator.exe";
const SUGGESTION_ADB: &str = "Install Android SDK Platform-Tools or add it to PATH.\n\
Common locations:\n  macOS: ~/Library/Android/sdk/platform-tools/adb\n  Linux: ~/Android/Sdk/platform-tools/adb\n  Windows: %LOCALAPPDATA%\\Android\\Sdk\\platform-tools\\adb.exe";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const SUGGESTION_XCRUN: &str = "Install Xcode Command Line Tools: xcode-select --install";

/// Candidate Android SDK roots, most specific first: `ANDROID_HOME`,
/// `ANDROID_SDK_ROOT`, then the platform's default install location.
fn android_sdk_roots() -> Vec<PathBuf> {
  let mut roots: Vec<PathBuf> = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
    .iter()
    .filter_map(std::env::var_os)
    .filter(|v| !v.is_empty())
    .map(PathBuf::from)
    .collect();

  #[cfg(target_os = "macos")]
  roots.extend(dirs::home_dir().map(|h| h.join("Library/Android/sdk")));
  #[cfg(target_os = "linux")]
  roots.extend(dirs::home_dir().map(|h| h.join("Android/Sdk")));
  #[cfg(target_os = "windows")]
  roots.extend(dirs::data_local_dir().map(|d| d.join("Android/Sdk")));

  roots
}

/// Locations of an SDK tool (e.g. `emulator/emulator`, `platform-tools/adb`).
fn android_sdk_tool_paths(relative: &str) -> Vec<PathBuf> {
  android_sdk_roots()
    .into_iter()
    .map(|root| root.join(format!("{}{}", relative, std::env::consts::EXE_SUFFIX)))
    .collect()
}

fn get_android_emulator_paths() -> Vec<PathBuf> {
  android_sdk_tool_paths("emulator/emulator")
}

fn get_adb_paths() -> Vec<PathBuf> {
  android_sdk_tool_paths("platform-tools/adb")
}

pub fn get_config_paths() -> Vec<PathBuf> {
  let mut paths = Vec::new();

  if let Some(home_dir) = dirs::home_dir() {
    // XDG-style path (~/.config/emulaunch/config.toml)
    // Checked explicitly because dirs::config_dir() returns
    // ~/Library/Application Support on macOS, not ~/.config
    paths.push(home_dir.join(".config").join("emulaunch").join("config.toml"));

    // Platform config dir (~/Library/Application Support on macOS, etc.)
    if let Some(config_dir) = dirs::config_dir() {
      let platform_path = config_dir.join("emulaunch").join("config.toml");
      if !paths.contains(&platform_path) {
        paths.push(platform_path);
      }
    }

    // Legacy fallback
    paths.push(home_dir.join(".emulaunch").join("config.toml"));
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

/// Load the config file once per process. A missing file yields `None`; an
/// unreadable or invalid file is reported on stderr and then ignored.
pub fn load_config() -> Option<&'static Config> {
  static CONFIG: OnceLock<Option<Config>> = OnceLock::new();
  CONFIG
    .get_or_init(|| {
      let path = get_config_paths().into_iter().find(|p| p.exists())?;
      let parsed = std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|contents| toml::from_str(&contents).map_err(|e| e.to_string()));
      match parsed {
        Ok(config) => Some(config),
        Err(e) => {
          eprintln!("warning: ignoring {}: {}", path.display(), e);
          None
        }
      }
    })
    .as_ref()
}

/// Print a warning to stderr at most once per process. Commands are resolved
/// again on every TUI refresh, and repeated stderr output would corrupt the screen.
fn warn_once(msg: String) {
  static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
  let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
  if !seen.contains(&msg) {
    eprintln!("warning: {}", msg);
    seen.push(msg);
  }
}

/// True if `cmd` is a bare command found on PATH or a path to an executable file.
fn is_runnable(cmd: &str) -> bool {
  which::which(cmd).is_ok()
}

/// Generic command resolution helper
fn resolve_command<F>(
  config_key: F,
  env_var: &str,
  default_cmd: &str,
  platform_paths: Vec<PathBuf>,
  error_variant: fn(String) -> CommandNotFoundError,
) -> Result<String, CommandNotFoundError>
where
  F: Fn(&Config) -> Option<&String>,
{
  // Check config first
  if let Some(config) = load_config() {
    if let Some(cmd) = config_key(config) {
      if is_runnable(cmd) {
        return Ok(cmd.to_string());
      }
      warn_once(format!("configured command '{}' not found, ignoring", cmd));
    }
  }

  // Check environment variable
  if let Ok(cmd) = std::env::var(env_var) {
    if is_runnable(&cmd) {
      return Ok(cmd);
    }
    warn_once(format!("${} is set to '{}' but not found, ignoring", env_var, cmd));
  }

  // Try platform-specific paths
  for path in &platform_paths {
    if path.exists() {
      return Ok(path.to_string_lossy().to_string());
    }
  }

  // Fall back to simple command name
  if is_runnable(default_cmd) {
    return Ok(default_cmd.to_string());
  }

  // Return error - caller provides specific suggestion
  Err(error_variant(
    "Command not found in PATH or common locations".to_string(),
  ))
}

pub fn get_android_emulator_cmd() -> Result<String, CommandNotFoundError> {
  resolve_command(
    |c| c.android_emulator_cmd.as_ref(),
    "ANDROID_EMULATOR_CMD",
    "emulator",
    get_android_emulator_paths(),
    |msg| CommandNotFoundError::AndroidEmulator {
      suggestion: format!("{}\n\n{}", msg, SUGGESTION_ANDROID_SDK),
    },
  )
}

pub fn get_adb_cmd() -> Result<String, CommandNotFoundError> {
  resolve_command(
    |c| c.adb_cmd.as_ref(),
    "ADB_CMD",
    "adb",
    get_adb_paths(),
    |msg| CommandNotFoundError::Adb {
      suggestion: format!("{}\n\n{}", msg, SUGGESTION_ADB),
    },
  )
}

#[cfg(target_os = "macos")]
pub fn get_xcrun_cmd() -> Result<String, CommandNotFoundError> {
  resolve_command(
    |c| c.xcrun_cmd.as_ref(),
    "XCRUN_CMD",
    "xcrun",
    Vec::new(), // xcrun is typically in PATH, not a fixed path
    |msg| CommandNotFoundError::Xcrun {
      suggestion: format!("{}\n\n{}", msg, SUGGESTION_XCRUN),
    },
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_documented_config_file_parses() {
    let config: Config = toml::from_str(
      r##"
        android_emulator_cmd = "/sdk/emulator/emulator"
        adb_cmd = "/sdk/platform-tools/adb"
        xcrun_cmd = "/usr/bin/xcrun"
        theme = "dracula"

        [theme_overrides]
        header_fg = "#ff00ff"
        selection_bg = "#202020"
      "##,
    )
    .unwrap();

    assert_eq!(
      config.android_emulator_cmd.as_deref(),
      Some("/sdk/emulator/emulator")
    );
    assert_eq!(config.adb_cmd.as_deref(), Some("/sdk/platform-tools/adb"));
    assert_eq!(config.xcrun_cmd.as_deref(), Some("/usr/bin/xcrun"));
    assert_eq!(config.theme.as_deref(), Some("dracula"));

    let overrides = config.theme_overrides.unwrap();
    assert_eq!(overrides.header_fg.as_deref(), Some("#ff00ff"));
    assert_eq!(overrides.selection_bg.as_deref(), Some("#202020"));
    assert_eq!(overrides.name_fg, None);
  }

  #[test]
  fn an_empty_config_file_means_all_defaults() {
    let config: Config = toml::from_str("").unwrap();
    assert!(config.android_emulator_cmd.is_none());
    assert!(config.adb_cmd.is_none());
    assert!(config.xcrun_cmd.is_none());
    assert!(config.theme.is_none());
    assert!(config.theme_overrides.is_none());
  }

  #[test]
  fn unknown_keys_are_tolerated() {
    let config: Config = toml::from_str("adb_cmd = \"adb\"\nsome_future_option = true\n").unwrap();
    assert_eq!(config.adb_cmd.as_deref(), Some("adb"));
  }

  #[test]
  fn values_of_the_wrong_type_are_an_error() {
    assert!(toml::from_str::<Config>("adb_cmd = 42").is_err());
    assert!(toml::from_str::<Config>("adb_cmd = [").is_err());
  }

  #[test]
  fn config_files_are_searched_in_priority_order() {
    let paths = get_config_paths();
    // Needs a home directory, which CI runners have
    if dirs::home_dir().is_none() {
      return;
    }
    let first = paths.first().unwrap();
    let last = paths.last().unwrap();
    assert!(
      first.ends_with(".config/emulaunch/config.toml"),
      "{:?}",
      first
    );
    assert!(last.ends_with(".emulaunch/config.toml"), "{:?}", last);
  }

  #[test]
  fn sdk_tool_paths_have_the_platform_executable_suffix() {
    for path in android_sdk_tool_paths("platform-tools/adb") {
      let name = path.file_name().unwrap().to_string_lossy().into_owned();
      assert_eq!(name, format!("adb{}", std::env::consts::EXE_SUFFIX));
      assert!(path.parent().unwrap().ends_with("platform-tools"));
    }
  }

  #[test]
  fn command_not_found_errors_name_the_tool_and_how_to_configure_it() {
    let err = CommandNotFoundError::Adb {
      suggestion: "hint".to_string(),
    }
    .to_string();
    assert!(err.contains("ADB command not found"));
    assert!(err.contains("hint"));
    assert!(err.contains("ADB_CMD"));
    assert!(err.contains("config.toml"));
  }
}
