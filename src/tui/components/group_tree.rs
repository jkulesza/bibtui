use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::bib::model::{GroupNode, GroupTree};
use crate::tui::theme::Theme;

pub struct GroupTreeState {
    pub list_state: ListState,
    /// Flattened list of (depth, group_name, node_path_index) for display
    pub flat_items: Vec<FlatGroupItem>,
    pub active_group: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FlatGroupItem {
    pub depth: usize,
    pub name: String,
    pub has_children: bool,
    pub expanded: bool,
    /// Path indices to locate this node in the tree (reserved for future tree editing)
    #[allow(dead_code)]
    pub path: Vec<usize>,
    pub entry_count: Option<usize>,
}

impl GroupTreeState {
    pub fn new(tree: &GroupTree) -> Self {
        let flat_items = flatten_tree(tree);
        let mut state = ListState::default();
        if !flat_items.is_empty() {
            state.select(Some(0));
        }
        GroupTreeState {
            list_state: state,
            flat_items,
            active_group: None,
        }
    }

    pub fn selected(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }

    pub fn select(&mut self, idx: usize) {
        if idx < self.flat_items.len() {
            self.list_state.select(Some(idx));
        }
    }

    pub fn selected_item(&self) -> Option<&FlatGroupItem> {
        self.flat_items.get(self.selected())
    }

    #[allow(dead_code)]
    pub fn refresh(&mut self, tree: &GroupTree) {
        let sel = self.selected();
        self.flat_items = flatten_tree(tree);
        if sel < self.flat_items.len() {
            self.select(sel);
        } else if !self.flat_items.is_empty() {
            self.select(self.flat_items.len() - 1);
        }
    }

    #[allow(dead_code)]
    pub fn set_entry_count(&mut self, name: &str, count: usize) {
        for item in &mut self.flat_items {
            if item.name == name {
                item.entry_count = Some(count);
            }
        }
    }
}

fn flatten_tree(tree: &GroupTree) -> Vec<FlatGroupItem> {
    let mut items = Vec::new();
    flatten_node(&tree.root, 0, &mut Vec::new(), &mut items);
    items
}

fn flatten_node(
    node: &GroupNode,
    depth: usize,
    path: &mut Vec<usize>,
    items: &mut Vec<FlatGroupItem>,
) {
    items.push(FlatGroupItem {
        depth,
        name: node.group.name.clone(),
        has_children: !node.children.is_empty(),
        expanded: node.expanded,
        path: path.clone(),
        entry_count: None,
    });

    if node.expanded {
        for (i, child) in node.children.iter().enumerate() {
            path.push(i);
            flatten_node(child, depth + 1, path, items);
            path.pop();
        }
    }
}

pub fn render_group_tree(
    f: &mut Frame,
    area: Rect,
    state: &mut GroupTreeState,
    theme: &Theme,
    focused: bool,
    total_entries: usize,
) {
    let border_style = if focused {
        theme.border.add_modifier(Modifier::BOLD)
    } else {
        theme.border
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Groups ");

    let items: Vec<ListItem> = state
        .flat_items
        .iter()
        .map(|item| {
            let indent = "  ".repeat(item.depth);
            let icon = if item.has_children {
                if item.expanded {
                    "v "
                } else {
                    "> "
                }
            } else {
                "  "
            };

            let count_str = if item.depth == 0 {
                format!(" ({})", total_entries)
            } else if let Some(count) = item.entry_count {
                format!(" ({})", count)
            } else {
                String::new()
            };

            let style = if state.active_group.as_ref() == Some(&item.name) {
                theme.group_active
            } else {
                theme.normal
            };

            ListItem::new(Line::from(vec![Span::styled(
                format!("{}{}{}{}", indent, icon, item.name, count_str),
                style,
            )]))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected);

    f.render_stateful_widget(list, area, &mut state.list_state);
}
