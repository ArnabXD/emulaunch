mod config;
mod emulators;
mod theme;

use clap::{Parser, Subcommand};
use crossterm::{
  event::{self, Event, KeyCode, KeyEventKind},
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
  ExecutableCommand,
};
use emulators::{EmulatorEntry, EmulatorType};
use ratatui::{
  layout::{Constraint, Layout},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
  Terminal,
};
use std::io;

#[derive(Parser)]
#[command(name = "emulators", about = "List and open Android/iOS emulators")]
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
      match emulators::find_emulator(&name) {
        Ok(EmulatorType::Android(emu_name)) => match emulators::open_android_emulator(&emu_name) {
          Ok(msg) => println!("{}", msg),
          Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
          }
        },
        Ok(EmulatorType::IOS(udid)) => match emulators::open_ios_simulator(&udid) {
          Ok(msg) => println!("{}", msg),
          Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
          }
        },
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
  entries: Vec<EmulatorEntry>,
  filtered_indices: Vec<usize>,
  list_state: ListState,
  filter: String,
  result_message: Option<String>,
}

impl App {
  fn new(entries: Vec<EmulatorEntry>) -> Self {
    let filtered_indices: Vec<usize> = (0..entries.len()).collect();
    let mut list_state = ListState::default();
    // Select first non-header item
    let first_selectable = filtered_indices
      .iter()
      .position(|&i| !entries[i].is_header());
    list_state.select(first_selectable);

    App {
      entries,
      filtered_indices,
      list_state,
      filter: String::new(),
      result_message: None,
    }
  }

  fn apply_filter(&mut self) {
    let query = self.filter.to_lowercase();
    self.filtered_indices = (0..self.entries.len())
      .filter(|&i| {
        let entry = &self.entries[i];
        if entry.is_header() {
          // Keep headers if any child in their section matches
          return self.section_has_match(i, &query);
        }
        query.is_empty() || entry.display_name().to_lowercase().contains(&query)
      })
      .collect();

    // Select first non-header item
    let first_selectable = self
      .filtered_indices
      .iter()
      .position(|&i| !self.entries[i].is_header());
    self.list_state.select(first_selectable);
  }

  fn section_has_match(&self, header_idx: usize, query: &str) -> bool {
    if query.is_empty() {
      return true;
    }
    for i in (header_idx + 1)..self.entries.len() {
      if self.entries[i].is_header() {
        break;
      }
      if self.entries[i]
        .display_name()
        .to_lowercase()
        .contains(query)
      {
        return true;
      }
    }
    false
  }

  fn move_selection(&mut self, delta: i32) {
    let selectable: Vec<usize> = self
      .filtered_indices
      .iter()
      .enumerate()
      .filter(|(_, &i)| !self.entries[i].is_header())
      .map(|(pos, _)| pos)
      .collect();

    if selectable.is_empty() {
      self.list_state.select(None);
      return;
    }

    let current = self.list_state.selected().unwrap_or(0);
    let current_pos = selectable.iter().position(|&p| p == current).unwrap_or(0);
    let new_pos = if delta > 0 {
      (current_pos + 1).min(selectable.len() - 1)
    } else {
      current_pos.saturating_sub(1)
    };
    self.list_state.select(Some(selectable[new_pos]));
  }

  fn selected_entry(&self) -> Option<&EmulatorEntry> {
    let selected = self.list_state.selected()?;
    let &entry_idx = self.filtered_indices.get(selected)?;
    let entry = &self.entries[entry_idx];
    if entry.is_header() {
      None
    } else {
      Some(entry)
    }
  }
}

fn state_color(state: &str, theme: &theme::ThemeColors) -> ratatui::style::Color {
  match state {
    emulators::STATE_BOOTED => theme.state_booted_fg,
    emulators::STATE_SHUTDOWN => theme.state_shutdown_fg,
    _ => theme.state_unknown_fg,
  }
}

fn run_tui() -> io::Result<()> {
  let entries = emulators::collect_all_entries();
  if entries.is_empty() {
    println!("No emulators or simulators found.");
    return Ok(());
  }

  let cfg = config::load_config();
  let theme = theme::resolve_theme(
    cfg.as_ref().and_then(|c| c.theme.as_deref()),
    cfg.as_ref().and_then(|c| c.theme_overrides.as_ref()),
  );

  enable_raw_mode()?;
  io::stdout().execute(EnterAlternateScreen)?;
  let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
  let mut terminal = Terminal::new(backend)?;

  let mut app = App::new(entries);
  let result = run_app(&mut terminal, &mut app, &theme);

  disable_raw_mode()?;
  io::stdout().execute(LeaveAlternateScreen)?;

  if let Some(msg) = app.result_message {
    println!("{}", msg);
  }

  result
}

fn run_app(
  terminal: &mut Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>,
  app: &mut App,
  theme: &theme::ThemeColors,
) -> io::Result<()> {
  loop {
    terminal.draw(|frame| {
      let chunks = Layout::vertical([
        Constraint::Length(3), // filter input
        Constraint::Min(1),    // list
        Constraint::Length(1), // help bar
      ])
      .split(frame.area());

      // Filter input
      let filter_text = if app.filter.is_empty() {
        "Type to filter..."
      } else {
        &app.filter
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

      // Emulator list
      let items: Vec<ListItem> = app
        .filtered_indices
        .iter()
        .map(|&i| {
          let entry = &app.entries[i];
          match entry {
            EmulatorEntry::SectionHeader(s) => ListItem::new(Line::from(Span::styled(
              format!(" {}", s),
              Style::default()
                .fg(theme.header_fg)
                .add_modifier(Modifier::BOLD),
            ))),
            EmulatorEntry::Android(e) => ListItem::new(Line::from(vec![
              Span::raw("   "),
              Span::styled(&e.name, Style::default().fg(theme.name_fg)),
              Span::raw("  "),
              Span::styled(
                format!("[{}]", e.state),
                Style::default().fg(state_color(&e.state, theme)),
              ),
              Span::styled(
                format!("  ({})", e.device_type),
                Style::default().fg(theme.meta_fg),
              ),
            ])),
            EmulatorEntry::IOS(s) => ListItem::new(Line::from(vec![
              Span::raw("   "),
              Span::styled(&s.name, Style::default().fg(theme.name_fg)),
              Span::raw("  "),
              Span::styled(
                format!("[{}]", s.state),
                Style::default().fg(state_color(&s.state, theme)),
              ),
              Span::styled(
                format!("  ({})", s.runtime),
                Style::default().fg(theme.meta_fg),
              ),
            ])),
          }
        })
        .collect();

      let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Emulators "))
        .highlight_style(
          Style::default()
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD),
        );
      frame.render_stateful_widget(list, chunks[1], &mut app.list_state);

      // Help bar
      let help = Paragraph::new(Line::from(vec![
        Span::styled(" j/k", Style::default().fg(theme.help_key_fg)),
        Span::styled(" navigate  ", Style::default().fg(theme.help_text_fg)),
        Span::styled("Enter", Style::default().fg(theme.help_key_fg)),
        Span::styled(" open  ", Style::default().fg(theme.help_text_fg)),
        Span::styled("q/Esc", Style::default().fg(theme.help_key_fg)),
        Span::styled(" quit", Style::default().fg(theme.help_text_fg)),
      ]));
      frame.render_widget(help, chunks[2]);
    })?;

    if event::poll(std::time::Duration::from_millis(100))? {
      if let Event::Key(key) = event::read()? {
        if key.kind != KeyEventKind::Press {
          continue;
        }
        match key.code {
          KeyCode::Esc => break,
          KeyCode::Char('q') if app.filter.is_empty() => break,
          KeyCode::Char('j') if app.filter.is_empty() => app.move_selection(1),
          KeyCode::Char('k') if app.filter.is_empty() => app.move_selection(-1),
          KeyCode::Down => app.move_selection(1),
          KeyCode::Up => app.move_selection(-1),
          KeyCode::Enter => {
            if let Some(entry) = app.selected_entry() {
              match emulators::open_entry(entry) {
                Ok(msg) => {
                  app.result_message = Some(msg);
                  break;
                }
                Err(e) => {
                  app.result_message = Some(format!("Error: {}", e));
                  break;
                }
              }
            }
          }
          KeyCode::Backspace => {
            app.filter.pop();
            app.apply_filter();
          }
          KeyCode::Char(c) => {
            app.filter.push(c);
            app.apply_filter();
          }
          _ => {}
        }
      }
    }
  }

  Ok(())
}
