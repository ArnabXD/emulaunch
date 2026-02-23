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

fn rgb(r: u8, g: u8, b: u8) -> Color {
  Color::Rgb(r, g, b)
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

fn base_theme(name: ThemeName) -> ThemeColors {
  match name {
    // RGB equivalents of the original ANSI colors
    ThemeName::Default => ThemeColors {
      header_fg: rgb(0, 205, 205),        // Cyan
      name_fg: rgb(0, 205, 0),            // Green
      state_booted_fg: rgb(0, 205, 0),    // Green
      state_shutdown_fg: rgb(205, 0, 0),  // Red
      state_unknown_fg: rgb(205, 205, 0), // Yellow
      meta_fg: rgb(128, 128, 128),        // DarkGray
      filter_placeholder_fg: rgb(128, 128, 128),
      filter_active_fg: rgb(229, 229, 229), // White
      selection_bg: rgb(68, 68, 68),        // DarkGray bg
      help_key_fg: rgb(205, 205, 0),        // Yellow
      help_text_fg: rgb(229, 229, 229),     // White
    },
    // https://github.com/catppuccin/catppuccin — Mocha palette
    ThemeName::CatppuccinMocha => ThemeColors {
      header_fg: rgb(137, 180, 250),         // Blue
      name_fg: rgb(166, 227, 161),           // Green
      state_booted_fg: rgb(166, 227, 161),   // Green
      state_shutdown_fg: rgb(243, 139, 168), // Red
      state_unknown_fg: rgb(249, 226, 175),  // Yellow
      meta_fg: rgb(147, 153, 178),           // Overlay0
      filter_placeholder_fg: rgb(147, 153, 178),
      filter_active_fg: rgb(205, 214, 244), // Text
      selection_bg: rgb(69, 71, 90),        // Surface1
      help_key_fg: rgb(245, 194, 231),      // Pink
      help_text_fg: rgb(186, 194, 222),     // Subtext0
    },
    // https://github.com/catppuccin/catppuccin — Latte palette
    ThemeName::CatppuccinLatte => ThemeColors {
      header_fg: rgb(30, 102, 245),        // Blue
      name_fg: rgb(64, 160, 43),           // Green
      state_booted_fg: rgb(64, 160, 43),   // Green
      state_shutdown_fg: rgb(210, 15, 57), // Red
      state_unknown_fg: rgb(223, 142, 29), // Yellow
      meta_fg: rgb(140, 143, 161),         // Overlay0
      filter_placeholder_fg: rgb(140, 143, 161),
      filter_active_fg: rgb(76, 79, 105), // Text
      selection_bg: rgb(188, 192, 204),   // Surface1
      help_key_fg: rgb(234, 118, 203),    // Pink
      help_text_fg: rgb(108, 111, 133),   // Subtext0
    },
    // https://draculatheme.com/contribute
    ThemeName::Dracula => ThemeColors {
      header_fg: rgb(139, 233, 253),        // Cyan
      name_fg: rgb(80, 250, 123),           // Green
      state_booted_fg: rgb(80, 250, 123),   // Green
      state_shutdown_fg: rgb(255, 85, 85),  // Red
      state_unknown_fg: rgb(241, 250, 140), // Yellow
      meta_fg: rgb(98, 114, 164),           // Comment
      filter_placeholder_fg: rgb(98, 114, 164),
      filter_active_fg: rgb(248, 248, 242), // Foreground
      selection_bg: rgb(68, 71, 90),        // Current Line
      help_key_fg: rgb(255, 121, 198),      // Pink
      help_text_fg: rgb(248, 248, 242),     // Foreground
    },
    // https://github.com/enkia/tokyo-night-vscode-theme
    ThemeName::TokyoNight => ThemeColors {
      header_fg: rgb(125, 207, 255),         // Blue
      name_fg: rgb(158, 206, 106),           // Green
      state_booted_fg: rgb(158, 206, 106),   // Green
      state_shutdown_fg: rgb(247, 118, 142), // Red
      state_unknown_fg: rgb(224, 175, 104),  // Yellow
      meta_fg: rgb(86, 95, 137),             // Comment
      filter_placeholder_fg: rgb(86, 95, 137),
      filter_active_fg: rgb(169, 177, 214), // Foreground
      selection_bg: rgb(41, 46, 66),        // Selection
      help_key_fg: rgb(187, 154, 247),      // Purple
      help_text_fg: rgb(169, 177, 214),     // Foreground
    },
    // https://github.com/morhetz/gruvbox
    ThemeName::GruvboxDark => ThemeColors {
      header_fg: rgb(131, 165, 152),       // Aqua
      name_fg: rgb(184, 187, 38),          // Green
      state_booted_fg: rgb(184, 187, 38),  // Green
      state_shutdown_fg: rgb(251, 73, 52), // Red
      state_unknown_fg: rgb(250, 189, 47), // Yellow
      meta_fg: rgb(146, 131, 116),         // Gray
      filter_placeholder_fg: rgb(146, 131, 116),
      filter_active_fg: rgb(235, 219, 178), // Foreground
      selection_bg: rgb(80, 73, 69),        // Bg2
      help_key_fg: rgb(211, 134, 155),      // Purple
      help_text_fg: rgb(213, 196, 161),     // Fg2
    },
    // https://www.nordtheme.com/docs/colors-and-palettes
    ThemeName::Nord => ThemeColors {
      header_fg: rgb(136, 192, 208),        // Nord8 (frost cyan)
      name_fg: rgb(163, 190, 140),          // Nord14 (green)
      state_booted_fg: rgb(163, 190, 140),  // Nord14
      state_shutdown_fg: rgb(191, 97, 106), // Nord11 (red)
      state_unknown_fg: rgb(235, 203, 139), // Nord13 (yellow)
      meta_fg: rgb(76, 86, 106),            // Nord3 (comment)
      filter_placeholder_fg: rgb(76, 86, 106),
      filter_active_fg: rgb(216, 222, 233), // Nord4 (snow)
      selection_bg: rgb(67, 76, 94),        // Nord2
      help_key_fg: rgb(180, 142, 173),      // Nord15 (purple)
      help_text_fg: rgb(229, 233, 240),     // Nord5
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
