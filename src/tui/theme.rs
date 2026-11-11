use ratatui::style::{Color, Modifier, Style};

use crate::config::schema::{parse_hex_color, ThemeConfig};

#[derive(Debug, Clone)]
pub struct Theme {
    pub selected: Style,
    pub header: Style,
    pub search_match: Style,
    pub border: Style,
    pub normal: Style,
    pub status_bar: Style,
    #[allow(dead_code)]
    pub title: Style,
    pub label: Style,
    pub value: Style,
    pub required_label: Style,
    pub group_active: Style,
}

impl Theme {
    /// Build a theme from config. A color that is not a valid `#rrggbb`
    /// string falls back to that slot's default color, so a bad value from
    /// YAML, the settings editor, or library overrides can never panic.
    pub fn from_config(config: &ThemeConfig) -> Self {
        let defaults = ThemeConfig::default();
        let pick = |value: &str, default: &str| {
            parse_color(value)
                .or_else(|| parse_color(default))
                .unwrap_or(Color::Reset)
        };
        let selected_bg = pick(&config.selected_bg, &defaults.selected_bg);
        let selected_fg = pick(&config.selected_fg, &defaults.selected_fg);
        let header_bg = pick(&config.header_bg, &defaults.header_bg);
        let header_fg = pick(&config.header_fg, &defaults.header_fg);
        let search_match = pick(&config.search_match, &defaults.search_match);
        let border_color = pick(&config.border_color, &defaults.border_color);
        Theme {
            selected: Style::default().bg(selected_bg).fg(selected_fg),
            header: Style::default()
                .bg(header_bg)
                .fg(header_fg)
                .add_modifier(Modifier::BOLD),
            search_match: Style::default()
                .fg(search_match)
                .add_modifier(Modifier::BOLD),
            border: Style::default().fg(border_color),
            normal: Style::default(),
            status_bar: Style::default().bg(header_bg).fg(header_fg),
            title: Style::default().fg(header_fg).add_modifier(Modifier::BOLD),
            label: Style::default().fg(Color::DarkGray),
            value: Style::default(),
            required_label: Style::default().fg(header_fg).add_modifier(Modifier::BOLD),
            group_active: Style::default()
                .fg(search_match)
                .add_modifier(Modifier::BOLD),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::from_config(&ThemeConfig::default())
    }
}

fn parse_color(hex: &str) -> Option<Color> {
    parse_hex_color(hex).map(|(r, g, b)| Color::Rgb(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::ThemeConfig;

    #[test]
    fn test_parse_color_valid_hex() {
        assert_eq!(parse_color("#ff8000"), Some(Color::Rgb(0xff, 0x80, 0x00)));
    }

    #[test]
    fn test_parse_color_uppercase_hex() {
        assert_eq!(parse_color("#FF80aB"), Some(Color::Rgb(0xff, 0x80, 0xab)));
    }

    #[test]
    fn test_parse_color_black() {
        assert_eq!(parse_color("#000000"), Some(Color::Rgb(0, 0, 0)));
    }

    #[test]
    fn test_parse_color_rejects_malformed() {
        for bad in [
            "ff8000", "#fff", "", "#gg0000", "#ff80000", "#+f8000", "# f8000",
        ] {
            assert_eq!(parse_color(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn test_parse_color_rejects_unicode_without_panicking() {
        // 7 bytes with a leading '#', but 'é' straddles the channel slices.
        for bad in ["#aéabc", "#ééé", "#\u{1F600}ab", "#ab\u{1F600}"] {
            assert_eq!(parse_color(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn test_from_config_invalid_color_falls_back_to_default() {
        let config = ThemeConfig {
            selected_bg: "#aéabc".into(),
            border_color: "nonsense".into(),
            ..ThemeConfig::default()
        };
        let theme = Theme::from_config(&config);
        let default = Theme::default();
        assert_eq!(theme.selected.bg, default.selected.bg);
        assert_eq!(theme.border.fg, default.border.fg);
    }

    #[test]
    fn test_theme_default_does_not_panic() {
        let _theme = Theme::default();
    }

    #[test]
    fn test_theme_from_config_does_not_panic() {
        let _theme = Theme::from_config(&ThemeConfig::default());
    }
}
