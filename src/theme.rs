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
  // The slicing below is by byte offset, so reject anything non-ASCII first
  if s.len() != 6 || !s.is_ascii() {
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hex_colors_with_multibyte_characters_are_rejected_not_a_panic() {
    // "aébcd" is 6 bytes, so the length check passes, but byte 2 is in the middle of 'é'
    assert_eq!(parse_hex_color("#aébcd"), None);
  }

  // --- hex colours -----------------------------------------------------

  #[test]
  fn hex_colors_are_parsed_to_rgb() {
    assert_eq!(parse_hex_color("#ff8800"), Some(Color::Rgb(255, 136, 0)));
    assert_eq!(parse_hex_color("#000000"), Some(Color::Rgb(0, 0, 0)));
    assert_eq!(
      parse_hex_color("#AbCdEf"),
      Some(Color::Rgb(0xab, 0xcd, 0xef))
    );
  }

  #[test]
  fn malformed_hex_colors_are_rejected() {
    for bad in [
      "", "#", "ff8800", "#ff88", "#ff88000", "#gg0000", "#ff 800", "red",
    ] {
      assert_eq!(parse_hex_color(bad), None, "for {:?}", bad);
    }
  }

  // --- theme names -----------------------------------------------------

  fn name_of(input: &str) -> String {
    format!("{:?}", ThemeName::from_str(input))
  }

  #[test]
  fn theme_names_are_case_insensitive_and_accept_aliases() {
    assert_eq!(name_of("dracula"), "Dracula");
    assert_eq!(name_of("DRACULA"), "Dracula");
    assert_eq!(name_of("Catppuccin Mocha"), "CatppuccinMocha");
    assert_eq!(name_of("catppuccin-latte"), "CatppuccinLatte");
    assert_eq!(name_of("tokyonight"), "TokyoNight");
    assert_eq!(name_of("tokyo-night"), "TokyoNight");
    assert_eq!(name_of("gruvbox"), "GruvboxDark");
    assert_eq!(name_of("nord"), "Nord");
  }

  #[test]
  fn unknown_theme_names_fall_back_to_the_default() {
    assert_eq!(name_of("no-such-theme"), "Default");
    assert_eq!(name_of(""), "Default");
  }

  // --- resolving a theme -----------------------------------------------

  #[test]
  fn no_theme_means_the_default_palette() {
    let theme = resolve_theme(None, None);
    assert_eq!(theme.header_fg, Color::Cyan);
    assert_eq!(theme.state_booted_fg, Color::Green);
    assert_eq!(theme.state_shutdown_fg, Color::Red);
  }

  #[test]
  fn a_named_theme_changes_the_palette() {
    let default = resolve_theme(None, None);
    let dracula = resolve_theme(Some("dracula"), None);
    assert_ne!(dracula.header_fg, default.header_fg);
  }

  #[test]
  fn overrides_replace_only_the_fields_they_set() {
    let overrides = ThemeOverrides {
      header_fg: Some("#112233".to_string()),
      selection_bg: Some("#445566".to_string()),
      ..Default::default()
    };
    let base = resolve_theme(Some("nord"), None);
    let themed = resolve_theme(Some("nord"), Some(&overrides));

    assert_eq!(themed.header_fg, Color::Rgb(0x11, 0x22, 0x33));
    assert_eq!(themed.selection_bg, Color::Rgb(0x44, 0x55, 0x66));
    // Untouched fields keep the base theme's values
    assert_eq!(themed.name_fg, base.name_fg);
    assert_eq!(themed.help_key_fg, base.help_key_fg);
  }

  #[test]
  fn invalid_overrides_are_ignored() {
    let overrides = ThemeOverrides {
      header_fg: Some("not-a-color".to_string()),
      name_fg: Some("#aébcd".to_string()),
      ..Default::default()
    };
    let base = resolve_theme(None, None);
    let themed = resolve_theme(None, Some(&overrides));

    assert_eq!(themed.header_fg, base.header_fg);
    assert_eq!(themed.name_fg, base.name_fg);
  }

  #[test]
  fn every_override_field_is_applied() {
    let hex = Some("#010203".to_string());
    let overrides = ThemeOverrides {
      header_fg: hex.clone(),
      name_fg: hex.clone(),
      state_booted_fg: hex.clone(),
      state_shutdown_fg: hex.clone(),
      state_unknown_fg: hex.clone(),
      meta_fg: hex.clone(),
      filter_placeholder_fg: hex.clone(),
      filter_active_fg: hex.clone(),
      selection_bg: hex.clone(),
      help_key_fg: hex.clone(),
      help_text_fg: hex,
    };
    let t = resolve_theme(None, Some(&overrides));
    let want = Color::Rgb(1, 2, 3);

    for (field, color) in [
      ("header_fg", t.header_fg),
      ("name_fg", t.name_fg),
      ("state_booted_fg", t.state_booted_fg),
      ("state_shutdown_fg", t.state_shutdown_fg),
      ("state_unknown_fg", t.state_unknown_fg),
      ("meta_fg", t.meta_fg),
      ("filter_placeholder_fg", t.filter_placeholder_fg),
      ("filter_active_fg", t.filter_active_fg),
      ("selection_bg", t.selection_bg),
      ("help_key_fg", t.help_key_fg),
      ("help_text_fg", t.help_text_fg),
    ] {
      assert_eq!(color, want, "{} was not overridden", field);
    }
  }
}
