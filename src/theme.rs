use ratatui::style::Color;

use crate::config::ThemeOverrides;

/// All color slots used by the TUI.
pub struct ThemeColors {
  pub header_fg: Color,
  pub name_fg: Color,
  pub state_booted_fg: Color,
  pub state_shutdown_fg: Color,
  pub state_unknown_fg: Color,
  pub meta_fg: Color,
  pub filter_placeholder_fg: Color,
  pub filter_active_fg: Color,
  pub selection_bg: Color,
  pub help_key_fg: Color,
  pub help_text_fg: Color,
}

#[derive(Debug, Clone, Copy)]
enum ThemeName {
  Default,
  CatppuccinMocha,
  CatppuccinLatte,
  Dracula,
  TokyoNight,
  GruvboxDark,
  Nord,
}

impl ThemeName {
  fn from_str(s: &str) -> Self {
    match s.to_lowercase().replace(' ', "-").as_str() {
      "catppuccin-mocha" => Self::CatppuccinMocha,
      "catppuccin-latte" => Self::CatppuccinLatte,
      "dracula" => Self::Dracula,
      "tokyo-night" | "tokyonight" => Self::TokyoNight,
      "gruvbox-dark" | "gruvbox" => Self::GruvboxDark,
      "nord" => Self::Nord,
      _ => Self::Default,
    }
  }
}

fn parse_hex_color(s: &str) -> Option<Color> {
  let s = s.strip_prefix('#')?;
  if s.len() != 6 {
    return None;
  }
  let r = u8::from_str_radix(&s[0..2], 16).ok()?;
  let g = u8::from_str_radix(&s[2..4], 16).ok()?;
  let b = u8::from_str_radix(&s[4..6], 16).ok()?;
  Some(Color::Rgb(r, g, b))
}

/// Shorthand for 256-color indexed palette.
const fn c(n: u8) -> Color {
  Color::Indexed(n)
}

fn base_theme(name: ThemeName) -> ThemeColors {
  match name {
    ThemeName::Default => ThemeColors {
      header_fg: Color::Cyan,
      name_fg: Color::Green,
      state_booted_fg: Color::Green,
      state_shutdown_fg: Color::Red,
      state_unknown_fg: Color::Yellow,
      meta_fg: Color::DarkGray,
      filter_placeholder_fg: Color::DarkGray,
      filter_active_fg: Color::White,
      selection_bg: Color::DarkGray,
      help_key_fg: Color::Yellow,
      help_text_fg: Color::White,
    },
    // https://github.com/catppuccin/catppuccin — Mocha palette
    ThemeName::CatppuccinMocha => ThemeColors {
      header_fg: c(111),        // Blue (#89b4fa)
      name_fg: c(151),          // Green (#a6e3a1)
      state_booted_fg: c(151),  // Green
      state_shutdown_fg: c(211), // Red (#f38ba8)
      state_unknown_fg: c(223), // Yellow (#f9e2af)
      meta_fg: c(103),          // Overlay0 (#9399b2)
      filter_placeholder_fg: c(103),
      filter_active_fg: c(189), // Text (#cdd6f4)
      selection_bg: c(59),      // Surface1 (#45475a)
      help_key_fg: c(218),      // Pink (#f5c2e7)
      help_text_fg: c(146),     // Subtext0 (#bac2de)
    },
    // https://github.com/catppuccin/catppuccin — Latte palette
    ThemeName::CatppuccinLatte => ThemeColors {
      header_fg: c(27),         // Blue (#1e66f5)
      name_fg: c(70),           // Green (#40a02b)
      state_booted_fg: c(70),   // Green
      state_shutdown_fg: c(161), // Red (#d20f39)
      state_unknown_fg: c(172), // Yellow (#df8e1d)
      meta_fg: c(103),          // Overlay0 (#8c8fa1)
      filter_placeholder_fg: c(103),
      filter_active_fg: c(59),  // Text (#4c4f69)
      selection_bg: c(146),     // Surface1 (#bcc0cc)
      help_key_fg: c(170),      // Pink (#ea76cb)
      help_text_fg: c(60),      // Subtext0 (#6c6f85)
    },
    // https://draculatheme.com/contribute
    ThemeName::Dracula => ThemeColors {
      header_fg: c(117),        // Cyan (#8be9fd)
      name_fg: c(83),           // Green (#50fa7b)
      state_booted_fg: c(83),   // Green
      state_shutdown_fg: c(203), // Red (#ff5555)
      state_unknown_fg: c(228), // Yellow (#f1fa8c)
      meta_fg: c(61),           // Comment (#6272a4)
      filter_placeholder_fg: c(61),
      filter_active_fg: c(231), // Foreground (#f8f8f2)
      selection_bg: c(59),      // Current Line (#44475a)
      help_key_fg: c(206),      // Pink (#ff79c6)
      help_text_fg: c(231),     // Foreground
    },
    // https://github.com/enkia/tokyo-night-vscode-theme
    ThemeName::TokyoNight => ThemeColors {
      header_fg: c(117),        // Blue (#7dcfff)
      name_fg: c(149),          // Green (#9ece6a)
      state_booted_fg: c(149),  // Green
      state_shutdown_fg: c(204), // Red (#f7768e)
      state_unknown_fg: c(179), // Yellow (#e0af68)
      meta_fg: c(60),           // Comment (#565f89)
      filter_placeholder_fg: c(60),
      filter_active_fg: c(146), // Foreground (#a9b1d6)
      selection_bg: c(236),     // Selection (#292e42)
      help_key_fg: c(141),      // Purple (#bb9af7)
      help_text_fg: c(146),     // Foreground
    },
    // https://github.com/morhetz/gruvbox
    ThemeName::GruvboxDark => ThemeColors {
      header_fg: c(108),        // Aqua (#83a598)
      name_fg: c(142),          // Green (#b8bb26)
      state_booted_fg: c(142),  // Green
      state_shutdown_fg: c(202), // Red (#fb4934)
      state_unknown_fg: c(214), // Yellow (#fabd2f)
      meta_fg: c(101),          // Gray (#928374)
      filter_placeholder_fg: c(101),
      filter_active_fg: c(223), // Foreground (#ebdbb2)
      selection_bg: c(239),     // Bg2 (#504945)
      help_key_fg: c(174),      // Purple (#d3869b)
      help_text_fg: c(181),     // Fg2 (#d5c4a1)
    },
    // https://www.nordtheme.com/docs/colors-and-palettes
    ThemeName::Nord => ThemeColors {
      header_fg: c(110),        // Nord8 frost cyan (#88c0d0)
      name_fg: c(144),          // Nord14 green (#a3be8c)
      state_booted_fg: c(144),  // Nord14
      state_shutdown_fg: c(131), // Nord11 red (#bf616a)
      state_unknown_fg: c(222), // Nord13 yellow (#ebcb8b)
      meta_fg: c(240),          // Nord3 comment (#4c566a)
      filter_placeholder_fg: c(240),
      filter_active_fg: c(188), // Nord4 snow (#d8dee9)
      selection_bg: c(239),     // Nord2 (#434c5e)
      help_key_fg: c(139),      // Nord15 purple (#b48ead)
      help_text_fg: c(189),     // Nord5 (#e5e9f0)
    },
  }
}

/// Resolve the final theme: base palette + optional per-slot overrides.
pub fn resolve_theme(theme_name: Option<&str>, overrides: Option<&ThemeOverrides>) -> ThemeColors {
  let name = theme_name
    .map(ThemeName::from_str)
    .unwrap_or(ThemeName::Default);
  let mut colors = base_theme(name);

  if let Some(ov) = overrides {
    macro_rules! apply {
      ($field:ident) => {
        if let Some(ref hex) = ov.$field {
          if let Some(c) = parse_hex_color(hex) {
            colors.$field = c;
          }
        }
      };
    }
    apply!(header_fg);
    apply!(name_fg);
    apply!(state_booted_fg);
    apply!(state_shutdown_fg);
    apply!(state_unknown_fg);
    apply!(meta_fg);
    apply!(filter_placeholder_fg);
    apply!(filter_active_fg);
    apply!(selection_bg);
    apply!(help_key_fg);
    apply!(help_text_fg);
  }

  colors
}
