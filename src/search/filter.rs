use crate::bib::model::{Entry, GroupNode, GroupType};

/// Filter entries by group membership.
pub fn filter_by_group(entries: &[&Entry], group: &GroupNode) -> Vec<usize> {
    match &group.group.group_type {
        GroupType::AllEntries => (0..entries.len()).collect(),
        GroupType::Static => {
            entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.group_memberships.contains(&group.group.name))
                .map(|(i, _)| i)
                .collect()
        }
        GroupType::Keyword {
            field,
            search_term,
            case_sensitive,
            regex: _,
        } => {
            entries
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    if let Some(value) = e.fields.get(field.as_str()) {
                        if *case_sensitive {
                            value.contains(search_term.as_str())
                        } else {
                            value.to_lowercase().contains(&search_term.to_lowercase())
                        }
                    } else if field == "author" {
                        // Also check for keyword groups that match author field
                        if let Some(author) = e.fields.get("author") {
                            if *case_sensitive {
                                author.contains(search_term.as_str())
                            } else {
                                author.to_lowercase().contains(&search_term.to_lowercase())
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                })
                .map(|(i, _)| i)
                .collect()
        }
    }
}
