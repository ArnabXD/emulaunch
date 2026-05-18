use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
  Terminal,
};
use std::io;
use std::sync::mpsc;

use crate::theme::ThemeColors;

const API_LEVELS: &[(&str, &str)] = &[
  ("35", "Android 15  (API 35)"),
  ("34", "Android 14  (API 34)  <- recommended"),
  ("33", "Android 13  (API 33)"),
  ("32", "Android 12L (API 32)"),
  ("31", "Android 12  (API 31)"),
];

const IMAGE_TYPES: &[(&str, &str, &str)] = &[
  (
    "google_apis_playstore",
    "Google Play",
    "Google Play Store + APIs (recommended)",
  ),
  ("google_apis", "Google APIs", "Google APIs, no Play Store"),
  ("default", "AOSP", "Pure Android Open Source"),
];

const DEVICE_PROFILES: &[(&str, &str)] = &[
  ("pixel_7", "Pixel 7"),
  ("pixel_7_pro", "Pixel 7 Pro"),
  ("pixel_6", "Pixel 6"),
  ("pixel_5", "Pixel 5"),
  ("pixel_4", "Pixel 4"),
  ("pixel_tablet", "Pixel Tablet"),
  ("Nexus 5X", "Nexus 5X"),
];

pub struct SdkStatus {
  pub sdk_root: Option<String>,
  pub sdkmanager_path: Option<String>,
  pub avdmanager_path: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Step {
  Welcome,
  ApiLevel,
  ImageType,
  DeviceProfile,
  AvdName,
  Confirm,
  Downloading,
  Done,
}

enum Msg {
  Line(String),
  Done,
  Err(String),
}

struct Wizard {
  step: Step,
  sdk: SdkStatus,
  api_idx: usize,
  image_idx: usize,
  device_idx: usize,
  avd_name: String,
  list_state: ListState,
  log: Vec<String>,
  finished: bool,
  cancelled: bool,
  error: Option<String>,
  rx: Option<mpsc::Receiver<Msg>>,
}

impl Wizard {
  fn new(sdk: SdkStatus) -> Self {
    let mut list_state = ListState::default();
    list_state.select(Some(1));
    Wizard {
      step: Step::Welcome,
      sdk,
      api_idx: 1,
      image_idx: 0,
      device_idx: 0,
      avd_name: String::new(),
      list_state,
      log: Vec::new(),
      finished: false,
      cancelled: false,
      error: None,
      rx: None,
    }
  }

  fn move_sel(&mut self, d: i32) {
    let len = match self.step {
      Step::ApiLevel => API_LEVELS.len(),
      Step::ImageType => IMAGE_TYPES.len(),
      Step::DeviceProfile => DEVICE_PROFILES.len(),
      _ => return,
    };
    let cur = self.list_state.selected().unwrap_or(0);
    let nxt = if d > 0 {
      (cur + 1).min(len - 1)
    } else {
      cur.saturating_sub(1)
    };
    self.list_state.select(Some(nxt));
  }

  fn confirm_sel(&mut self) {
    let sel = self.list_state.selected().unwrap_or(0);
    match self.step {
      Step::ApiLevel => {
        self.api_idx = sel;
        self.step = Step::ImageType;
        self.list_state.select(Some(self.image_idx));
      }
      Step::ImageType => {
        self.image_idx = sel;
        self.step = Step::DeviceProfile;
        self.list_state.select(Some(self.device_idx));
      }
      Step::DeviceProfile => {
        self.device_idx = sel;
        self.step = Step::AvdName;
        if self.avd_name.is_empty() {
          let dev = DEVICE_PROFILES[self.device_idx].0.replace(' ', "_");
          let api = API_LEVELS[self.api_idx].0;
          self.avd_name = format!("{}_{}", dev, api);
        }
      }
      _ => {}
    }
  }

  fn pkg(&self) -> String {
    let api = API_LEVELS[self.api_idx].0;
    let img = IMAGE_TYPES[self.image_idx].0;
    let arch = host_arch();
    format!("system-images;android-{};{};{}", api, img, arch)
  }

  fn start(&mut self) {
    let (tx, rx) = mpsc::channel::<Msg>();
    self.rx = Some(rx);
    self.step = Step::Downloading;
    self.log.push("Starting Android SDK setup...".into());

    let sdkmgr = self.sdk.sdkmanager_path.clone().unwrap_or_default();
    let avdmgr = self.sdk.avdmanager_path.clone().unwrap_or_default();
    let pkg = self.pkg();
    let name = self.avd_name.clone();
    let device = DEVICE_PROFILES[self.device_idx].0.to_string();

    std::thread::spawn(move || {
      // Step 1 — licenses
      send_msg(&tx, "Step 1/4 - Accepting SDK licenses...");
      if let Err(e) = accept_licenses(&sdkmgr) {
        let _ = tx.send(Msg::Err(format!("License error: {}", e)));
        return;
      }
      send_msg(&tx, "Licenses accepted.");

      // Step 2 — platform-tools + emulator (best-effort; don't abort on failure)
      send_msg(&tx, "Step 2/4 - Installing platform-tools & emulator...");
      for tool_pkg in &["platform-tools", "emulator"] {
        if let Err(e) = run_sdkmanager_install(&sdkmgr, tool_pkg, &tx) {
          send_msg(&tx, &format!("  Warning ({}): {}", tool_pkg, e));
        }
      }
      send_msg(&tx, "Platform-tools & emulator ready.");

      // Step 3 — system image
      send_msg(&tx, &format!("Step 3/4 - Downloading system image: {}", pkg));
      if let Err(e) = run_sdkmanager_install(&sdkmgr, &pkg, &tx) {
        let _ = tx.send(Msg::Err(format!("Download error: {}", e)));
        return;
      }
      send_msg(&tx, "System image downloaded.");

      // Step 4 — create AVD
      send_msg(&tx, &format!("Step 4/4 - Creating AVD '{}'...", name));
      match create_avd(&avdmgr, &name, &pkg, &device) {
        Ok(out) => {
          if !out.trim().is_empty() {
            send_msg(&tx, &out);
          }
          send_msg(&tx, &format!("AVD '{}' created successfully!", name));
          let _ = tx.send(Msg::Done);
        }
        Err(e) => {
          let _ = tx.send(Msg::Err(format!("AVD creation error: {}", e)));
        }
      }
    });
  }

  fn poll(&mut self) {
    loop {
      // Borrow self.rx only for try_recv(), releasing it before any mutation below.
      let msg = match self.rx {
        Some(ref rx) => rx.try_recv(),
        None => break,
      };
      match msg {
        Ok(Msg::Line(s)) => {
          if !s.trim().is_empty() {
            self.log.push(s);
          }
        }
        Ok(Msg::Done) => {
          self.step = Step::Done;
          self.rx = None;
          break;
        }
        Ok(Msg::Err(e)) => {
          self.error = Some(e);
          self.step = Step::Done;
          self.rx = None;
          break;
        }
        Err(mpsc::TryRecvError::Empty) => break,
        Err(mpsc::TryRecvError::Disconnected) => {
          self.rx = None;
          break;
        }
      }
    }
  }
}

fn send_msg(tx: &mpsc::Sender<Msg>, msg: &str) {
  for line in msg.lines() {
    let _ = tx.send(Msg::Line(line.to_string()));
  }
}

fn host_arch() -> &'static str {
  #[cfg(target_arch = "aarch64")]
  return "arm64-v8a";
  #[cfg(not(target_arch = "aarch64"))]
  return "x86_64";
}

pub fn detect_sdk_status() -> SdkStatus {
  let sdk_root = detect_sdk_root();
  let (sdkmanager_path, avdmanager_path) = if let Some(ref root) = sdk_root {
    (find_tool(root, "sdkmanager"), find_tool(root, "avdmanager"))
  } else {
    (which_tool("sdkmanager"), which_tool("avdmanager"))
  };
  SdkStatus {
    sdk_root,
    sdkmanager_path,
    avdmanager_path,
  }
}

fn detect_sdk_root() -> Option<String> {
  for var in &["ANDROID_SDK_ROOT", "ANDROID_HOME"] {
    if let Ok(val) = std::env::var(var) {
      if std::path::Path::new(&val).exists() {
        return Some(val);
      }
    }
  }
  let home = dirs::home_dir()?;
  platform_sdk_roots(&home)
    .into_iter()
    .find(|p| std::path::Path::new(p).exists())
}

fn platform_sdk_roots(home: &std::path::Path) -> Vec<String> {
  #[cfg(target_os = "macos")]
  return vec![home.join("Library/Android/sdk").to_string_lossy().into_owned()];

  #[cfg(target_os = "linux")]
  return vec![
    home.join("Android/Sdk").to_string_lossy().into_owned(),
    home.join(".android/sdk").to_string_lossy().into_owned(),
  ];

  #[cfg(target_os = "windows")]
  return vec![home
    .join("AppData/Local/Android/Sdk")
    .to_string_lossy()
    .into_owned()];

  #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
  return vec![];
}

fn find_tool(sdk_root: &str, tool: &str) -> Option<String> {
  #[cfg(target_os = "windows")]
  let ext = ".bat";
  #[cfg(not(target_os = "windows"))]
  let ext = "";

  let candidates = vec![
    format!("{}/cmdline-tools/latest/bin/{}{}", sdk_root, tool, ext),
    format!("{}/cmdline-tools/bin/{}{}", sdk_root, tool, ext),
    format!("{}/tools/bin/{}{}", sdk_root, tool, ext),
    // emulator binary lives in its own dir
    format!("{}/emulator/{}{}", sdk_root, tool, ext),
  ];
  candidates
    .into_iter()
    .find(|p| std::path::Path::new(p).exists())
}

fn which_tool(tool: &str) -> Option<String> {
  #[cfg(target_os = "windows")]
  let which_cmd = "where";
  #[cfg(not(target_os = "windows"))]
  let which_cmd = "which";

  let out = std::process::Command::new(which_cmd)
    .arg(tool)
    .output()
    .ok()?;
  if out.status.success() {
    Some(
      String::from_utf8_lossy(&out.stdout)
        .trim()
        .to_string(),
    )
  } else {
    None
  }
}

fn accept_licenses(sdkmanager: &str) -> Result<(), String> {
  use std::io::Write;
  let mut child = std::process::Command::new(sdkmanager)
    .arg("--licenses")
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .spawn()
    .map_err(|e| format!("Failed to run sdkmanager --licenses: {}", e))?;
  if let Some(mut stdin) = child.stdin.take() {
    let _ = stdin.write_all("y\n".repeat(20).as_bytes());
  }
  let _ = child.wait();
  Ok(())
}

fn run_sdkmanager_install(
  sdkmanager: &str,
  package: &str,
  tx: &mpsc::Sender<Msg>,
) -> Result<(), String> {
  use std::io::Write;

  let mut child = std::process::Command::new(sdkmanager)
    .args(["--install", package])
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .map_err(|e| format!("Failed to run sdkmanager: {}", e))?;

  if let Some(mut stdin) = child.stdin.take() {
    let _ = stdin.write_all("y\n".repeat(5).as_bytes());
  }

  if let Some(stdout) = child.stdout.take() {
    stream_lines(stdout, tx.clone());
  }
  if let Some(stderr) = child.stderr.take() {
    stream_lines(stderr, tx.clone());
  }

  child
    .wait()
    .map_err(|e| format!("sdkmanager install failed: {}", e))?;
  Ok(())
}

/// Reads from `reader` in chunks, splitting on both `\n` and `\r` so that
/// sdkmanager's carriage-return progress updates appear as individual lines.
fn stream_lines(reader: impl io::Read + Send + 'static, tx: mpsc::Sender<Msg>) {
  std::thread::spawn(move || {
    let mut reader = reader;
    let mut buf = [0u8; 512];
    let mut pending = String::new();

    loop {
      match reader.read(&mut buf) {
        Ok(0) | Err(_) => break,
        Ok(n) => {
          pending.push_str(&String::from_utf8_lossy(&buf[..n]));
          loop {
            if let Some(pos) = pending.find(|c: char| c == '\n' || c == '\r') {
              let line = pending[..pos].trim().to_string();
              if !line.is_empty() {
                let _ = tx.send(Msg::Line(line));
              }
              pending.drain(..=pos);
            } else {
              break;
            }
          }
        }
      }
    }
    // Flush anything remaining after the stream closes
    let line = pending.trim().to_string();
    if !line.is_empty() {
      let _ = tx.send(Msg::Line(line));
    }
  });
}

fn create_avd(
  avdmanager: &str,
  name: &str,
  package: &str,
  device_id: &str,
) -> Result<String, String> {
  use std::io::Write;

  let mut child = std::process::Command::new(avdmanager)
    .args([
      "create", "avd", "-n", name, "-k", package, "-d", device_id, "--force",
    ])
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .map_err(|e| format!("Failed to run avdmanager: {}", e))?;

  if let Some(mut stdin) = child.stdin.take() {
    let _ = stdin.write_all(b"no\n");
  }

  let out = child
    .wait_with_output()
    .map_err(|e| format!("avdmanager failed: {}", e))?;

  let stdout = String::from_utf8_lossy(&out.stdout).to_string();
  let stderr = String::from_utf8_lossy(&out.stderr).to_string();

  if !out.status.success() && !stderr.is_empty() {
    return Err(stderr);
  }
  Ok(if stdout.is_empty() { stderr } else { stdout })
}

pub fn run_onboarding(
  terminal: &mut Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>,
  theme: &ThemeColors,
) -> io::Result<bool> {
  let sdk = detect_sdk_status();
  let mut wizard = Wizard::new(sdk);

  loop {
    if wizard.step == Step::Downloading {
      wizard.poll();
    }

    terminal.draw(|frame| {
      render(frame, &mut wizard, theme);
    })?;

    if wizard.cancelled || (wizard.step == Step::Done && wizard.finished) {
      break;
    }

    let timeout = if wizard.step == Step::Downloading {
      std::time::Duration::from_millis(50)
    } else {
      std::time::Duration::from_millis(200)
    };

    if !event::poll(timeout)? {
      continue;
    }

    if let Event::Key(key) = event::read()? {
      if key.kind != KeyEventKind::Press {
        continue;
      }
      handle_key(&mut wizard, key.code);
    }
  }

  Ok(!wizard.cancelled && wizard.error.is_none())
}

fn handle_key(w: &mut Wizard, code: KeyCode) {
  match w.step {
    Step::Welcome => match code {
      KeyCode::Enter => {
        if w.sdk.sdkmanager_path.is_some() {
          w.step = Step::ApiLevel;
          w.list_state.select(Some(w.api_idx));
        }
      }
      KeyCode::Esc | KeyCode::Char('q') => w.cancelled = true,
      _ => {}
    },
    Step::ApiLevel | Step::ImageType | Step::DeviceProfile => match code {
      KeyCode::Up | KeyCode::Char('k') => w.move_sel(-1),
      KeyCode::Down | KeyCode::Char('j') => w.move_sel(1),
      KeyCode::Enter => w.confirm_sel(),
      KeyCode::Esc | KeyCode::Char('q') => w.cancelled = true,
      _ => {}
    },
    Step::AvdName => match code {
      KeyCode::Enter => {
        if !w.avd_name.is_empty() {
          w.step = Step::Confirm;
        }
      }
      KeyCode::Backspace => {
        w.avd_name.pop();
      }
      KeyCode::Esc => w.cancelled = true,
      KeyCode::Char(c) if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' => {
        w.avd_name.push(c);
      }
      _ => {}
    },
    Step::Confirm => match code {
      KeyCode::Enter | KeyCode::Char('y') => w.start(),
      KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('n') => w.cancelled = true,
      _ => {}
    },
    Step::Downloading => match code {
      // Dropping rx signals the background thread to stop sending (it gets SendError).
      KeyCode::Char('q') | KeyCode::Esc => w.cancelled = true,
      _ => {}
    },
    Step::Done => match code {
      KeyCode::Enter | KeyCode::Char('q') | KeyCode::Esc => w.finished = true,
      _ => {}
    },
  }
}

fn render(frame: &mut ratatui::Frame, w: &mut Wizard, theme: &ThemeColors) {
  let area = frame.area();
  let chunks = Layout::vertical([
    Constraint::Length(3),
    Constraint::Min(1),
    Constraint::Length(1),
  ])
  .split(area);

  let step_label = match w.step {
    Step::Welcome => " Android Setup — Welcome ",
    Step::ApiLevel => " Android Setup — API Level (1/4) ",
    Step::ImageType => " Android Setup — System Image (2/4) ",
    Step::DeviceProfile => " Android Setup — Device Profile (3/4) ",
    Step::AvdName => " Android Setup — AVD Name (4/4) ",
    Step::Confirm => " Android Setup — Confirm ",
    Step::Downloading => " Android Setup — Downloading & Creating ",
    Step::Done => " Android Setup — Complete ",
  };

  let title = Paragraph::new(step_label).style(
    Style::default()
      .fg(theme.header_fg)
      .add_modifier(Modifier::BOLD),
  ).block(Block::default().borders(Borders::ALL));
  frame.render_widget(title, chunks[0]);

  match w.step {
    Step::Welcome => render_welcome(frame, w, theme, chunks[1]),
    Step::ApiLevel => {
      let items: Vec<String> = API_LEVELS.iter().map(|(_, d)| d.to_string()).collect();
      render_list(frame, w, theme, chunks[1], " Select Android API Level ", &items);
    }
    Step::ImageType => {
      let items: Vec<String> = IMAGE_TYPES
        .iter()
        .map(|(_, n, d)| format!("{}  —  {}", n, d))
        .collect();
      render_list(frame, w, theme, chunks[1], " Select System Image Type ", &items);
    }
    Step::DeviceProfile => {
      let items: Vec<String> = DEVICE_PROFILES.iter().map(|(_, n)| n.to_string()).collect();
      render_list(frame, w, theme, chunks[1], " Select Device Profile ", &items);
    }
    Step::AvdName => render_avd_name(frame, w, theme, chunks[1]),
    Step::Confirm => render_confirm(frame, w, theme, chunks[1]),
    Step::Downloading => render_progress(frame, w, theme, chunks[1]),
    Step::Done => render_done(frame, w, theme, chunks[1]),
  }

  let has_sdkmgr = w.sdk.sdkmanager_path.is_some();
  let help_line: Line = match w.step {
    Step::Welcome => {
      if has_sdkmgr {
        Line::from(vec![
          Span::styled(" Enter", Style::default().fg(theme.help_key_fg)),
          Span::styled(" start setup  ", Style::default().fg(theme.help_text_fg)),
          Span::styled("q/Esc", Style::default().fg(theme.help_key_fg)),
          Span::styled(" quit", Style::default().fg(theme.help_text_fg)),
        ])
      } else {
        Line::from(vec![
          Span::styled(" q/Esc", Style::default().fg(theme.help_key_fg)),
          Span::styled(" quit", Style::default().fg(theme.help_text_fg)),
        ])
      }
    }
    Step::ApiLevel | Step::ImageType | Step::DeviceProfile => Line::from(vec![
      Span::styled(" j/k/arrows", Style::default().fg(theme.help_key_fg)),
      Span::styled(" navigate  ", Style::default().fg(theme.help_text_fg)),
      Span::styled("Enter", Style::default().fg(theme.help_key_fg)),
      Span::styled(" select  ", Style::default().fg(theme.help_text_fg)),
      Span::styled("q/Esc", Style::default().fg(theme.help_key_fg)),
      Span::styled(" cancel", Style::default().fg(theme.help_text_fg)),
    ]),
    Step::AvdName => Line::from(vec![
      Span::styled(" Enter", Style::default().fg(theme.help_key_fg)),
      Span::styled(" confirm  ", Style::default().fg(theme.help_text_fg)),
      Span::styled("Esc", Style::default().fg(theme.help_key_fg)),
      Span::styled(" cancel", Style::default().fg(theme.help_text_fg)),
    ]),
    Step::Confirm => Line::from(vec![
      Span::styled(" Enter/y", Style::default().fg(theme.help_key_fg)),
      Span::styled(" proceed  ", Style::default().fg(theme.help_text_fg)),
      Span::styled("n/q/Esc", Style::default().fg(theme.help_key_fg)),
      Span::styled(" cancel", Style::default().fg(theme.help_text_fg)),
    ]),
    Step::Downloading => Line::from(vec![
      Span::styled(" Downloading...  ", Style::default().fg(theme.meta_fg)),
      Span::styled("q/Esc", Style::default().fg(theme.help_key_fg)),
      Span::styled(
        " cancel (download continues in background)",
        Style::default().fg(theme.meta_fg),
      ),
    ]),
    Step::Done => Line::from(vec![
      Span::styled(" Enter/q", Style::default().fg(theme.help_key_fg)),
      Span::styled(" continue", Style::default().fg(theme.help_text_fg)),
    ]),
  };
  frame.render_widget(Paragraph::new(help_line), chunks[2]);
}

fn render_welcome(frame: &mut ratatui::Frame, w: &Wizard, theme: &ThemeColors, area: Rect) {
  let has_sdk = w.sdk.sdk_root.is_some();
  let has_sdkmgr = w.sdk.sdkmanager_path.is_some();
  let has_avdmgr = w.sdk.avdmanager_path.is_some();

  let tick = Span::styled("  [OK]  ", Style::default().fg(theme.state_booted_fg));
  let cross = Span::styled("  [!!]  ", Style::default().fg(theme.state_shutdown_fg));

  let mut lines = vec![
    Line::from(""),
    Line::from(Span::styled(
      "  No Android Virtual Device found.",
      Style::default()
        .fg(theme.header_fg)
        .add_modifier(Modifier::BOLD),
    )),
    Line::from(""),
    Line::from(Span::styled(
      "  System check:",
      Style::default().fg(theme.meta_fg),
    )),
    Line::from(""),
    Line::from(vec![
      if has_sdk { tick.clone() } else { cross.clone() },
      Span::styled(
        format!(
          "Android SDK root     {}",
          w.sdk.sdk_root.as_deref().unwrap_or("not found")
        ),
        Style::default().fg(theme.name_fg),
      ),
    ]),
    Line::from(vec![
      if has_sdkmgr { tick.clone() } else { cross.clone() },
      Span::styled(
        format!(
          "sdkmanager           {}",
          w.sdk.sdkmanager_path.as_deref().unwrap_or("not found")
        ),
        Style::default().fg(theme.name_fg),
      ),
    ]),
    Line::from(vec![
      if has_avdmgr { tick.clone() } else { cross.clone() },
      Span::styled(
        format!(
          "avdmanager           {}",
          w.sdk.avdmanager_path.as_deref().unwrap_or("not found")
        ),
        Style::default().fg(theme.name_fg),
      ),
    ]),
    Line::from(""),
  ];

  if has_sdkmgr && has_avdmgr {
    lines.push(Line::from(Span::styled(
      "  Press Enter to set up a new Android Virtual Device.",
      Style::default()
        .fg(theme.state_booted_fg)
        .add_modifier(Modifier::BOLD),
    )));
  } else if has_sdk {
    lines.extend([
      Line::from(Span::styled(
        "  SDK found but cmdline-tools are missing.",
        Style::default().fg(theme.state_shutdown_fg),
      )),
      Line::from(""),
      Line::from(Span::styled(
        "  Fix: Open Android Studio -> SDK Manager -> SDK Tools",
        Style::default().fg(theme.meta_fg),
      )),
      Line::from(Span::styled(
        "       -> check 'Android SDK Command-line Tools' -> Apply.",
        Style::default().fg(theme.meta_fg),
      )),
      Line::from(""),
      Line::from(Span::styled(
        "  Or download from: developer.android.com/studio",
        Style::default().fg(theme.name_fg),
      )),
    ]);
  } else {
    lines.extend([
      Line::from(Span::styled(
        "  Android SDK not found.",
        Style::default().fg(theme.state_shutdown_fg),
      )),
      Line::from(""),
      Line::from(Span::styled(
        "  Install Android Studio to get started:",
        Style::default().fg(theme.meta_fg),
      )),
      Line::from(Span::styled(
        "    developer.android.com/studio",
        Style::default().fg(theme.name_fg),
      )),
      Line::from(""),
      Line::from(Span::styled(
        "  After installing, set ANDROID_SDK_ROOT and relaunch.",
        Style::default().fg(theme.meta_fg),
      )),
    ]);
  }

  let para = Paragraph::new(lines)
    .block(Block::default().borders(Borders::ALL))
    .wrap(Wrap { trim: false });
  frame.render_widget(para, area);
}

fn render_list(
  frame: &mut ratatui::Frame,
  w: &mut Wizard,
  theme: &ThemeColors,
  area: Rect,
  title: &str,
  items: &[String],
) {
  let list_items: Vec<ListItem> = items
    .iter()
    .map(|s| ListItem::new(Line::from(format!("  {}", s))))
    .collect();

  let list = List::new(list_items)
    .block(Block::default().borders(Borders::ALL).title(title))
    .highlight_style(
      Style::default()
        .bg(theme.selection_bg)
        .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("> ");

  frame.render_stateful_widget(list, area, &mut w.list_state);
}

fn render_avd_name(frame: &mut ratatui::Frame, w: &Wizard, theme: &ThemeColors, area: Rect) {
  let lines = vec![
    Line::from(""),
    Line::from(Span::styled(
      "  Enter a name for the new AVD:",
      Style::default().fg(theme.meta_fg),
    )),
    Line::from(""),
    Line::from(Span::styled(
      format!("  > {}_", w.avd_name),
      Style::default()
        .fg(theme.filter_active_fg)
        .add_modifier(Modifier::BOLD),
    )),
    Line::from(""),
    Line::from(Span::styled(
      "  Allowed: letters, digits, _ - .",
      Style::default().fg(theme.meta_fg),
    )),
  ];
  let para = Paragraph::new(lines)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title(" AVD Name "),
    )
    .wrap(Wrap { trim: false });
  frame.render_widget(para, area);
}

fn render_confirm(frame: &mut ratatui::Frame, w: &Wizard, theme: &ThemeColors, area: Rect) {
  let api_label = API_LEVELS[w.api_idx].1;
  let img_label = IMAGE_TYPES[w.image_idx].1;
  let dev_label = DEVICE_PROFILES[w.device_idx].1;
  let pkg = w.pkg();

  let lines = vec![
    Line::from(""),
    Line::from(Span::styled(
      "  Review your new AVD configuration:",
      Style::default()
        .fg(theme.header_fg)
        .add_modifier(Modifier::BOLD),
    )),
    Line::from(""),
    Line::from(vec![
      Span::styled("  API Level:      ", Style::default().fg(theme.help_key_fg).add_modifier(Modifier::BOLD)),
      Span::styled(api_label.to_string(), Style::default().fg(theme.name_fg)),
    ]),
    Line::from(vec![
      Span::styled("  Image Type:     ", Style::default().fg(theme.help_key_fg).add_modifier(Modifier::BOLD)),
      Span::styled(img_label.to_string(), Style::default().fg(theme.name_fg)),
    ]),
    Line::from(vec![
      Span::styled("  Device:         ", Style::default().fg(theme.help_key_fg).add_modifier(Modifier::BOLD)),
      Span::styled(dev_label.to_string(), Style::default().fg(theme.name_fg)),
    ]),
    Line::from(vec![
      Span::styled("  AVD Name:       ", Style::default().fg(theme.help_key_fg).add_modifier(Modifier::BOLD)),
      Span::styled(w.avd_name.clone(), Style::default().fg(theme.name_fg)),
    ]),
    Line::from(""),
    Line::from(vec![
      Span::styled("  Package:  ", Style::default().fg(theme.meta_fg)),
      Span::styled(pkg, Style::default().fg(theme.meta_fg)),
    ]),
    Line::from(""),
    Line::from(Span::styled(
      "  This will download the system image (several minutes).",
      Style::default().fg(theme.meta_fg),
    )),
    Line::from(""),
    Line::from(Span::styled(
      "  Press Enter or y to proceed.",
      Style::default()
        .fg(theme.state_booted_fg)
        .add_modifier(Modifier::BOLD),
    )),
  ];

  let para = Paragraph::new(lines)
    .block(Block::default().borders(Borders::ALL).title(" Confirm "))
    .wrap(Wrap { trim: false });
  frame.render_widget(para, area);
}

fn render_progress(frame: &mut ratatui::Frame, w: &Wizard, theme: &ThemeColors, area: Rect) {
  let height = area.height.saturating_sub(2) as usize;
  let start = w.log.len().saturating_sub(height);

  let visible: Vec<Line> = w.log[start..]
    .iter()
    .map(|s| {
      let style = if s.contains("error") || s.contains("Error") {
        Style::default().fg(theme.state_shutdown_fg)
      } else if s.contains("complete")
        || s.contains("successfully")
        || s.contains("accepted")
      {
        Style::default().fg(theme.state_booted_fg)
      } else if s.starts_with("Step ") || s.starts_with("Starting") {
        Style::default()
          .fg(theme.header_fg)
          .add_modifier(Modifier::BOLD)
      } else {
        Style::default().fg(theme.meta_fg)
      };
      Line::from(Span::styled(format!("  {}", s), style))
    })
    .collect();

  let para = Paragraph::new(visible)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title(" Progress "),
    )
    .wrap(Wrap { trim: true });
  frame.render_widget(para, area);
}

fn render_done(frame: &mut ratatui::Frame, w: &Wizard, theme: &ThemeColors, area: Rect) {
  let lines = if let Some(ref err) = w.error {
    vec![
      Line::from(""),
      Line::from(Span::styled(
        "  Setup failed.",
        Style::default()
          .fg(theme.state_shutdown_fg)
          .add_modifier(Modifier::BOLD),
      )),
      Line::from(""),
      Line::from(Span::styled(
        format!("  {}", err),
        Style::default().fg(theme.state_shutdown_fg),
      )),
      Line::from(""),
      Line::from(Span::styled(
        "  Check that sdkmanager and avdmanager are working,",
        Style::default().fg(theme.meta_fg),
      )),
      Line::from(Span::styled(
        "  then relaunch emulaunch.",
        Style::default().fg(theme.meta_fg),
      )),
      Line::from(""),
      Line::from(Span::styled(
        "  Press q to exit.",
        Style::default().fg(theme.meta_fg),
      )),
    ]
  } else {
    vec![
      Line::from(""),
      Line::from(Span::styled(
        "  [OK] AVD created successfully!",
        Style::default()
          .fg(theme.state_booted_fg)
          .add_modifier(Modifier::BOLD),
      )),
      Line::from(""),
      Line::from(Span::styled(
        format!("  Your AVD '{}' is ready.", w.avd_name),
        Style::default().fg(theme.name_fg),
      )),
      Line::from(""),
      Line::from(Span::styled(
        "  Press Enter to open the emulator list.",
        Style::default().fg(theme.meta_fg),
      )),
    ]
  };

  let para = Paragraph::new(lines)
    .block(Block::default().borders(Borders::ALL))
    .wrap(Wrap { trim: false });
  frame.render_widget(para, area);
}
