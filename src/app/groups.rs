//! Group management: CRUD on the JabRef group tree and raw-file sync.

use super::*;

impl App {
    pub(super) fn select_group(&mut self) {
        if let Some(item) = self.group_tree_state.selected_item() {
            let name = item.name.clone();

            if self.group_tree_state.active_group.as_ref() == Some(&name) {
                // Deselect
                self.group_tree_state.active_group = None;
                self.filtered_indices = None;
            } else {
                // Find the group node and filter
                if let Some(node) = find_group_node(&self.database.groups.root, &name) {
                    let entries: Vec<&Entry> = self
                        .sorted_keys
                        .iter()
                        .filter_map(|k| self.database.entries.get(k))
                        .collect();
                    let indices = filter_by_group(&entries, node);
                    self.search_bar_state.result_count = indices.len();
                    self.filtered_indices = Some(indices);
                    self.group_tree_state.active_group = Some(name);
                    self.entry_list_state.select(0);
                }
            }
            self.focus = Focus::List;
        }
    }

    pub(super) fn start_add_group(&mut self) {
        let parent_path = self
            .group_tree_state
            .selected_item()
            .map(|item| item.path.clone())
            .unwrap_or_default();
        self.field_editor_state = Some(FieldEditorState::for_input("Group name"));
        self.pending_action = Some(PendingAction::AddGroup { parent_path });
        self.mode = InputMode::Editing;
    }

    pub(super) fn start_delete_group(&mut self) {
        let item = match self.group_tree_state.selected_item() {
            Some(item) => item.clone(),
            None => return,
        };
        if item.depth == 0 {
            self.status_message = Some("Cannot delete root group".to_string());
            return;
        }
        let name = item.name.clone();
        let path = item.path.clone();
        self.dialog_state = Some(DialogState::confirm(
            "Delete Group",
            &format!("Delete group '{}'?", name),
        ));
        self.pending_action = Some(PendingAction::DeleteGroup { path });
        self.mode = InputMode::Dialog;
    }

    pub(super) fn start_edit_groups(&mut self) {
        let entry_key = match self.detail_entry_key.clone() {
            Some(k) => k,
            None => return,
        };
        let entry = match self.database.entries.get(&entry_key) {
            Some(e) => e,
            None => return,
        };
        let memberships = entry.group_memberships.clone();
        let mut group_names = Vec::new();
        collect_group_names(&self.database.groups.root, &mut group_names);
        if group_names.is_empty() {
            self.status_message = Some("No groups defined".to_string());
            return;
        }
        let groups: Vec<(String, bool)> = group_names
            .into_iter()
            .map(|name| {
                let checked = memberships.contains(&name);
                (name, checked)
            })
            .collect();
        self.dialog_state = Some(DialogState::group_assign(groups));
        self.pending_action = Some(PendingAction::AssignGroups { entry_key });
        self.mode = InputMode::Dialog;
    }

    pub(super) fn finish_add_group(&mut self, name: String, parent_path: Vec<usize>) {
        self.push_undo(UndoItem::GroupTreeChanged { old_tree: self.database.groups.clone() });
        let new_node = GroupNode {
            group: Group {
                name: name.clone(),
                group_type: GroupType::Static,
            },
            children: Vec::new(),
            expanded: true,
        };
        if let Some(parent) =
            find_group_node_mut(&mut self.database.groups.root, &parent_path)
        {
            parent.children.push(new_node);
        }
        self.sync_groups_to_raw();
        self.group_tree_state.refresh(&self.database.groups);
        self.status_message = Some(format!("Group '{}' added", name));
    }

    pub(super) fn finish_delete_group(&mut self, path: Vec<usize>) {
        if path.is_empty() {
            return;
        }
        self.push_undo(UndoItem::GroupTreeChanged { old_tree: self.database.groups.clone() });
        let (parent_path, child_idx) = path.split_at(path.len() - 1);
        let child_idx = child_idx[0];
        if let Some(parent) =
            find_group_node_mut(&mut self.database.groups.root, parent_path)
        {
            if child_idx < parent.children.len() {
                let removed = parent.children.remove(child_idx);
                self.sync_groups_to_raw();
                self.group_tree_state.refresh(&self.database.groups);
                // Clear active group filter if the deleted group was active
                if self.group_tree_state.active_group.as_deref()
                    == Some(removed.group.name.as_str())
                {
                    self.group_tree_state.active_group = None;
                    self.filtered_indices = None;
                }
                self.status_message =
                    Some(format!("Group '{}' deleted", removed.group.name));
            }
        }
    }

    pub(super) fn finish_assign_groups(&mut self, entry_key: &str, selected_groups: Vec<String>) {
        // Snapshot before mutating (avoid holding a mutable borrow while calling push_undo)
        let undo_item = self.database.entries.get(entry_key).map(|entry| {
            UndoItem::GroupMembershipChanged {
                entry_key: entry_key.to_string(),
                old_memberships: entry.group_memberships.clone(),
                old_groups_field: entry.fields.get("groups").cloned(),
            }
        });
        if let Some(item) = undo_item {
            self.push_undo(item);
        }
        if let Some(entry) = self.database.entries.get_mut(entry_key) {
            if selected_groups.is_empty() {
                entry.fields.shift_remove("groups");
            } else {
                entry
                    .fields
                    .insert("groups".to_string(), selected_groups.join(","));
            }
            entry.group_memberships = selected_groups;
            entry.dirty = true;
            let entry_clone = entry.clone();
            if let Some(ref mut detail) = self.detail_state {
                detail.refresh(&entry_clone);
            }
        }
    }

    pub(super) fn sync_groups_to_raw(&mut self) {
        let serialized = serialize_group_tree(&self.database.groups);
        let new_raw = format!("@Comment{{jabref-meta: grouping:\n{};}}", serialized);
        for item in &mut self.database.raw_file.items {
            if let RawItem::Comment { raw_text } = item {
                if raw_text.contains("jabref-meta: grouping:") {
                    *raw_text = new_raw;
                    self.database
                        .jabref_meta
                        .unknown_meta
                        .insert("grouping".to_string(), serialized);
                    return;
                }
            }
        }
        // No existing grouping comment — append one
        self.database
            .raw_file
            .items
            .push(RawItem::Comment { raw_text: new_raw });
        self.database
            .jabref_meta
            .unknown_meta
            .insert("grouping".to_string(), serialized);
    }

    /// Filter the entry list to a named group (used by `:group <name>` command).
    pub(super) fn apply_group_filter(&mut self, group_name: &str) {
        if let Some(node) = find_group_node(&self.database.groups.root, group_name) {
            let entries: Vec<&Entry> = self
                .sorted_keys
                .iter()
                .filter_map(|k| self.database.entries.get(k))
                .collect();
            let indices = filter_by_group(&entries, node);
            self.search_bar_state.result_count = indices.len();
            self.filtered_indices = Some(indices);
            self.group_tree_state.active_group = Some(group_name.to_string());
            self.entry_list_state.select(0);
            self.status_message = Some(format!("Group: {}", group_name));
        } else {
            self.status_message = Some(format!("Group not found: {}", group_name));
        }
    }
}

pub(super) fn find_group_node<'a>(node: &'a GroupNode, name: &str) -> Option<&'a GroupNode> {
    if node.group.name == name {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_group_node(child, name) {
            return Some(found);
        }
    }
    None
}

pub(super) fn find_group_node_mut<'a>(
    node: &'a mut GroupNode,
    path: &[usize],
) -> Option<&'a mut GroupNode> {
    if path.is_empty() {
        return Some(node);
    }
    let idx = path[0];
    node.children
        .get_mut(idx)
        .and_then(|child| find_group_node_mut(child, &path[1..]))
}

pub(super) fn collect_group_names(node: &GroupNode, names: &mut Vec<String>) {
    if !matches!(node.group.group_type, GroupType::AllEntries) {
        names.push(node.group.name.clone());
    }
    for child in &node.children {
        collect_group_names(child, names);
    }
}
