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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::model::{Group, GroupNode, GroupType, GroupTree};

    fn make_tree_with_child() -> GroupTree {
        GroupTree {
            root: GroupNode {
                group: Group { name: "All Entries".to_string(), group_type: GroupType::AllEntries },
                children: vec![
                    GroupNode {
                        group: Group { name: "Physics".to_string(), group_type: GroupType::Static },
                        children: vec![
                            GroupNode {
                                group: Group { name: "Nuclear".to_string(), group_type: GroupType::Static },
                                children: vec![],
                                expanded: true,
                            }
                        ],
                        expanded: true,
                    }
                ],
                expanded: true,
            },
        }
    }

    #[test]
    fn test_new_default_tree() {
        let tree = GroupTree::default();
        let state = GroupTreeState::new(&tree);
        assert_eq!(state.flat_items.len(), 1); // just "All Entries"
        assert_eq!(state.flat_items[0].name, "All Entries");
        assert_eq!(state.selected(), 0);
    }

    #[test]
    fn test_new_with_children() {
        let tree = make_tree_with_child();
        let state = GroupTreeState::new(&tree);
        assert_eq!(state.flat_items.len(), 3); // All Entries, Physics, Nuclear
        assert_eq!(state.flat_items[0].name, "All Entries");
        assert_eq!(state.flat_items[1].name, "Physics");
        assert_eq!(state.flat_items[2].name, "Nuclear");
    }

    #[test]
    fn test_depth_is_correct() {
        let tree = make_tree_with_child();
        let state = GroupTreeState::new(&tree);
        assert_eq!(state.flat_items[0].depth, 0);
        assert_eq!(state.flat_items[1].depth, 1);
        assert_eq!(state.flat_items[2].depth, 2);
    }

    #[test]
    fn test_has_children_flag() {
        let tree = make_tree_with_child();
        let state = GroupTreeState::new(&tree);
        assert!(state.flat_items[0].has_children);
        assert!(state.flat_items[1].has_children);
        assert!(!state.flat_items[2].has_children);
    }

    #[test]
    fn test_select() {
        let tree = make_tree_with_child();
        let mut state = GroupTreeState::new(&tree);
        state.select(2);
        assert_eq!(state.selected(), 2);
    }

    #[test]
    fn test_select_out_of_bounds_is_noop() {
        let tree = GroupTree::default();
        let mut state = GroupTreeState::new(&tree);
        state.select(99);
        assert_eq!(state.selected(), 0); // unchanged
    }

    #[test]
    fn test_selected_item() {
        let tree = make_tree_with_child();
        let mut state = GroupTreeState::new(&tree);
        state.select(1);
        let item = state.selected_item().unwrap();
        assert_eq!(item.name, "Physics");
    }

    #[test]
    fn test_refresh() {
        let tree = GroupTree::default();
        let mut state = GroupTreeState::new(&tree);
        let tree2 = make_tree_with_child();
        state.refresh(&tree2);
        assert_eq!(state.flat_items.len(), 3);
    }

    #[test]
    fn test_set_entry_count() {
        let tree = make_tree_with_child();
        let mut state = GroupTreeState::new(&tree);
        state.set_entry_count("Physics", 42);
        let item = state.flat_items.iter().find(|i| i.name == "Physics").unwrap();
        assert_eq!(item.entry_count, Some(42));
    }

    #[test]
    fn test_collapsed_node_hides_children() {
        let mut tree = make_tree_with_child();
        tree.root.children[0].expanded = false; // collapse Physics
        let state = GroupTreeState::new(&tree);
        assert_eq!(state.flat_items.len(), 2); // All Entries + Physics only
    }

    #[test]
    fn test_selected_item_empty_tree() {
        // A tree whose root has no children (flat_items = just root, index 0)
        // shouldn't return None normally, but test that selected() works at boundary.
        let tree = GroupTree::default();
        let mut state = GroupTreeState::new(&tree);
        // Force list_state to have no selection
        state.list_state.select(None);
        // selected() falls back to 0, flat_items.get(0) = root item
        let item = state.selected_item();
        assert!(item.is_some());
    }

    #[test]
    fn test_refresh_clamps_out_of_bounds_selection() {
        let tree = make_tree_with_child(); // 3 items
        let mut state = GroupTreeState::new(&tree);
        state.select(2); // last item ("Nuclear")
        // Refresh with a smaller tree (only 1 item)
        let small_tree = GroupTree::default();
        state.refresh(&small_tree);
        assert_eq!(state.flat_items.len(), 1);
        // Selection should clamp to last valid index (0)
        assert_eq!(state.selected(), 0);
    }

    #[test]
    fn test_refresh_preserves_valid_selection() {
        let tree = make_tree_with_child(); // 3 items
        let mut state = GroupTreeState::new(&tree);
        state.select(1); // "Physics"
        // Refresh with same tree
        let tree2 = make_tree_with_child();
        state.refresh(&tree2);
        assert_eq!(state.flat_items.len(), 3);
        assert_eq!(state.selected(), 1); // still on "Physics"
    }

    #[test]
    fn test_set_entry_count_no_match() {
        let tree = make_tree_with_child();
        let mut state = GroupTreeState::new(&tree);
        state.set_entry_count("NonExistent", 10);
        // No item should have a count set
        for item in &state.flat_items {
            assert!(item.entry_count.is_none());
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
