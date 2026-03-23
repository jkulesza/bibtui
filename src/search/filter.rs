use crate::bib::model::{Entry, GroupNode, GroupType};
use regex::Regex;

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
            regex: use_regex,
        } => {
            // Pre-compile regex once for the whole filter pass.
            let compiled_re: Option<Regex> = if *use_regex {
                let pattern = if *case_sensitive {
                    search_term.clone()
                } else {
                    format!("(?i){}", search_term)
                };
                Regex::new(&pattern).ok()
            } else {
                None
            };

            entries
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    let value = e.fields.get(field.as_str());
                    if let Some(v) = value {
                        if let Some(re) = &compiled_re {
                            re.is_match(v)
                        } else if *case_sensitive {
                            v.contains(search_term.as_str())
                        } else {
                            v.to_lowercase().contains(&search_term.to_lowercase())
                        }
                    } else if field == "author" {
                        // Fallback: also check the author field when it is the
                        // target but was not found via the primary lookup path.
                        if let Some(author) = e.fields.get("author") {
                            if let Some(re) = &compiled_re {
                                re.is_match(author)
                            } else if *case_sensitive {
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

    #[test]
    fn test_keyword_group_case_sensitive_match() {
        let e1 = make_entry("A", &[], &[("keywords", "Reactor Physics")]);
        let entries = vec![&e1];
        let node = make_node("Reactor", GroupType::Keyword {
            field: "keywords".to_string(),
            search_term: "Reactor".to_string(),
            case_sensitive: true,
            regex: false,
        });
        assert_eq!(filter_by_group(&entries, &node), vec![0]);
    }

    /// The author field is checked when the group field is "author" but the
    /// primary lookup in `e.fields.get("author")` might follow a different
    /// branch.  Verify that entries whose author contains the search term match.
    #[test]
    fn test_keyword_group_author_field_match() {
        let e1 = make_entry("A", &[], &[("author", "Smith, John and Doe, Jane")]);
        let e2 = make_entry("B", &[], &[("author", "Brown, Robert")]);
        let entries = vec![&e1, &e2];
        let node = make_node("smith", GroupType::Keyword {
            field: "author".to_string(),
            search_term: "smith".to_string(),
            case_sensitive: false,
            regex: false,
        });
        assert_eq!(filter_by_group(&entries, &node), vec![0]);
    }

    #[test]
    fn test_keyword_group_author_field_no_author() {
        // Entry has no author field; group field is "author" → no match.
        let e1 = make_entry("A", &[], &[("title", "Some Paper")]);
        let entries = vec![&e1];
        let node = make_node("smith", GroupType::Keyword {
            field: "author".to_string(),
            search_term: "smith".to_string(),
            case_sensitive: false,
            regex: false,
        });
        assert_eq!(filter_by_group(&entries, &node), Vec::<usize>::new());
    }

    #[test]
    fn test_static_group_multiple_memberships() {
        let e1 = make_entry("A", &["Physics", "Nuclear"], &[]);
        let e2 = make_entry("B", &["Nuclear"], &[]);
        let entries = vec![&e1, &e2];
        let node = make_node("Nuclear", GroupType::Static);
        assert_eq!(filter_by_group(&entries, &node), vec![0, 1]);
    }

    #[test]
    fn test_all_entries_empty_input() {
        let node = make_node("All Entries", GroupType::AllEntries);
        let result = filter_by_group(&[], &node);
        assert!(result.is_empty());
    }

    #[test]
    fn test_keyword_group_regex_match() {
        let e1 = make_entry("A", &[], &[("keywords", "Reactor Physics")]);
        let e2 = make_entry("B", &[], &[("keywords", "fusion energy")]);
        let entries = vec![&e1, &e2];
        let node = make_node("reactor", GroupType::Keyword {
            field: "keywords".to_string(),
            search_term: "(?i)react".to_string(),
            case_sensitive: true, // case_sensitive is overridden by (?i) in pattern
            regex: true,
        });
        assert_eq!(filter_by_group(&entries, &node), vec![0]);
    }

    #[test]
    fn test_keyword_group_regex_case_insensitive_flag() {
        // When regex=true and case_sensitive=false, (?i) is prepended automatically.
        let e1 = make_entry("A", &[], &[("keywords", "FISSION")]);
        let e2 = make_entry("B", &[], &[("keywords", "fusion")]);
        let entries = vec![&e1, &e2];
        let node = make_node("fission", GroupType::Keyword {
            field: "keywords".to_string(),
            search_term: "fission".to_string(),
            case_sensitive: false,
            regex: true,
        });
        assert_eq!(filter_by_group(&entries, &node), vec![0]);
    }

    #[test]
    fn test_keyword_group_regex_invalid_pattern_no_panic() {
        // An invalid regex should not panic — the group simply matches nothing.
        let e1 = make_entry("A", &[], &[("keywords", "Nuclear")]);
        let entries = vec![&e1];
        let node = make_node("bad", GroupType::Keyword {
            field: "keywords".to_string(),
            search_term: "[invalid(regex".to_string(),
            case_sensitive: false,
            regex: true,
        });
        // Must not panic; result is empty because the regex failed to compile.
        let result = filter_by_group(&entries, &node);
        assert!(result.is_empty());
    }

    #[test]
    fn test_keyword_group_regex_author_field() {
        let e1 = make_entry("A", &[], &[("author", "Smith, John and Doe, Jane")]);
        let entries = vec![&e1];
        let node = make_node("smith", GroupType::Keyword {
            field: "author".to_string(),
            search_term: "Smith".to_string(),
            case_sensitive: true,
            regex: true,
        });
        assert_eq!(filter_by_group(&entries, &node), vec![0]);
    }
}
