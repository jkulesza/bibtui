use ratatui::style::{Color, Modifier, Style};

use crate::config::schema::ThemeConfig;

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
    pub fn from_config(config: &ThemeConfig) -> Self {
        Theme {
            selected: Style::default()
                .bg(parse_color(&config.selected_bg))
                .fg(parse_color(&config.selected_fg)),
            header: Style::default()
                .bg(parse_color(&config.header_bg))
                .fg(parse_color(&config.header_fg))
                .add_modifier(Modifier::BOLD),
            search_match: Style::default()
                .fg(parse_color(&config.search_match))
                .add_modifier(Modifier::BOLD),
            border: Style::default().fg(parse_color(&config.border_color)),
            normal: Style::default(),
            status_bar: Style::default()
                .bg(parse_color(&config.header_bg))
                .fg(parse_color(&config.header_fg)),
            title: Style::default()
                .fg(parse_color(&config.header_fg))
                .add_modifier(Modifier::BOLD),
            label: Style::default().fg(Color::DarkGray),
            value: Style::default(),
            required_label: Style::default()
                .fg(parse_color(&config.header_fg))
                .add_modifier(Modifier::BOLD),
            group_active: Style::default()
                .fg(parse_color(&config.search_match))
                .add_modifier(Modifier::BOLD),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::from_config(&ThemeConfig::default())
    }
}

fn parse_color(hex: &str) -> Color {
    if hex.starts_with('#') && hex.len() == 7 {
        let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(255);
        Color::Rgb(r, g, b)
    } else {
        Color::White
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::ThemeConfig;

    #[test]
    fn test_parse_color_valid_hex() {
        let c = parse_color("#ff8000");
        assert_eq!(c, Color::Rgb(0xff, 0x80, 0x00));
    }

    #[test]
    fn test_parse_color_black() {
        let c = parse_color("#000000");
        assert_eq!(c, Color::Rgb(0, 0, 0));
    }

    #[test]
    fn test_parse_color_invalid_no_hash() {
        let c = parse_color("ff8000");
        assert_eq!(c, Color::White);
    }

    #[test]
    fn test_parse_color_invalid_too_short() {
        let c = parse_color("#fff");
        assert_eq!(c, Color::White);
    }

    #[test]
    fn test_parse_color_empty() {
        let c = parse_color("");
        assert_eq!(c, Color::White);
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
