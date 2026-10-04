//! Group management: CRUD on the JabRef group tree and raw-file sync.

use super::*;

impl App {
    pub(super) fn select_group(&mut self) {
        if let Some(item) = self.group_tree_state.selected_item() {
            let path = item.path.clone();
            if self.group_tree_state.active_path.as_ref() == Some(&path) {
                self.group_tree_state.active_path = None;
                self.group_tree_state.active_group = None;
            } else {
                self.group_tree_state.active_group = Some(item.name.clone());
                self.group_tree_state.active_path = Some(path);
            }
            self.update_search();
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

    /// Prompt for a new name for the selected group, pre-filled with the
    /// current one (`e` in the group pane).
    pub(super) fn start_rename_group(&mut self) {
        let Some(item) = self.group_tree_state.selected_item().cloned() else {
            return;
        };
        if item.depth == 0 {
            self.status_message = Some("The All Entries group cannot be renamed".into());
            return;
        }
        self.field_editor_state = Some(FieldEditorState::new("Group name", &item.name));
        self.pending_action = Some(PendingAction::RenameGroup { path: item.path });
        self.mode = InputMode::Editing;
    }

    /// Rename the group at `path`. A static group's members carry its name in
    /// their `groups` field (that is how JabRef stores membership), so every
    /// member is updated in the same undoable step.
    pub(super) fn finish_rename_group(&mut self, path: &[usize], new_name: String) {
        let Some(node) = find_group_node_by_path(&self.database.groups.root, path) else {
            return;
        };
        let old_name = node.group.name.clone();
        let is_static = matches!(node.group.group_type, GroupType::Static);
        if path.is_empty() || matches!(node.group.group_type, GroupType::AllEntries) {
            self.status_message = Some("The All Entries group cannot be renamed".into());
            return;
        }
        if new_name == old_name {
            return;
        }
        let refuse = |reason: String| Some(format!("Group not renamed: {reason}"));
        let mut names = Vec::new();
        collect_group_names(&self.database.groups.root, &mut names);
        let problem = if new_name.is_empty() {
            refuse("the name cannot be empty".into())
        } else if let Some(problem) = invalid_group_name(&new_name) {
            refuse(problem)
        } else if names.contains(&new_name) {
            refuse(format!("another group is already named '{new_name}'"))
        } else if is_static
            && count_static_groups_named(&self.database.groups.root, &old_name) > 1
            && self
                .database
                .entries
                .values()
                .any(|e| e.group_memberships.contains(&old_name))
        {
            // Entries name their static groups, so with two static groups
            // called `old_name` there is no telling which one they belong to.
            refuse(format!(
                "another static group is also named '{old_name}', so its entries can't be told apart; delete one of them with dd (keeping memberships) or rename it in JabRef first"
            ))
        } else {
            None
        };
        if let Some(message) = problem {
            self.status_message = Some(message);
            return;
        }

        let mut undo = vec![UndoItem::GroupTreeChanged {
            old_tree: self.database.groups.clone(),
            active_path: self.group_tree_state.active_path.clone(),
        }];
        if let Some(node) = find_group_node_mut(&mut self.database.groups.root, path) {
            node.group.name = new_name.clone();
        }
        let mut updated = 0;
        if is_static {
            for (key, entry) in self.database.entries.iter_mut() {
                if !entry.group_memberships.contains(&old_name) {
                    continue;
                }
                undo.push(UndoItem::GroupMembershipChanged {
                    entry_key: key.clone(),
                    old_memberships: entry.group_memberships.clone(),
                    old_groups_field: entry.fields.get("groups").cloned(),
                });
                for membership in &mut entry.group_memberships {
                    if *membership == old_name {
                        *membership = new_name.clone();
                    }
                }
                if let Some(field) = entry.fields.get_mut("groups") {
                    *field = rename_in_groups_field(field, &old_name, &new_name);
                }
                entry.dirty = true;
                updated += 1;
            }
        }
        self.sync_groups_to_raw();
        self.group_tree_state.refresh(&self.database.groups);
        self.push_undo(UndoItem::Batch(undo));
        self.refresh_view();
        self.status_message = Some(if is_static {
            format!(
                "Renamed group '{}' to '{}' in {} entr{} (u to undo)",
                old_name,
                new_name,
                updated,
                if updated == 1 { "y" } else { "ies" }
            )
        } else {
            format!("Renamed group '{old_name}' to '{new_name}' (u to undo)")
        });
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
        self.push_undo(UndoItem::GroupTreeChanged {
            old_tree: self.database.groups.clone(),
            active_path: self.group_tree_state.active_path.clone(),
        });
        let new_node = GroupNode {
            group: Group {
                name: name.clone(),
                group_type: GroupType::Static,
            },
            children: Vec::new(),
            expanded: true,
            original_fields: None,
        };
        if let Some(parent) = find_group_node_mut(&mut self.database.groups.root, &parent_path) {
            parent.children.push(new_node);
        }
        self.sync_groups_to_raw();
        self.group_tree_state.refresh(&self.database.groups);
        self.refresh_view();
        self.status_message = Some(format!("Group '{}' added", name));
    }

    pub(super) fn finish_delete_group(&mut self, path: Vec<usize>) {
        if path.is_empty() {
            return;
        }
        self.push_undo(UndoItem::GroupTreeChanged {
            old_tree: self.database.groups.clone(),
            active_path: self.group_tree_state.active_path.clone(),
        });
        let (parent_path, child_idx) = path.split_at(path.len() - 1);
        let child_idx = child_idx[0];
        if let Some(parent) = find_group_node_mut(&mut self.database.groups.root, parent_path) {
            if child_idx < parent.children.len() {
                let removed = parent.children.remove(child_idx);
                self.sync_groups_to_raw();
                self.group_tree_state.refresh(&self.database.groups);
                if let Some(active) = &mut self.group_tree_state.active_path {
                    if active.starts_with(&path) {
                        self.group_tree_state.active_path = None;
                        self.group_tree_state.active_group = None;
                    } else if active.starts_with(parent_path)
                        && active.len() > parent_path.len()
                        && active[parent_path.len()] > child_idx
                    {
                        active[parent_path.len()] -= 1;
                    }
                }
                self.status_message = Some(format!("Group '{}' deleted", removed.group.name));
                // Deleting the group node does not touch the entries: any entry
                // whose `groups` field listed the group keeps the now-stale
                // name.  Offer to strip it.
                let name = removed.group.name;
                let affected = self
                    .database
                    .entries
                    .values()
                    .filter(|e| e.group_memberships.iter().any(|m| m == &name))
                    .count();
                if affected > 0 {
                    self.dialog_state = Some(DialogState::confirm(
                        "Remove Group Memberships",
                        &format!(
                            "Remove '{}' from {} entr{}?",
                            name,
                            affected,
                            if affected == 1 { "y" } else { "ies" }
                        ),
                    ));
                    self.pending_action =
                        Some(PendingAction::StripGroupMembership { group_name: name });
                    self.mode = InputMode::Dialog;
                }
            }
        }
        self.refresh_view();
    }

    /// Remove `group_name` from the `groups` field and memberships of every
    /// entry that lists it.  All changes are pushed as one undo batch.
    pub(super) fn strip_group_membership(&mut self, group_name: &str) {
        let mut undo_items: Vec<UndoItem> = Vec::new();
        for (key, entry) in self.database.entries.iter_mut() {
            if !entry.group_memberships.iter().any(|m| m == group_name) {
                continue;
            }
            undo_items.push(UndoItem::GroupMembershipChanged {
                entry_key: key.clone(),
                old_memberships: entry.group_memberships.clone(),
                old_groups_field: entry.fields.get("groups").cloned(),
            });
            entry.group_memberships.retain(|m| m != group_name);
            if entry.group_memberships.is_empty() {
                entry.fields.shift_remove("groups");
            } else {
                entry
                    .fields
                    .insert("groups".to_string(), entry.group_memberships.join(","));
            }
            entry.dirty = true;
        }
        let n = undo_items.len();
        if n > 0 {
            self.push_undo(UndoItem::Batch(undo_items));
        }
        self.refresh_view();
        self.status_message = Some(format!(
            "Removed '{}' from {} entr{}",
            group_name,
            n,
            if n == 1 { "y" } else { "ies" }
        ));
    }

    pub(super) fn finish_assign_groups(&mut self, entry_key: &str, selected_groups: Vec<String>) {
        // Snapshot before mutating (avoid holding a mutable borrow while calling push_undo)
        let undo_item =
            self.database
                .entries
                .get(entry_key)
                .map(|entry| UndoItem::GroupMembershipChanged {
                    entry_key: entry_key.to_string(),
                    old_memberships: entry.group_memberships.clone(),
                    old_groups_field: entry.fields.get("groups").cloned(),
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
        self.refresh_view();
    }

    pub(super) fn sync_groups_to_raw(&mut self) {
        let serialized = serialize_group_tree(&self.database.groups);
        // JabRef's layout: each group line ends with `;`, and the closing
        // brace is on its own line.
        let new_raw = format!("@Comment{{jabref-meta: grouping:\n{}\n}}", serialized);
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
        if let Some(path) = find_group_path(&self.database.groups.root, group_name) {
            self.group_tree_state.active_path = Some(path);
            self.group_tree_state.active_group = Some(group_name.to_string());
            self.update_search();
            self.status_message = Some(format!("Group: {}", group_name));
        } else {
            self.status_message = Some(format!("Group not found: {}", group_name));
        }
    }
}

fn find_group_path(node: &GroupNode, name: &str) -> Option<Vec<usize>> {
    if node.group.name == name {
        return Some(vec![]);
    }
    for (index, child) in node.children.iter().enumerate() {
        if let Some(mut path) = find_group_path(child, name) {
            path.insert(0, index);
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
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

/// Immutable path-based lookup: `path` is a list of child indices from the
/// root (an empty path is the root itself).
pub(super) fn find_group_node_by_path<'a>(
    node: &'a GroupNode,
    path: &[usize],
) -> Option<&'a GroupNode> {
    if path.is_empty() {
        return Some(node);
    }
    node.children
        .get(path[0])
        .and_then(|child| find_group_node_by_path(child, &path[1..]))
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

/// Why `name` cannot be used as a group name, if it cannot.
///
/// JabRef separates the names in an entry's `groups` field with commas, and
/// `;` and `\` are separators and escapes in its `grouping` metadata.
pub(super) fn invalid_group_name(name: &str) -> Option<String> {
    let bad: Vec<String> = [',', ';', '\\']
        .iter()
        .filter(|c| name.contains(**c))
        .map(|c| format!("'{c}'"))
        .collect();
    (!bad.is_empty()).then(|| {
        format!(
            "group names cannot contain {} (JabRef uses them as separators)",
            bad.join(" or ")
        )
    })
}

/// Replace `old` with `new` in a `groups` field, keeping the field's own
/// separators and spacing (e.g. `A, Old,B` → `A, New,B`).
pub(super) fn rename_in_groups_field(field: &str, old: &str, new: &str) -> String {
    field
        .split(',')
        .map(|part| {
            if part.trim() == old {
                let start = part.len() - part.trim_start().len();
                let end = part.trim_end().len();
                format!("{}{}{}", &part[..start], new, &part[end..])
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Number of static groups in the tree called `name`.
fn count_static_groups_named(node: &GroupNode, name: &str) -> usize {
    let own =
        usize::from(matches!(node.group.group_type, GroupType::Static) && node.group.name == name);
    own + node
        .children
        .iter()
        .map(|child| count_static_groups_named(child, name))
        .sum::<usize>()
}

pub(super) fn collect_group_names(node: &GroupNode, names: &mut Vec<String>) {
    if !matches!(node.group.group_type, GroupType::AllEntries) {
        names.push(node.group.name.clone());
    }
    for child in &node.children {
        collect_group_names(child, names);
    }
}
