use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

#[derive(Debug, Clone)]
pub enum DialogKind {
    Confirm {
        title: String,
        message: String,
    },
    TypePicker {
        title: String,
        options: Vec<String>,
    },
}

pub struct DialogState {
    pub kind: DialogKind,
    pub list_state: ListState,
}

impl DialogState {
    pub fn confirm(title: &str, message: &str) -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        DialogState {
            kind: DialogKind::Confirm {
                title: title.to_string(),
                message: message.to_string(),
            },
            list_state: state,
        }
    }

    pub fn type_picker(options: Vec<String>) -> Self {
        Self::type_picker_titled("Select Entry Type", options)
    }

    pub fn type_picker_titled(title: &str, options: Vec<String>) -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        DialogState {
            kind: DialogKind::TypePicker {
                title: title.to_string(),
                options,
            },
            list_state: state,
        }
    }

    pub fn selected(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }

    pub fn select(&mut self, idx: usize) {
        self.list_state.select(Some(idx));
    }

    pub fn option_count(&self) -> usize {
        match &self.kind {
            DialogKind::Confirm { .. } => 2,
            DialogKind::TypePicker { options, .. } => options.len(),

        }
    }
}

pub fn render_dialog(f: &mut Frame, area: Rect, state: &mut DialogState, theme: &Theme) {
    // Center a dialog box
    let dialog_width = 40u16.min(area.width.saturating_sub(4));
    let dialog_height = match &state.kind {
        DialogKind::Confirm { .. } => 5,
        DialogKind::TypePicker { options, .. } => (options.len() as u16 + 4).min(area.height - 4),
    };

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    f.render_widget(Clear, dialog_area);

    match &state.kind {
        DialogKind::Confirm { title, message } => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border)
                .title(format!(" {} ", title));

            let inner = block.inner(dialog_area);
            f.render_widget(block, dialog_area);

            let lines = vec![
                Line::from(message.as_str()),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[y]es", theme.search_match),
                    Span::raw("  "),
                    Span::styled("[n]o", theme.label),
                ]),
            ];
            let para = Paragraph::new(lines);
            f.render_widget(para, inner);
        }
        DialogKind::TypePicker { title, options } => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border)
                .title(format!(" {} ", title));

            let items: Vec<ListItem> = options
                .iter()
                .map(|opt| ListItem::new(Line::from(format!("  {}", opt))))
                .collect();

            let list = List::new(items)
                .block(block)
                .highlight_style(theme.selected);

            f.render_stateful_widget(list, dialog_area, &mut state.list_state);
        }
    }
}
