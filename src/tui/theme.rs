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
