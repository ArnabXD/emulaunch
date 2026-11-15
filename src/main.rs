mod config;
mod emulators;
mod error;
mod theme;

use clap::{Parser, Subcommand};
use emulators::{Collected, Device, Platform, State};
use ratatui::{
  crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
  layout::{Constraint, Layout, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
  DefaultTerminal,
};
use std::io;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

#[derive(Parser)]
#[command(version, about = "List and open Android/iOS emulators")]
struct Cli {
  #[command(subcommand)]
  command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
  /// Print a plain text list of all emulators
  List,
  /// Open an emulator by name
  Open {
    /// Name of the emulator to open
    name: Vec<String>,
  },
}

fn main() {
  let cli = Cli::parse();

  match cli.command {
    Some(Commands::List) => {
      print!("{}", emulators::format_emulator_list());
    }
    Some(Commands::Open { name }) => {
      let name = name.join(" ");
      match emulators::find_device(&name).and_then(|device| device.open()) {
        Ok(msg) => println!("{}", msg),
        Err(e) => {
          eprintln!("Error: {}", e);
          std::process::exit(1);
        }
      }
    }
    None => {
      if let Err(e) = run_tui() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
      }
    }
  }
}

struct App {
  devices: Vec<Device>,
  /// Indices into `devices` that match the filter, in display order.
  filtered: Vec<usize>,
  /// Position of the selected device within `filtered`.
  selected: usize,
  /// Row highlighted in the rendered list. Recomputed every frame because
  /// section headers are rows too.
  list_state: ListState,
  filter: String,
  result_message: Option<String>,
  show_help: bool,
  /// Receives the result of a listing running on a background thread.
  loading: Option<Receiver<Collected>>,
  /// Advances every frame to animate the spinner.
  spinner_tick: usize,
  /// Set when a listing finished without any device, so the loop can stop.
  nothing_listed: Option<NothingListed>,
}

/// Why the picker has nothing to show after a listing.
#[derive(Debug, PartialEq, Eq)]
enum NothingListed {
  /// Every platform answered, there just are no devices.
  NoDevices,
  /// A platform could not be listed; the text explains why.
  Failed(String),
}

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

impl App {
  fn new(devices: Vec<Device>) -> Self {
    let mut app = App {
      devices,
      filtered: Vec::new(),
      selected: 0,
      list_state: ListState::default(),
      filter: String::new(),
      result_message: None,
      show_help: false,
      loading: None,
      spinner_tick: 0,
      nothing_listed: None,
    };
    app.apply_filter();
    app
  }

  fn is_loading(&self) -> bool {
    self.loading.is_some()
  }

  fn spinner(&self) -> &'static str {
    SPINNER[self.spinner_tick % SPINNER.len()]
  }

  /// Run `list` on a background thread so the UI keeps responding. Does
  /// nothing while a listing is already running.
  fn start_loading(&mut self, list: impl FnOnce() -> Collected + Send + 'static) {
    if self.is_loading() {
      return;
    }
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
      // The receiver is gone when the user quit meanwhile; nothing to do then.
      let _ = tx.send(list());
    });
    self.loading = Some(rx);
  }

  /// Pick up the result of the background listing if it has finished.
  fn poll_loading(&mut self) {
    let Some(rx) = &self.loading else { return };
    let collected = match rx.try_recv() {
      Ok(collected) => collected,
      Err(TryRecvError::Empty) => return,
      // The thread died without answering (a panic): show an empty list
      // rather than spinning forever.
      Err(TryRecvError::Disconnected) => Collected {
        devices: Vec::new(),
        errors: Vec::new(),
      },
    };
    self.loading = None;
    self.apply_collected(collected);
  }

  /// Show a finished listing. If there was nothing to show before and there
  /// still is not, record why so the loop can end with an explanation.
  fn apply_collected(&mut self, collected: Collected) {
    let Collected { devices, errors } = collected;
    if devices.is_empty() && self.devices.is_empty() {
      self.nothing_listed = Some(if errors.is_empty() {
        NothingListed::NoDevices
      } else {
        // Explain why nothing could be listed (e.g. adb / emulator not found)
        let reasons: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        NothingListed::Failed(reasons.join("\n\n"))
      });
      return;
    }
    self.replace_devices(devices);
  }

  /// Recompute `filtered` from the filter text and select the first match.
  fn apply_filter(&mut self) {
    let query = self.filter.to_lowercase();
    self.filtered = self
      .devices
      .iter()
      .enumerate()
      .filter(|(_, d)| query.is_empty() || d.name.to_lowercase().contains(&query))
      .map(|(i, _)| i)
      .collect();
    self.selected = 0;
  }

  fn move_selection(&mut self, delta: isize) {
    if let Some(last) = self.filtered.len().checked_sub(1) {
      self.selected = self.selected.saturating_add_signed(delta).min(last);
    }
  }

  fn selected_device(&self) -> Option<&Device> {
    let &index = self.filtered.get(self.selected)?;
    self.devices.get(index)
  }

  /// Swap in a new device list, keeping the same device selected if it is
  /// still listed.
  fn replace_devices(&mut self, devices: Vec<Device>) {
    let previous = self.selected_device().map(|d| (d.platform, d.id.clone()));
    self.devices = devices;
    self.apply_filter();

    if let Some((platform, id)) = previous {
      if let Some(pos) = self.filtered.iter().position(|&i| {
        let d = &self.devices[i];
        d.platform == platform && d.id == id
      }) {
        self.selected = pos;
      }
    }
  }

  /// Update the state for one key press and say what the event loop should do.
  fn handle_key(&mut self, key: KeyEvent) -> Action {
    // When the help overlay is open, only handle overlay-dismissal keys
    if self.show_help {
      if matches!(
        key.code,
        KeyCode::Esc | KeyCode::Char('q' | '?') | KeyCode::F(1)
      ) {
        self.show_help = false;
      }
      return Action::Continue;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
      KeyCode::Esc => return Action::Quit,
      KeyCode::Char('c') if ctrl => return Action::Quit,
      KeyCode::Char('?') if self.filter.is_empty() => self.show_help = true,
      KeyCode::F(1) => self.show_help = true,
      KeyCode::Char('r') if ctrl => return Action::Reload,
      KeyCode::F(5) => return Action::Reload,
      KeyCode::Char('u') if ctrl => {
        self.filter.clear();
        self.apply_filter();
      }
      KeyCode::Char('n') if ctrl => self.move_selection(1),
      KeyCode::Char('p') if ctrl => self.move_selection(-1),
      KeyCode::Down => self.move_selection(1),
      KeyCode::Up => self.move_selection(-1),
      KeyCode::Enter if self.selected_device().is_some() => return Action::Open,
      KeyCode::Backspace => {
        self.filter.pop();
        self.apply_filter();
      }
      // Ignore other Ctrl/Alt chords so they don't end up in the filter
      KeyCode::Char(_)
        if key
          .modifiers
          .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {}
      KeyCode::Char(c) => {
        self.filter.push(c);
        self.apply_filter();
      }
      _ => {}
    }
    Action::Continue
  }
}

/// What the event loop should do after a key press.
#[derive(Debug, PartialEq, Eq)]
enum Action {
  Continue,
  Quit,
  /// Re-list the devices.
  Reload,
  /// Launch the selected device and exit.
  Open,
}

/// Build the list rows: a heading whenever the platform changes, then the
/// device. Devices are grouped by platform, so this yields one heading per
/// group. Also returns the row to highlight for the `selected` position
/// within `filtered`.
fn list_rows<'a>(
  devices: &'a [Device],
  filtered: &[usize],
  selected: usize,
  theme: &theme::ThemeColors,
) -> (Vec<ListItem<'a>>, Option<usize>) {
  let mut items = Vec::new();
  let mut selected_row = None;
  let mut group: Option<Platform> = None;
  for (pos, &index) in filtered.iter().enumerate() {
    let device = &devices[index];
    if group != Some(device.platform) {
      group = Some(device.platform);
      items.push(header_row(device.platform, theme));
    }
    if pos == selected {
      selected_row = Some(items.len());
    }
    items.push(device_row(device, theme));
  }
  (items, selected_row)
}

fn state_color(state: &State, theme: &theme::ThemeColors) -> ratatui::style::Color {
  match state {
    State::Booted => theme.state_booted_fg,
    State::Shutdown => theme.state_shutdown_fg,
    State::Other(_) => theme.state_unknown_fg,
  }
}

/// "● Booted" / "○ Shutdown", colored by state.
fn state_span(state: &State, theme: &theme::ThemeColors) -> Span<'static> {
  let dot = if state.is_booted() { "● " } else { "○ " };
  Span::styled(
    format!("{}{}", dot, state),
    Style::default().fg(state_color(state, theme)),
  )
}

fn header_row(platform: Platform, theme: &theme::ThemeColors) -> ListItem<'static> {
  ListItem::new(Line::from(Span::styled(
    format!(" {}", platform.section_title()),
    Style::default()
      .fg(theme.header_fg)
      .add_modifier(Modifier::BOLD),
  )))
}

fn device_row<'a>(device: &'a Device, theme: &theme::ThemeColors) -> ListItem<'a> {
  ListItem::new(Line::from(vec![
    Span::raw("   "),
    Span::styled(&device.name, Style::default().fg(theme.name_fg)),
    Span::raw("  "),
    state_span(&device.state, theme),
    Span::styled(
      format!("  ({})", device.meta),
      Style::default().fg(theme.meta_fg),
    ),
  ]))
}

/// The detail panel contents for one device.
fn detail_lines<'a>(device: &'a Device, theme: &theme::ThemeColors) -> Vec<Line<'a>> {
  let label = |text: &'a str| {
    Line::from(Span::styled(
      format!(" {}:", text),
      Style::default()
        .fg(theme.help_key_fg)
        .add_modifier(Modifier::BOLD),
    ))
  };
  let text = |value: &'a str| {
    Line::from(vec![
      Span::raw("   "),
      Span::styled(value, Style::default().fg(theme.name_fg)),
    ])
  };

  vec![
    Line::from(Span::styled(
      format!(" {}", device.platform.kind()),
      Style::default()
        .fg(theme.header_fg)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    )),
    Line::from(""),
    label("Name"),
    text(&device.name),
    Line::from(""),
    label(device.platform.id_label()),
    text(&device.id),
    Line::from(""),
    label("State"),
    Line::from(vec![Span::raw("   "), state_span(&device.state, theme)]),
    Line::from(""),
    label(device.platform.meta_label()),
    text(&device.meta),
    Line::from(""),
  ]
}

/// Key hints shown in the bottom bar: (key, action).
const HELP_BAR: [(&str, &str); 5] = [
  ("↑/↓", "navigate"),
  ("Enter", "open"),
  ("^R", "refresh"),
  ("?", "help"),
  ("Esc", "quit"),
];

/// Rows of the keyboard shortcuts popup: (keys, description).
const SHORTCUTS: [(&str, &str); 8] = [
  ("↑/↓ ^N/^P", "Navigate list"),
  ("Enter", "Launch selected emulator"),
  ("^R / F5", "Refresh emulator list"),
  ("^U", "Clear filter"),
  ("Type", "Filter emulators"),
  ("Backspace", "Delete filter character"),
  ("? / F1", "Toggle this help"),
  ("Esc / ^C", "Quit"),
];

fn help_bar(theme: &theme::ThemeColors) -> Paragraph<'static> {
  let key_style = Style::default().fg(theme.help_key_fg);
  let text_style = Style::default().fg(theme.help_text_fg);
  let last = HELP_BAR.len() - 1;
  let spans: Vec<Span> = HELP_BAR
    .iter()
    .enumerate()
    .flat_map(|(i, (key, action))| {
      let lead = if i == 0 { " " } else { "" };
      let trail = if i == last { "" } else { "  " };
      [
        Span::styled(format!("{lead}{key}"), key_style),
        Span::styled(format!(" {action}{trail}"), text_style),
      ]
    })
    .collect();
  Paragraph::new(Line::from(spans))
}

fn shortcuts_popup(theme: &theme::ThemeColors) -> Paragraph<'static> {
  let key_style = Style::default()
    .fg(theme.help_key_fg)
    .add_modifier(Modifier::BOLD);
  let text_style = Style::default().fg(theme.help_text_fg);

  let mut lines = vec![Line::from("")];
  lines.extend(SHORTCUTS.iter().map(|(keys, description)| {
    Line::from(vec![
      Span::styled(format!("  {keys:<11}"), key_style),
      Span::styled(*description, text_style),
    ])
  }));
  lines.push(Line::from(""));

  Paragraph::new(lines).block(
    Block::default()
      .borders(Borders::ALL)
      .title(" Keyboard Shortcuts ")
      .title_style(
        Style::default()
          .fg(theme.header_fg)
          .add_modifier(Modifier::BOLD),
      ),
  )
}

/// Return a centered Rect of fixed `width` x `height` within `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
  let x = area.x + area.width.saturating_sub(width) / 2;
  let y = area.y + area.height.saturating_sub(height) / 2;
  Rect {
    x,
    y,
    width: width.min(area.width),
    height: height.min(area.height),
  }
}

fn run_tui() -> io::Result<()> {
  let cfg = config::load_config();
  let theme = theme::resolve_theme(
    cfg.as_ref().and_then(|c| c.theme.as_deref()),
    cfg.as_ref().and_then(|c| c.theme_overrides.as_ref()),
  );

  // init() enters raw mode + the alternate screen and installs a panic hook
  // that restores the terminal; restore() always runs, even if run_app fails.
  let mut terminal = ratatui::init();
  let mut app = App::new(Vec::new());
  app.start_loading(emulators::collect_devices);
  let result = run_app(&mut terminal, &mut app, &theme);
  ratatui::restore();

  if let Some(msg) = app.result_message {
    println!("{}", msg);
  }
  match app.nothing_listed {
    Some(NothingListed::NoDevices) => println!("No emulators or simulators found."),
    Some(NothingListed::Failed(reasons)) => return Err(io::Error::other(reasons)),
    None => {}
  }

  result
}

fn run_app(
  terminal: &mut DefaultTerminal,
  app: &mut App,
  theme: &theme::ThemeColors,
) -> io::Result<()> {
  loop {
    app.poll_loading();
    if app.nothing_listed.is_some() {
      break;
    }
    app.spinner_tick = app.spinner_tick.wrapping_add(1);

    terminal.draw(|frame| {
      let area = frame.area();

      let chunks = Layout::vertical([
        Constraint::Length(3), // filter input
        Constraint::Min(1),    // list (+ optional detail panel)
        Constraint::Length(1), // help bar
      ])
      .split(area);

      // Split the list area horizontally when terminal is wide enough for the detail panel
      let show_detail = area.width >= 90;
      let (list_area, detail_area_opt) = if show_detail {
        let split = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
          .split(chunks[1]);
        (split[0], Some(split[1]))
      } else {
        (chunks[1], None)
      };

      // Filter input
      let filter_text = if app.filter.is_empty() {
        "Type to filter...".to_string()
      } else {
        app.filter.clone()
      };
      let filter_style = if app.filter.is_empty() {
        Style::default().fg(theme.filter_placeholder_fg)
      } else {
        Style::default().fg(theme.filter_active_fg)
      };
      let filter = Paragraph::new(filter_text)
        .style(filter_style)
        .block(Block::default().borders(Borders::ALL).title(" Filter "));
      frame.render_widget(filter, chunks[0]);

      // List title with visible/total count
      let mut list_title = if app.filter.is_empty() {
        format!(" Emulators ({}) ", app.devices.len())
      } else {
        format!(" Emulators ({}/{}) ", app.filtered.len(), app.devices.len())
      };
      if app.is_loading() {
        list_title = format!("{}{} Loading ", list_title, app.spinner());
      }

      let (items, selected_row) = list_rows(&app.devices, &app.filtered, app.selected, theme);
      app.list_state.select(selected_row);

      let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(list_title))
        .highlight_style(
          Style::default()
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD),
        );
      frame.render_stateful_widget(list, list_area, &mut app.list_state);

      // Detail panel — shown when terminal is wide enough
      if let Some(detail_area) = detail_area_opt {
        let detail_lines: Vec<Line> = match app.selected_device() {
          Some(device) => detail_lines(device, theme),
          None => vec![
            Line::from(""),
            Line::from(Span::styled(
              " Select an emulator",
              Style::default().fg(theme.meta_fg),
            )),
            Line::from(Span::styled(
              " to see details",
              Style::default().fg(theme.meta_fg),
            )),
          ],
        };

        let detail_widget = Paragraph::new(detail_lines)
          .block(Block::default().borders(Borders::ALL).title(" Details "))
          .wrap(Wrap { trim: false });
        frame.render_widget(detail_widget, detail_area);
      }

      frame.render_widget(help_bar(theme), chunks[2]);

      if app.show_help {
        let popup_area = centered_rect(46, SHORTCUTS.len() as u16 + 5, area);
        frame.render_widget(Clear, popup_area);
        frame.render_widget(shortcuts_popup(theme), popup_area);
      }
    })?;

    if event::poll(std::time::Duration::from_millis(100))? {
      if let Event::Key(key) = event::read()? {
        if key.kind != KeyEventKind::Press {
          continue;
        }

        match app.handle_key(key) {
          Action::Continue => {}
          Action::Quit => break,
          Action::Reload => app.start_loading(emulators::collect_devices),
          Action::Open => {
            if let Some(device) = app.selected_device() {
              let message = match device.open() {
                Ok(msg) => msg,
                Err(e) => format!("Error: {}", e),
              };
              app.result_message = Some(message);
            }
            break;
          }
        }
      }
    }
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use clap::CommandFactory;
  use emulators::State;

  fn device(platform: Platform, name: &str, id: &str) -> Device {
    Device {
      platform,
      name: name.to_string(),
      id: id.to_string(),
      state: State::Shutdown,
      meta: String::new(),
    }
  }

  fn android(name: &str, id: &str) -> Device {
    device(Platform::Android, name, id)
  }

  fn ios(name: &str, id: &str) -> Device {
    device(Platform::Ios, name, id)
  }

  /// Two Android devices followed by one iOS device.
  fn sample_app() -> App {
    App::new(vec![
      android("Pixel 8", "pixel8"),
      android("Resizable", "resizable"),
      ios("iPhone 17", "UDID-1"),
    ])
  }

  fn names(app: &App) -> Vec<&str> {
    app
      .filtered
      .iter()
      .map(|&i| app.devices[i].name.as_str())
      .collect()
  }

  fn press(app: &mut App, code: KeyCode) -> Action {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
  }

  fn press_with(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Action {
    app.handle_key(KeyEvent::new(code, modifiers))
  }

  fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
      assert_eq!(press(app, KeyCode::Char(c)), Action::Continue);
    }
  }

  // --- filtering -------------------------------------------------------

  #[test]
  fn everything_is_listed_without_a_filter() {
    let app = sample_app();
    assert_eq!(names(&app), vec!["Pixel 8", "Resizable", "iPhone 17"]);
    assert_eq!(app.selected_device().unwrap().name, "Pixel 8");
  }

  #[test]
  fn filter_is_a_case_insensitive_substring_match() {
    let mut app = sample_app();
    type_text(&mut app, "PHONE");
    assert_eq!(names(&app), vec!["iPhone 17"]);
  }

  #[test]
  fn filter_with_no_match_selects_nothing() {
    let mut app = sample_app();
    type_text(&mut app, "zzz");
    assert!(app.filtered.is_empty());
    assert!(app.selected_device().is_none());
  }

  #[test]
  fn changing_the_filter_selects_the_first_match() {
    let mut app = sample_app();
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    type_text(&mut app, "e");
    assert_eq!(app.selected, 0);
  }

  // --- selection -------------------------------------------------------

  #[test]
  fn selection_stops_at_both_ends() {
    let mut app = sample_app();
    press(&mut app, KeyCode::Up);
    assert_eq!(app.selected, 0);
    for _ in 0..10 {
      press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.selected, 2);
    assert_eq!(app.selected_device().unwrap().name, "iPhone 17");
  }

  #[test]
  fn moving_in_an_empty_list_does_nothing() {
    let mut app = App::new(Vec::new());
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.selected, 0);
    assert!(app.selected_device().is_none());
  }

  #[test]
  fn refresh_keeps_the_same_device_selected_when_the_order_changes() {
    let mut app = sample_app();
    press(&mut app, KeyCode::Down); // Resizable
    app.replace_devices(vec![
      ios("iPhone 17", "UDID-1"),
      android("Resizable", "resizable"),
      android("Pixel 8", "pixel8"),
    ]);
    assert_eq!(app.selected_device().unwrap().id, "resizable");
  }

  #[test]
  fn refresh_tells_apart_devices_with_the_same_name() {
    let mut app = App::new(vec![ios("iPhone 17", "UDID-1"), ios("iPhone 17", "UDID-2")]);
    press(&mut app, KeyCode::Down); // second iPhone 17
    assert_eq!(app.selected_device().unwrap().id, "UDID-2");

    // UDID-2 is no longer first, and another device with the same name now
    // precedes it, so matching by name (or platform) alone would pick wrongly.
    app.replace_devices(vec![
      ios("iPhone 17", "UDID-1"),
      ios("iPhone 17", "UDID-3"),
      ios("iPhone 17", "UDID-2"),
    ]);
    assert_eq!(app.selected_device().unwrap().id, "UDID-2");
  }

  #[test]
  fn refresh_selects_the_first_device_when_the_selected_one_is_gone() {
    let mut app = sample_app();
    press(&mut app, KeyCode::Down);
    app.replace_devices(vec![android("Pixel 8", "pixel8")]);
    assert_eq!(app.selected_device().unwrap().id, "pixel8");
  }

  // --- rows ------------------------------------------------------------

  #[test]
  fn rows_have_one_heading_per_platform() {
    let app = sample_app();
    let theme = theme::resolve_theme(None, None);
    let (rows, selected) = list_rows(&app.devices, &app.filtered, 0, &theme);

    // Android heading, 2 Android devices, iOS heading, 1 iOS device
    assert_eq!(rows.len(), 5);
    assert_eq!(selected, Some(1));
  }

  #[test]
  fn selected_row_accounts_for_headings() {
    let app = sample_app();
    let theme = theme::resolve_theme(None, None);

    assert_eq!(list_rows(&app.devices, &app.filtered, 1, &theme).1, Some(2));
    // Third device sits below the iOS heading
    assert_eq!(list_rows(&app.devices, &app.filtered, 2, &theme).1, Some(4));
  }

  #[test]
  fn rows_omit_headings_of_filtered_out_platforms() {
    let mut app = sample_app();
    type_text(&mut app, "iphone");
    let theme = theme::resolve_theme(None, None);
    let (rows, selected) = list_rows(&app.devices, &app.filtered, app.selected, &theme);

    assert_eq!(rows.len(), 2); // iOS heading + the device
    assert_eq!(selected, Some(1));
  }

  #[test]
  fn no_rows_and_no_selection_when_nothing_matches() {
    let mut app = sample_app();
    type_text(&mut app, "zzz");
    let theme = theme::resolve_theme(None, None);
    let (rows, selected) = list_rows(&app.devices, &app.filtered, app.selected, &theme);

    assert!(rows.is_empty());
    assert_eq!(selected, None);
  }

  // --- keys ------------------------------------------------------------

  #[test]
  fn former_command_letters_are_typed_into_the_filter() {
    // Regression: j, k, q and r used to be commands while the filter was empty,
    // so an emulator named "Resizable" could not be searched for.
    let mut app = sample_app();
    type_text(&mut app, "res");
    assert_eq!(app.filter, "res");
    assert_eq!(names(&app), vec!["Resizable"]);

    let mut app = sample_app();
    for c in ['q', 'j', 'k', 'r'] {
      assert_eq!(press(&mut app, KeyCode::Char(c)), Action::Continue);
    }
    assert_eq!(app.filter, "qjkr");
  }

  #[test]
  fn escape_and_ctrl_c_quit() {
    let mut app = sample_app();
    assert_eq!(press(&mut app, KeyCode::Esc), Action::Quit);
    assert_eq!(
      press_with(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL),
      Action::Quit
    );
  }

  #[test]
  fn plain_c_is_typed_not_quit() {
    let mut app = sample_app();
    assert_eq!(press(&mut app, KeyCode::Char('c')), Action::Continue);
    assert_eq!(app.filter, "c");
  }

  #[test]
  fn ctrl_r_and_f5_reload_but_plain_r_does_not() {
    let mut app = sample_app();
    assert_eq!(
      press_with(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL),
      Action::Reload
    );
    assert_eq!(press(&mut app, KeyCode::F(5)), Action::Reload);
    assert_eq!(press(&mut app, KeyCode::Char('r')), Action::Continue);
    assert_eq!(app.filter, "r");
  }

  #[test]
  fn ctrl_n_and_ctrl_p_and_arrows_move_the_selection() {
    let mut app = sample_app();
    press_with(&mut app, KeyCode::Char('n'), KeyModifiers::CONTROL);
    assert_eq!(app.selected, 1);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.selected, 2);
    press_with(&mut app, KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert_eq!(app.selected, 1);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.selected, 0);
  }

  #[test]
  fn backspace_and_ctrl_u_edit_the_filter() {
    let mut app = sample_app();
    type_text(&mut app, "pix");
    press(&mut app, KeyCode::Backspace);
    assert_eq!(app.filter, "pi");

    press_with(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(app.filter, "");
    assert_eq!(app.filtered.len(), 3);
  }

  #[test]
  fn other_control_chords_are_ignored() {
    // Ctrl+J is the same byte as Enter in a terminal; it must never be typed
    // into the filter or launch anything.
    let mut app = sample_app();
    for (c, modifier) in [
      ('j', KeyModifiers::CONTROL),
      ('w', KeyModifiers::CONTROL),
      ('x', KeyModifiers::ALT),
    ] {
      assert_eq!(
        press_with(&mut app, KeyCode::Char(c), modifier),
        Action::Continue
      );
    }
    assert_eq!(app.filter, "");
  }

  #[test]
  fn enter_opens_the_selected_device_only_when_there_is_one() {
    let mut app = sample_app();
    assert_eq!(press(&mut app, KeyCode::Enter), Action::Open);

    type_text(&mut app, "zzz");
    assert_eq!(press(&mut app, KeyCode::Enter), Action::Continue);
  }

  #[test]
  fn question_mark_opens_help_only_while_the_filter_is_empty() {
    let mut app = sample_app();
    press(&mut app, KeyCode::Char('?'));
    assert!(app.show_help);

    let mut app = sample_app();
    type_text(&mut app, "a?");
    assert!(!app.show_help);
    assert_eq!(app.filter, "a?");

    let mut app = sample_app();
    press(&mut app, KeyCode::F(1));
    assert!(app.show_help);
  }

  #[test]
  fn help_overlay_swallows_keys_and_is_dismissed_without_quitting() {
    let mut app = sample_app();
    app.show_help = true;

    // Typing and navigation do nothing while the overlay is open
    assert_eq!(press(&mut app, KeyCode::Char('x')), Action::Continue);
    assert_eq!(press(&mut app, KeyCode::Down), Action::Continue);
    assert_eq!(app.filter, "");
    assert_eq!(app.selected, 0);
    assert!(app.show_help);

    // Esc closes the overlay instead of quitting
    assert_eq!(press(&mut app, KeyCode::Esc), Action::Continue);
    assert!(!app.show_help);

    for key in [KeyCode::Char('q'), KeyCode::Char('?'), KeyCode::F(1)] {
      app.show_help = true;
      assert_eq!(press(&mut app, key), Action::Continue);
      assert!(!app.show_help);
    }
  }

  // --- command line ----------------------------------------------------

  fn collected(devices: Vec<Device>) -> Collected {
    Collected {
      devices,
      errors: Vec::new(),
    }
  }

  /// Poll until the background listing has been picked up.
  fn wait_for_listing(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.is_loading() {
      assert!(std::time::Instant::now() < deadline, "listing never finished");
      app.poll_loading();
      thread::sleep(std::time::Duration::from_millis(1));
    }
  }

  #[test]
  fn background_listing_fills_the_list_when_it_finishes() {
    let mut app = App::new(Vec::new());
    app.start_loading(|| collected(vec![android("Pixel 8", "pixel8")]));
    assert!(app.is_loading());
    wait_for_listing(&mut app);
    assert_eq!(names(&app), ["Pixel 8"]);
    assert_eq!(app.nothing_listed, None);
  }

  #[test]
  fn listing_is_not_visible_until_it_finishes() {
    let (release, wait) = mpsc::channel::<()>();
    let mut app = sample_app();
    app.start_loading(move || {
      wait.recv().unwrap();
      collected(vec![android("New", "new")])
    });
    app.poll_loading();
    assert!(app.is_loading());
    assert_eq!(names(&app).len(), 3, "old list stays while refreshing");
    release.send(()).unwrap();
    wait_for_listing(&mut app);
    assert_eq!(names(&app), ["New"]);
  }

  #[test]
  fn a_second_refresh_is_ignored_while_one_is_running() {
    let (release, wait) = mpsc::channel::<()>();
    let mut app = App::new(Vec::new());
    app.start_loading(move || {
      wait.recv().unwrap();
      collected(vec![android("First", "first")])
    });
    app.start_loading(|| collected(vec![android("Second", "second")]));
    release.send(()).unwrap();
    wait_for_listing(&mut app);
    assert_eq!(names(&app), ["First"]);
  }

  #[test]
  fn a_panicking_listing_ends_the_spinner_instead_of_hanging() {
    let mut app = sample_app();
    app.start_loading(|| panic!("boom"));
    wait_for_listing(&mut app);
    assert!(!app.is_loading());
  }

  #[test]
  fn empty_first_listing_is_reported() {
    let mut app = App::new(Vec::new());
    app.apply_collected(collected(Vec::new()));
    assert_eq!(app.nothing_listed, Some(NothingListed::NoDevices));
  }

  #[test]
  fn failed_first_listing_carries_the_reasons() {
    let mut app = App::new(Vec::new());
    app.apply_collected(Collected {
      devices: Vec::new(),
      errors: vec![error::Error::NoAvdDirectory],
    });
    match app.nothing_listed {
      Some(NothingListed::Failed(reasons)) => assert!(!reasons.is_empty()),
      other => panic!("unexpected {:?}", other),
    }
  }

  #[test]
  fn refresh_that_finds_nothing_keeps_the_picker_open() {
    let mut app = sample_app();
    app.apply_collected(collected(Vec::new()));
    assert_eq!(app.nothing_listed, None);
    assert!(app.devices.is_empty());
  }

  #[test]
  fn spinner_cycles_through_its_frames() {
    let mut app = App::new(Vec::new());
    let first = app.spinner();
    app.spinner_tick = SPINNER.len();
    assert_eq!(app.spinner(), first);
    app.spinner_tick += 1;
    assert_ne!(app.spinner(), first);
  }

  #[test]
  fn command_line_definition_is_valid() {
    Cli::command().debug_assert();
  }

  #[test]
  fn binary_is_called_emulaunch() {
    assert_eq!(Cli::command().get_name(), "emulaunch");
  }

  #[test]
  fn open_takes_a_name_that_may_contain_spaces() {
    let cli = Cli::try_parse_from(["emulaunch", "open", "Pixel", "8", "Pro"]).unwrap();
    match cli.command {
      Some(Commands::Open { name }) => assert_eq!(name.join(" "), "Pixel 8 Pro"),
      _ => panic!("expected the open command"),
    }
  }

  #[test]
  fn no_arguments_means_the_tui_and_list_is_a_subcommand() {
    assert!(Cli::try_parse_from(["emulaunch"])
      .unwrap()
      .command
      .is_none());
    assert!(matches!(
      Cli::try_parse_from(["emulaunch", "list"]).unwrap().command,
      Some(Commands::List)
    ));
  }
}
