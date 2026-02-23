use crate::config;

use std::fmt;
use std::process::Stdio;

// State constants
pub const STATE_BOOTED: &str = "Booted";
pub const STATE_SHUTDOWN: &str = "Shutdown";
pub const STATE_AVAILABLE: &str = "Available";

// Section headers
pub const SECTION_ANDROID_EMULATORS: &str = "Android Emulators";
pub const SECTION_IOS_SIMULATORS: &str = "iOS Simulators";

// Device types
pub const DEVICE_TYPE_AVD: &str = "AVD";
pub const DEVICE_TYPE_RUNNING: &str = "Running Device";

// Error messages
#[allow(dead_code)]
pub const ERR_IOS_ONLY_MACOS: &str = "iOS simulators are only available on macOS";

#[derive(Debug, Clone)]
pub struct AndroidEmulator {
  pub name: String,
  pub id: String,
  pub device_type: String,
  pub state: String,
}

#[derive(Debug, Clone)]
pub struct IOSSimulator {
  pub name: String,
  pub udid: String,
  pub state: String,
  pub runtime: String,
}

#[allow(clippy::upper_case_acronyms)]
pub enum EmulatorType {
  Android(String),
  IOS(String),
}

/// A unified entry for display in the TUI list
#[allow(clippy::upper_case_acronyms)]
pub enum EmulatorEntry {
  SectionHeader(String),
  Android(AndroidEmulator),
  IOS(IOSSimulator),
}

impl EmulatorEntry {
  pub fn display_name(&self) -> &str {
    match self {
      EmulatorEntry::SectionHeader(s) => s,
      EmulatorEntry::Android(e) => &e.name,
      EmulatorEntry::IOS(s) => &s.name,
    }
  }

  pub fn is_header(&self) -> bool {
    matches!(self, EmulatorEntry::SectionHeader(_))
  }
}

impl fmt::Display for EmulatorEntry {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      EmulatorEntry::SectionHeader(s) => write!(f, "{}", s),
      EmulatorEntry::Android(e) => write!(f, "{} [{}] ({})", e.name, e.state, e.device_type),
      EmulatorEntry::IOS(s) => write!(f, "{} [{}] ({})", s.name, s.state, s.runtime),
    }
  }
}

fn get_android_emulator_cmd() -> Result<String, String> {
  config::get_android_emulator_cmd().map_err(|e: config::CommandNotFoundError| e.to_string())
}

fn get_adb_cmd() -> Result<String, String> {
  config::get_adb_cmd().map_err(|e: config::CommandNotFoundError| e.to_string())
}

#[cfg(target_os = "macos")]
fn get_xcrun_cmd() -> Result<String, String> {
  config::get_xcrun_cmd().map_err(|e: config::CommandNotFoundError| e.to_string())
}

/// Read the display name from an AVD's config.ini
fn get_avd_display_name(avd_id: &str) -> Option<String> {
  let home = std::env::var("HOME").ok()?;
  let config_path = std::path::PathBuf::from(&home)
    .join(".android/avd")
    .join(format!("{}.avd", avd_id))
    .join("config.ini");
  let contents = std::fs::read_to_string(config_path).ok()?;
  contents
    .lines()
    .find(|line| line.starts_with("avd.ini.displayname="))
    .and_then(|line| line.strip_prefix("avd.ini.displayname="))
    .map(|s| s.trim().to_string())
}

/// Get the set of AVD names that are currently running via adb
fn get_running_avd_names() -> Result<Vec<String>, String> {
  let adb_cmd = get_adb_cmd()?;

  let output = std::process::Command::new(&adb_cmd)
    .args(["devices"])
    .output()
    .map_err(|e| format!("Failed to run adb devices: {}", e))?;

  if !output.status.success() {
    return Ok(Vec::new());
  }

  let stdout = String::from_utf8_lossy(&output.stdout);
  let serials: Vec<String> = stdout
    .lines()
    .skip(1)
    .filter_map(|line| {
      let parts: Vec<&str> = line.split_whitespace().collect();
      if parts.len() >= 2 && parts[1] == "device" && parts[0].starts_with("emulator-") {
        Some(parts[0].to_string())
      } else {
        None
      }
    })
    .collect();

  let names = serials
    .iter()
    .filter_map(|serial| {
      let result = std::process::Command::new(&adb_cmd)
        .args(["-s", serial, "emu", "avd", "name"])
        .output()
        .ok()?;
      if result.status.success() {
        let stdout = String::from_utf8_lossy(&result.stdout);
        stdout.lines().next().map(|s| s.trim().to_string())
      } else {
        None
      }
    })
    .collect();

  Ok(names)
}

/// List AVDs by scanning ~/.android/avd/ directory
fn list_avds_from_directory() -> Result<Vec<AndroidEmulator>, String> {
  let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
  let avd_dir = std::path::PathBuf::from(&home).join(".android/avd");

  let entries =
    std::fs::read_dir(&avd_dir).map_err(|e| format!("Cannot read AVD directory: {}", e))?;

  let mut emulators = Vec::new();
  for entry in entries.flatten() {
    let path = entry.path();
    if path.extension().and_then(|e| e.to_str()) == Some("ini") && !path.is_dir() {
      if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        // Verify the .avd directory exists
        if avd_dir.join(format!("{}.avd", stem)).is_dir() {
          let display_name = get_avd_display_name(stem).unwrap_or_else(|| stem.to_string());
          emulators.push(AndroidEmulator {
            name: display_name,
            id: stem.to_string(),
            device_type: DEVICE_TYPE_AVD.to_string(),
            state: STATE_SHUTDOWN.to_string(),
          });
        }
      }
    }
  }

  if emulators.is_empty() {
    Err("No AVDs found in ~/.android/avd/".to_string())
  } else {
    Ok(emulators)
  }
}

pub fn list_android_emulators() -> Result<Vec<AndroidEmulator>, String> {
  let emulator_cmd = get_android_emulator_cmd()?;
  let running_names = get_running_avd_names().unwrap_or_default();

  let output = std::process::Command::new(&emulator_cmd)
    .arg("-list-avds")
    .output();

  let mut emulators = match output {
    Ok(result) if result.status.success() => {
      let stdout = String::from_utf8_lossy(&result.stdout);
      Ok(
        stdout
          .lines()
          .filter(|line| !line.is_empty())
          .map(|line| {
            let id = line.trim().to_string();
            let name = get_avd_display_name(&id).unwrap_or_else(|| id.clone());
            let state = if running_names.contains(&id) {
              STATE_BOOTED.to_string()
            } else {
              STATE_SHUTDOWN.to_string()
            };
            AndroidEmulator {
              name,
              id,
              device_type: DEVICE_TYPE_AVD.to_string(),
              state,
            }
          })
          .collect(),
      )
    }
    Ok(_) | Err(_) => {
      // emulator command failed or not found — try scanning AVD directory, then adb
      list_avds_from_directory().or_else(|_| list_android_devices_via_adb())
    }
  }?;

  // Sort: booted first
  emulators.sort_by(|a, b| {
    let a_booted = a.state == STATE_BOOTED;
    let b_booted = b.state == STATE_BOOTED;
    b_booted.cmp(&a_booted)
  });

  Ok(emulators)
}

fn list_android_devices_via_adb() -> Result<Vec<AndroidEmulator>, String> {
  let adb_cmd = get_adb_cmd()?;

  let output = std::process::Command::new(&adb_cmd)
    .args(["devices", "-l"])
    .output()
    .map_err(|e| format!("Failed to run adb command: {}", e))?;

  if !output.status.success() {
    return Err(format!(
      "adb devices failed: {}",
      String::from_utf8_lossy(&output.stderr)
    ));
  }

  let stdout = String::from_utf8_lossy(&output.stdout);
  Ok(
    stdout
      .lines()
      .skip(1)
      .filter(|line| !line.is_empty() && line.contains("device"))
      .filter_map(|line| {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let id = parts.first().map(|s| s.to_string())?;
        let name = parts
          .iter()
          .find(|p| p.starts_with("model:"))
          .and_then(|p| p.strip_prefix("model:"))
          .map(|s| s.to_string())
          .unwrap_or_else(|| id.clone());

        Some(AndroidEmulator {
          name,
          id,
          device_type: DEVICE_TYPE_RUNNING.to_string(),
          state: STATE_BOOTED.to_string(),
        })
      })
      .collect(),
  )
}

#[cfg(target_os = "macos")]
pub fn list_ios_simulators() -> Result<Vec<IOSSimulator>, String> {
  let xcrun = get_xcrun_cmd()?;

  let output = std::process::Command::new(&xcrun)
    .args(["simctl", "list", "devices", "available", "--json"])
    .output()
    .map_err(|e| format!("Failed to run xcrun simctl: {}", e))?;

  if !output.status.success() {
    return Err(format!(
      "xcrun simctl failed: {}",
      String::from_utf8_lossy(&output.stderr)
    ));
  }

  let json = String::from_utf8_lossy(&output.stdout);
  parse_ios_simulators(&json)
}

#[cfg(not(target_os = "macos"))]
pub fn list_ios_simulators() -> Result<Vec<IOSSimulator>, String> {
  Err(ERR_IOS_ONLY_MACOS.to_string())
}

#[cfg(target_os = "macos")]
fn parse_ios_simulators(json: &str) -> Result<Vec<IOSSimulator>, String> {
  #[derive(serde::Deserialize)]
  struct DevicesResponse {
    devices: serde_json::Value,
  }

  let response: DevicesResponse =
    serde_json::from_str(json).map_err(|e| format!("Failed to parse simctl JSON: {}", e))?;

  let mut simulators = Vec::new();

  if let Some(devices_map) = response.devices.as_object() {
    for (runtime, devices) in devices_map {
      if let Some(device_list) = devices.as_array() {
        for device in device_list {
          let Some(name) = device.get("name").and_then(|v| v.as_str()) else {
            continue;
          };
          let Some(udid) = device.get("udid").and_then(|v| v.as_str()) else {
            continue;
          };
          let Some(state) = device.get("state").and_then(|v| v.as_str()) else {
            continue;
          };

          if matches!(state, STATE_BOOTED | STATE_SHUTDOWN | STATE_AVAILABLE) {
            simulators.push(IOSSimulator {
              name: name.to_string(),
              udid: udid.to_string(),
              state: state.to_string(),
              runtime: runtime.clone(),
            });
          }
        }
      }
    }
  }

  Ok(simulators)
}

pub fn open_android_emulator(name: &str) -> Result<String, String> {
  let emulator_cmd = get_android_emulator_cmd()?;

  std::process::Command::new(&emulator_cmd)
    .args(["-avd", name])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .map_err(|e| format!("Failed to launch emulator '{}': {}", name, e))?;

  Ok(format!("Launching Android emulator: {}", name))
}

#[cfg(target_os = "macos")]
pub fn open_ios_simulator(udid: &str) -> Result<String, String> {
  let xcrun = get_xcrun_cmd()?;

  let boot_output = std::process::Command::new(&xcrun)
    .args(["simctl", "boot", udid])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .output();

  match boot_output {
    Ok(result) => {
      let stderr = String::from_utf8_lossy(&result.stderr);
      if !result.status.success()
        && !stderr.contains("Unable to boot device in current state: Booted")
      {
        return Err(format!("Failed to boot simulator: {}", stderr));
      }
    }
    Err(e) => return Err(format!("Failed to run simctl boot: {}", e)),
  }

  let _ = std::process::Command::new("open")
    .args(["-a", "Simulator"])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn();

  Ok(format!("Opening iOS simulator: {}", udid))
}

#[cfg(not(target_os = "macos"))]
pub fn open_ios_simulator(_udid: &str) -> Result<String, String> {
  Err(ERR_IOS_ONLY_MACOS.to_string())
}

pub fn find_emulator(name: &str) -> Result<EmulatorType, String> {
  if let Ok(android) = list_android_emulators() {
    if let Some(emu) = android.iter().find(|e| e.name == name || e.id == name) {
      return Ok(EmulatorType::Android(emu.id.clone()));
    }
  }

  if let Ok(ios) = list_ios_simulators() {
    if let Some(sim) = ios.iter().find(|s| s.name == name || s.udid == name) {
      return Ok(EmulatorType::IOS(sim.udid.clone()));
    }
  }

  Err(format!("Emulator '{}' not found", name))
}

/// Collect all emulators into a unified list with section headers
pub fn collect_all_entries() -> Vec<EmulatorEntry> {
  let mut entries = Vec::new();

  let android = list_android_emulators().unwrap_or_default();
  if !android.is_empty() {
    entries.push(EmulatorEntry::SectionHeader(
      SECTION_ANDROID_EMULATORS.to_string(),
    ));
    for emu in android {
      entries.push(EmulatorEntry::Android(emu));
    }
  }

  let ios = list_ios_simulators().unwrap_or_default();
  if !ios.is_empty() {
    entries.push(EmulatorEntry::SectionHeader(
      SECTION_IOS_SIMULATORS.to_string(),
    ));
    for sim in ios {
      entries.push(EmulatorEntry::IOS(sim));
    }
  }

  entries
}

/// Open an emulator entry (non-header)
pub fn open_entry(entry: &EmulatorEntry) -> Result<String, String> {
  match entry {
    EmulatorEntry::Android(e) => open_android_emulator(&e.id),
    EmulatorEntry::IOS(s) => open_ios_simulator(&s.udid),
    EmulatorEntry::SectionHeader(_) => Err("Cannot open a section header".to_string()),
  }
}

/// Format a plain text list for the `list` subcommand
pub fn format_emulator_list() -> String {
  let mut output = String::new();

  match list_android_emulators() {
    Ok(android) if !android.is_empty() => {
      output.push_str(SECTION_ANDROID_EMULATORS);
      output.push_str(":\n");
      for emu in android {
        output.push_str(&format!(
          "  {} [{}] ({})\n",
          emu.name, emu.state, emu.device_type
        ));
      }
      output.push('\n');
    }
    Ok(_) => output.push_str("No Android emulators found\n\n"),
    Err(e) => output.push_str(&format!("Android emulators error: {}\n\n", e)),
  }

  match list_ios_simulators() {
    Ok(ios) if !ios.is_empty() => {
      output.push_str(SECTION_IOS_SIMULATORS);
      output.push_str(":\n");
      for sim in ios {
        output.push_str(&format!(
          "  {} [{}] ({})\n",
          sim.name, sim.state, sim.runtime
        ));
      }
    }
    Ok(_) => output.push_str("No iOS simulators found\n"),
    Err(e) => output.push_str(&format!("iOS simulators error: {}\n", e)),
  }

  output
}
