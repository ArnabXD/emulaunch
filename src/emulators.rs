use crate::config;
use crate::error::{Error, Result};

use std::fmt;
use std::process::Stdio;

// Device types
pub const DEVICE_TYPE_AVD: &str = "AVD";
pub const DEVICE_TYPE_RUNNING: &str = "Running Device";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
  Android,
  Ios,
}

impl Platform {
  /// Heading for this platform's group in lists.
  pub fn section_title(self) -> &'static str {
    match self {
      Platform::Android => "Android Emulators",
      Platform::Ios => "iOS Simulators",
    }
  }

  /// What a device of this platform is called in the detail view.
  pub fn kind(self) -> &'static str {
    match self {
      Platform::Android => "Android Emulator",
      Platform::Ios => "iOS Simulator",
    }
  }

  pub fn id_label(self) -> &'static str {
    match self {
      Platform::Android => "ID",
      Platform::Ios => "UDID",
    }
  }

  /// Label for [`Device::meta`]: the AVD kind on Android, the runtime on iOS.
  pub fn meta_label(self) -> &'static str {
    match self {
      Platform::Android => "Type",
      Platform::Ios => "Runtime",
    }
  }
}

/// Whether a device is running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
  Booted,
  Shutdown,
  /// Any other state reported by the platform tooling, e.g. "Shutting Down".
  Other(String),
}

impl State {
  pub fn is_booted(&self) -> bool {
    matches!(self, State::Booted)
  }

  /// Parse a simctl state. Only used for iOS, but compiled everywhere so the
  /// `Other` variant counts as constructed on every platform.
  #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
  fn from_simctl(state: &str) -> State {
    match state {
      "Booted" => State::Booted,
      "Shutdown" => State::Shutdown,
      other => State::Other(other.to_string()),
    }
  }
}

impl fmt::Display for State {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      State::Booted => f.write_str("Booted"),
      State::Shutdown => f.write_str("Shutdown"),
      State::Other(s) => f.write_str(s),
    }
  }
}

/// An Android emulator (AVD) or iOS simulator.
#[derive(Debug, Clone)]
pub struct Device {
  pub platform: Platform,
  /// Display name.
  pub name: String,
  /// What the platform tooling identifies it by: the AVD id or the simulator UDID.
  pub id: String,
  pub state: State,
  /// Platform-specific extra: the AVD kind on Android, the runtime on iOS.
  pub meta: String,
}

impl Device {
  /// Launch this device.
  pub fn open(&self) -> Result<String> {
    match self.platform {
      Platform::Android => open_android_emulator(&self.id),
      Platform::Ios => open_ios_simulator(&self.id),
    }
  }
}

impl fmt::Display for Device {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{} [{}] ({})", self.name, self.state, self.meta)
  }
}

fn get_android_emulator_cmd() -> Result<String> {
  Ok(config::get_android_emulator_cmd()?)
}

fn get_adb_cmd() -> Result<String> {
  Ok(config::get_adb_cmd()?)
}

#[cfg(target_os = "macos")]
fn get_xcrun_cmd() -> Result<String> {
  Ok(config::get_xcrun_cmd()?)
}

/// Directory holding AVD definitions, following the Android tooling rules:
/// `$ANDROID_AVD_HOME`, else `$ANDROID_USER_HOME/avd`, else `~/.android/avd`.
fn avd_home() -> Option<std::path::PathBuf> {
  let env_dir = |var| std::env::var_os(var).filter(|v| !v.is_empty());
  resolve_avd_home(
    env_dir("ANDROID_AVD_HOME"),
    env_dir("ANDROID_USER_HOME"),
    dirs::home_dir(),
  )
}

/// The precedence rules behind [`avd_home`], with the environment passed in.
fn resolve_avd_home(
  avd_home: Option<std::ffi::OsString>,
  user_home: Option<std::ffi::OsString>,
  home: Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
  if let Some(dir) = avd_home {
    return Some(dir.into());
  }
  if let Some(dir) = user_home {
    return Some(std::path::PathBuf::from(dir).join("avd"));
  }
  home.map(|h| h.join(".android").join("avd"))
}

/// The `avd.ini.displayname` value from the text of an AVD's config.ini.
fn display_name_from_config(contents: &str) -> Option<String> {
  contents
    .lines()
    .find_map(|line| line.strip_prefix("avd.ini.displayname="))
    .map(|s| s.trim().to_string())
}

/// Read the display name from an AVD's config.ini
fn read_display_name(avd_dir: &std::path::Path, avd_id: &str) -> Option<String> {
  let config_path = avd_dir.join(format!("{}.avd", avd_id)).join("config.ini");
  display_name_from_config(&std::fs::read_to_string(config_path).ok()?)
}

fn get_avd_display_name(avd_id: &str) -> Option<String> {
  read_display_name(&avd_home()?, avd_id)
}

/// Serials of the emulators that `adb devices` reports as online.
fn running_emulator_serials(adb_devices_output: &str) -> Vec<String> {
  adb_devices_output
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
    .collect()
}

/// The AVD id from the output of `adb -s <serial> emu avd name`.
fn avd_name_from_emu_output(output: &str) -> Option<String> {
  output.lines().next().map(|s| s.trim().to_string())
}

/// Get the set of AVD names that are currently running via adb
fn get_running_avd_names() -> Result<Vec<String>> {
  let adb_cmd = get_adb_cmd()?;

  let output = std::process::Command::new(&adb_cmd)
    .args(["devices"])
    .output()
    .map_err(Error::io("Failed to run adb devices"))?;

  if !output.status.success() {
    return Ok(Vec::new());
  }

  let serials = running_emulator_serials(&String::from_utf8_lossy(&output.stdout));

  let names = serials
    .iter()
    .filter_map(|serial| {
      let result = std::process::Command::new(&adb_cmd)
        .args(["-s", serial, "emu", "avd", "name"])
        .output()
        .ok()?;
      if result.status.success() {
        avd_name_from_emu_output(&String::from_utf8_lossy(&result.stdout))
      } else {
        None
      }
    })
    .collect();

  Ok(names)
}

/// List AVDs by scanning the AVD directory
fn list_avds_from_directory() -> Result<Vec<Device>> {
  scan_avd_dir(&avd_home().ok_or(Error::NoAvdDirectory)?)
}

/// List the AVDs defined in `avd_dir`: each `<id>.ini` that has a matching `<id>.avd/` directory.
fn scan_avd_dir(avd_dir: &std::path::Path) -> Result<Vec<Device>> {
  let entries = std::fs::read_dir(avd_dir).map_err(Error::io("Cannot read AVD directory"))?;

  let mut emulators = Vec::new();
  for entry in entries.flatten() {
    let path = entry.path();
    if path.extension().and_then(|e| e.to_str()) == Some("ini") && !path.is_dir() {
      if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        // Verify the .avd directory exists
        if avd_dir.join(format!("{}.avd", stem)).is_dir() {
          let display_name = read_display_name(avd_dir, stem).unwrap_or_else(|| stem.to_string());
          emulators.push(Device {
            platform: Platform::Android,
            name: display_name,
            id: stem.to_string(),
            state: State::Shutdown,
            meta: DEVICE_TYPE_AVD.to_string(),
          });
        }
      }
    }
  }

  if emulators.is_empty() {
    Err(Error::NoAvds(avd_dir.to_path_buf()))
  } else {
    // read_dir order is arbitrary; keep the list stable between runs
    emulators.sort_by_cached_key(|d| (d.name.to_lowercase(), d.id.clone()));
    Ok(emulators)
  }
}

pub fn list_android_emulators() -> Result<Vec<Device>> {
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
              State::Booted
            } else {
              State::Shutdown
            };
            Device {
              platform: Platform::Android,
              name,
              id,
              state,
              meta: DEVICE_TYPE_AVD.to_string(),
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
  emulators.sort_by_key(|d| std::cmp::Reverse(d.state.is_booted()));

  Ok(emulators)
}

fn list_android_devices_via_adb() -> Result<Vec<Device>> {
  let adb_cmd = get_adb_cmd()?;

  let output = std::process::Command::new(&adb_cmd)
    .args(["devices", "-l"])
    .output()
    .map_err(Error::io("Failed to run adb command"))?;

  if !output.status.success() {
    return Err(Error::CommandFailed {
      tool: "adb devices",
      stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    });
  }

  Ok(parse_adb_devices_long(&String::from_utf8_lossy(
    &output.stdout,
  )))
}

/// Parse the output of `adb devices -l` into running devices.
///
/// Each device line is `<serial> <state> key:value...`. Only lines whose state
/// is `device` are kept, which also drops the header, any `* daemon ...`
/// notices, and devices that are `offline` or `unauthorized`.
fn parse_adb_devices_long(output: &str) -> Vec<Device> {
  output
    .lines()
    .filter_map(|line| {
      let mut parts = line.split_whitespace();
      let id = parts.next()?.to_string();
      if parts.next()? != "device" {
        return None;
      }
      let name = parts
        .find_map(|p| p.strip_prefix("model:"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| id.clone());

      Some(Device {
        platform: Platform::Android,
        name,
        id,
        state: State::Booted,
        meta: DEVICE_TYPE_RUNNING.to_string(),
      })
    })
    .collect()
}

#[cfg(target_os = "macos")]
pub fn list_ios_simulators() -> Result<Vec<Device>> {
  let xcrun = get_xcrun_cmd()?;

  let output = std::process::Command::new(&xcrun)
    .args(["simctl", "list", "devices", "available", "--json"])
    .output()
    .map_err(Error::io("Failed to run xcrun simctl"))?;

  if !output.status.success() {
    return Err(Error::CommandFailed {
      tool: "xcrun simctl",
      stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    });
  }

  let json = String::from_utf8_lossy(&output.stdout);
  parse_ios_simulators(&json)
}

#[cfg(not(target_os = "macos"))]
pub fn list_ios_simulators() -> Result<Vec<Device>> {
  Err(Error::IosUnsupported)
}

/// Turn a simctl runtime identifier such as
/// `com.apple.CoreSimulator.SimRuntime.iOS-17-0` into `iOS 17.0`. Any
/// non-numeric trailing part (device-specific runtimes such as
/// `iOS-27-2-iPhone19-4`) is appended after a dash. Unrecognised identifiers
/// are returned unchanged.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn pretty_runtime(identifier: &str) -> String {
  let Some(rest) = identifier.strip_prefix("com.apple.CoreSimulator.SimRuntime.") else {
    return identifier.to_string();
  };
  let mut parts = rest.split('-');
  let platform = parts.next().unwrap_or_default();
  let parts: Vec<&str> = parts.collect();
  let numeric = parts
    .iter()
    .take_while(|p| p.chars().all(|c| c.is_ascii_digit()))
    .count();
  let version = parts[..numeric].join(".");
  let suffix = parts[numeric..].join("-");
  match (version.is_empty(), suffix.is_empty()) {
    (true, true) => platform.to_string(),
    (true, false) => format!("{} - {}", platform, suffix),
    (false, true) => format!("{} {}", platform, version),
    (false, false) => format!("{} {} - {}", platform, version, suffix),
  }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_ios_simulators(json: &str) -> Result<Vec<Device>> {
  #[derive(serde::Deserialize)]
  struct DevicesResponse {
    devices: serde_json::Value,
  }

  let response: DevicesResponse = serde_json::from_str(json)?;

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

          simulators.push(Device {
            platform: Platform::Ios,
            name: name.to_string(),
            id: udid.to_string(),
            state: State::from_simctl(state),
            meta: pretty_runtime(runtime),
          });
        }
      }
    }
  }

  Ok(simulators)
}

/// Detach the child from our terminal's process group (and, on Windows, from
/// our console) so closing the terminal or pressing Ctrl+C does not kill it.
fn detach(cmd: &mut std::process::Command) {
  #[cfg(unix)]
  {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
  }
  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
  }
}

/// How long to watch a freshly spawned emulator for an immediate failure.
const LAUNCH_GRACE: std::time::Duration = std::time::Duration::from_millis(1500);

/// Last few lines of the emulator log, for error messages.
fn log_tail(path: &std::path::Path) -> String {
  tail_lines(&std::fs::read_to_string(path).unwrap_or_default(), 8)
}

/// The last `count` lines of `text`.
fn tail_lines(text: &str, count: usize) -> String {
  let lines: Vec<&str> = text.lines().collect();
  lines[lines.len().saturating_sub(count)..].join("\n")
}

pub fn open_android_emulator(avd: &str) -> Result<String> {
  let emulator_cmd = get_android_emulator_cmd()?;

  // A second `emulator -avd` for the same AVD refuses to start
  if get_running_avd_names()
    .unwrap_or_default()
    .iter()
    .any(|n| n == avd)
  {
    return Err(Error::AlreadyRunning(avd.to_string()));
  }

  // Send output to a log file rather than a pipe: a pipe would break (SIGPIPE)
  // once we exit while the emulator keeps running.
  let log_path = std::env::temp_dir().join(format!("emulaunch-{}.log", avd));
  let log = std::fs::File::create(&log_path).map_err(Error::io(format!(
    "Cannot create log file {}",
    log_path.display()
  )))?;
  let log_err = log.try_clone().map_err(Error::io(format!(
    "Cannot open log file {}",
    log_path.display()
  )))?;

  let mut cmd = std::process::Command::new(&emulator_cmd);
  cmd
    .args(["-avd", avd])
    .stdin(Stdio::null())
    .stdout(log)
    .stderr(log_err);
  detach(&mut cmd);

  let mut child = cmd
    .spawn()
    .map_err(Error::io(format!("Failed to launch emulator '{}'", avd)))?;

  // Real startup takes much longer than this, but configuration errors (bad
  // AVD, missing system image, ...) make the emulator exit almost immediately.
  let deadline = std::time::Instant::now() + LAUNCH_GRACE;
  while std::time::Instant::now() < deadline {
    match child.try_wait() {
      Ok(Some(status)) => {
        return Err(Error::ExitedEarly {
          avd: avd.to_string(),
          status,
          log_tail: log_tail(&log_path),
          log_path,
        });
      }
      Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
      Err(_) => break,
    }
  }

  Ok(format!(
    "Launching Android emulator: {} (log: {})",
    avd,
    log_path.display()
  ))
}

/// Bring up the app that displays booted simulators and report whether one
/// could be opened.
///
/// Xcode 27 replaced Simulator.app with Device Hub, so try Simulator.app
/// first (older Xcode; it honours -CurrentDeviceUDID when it is not already
/// running) and fall back to Device Hub, which we cannot point at a device.
/// Apps are addressed by bundle identifier because the app name differs.
#[cfg(target_os = "macos")]
fn open_simulator_ui(udid: &str) -> bool {
  let open = |args: &[&str]| {
    std::process::Command::new("open")
      .args(args)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .status()
      .map(|s| s.success())
      .unwrap_or(false)
  };
  open(&[
    "-b",
    "com.apple.iphonesimulator",
    "--args",
    "-CurrentDeviceUDID",
    udid,
  ]) || open(&["-b", "com.apple.dt.Devices"])
}

#[cfg(target_os = "macos")]
pub fn open_ios_simulator(udid: &str) -> Result<String> {
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
        return Err(Error::BootFailed(stderr.into_owned()));
      }
    }
    Err(e) => {
      return Err(Error::Io {
        context: "Failed to run simctl boot".to_string(),
        source: e,
      })
    }
  }

  if open_simulator_ui(udid) {
    Ok(format!("Opening iOS simulator: {}", udid))
  } else {
    Ok(format!(
      "Booted iOS simulator: {} (could not open Simulator or Device Hub)",
      udid
    ))
  }
}

#[cfg(not(target_os = "macos"))]
pub fn open_ios_simulator(_udid: &str) -> Result<String> {
  Err(Error::IosUnsupported)
}

/// Find the single device whose display name or id equals `query`.
///
/// Fails when nothing matches, or when several do (the same name on both
/// platforms, or on two iOS runtimes) so we never launch the wrong one.
pub fn find_device(query: &str) -> Result<Device> {
  let all = list_android_emulators()
    .unwrap_or_default()
    .into_iter()
    .chain(list_ios_simulators().unwrap_or_default())
    .collect();
  select_device(all, query)
}

/// Pick the one device in `candidates` whose name or id equals `query`.
fn select_device(candidates: Vec<Device>, query: &str) -> Result<Device> {
  let mut matches: Vec<Device> = candidates
    .into_iter()
    .filter(|d| d.name == query || d.id == query)
    .collect();

  match matches.len() {
    0 => Err(Error::NotFound(query.to_string())),
    1 => Ok(matches.remove(0)),
    _ => {
      let choices: Vec<String> = matches
        .iter()
        .map(|d| {
          format!(
            "  {} ({}, {}: {})",
            d.name,
            d.meta,
            d.platform.id_label(),
            d.id
          )
        })
        .collect();
      Err(Error::Ambiguous {
        query: query.to_string(),
        count: matches.len(),
        choices: choices.join("\n"),
      })
    }
  }
}

/// Result of gathering every platform's devices for the TUI.
pub struct Collected {
  /// Android devices first, then iOS.
  pub devices: Vec<Device>,
  /// Why a platform could not be listed (e.g. a missing `adb`). Platforms that
  /// are simply unsupported on this OS are not reported.
  pub errors: Vec<Error>,
}

/// List the devices of every platform.
pub fn collect_devices() -> Collected {
  let (android, ios) = list_all_platforms();
  let mut devices = Vec::new();
  let mut errors = Vec::new();

  match android {
    Ok(android) => devices.extend(android),
    Err(e) => errors.push(e),
  }

  match ios {
    Ok(ios) => devices.extend(ios),
    Err(Error::IosUnsupported) => {}
    Err(e) => errors.push(e),
  }

  Collected { devices, errors }
}

/// List Android and iOS devices at the same time. Each side waits on external
/// tools (`emulator`, `adb`, `xcrun`), so running them together takes as long
/// as the slower one instead of the sum.
fn list_all_platforms() -> (Result<Vec<Device>>, Result<Vec<Device>>) {
  std::thread::scope(|scope| {
    let ios = scope.spawn(list_ios_simulators);
    let android = list_android_emulators();
    (android, ios.join().expect("iOS listing thread panicked"))
  })
}

/// Format a plain text list for the `list` subcommand
pub fn format_emulator_list() -> String {
  let mut output = String::new();

  let (android, ios) = list_all_platforms();
  let sections = [
    (
      Platform::Android,
      android,
      "No Android emulators found",
      "Android emulators",
    ),
    (
      Platform::Ios,
      ios,
      "No iOS simulators found",
      "iOS simulators",
    ),
  ];
  for (i, (platform, result, none_found, error_prefix)) in sections.into_iter().enumerate() {
    if i > 0 {
      output.push('\n');
    }
    match result {
      Ok(devices) if !devices.is_empty() => {
        output.push_str(platform.section_title());
        output.push_str(":\n");
        for device in devices {
          output.push_str(&format!("  {}\n", device));
        }
      }
      Ok(_) => output.push_str(&format!("{}\n", none_found)),
      Err(e) => output.push_str(&format!("{} error: {}\n", error_prefix, e)),
    }
  }

  output
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::ffi::OsString;
  use std::path::{Path, PathBuf};
  use std::sync::atomic::{AtomicUsize, Ordering};

  fn device(platform: Platform, name: &str, id: &str) -> Device {
    Device {
      platform,
      name: name.to_string(),
      id: id.to_string(),
      state: State::Shutdown,
      meta: String::new(),
    }
  }

  /// A scratch directory that is removed when dropped.
  struct TempDir(PathBuf);

  impl TempDir {
    fn new() -> Self {
      static COUNTER: AtomicUsize = AtomicUsize::new(0);
      let dir = std::env::temp_dir().join(format!(
        "emulaunch-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
      ));
      std::fs::create_dir_all(&dir).unwrap();
      TempDir(dir)
    }

    fn path(&self) -> &Path {
      &self.0
    }

    fn file(&self, relative: &str, contents: &str) {
      let path = self.0.join(relative);
      std::fs::create_dir_all(path.parent().unwrap()).unwrap();
      std::fs::write(path, contents).unwrap();
    }

    fn dir(&self, relative: &str) {
      std::fs::create_dir_all(self.0.join(relative)).unwrap();
    }
  }

  impl Drop for TempDir {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.0);
    }
  }

  // --- AVD home --------------------------------------------------------

  #[test]
  fn avd_home_prefers_android_avd_home() {
    let dir = resolve_avd_home(
      Some(OsString::from("/custom/avds")),
      Some(OsString::from("/user")),
      Some(PathBuf::from("/home/me")),
    );
    assert_eq!(dir, Some(PathBuf::from("/custom/avds")));
  }

  #[test]
  fn avd_home_falls_back_to_android_user_home() {
    let dir = resolve_avd_home(
      None,
      Some(OsString::from("/user")),
      Some(PathBuf::from("/home/me")),
    );
    assert_eq!(dir, Some(PathBuf::from("/user/avd")));
  }

  #[test]
  fn avd_home_defaults_to_dot_android_in_home() {
    let dir = resolve_avd_home(None, None, Some(PathBuf::from("/home/me")));
    assert_eq!(dir, Some(PathBuf::from("/home/me/.android/avd")));
  }

  #[test]
  fn avd_home_is_none_without_any_input() {
    assert_eq!(resolve_avd_home(None, None, None), None);
  }

  // --- config.ini ------------------------------------------------------

  #[test]
  fn display_name_is_read_from_config_ini() {
    let ini = "AvdId=Pixel_8_Pro\navd.ini.displayname=Pixel 8 Pro\nhw.ramSize=2048\n";
    assert_eq!(
      display_name_from_config(ini),
      Some("Pixel 8 Pro".to_string())
    );
  }

  #[test]
  fn display_name_is_trimmed() {
    assert_eq!(
      display_name_from_config("avd.ini.displayname=  Spaced  \r\n"),
      Some("Spaced".to_string())
    );
  }

  #[test]
  fn display_name_is_none_when_key_is_missing() {
    assert_eq!(
      display_name_from_config("AvdId=Foo\nhw.ramSize=2048\n"),
      None
    );
    assert_eq!(display_name_from_config(""), None);
  }

  // --- adb -------------------------------------------------------------

  const ADB_DEVICES: &str = "List of devices attached\n\
emulator-5554\tdevice\n\
emulator-5556\toffline\n\
R58M12ABCDE\tdevice\n\
emulator-5558\tunauthorized\n";

  #[test]
  fn only_online_emulators_are_reported_as_running() {
    assert_eq!(running_emulator_serials(ADB_DEVICES), vec!["emulator-5554"]);
  }

  #[test]
  fn no_running_emulators_in_header_only_output() {
    assert!(running_emulator_serials("List of devices attached\n\n").is_empty());
    assert!(running_emulator_serials("").is_empty());
  }

  #[test]
  fn avd_name_is_the_first_line_of_emu_output() {
    assert_eq!(
      avd_name_from_emu_output("Pixel_8_Pro\r\nOK\r\n"),
      Some("Pixel_8_Pro".to_string())
    );
    assert_eq!(avd_name_from_emu_output(""), None);
  }

  #[test]
  fn adb_long_listing_uses_the_model_as_the_name() {
    let output = "List of devices attached\n\
emulator-5554          device product:sdk_gphone64 model:Pixel_8 device:emu64a transport_id:1\n\
0123456789ABCDEF       device usb:1-1 transport_id:2\n";
    let devices = parse_adb_devices_long(output);

    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].name, "Pixel_8");
    assert_eq!(devices[0].id, "emulator-5554");
    assert_eq!(devices[0].meta, DEVICE_TYPE_RUNNING);
    assert!(devices[0].state.is_booted());
    // Without a model token the serial is used as the name
    assert_eq!(devices[1].name, "0123456789ABCDEF");
  }

  #[test]
  fn adb_long_listing_only_includes_devices_in_the_device_state() {
    let output = "* daemon not running; starting now at tcp:5037\n\
* daemon started successfully\n\
List of devices attached\n\
emulator-5554          device product:sdk model:Pixel_8 device:emu64a transport_id:1\n\
emulator-5556          offline transport_id:2\n\
R58M12ABCDE            unauthorized usb:1-1 transport_id:3\n\
\n";
    let devices = parse_adb_devices_long(output);

    assert_eq!(
      devices.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
      vec!["emulator-5554"]
    );
  }

  // --- AVD directory scan ----------------------------------------------

  #[test]
  fn scan_finds_avds_that_have_both_ini_and_avd_directory() {
    let tmp = TempDir::new();
    tmp.file("Pixel_8.ini", "path=x\n");
    tmp.file("Pixel_8.avd/config.ini", "avd.ini.displayname=Pixel 8\n");
    tmp.file("Plain.ini", "path=x\n");
    tmp.dir("Plain.avd");

    let found = scan_avd_dir(tmp.path()).unwrap();

    assert_eq!(found.len(), 2);
    // Sorted by display name, case-insensitively
    assert_eq!(
      (found[0].id.as_str(), found[0].name.as_str()),
      ("Pixel_8", "Pixel 8")
    );
    assert_eq!(
      (found[1].id.as_str(), found[1].name.as_str()),
      ("Plain", "Plain")
    );
    assert!(found.iter().all(|d| d.platform == Platform::Android));
    assert!(found.iter().all(|d| d.state == State::Shutdown));
    assert!(found.iter().all(|d| d.meta == DEVICE_TYPE_AVD));
  }

  #[test]
  fn scan_returns_avds_sorted_by_display_name() {
    let tmp = TempDir::new();
    for (id, name) in [("c_id", "charlie"), ("a_id", "Bravo"), ("b_id", "alpha")] {
      tmp.file(&format!("{id}.ini"), "path=x\n");
      tmp.file(
        &format!("{id}.avd/config.ini"),
        &format!("avd.ini.displayname={name}\n"),
      );
    }

    let names: Vec<String> = scan_avd_dir(tmp.path())
      .unwrap()
      .into_iter()
      .map(|d| d.name)
      .collect();

    assert_eq!(names, vec!["alpha", "Bravo", "charlie"]);
  }

  #[test]
  fn scan_ignores_incomplete_avds() {
    let tmp = TempDir::new();
    tmp.file("NoDir.ini", "path=x\n"); // .ini without an .avd directory
    tmp.dir("OnlyDir.avd"); // .avd directory without an .ini
    tmp.file("notes.txt", "hello"); // unrelated file

    assert!(matches!(
      scan_avd_dir(tmp.path()),
      Err(Error::NoAvds(dir)) if dir == tmp.path()
    ));
  }

  #[test]
  fn scan_reports_an_unreadable_directory() {
    let tmp = TempDir::new();
    let missing = tmp.path().join("does-not-exist");
    assert!(matches!(scan_avd_dir(&missing), Err(Error::Io { .. })));
  }

  // --- choosing a device -----------------------------------------------

  #[test]
  fn select_device_matches_by_name_or_id() {
    let devices = vec![
      device(Platform::Android, "Pixel 8", "Pixel_8"),
      device(Platform::Ios, "iPhone 17", "UDID-1"),
    ];

    assert_eq!(
      select_device(devices.clone(), "Pixel 8").unwrap().id,
      "Pixel_8"
    );
    assert_eq!(
      select_device(devices.clone(), "Pixel_8").unwrap().id,
      "Pixel_8"
    );
    assert_eq!(select_device(devices, "UDID-1").unwrap().name, "iPhone 17");
  }

  #[test]
  fn select_device_reports_missing_devices() {
    let devices = vec![device(Platform::Android, "Pixel 8", "Pixel_8")];
    assert!(matches!(
      select_device(devices, "nope"),
      Err(Error::NotFound(name)) if name == "nope"
    ));
  }

  #[test]
  fn select_device_refuses_ambiguous_names_and_lists_the_choices() {
    let devices = vec![
      device(Platform::Android, "iPhone 17", "iphone17"),
      device(Platform::Ios, "iPhone 17", "UDID-1"),
    ];

    match select_device(devices.clone(), "iPhone 17") {
      Err(Error::Ambiguous { count, choices, .. }) => {
        assert_eq!(count, 2);
        assert!(choices.contains("iphone17"));
        assert!(choices.contains("UDID-1"));
      }
      other => panic!("expected an ambiguity error, got {:?}", other),
    }

    // The id disambiguates
    assert_eq!(
      select_device(devices, "UDID-1").unwrap().platform,
      Platform::Ios
    );
  }

  // --- simctl ----------------------------------------------------------

  #[test]
  fn runtime_identifiers_are_made_readable() {
    let cases = [
      ("com.apple.CoreSimulator.SimRuntime.iOS-17-0", "iOS 17.0"),
      (
        "com.apple.CoreSimulator.SimRuntime.iOS-26-4-1",
        "iOS 26.4.1",
      ),
      (
        "com.apple.CoreSimulator.SimRuntime.watchOS-10-2",
        "watchOS 10.2",
      ),
      (
        "com.apple.CoreSimulator.SimRuntime.iOS-27-2-iPhone19-4",
        "iOS 27.2 - iPhone19-4",
      ),
      ("com.apple.CoreSimulator.SimRuntime.visionOS", "visionOS"),
      ("something.else", "something.else"),
    ];
    for (input, expected) in cases {
      assert_eq!(pretty_runtime(input), expected, "for {}", input);
    }
  }

  const SIMCTL_JSON: &str = r#"{
    "devices": {
      "com.apple.CoreSimulator.SimRuntime.iOS-17-0": [
        {"name": "iPhone 15", "udid": "AAAA", "state": "Shutdown", "isAvailable": true},
        {"name": "iPhone 15 Pro", "udid": "BBBB", "state": "Booted", "isAvailable": true},
        {"name": "Broken entry without udid", "state": "Shutdown"}
      ],
      "com.apple.CoreSimulator.SimRuntime.iOS-16-4": [
        {"name": "iPad", "udid": "CCCC", "state": "Shutting Down"}
      ],
      "com.apple.CoreSimulator.SimRuntime.tvOS-17-0": []
    }
  }"#;

  #[test]
  fn simctl_json_is_parsed_into_devices() {
    let mut devices = parse_ios_simulators(SIMCTL_JSON).unwrap();
    devices.sort_by(|a, b| a.id.cmp(&b.id));

    assert_eq!(devices.len(), 3, "entry without a udid is skipped");
    assert!(devices.iter().all(|d| d.platform == Platform::Ios));

    assert_eq!(devices[0].name, "iPhone 15");
    assert_eq!(devices[0].state, State::Shutdown);
    assert_eq!(devices[0].meta, "iOS 17.0");

    assert_eq!(devices[1].state, State::Booted);

    assert_eq!(devices[2].name, "iPad");
    assert_eq!(devices[2].state, State::Other("Shutting Down".to_string()));
    assert_eq!(devices[2].meta, "iOS 16.4");
  }

  #[test]
  fn simctl_json_without_devices_gives_an_empty_list() {
    assert!(parse_ios_simulators(r#"{"devices": {}}"#)
      .unwrap()
      .is_empty());
  }

  #[test]
  fn invalid_simctl_json_is_an_error() {
    assert!(matches!(
      parse_ios_simulators("not json"),
      Err(Error::ParseSimctl(_))
    ));
  }

  // --- small helpers ---------------------------------------------------

  #[test]
  fn tail_lines_keeps_the_last_lines() {
    assert_eq!(tail_lines("a\nb\nc\nd", 2), "c\nd");
    assert_eq!(tail_lines("a\nb", 8), "a\nb");
    assert_eq!(tail_lines("", 8), "");
  }

  #[test]
  fn device_display_is_name_state_and_meta() {
    let mut d = device(Platform::Ios, "iPhone 15", "AAAA");
    d.state = State::Booted;
    d.meta = "iOS 17.0".to_string();
    assert_eq!(d.to_string(), "iPhone 15 [Booted] (iOS 17.0)");
  }

  #[test]
  fn state_text_and_booted_flag() {
    assert_eq!(State::Booted.to_string(), "Booted");
    assert_eq!(State::Shutdown.to_string(), "Shutdown");
    assert_eq!(State::Other("Creating".into()).to_string(), "Creating");
    assert!(State::Booted.is_booted());
    assert!(!State::Shutdown.is_booted());
    assert!(!State::Other("Creating".into()).is_booted());
  }
}
