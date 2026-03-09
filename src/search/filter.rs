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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::model::Group;
    use indexmap::IndexMap;

    fn make_entry(key: &str, groups: &[&str], fields: &[(&str, &str)]) -> Entry {
        let mut f = IndexMap::new();
        for (k, v) in fields {
            f.insert(k.to_string(), v.to_string());
        }
        Entry {
            entry_type: crate::bib::model::EntryType::Article,
            citation_key: key.to_string(),
            fields: f,
            group_memberships: groups.iter().map(|s| s.to_string()).collect(),
            raw_index: 0,
            dirty: false,
        }
    }

    fn make_node(name: &str, group_type: GroupType) -> GroupNode {
        GroupNode {
            group: Group { name: name.to_string(), group_type },
            children: vec![],
            expanded: true,
        }
    }

    #[test]
    fn test_all_entries_returns_all() {
        let e1 = make_entry("A", &[], &[]);
        let e2 = make_entry("B", &[], &[]);
        let entries = vec![&e1, &e2];
        let node = make_node("All Entries", GroupType::AllEntries);
        assert_eq!(filter_by_group(&entries, &node), vec![0, 1]);
    }

    #[test]
    fn test_static_group_matching() {
        let e1 = make_entry("A", &["Physics"], &[]);
        let e2 = make_entry("B", &["Chemistry"], &[]);
        let entries = vec![&e1, &e2];
        let node = make_node("Physics", GroupType::Static);
        assert_eq!(filter_by_group(&entries, &node), vec![0]);
    }

    #[test]
    fn test_static_group_no_match() {
        let e1 = make_entry("A", &["Chemistry"], &[]);
        let entries = vec![&e1];
        let node = make_node("Physics", GroupType::Static);
        assert_eq!(filter_by_group(&entries, &node), Vec::<usize>::new());
    }

    #[test]
    fn test_keyword_group_case_insensitive() {
        let e1 = make_entry("A", &[], &[("keywords", "Reactor Physics")]);
        let e2 = make_entry("B", &[], &[("keywords", "chemistry")]);
        let entries = vec![&e1, &e2];
        let node = make_node("reactor", GroupType::Keyword {
            field: "keywords".to_string(),
            search_term: "reactor".to_string(),
            case_sensitive: false,
            regex: false,
        });
        assert_eq!(filter_by_group(&entries, &node), vec![0]);
    }

    #[test]
    fn test_keyword_group_case_sensitive_no_match() {
        let e1 = make_entry("A", &[], &[("keywords", "Reactor Physics")]);
        let entries = vec![&e1];
        let node = make_node("reactor", GroupType::Keyword {
            field: "keywords".to_string(),
            search_term: "reactor".to_string(),
            case_sensitive: true,
            regex: false,
        });
        assert_eq!(filter_by_group(&entries, &node), Vec::<usize>::new());
    }

    #[test]
    fn test_keyword_group_missing_field() {
        let e1 = make_entry("A", &[], &[]);
        let entries = vec![&e1];
        let node = make_node("physics", GroupType::Keyword {
            field: "keywords".to_string(),
            search_term: "physics".to_string(),
            case_sensitive: false,
            regex: false,
        });
        assert_eq!(filter_by_group(&entries, &node), Vec::<usize>::new());
    }
}
