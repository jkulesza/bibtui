use super::model::*;

/// Serialize a GroupTree back to JabRef grouping text format.
/// The returned string is the content between `grouping:\n` and the closing `}`.
pub fn serialize_group_tree(tree: &GroupTree) -> String {
    let mut lines = Vec::new();
    serialize_node(&tree.root, 0, &mut lines);
    lines.join("\n")
}

fn serialize_node(node: &GroupNode, depth: usize, lines: &mut Vec<String>) {
    // Groups parsed from a JabRef file keep their original split fields
    // (including tail fields like color, icon, and description that we do
    // not model). Echo those back verbatim, updating only the slots that
    // bibtui can actually mutate.
    let line = if let Some((type_str, orig_fields)) = &node.original_fields {
        let mut fields = orig_fields.clone();
        let expanded = if node.expanded { "1" } else { "0" };
        match (&node.group.group_type, type_str.as_str()) {
            (GroupType::Static, "StaticGroup") => {
                set_group_field(&mut fields, 0, node.group.name.clone());
                set_group_field(&mut fields, 2, expanded.to_string());
            }
            (
                GroupType::Keyword { field, search_term, case_sensitive, regex },
                "KeywordGroup",
            ) => {
                set_group_field(&mut fields, 0, node.group.name.clone());
                set_group_field(&mut fields, 2, field.clone());
                set_group_field(&mut fields, 3, search_term.clone());
                set_group_field(
                    &mut fields,
                    4,
                    if *case_sensitive { "1" } else { "0" }.to_string(),
                );
                set_group_field(&mut fields, 5, if *regex { "1" } else { "0" }.to_string());
                set_group_field(&mut fields, 6, expanded.to_string());
            }
            // AllEntries and unrecognized group types: write back unchanged.
            _ => {}
        }
        format!("{} {}:{};", depth, type_str, fields.join("\\;"))
    } else {
        match &node.group.group_type {
            GroupType::AllEntries => format!("{} AllEntriesGroup:;", depth),
            GroupType::Static => {
                let expanded = if node.expanded { "1" } else { "0" };
                format!(
                    "{} StaticGroup:{}\\;2\\;{}\\;\\;\\;\\;;",
                    depth, node.group.name, expanded
                )
            }
            GroupType::Keyword {
                field,
                search_term,
                case_sensitive,
                regex,
            } => {
                let cs = if *case_sensitive { "1" } else { "0" };
                let rx = if *regex { "1" } else { "0" };
                let expanded = if node.expanded { "1" } else { "0" };
                format!(
                    "{} KeywordGroup:{}\\;0\\;{}\\;{}\\;{}\\;{}\\;{}\\;\\;\\;\\;;",
                    depth, node.group.name, field, search_term, cs, rx, expanded
                )
            }
        }
    };
    lines.push(line);
    for child in &node.children {
        serialize_node(child, depth + 1, lines);
    }
}

/// Set `fields[idx] = value`, growing the vector with empty fields if needed.
fn set_group_field(fields: &mut Vec<String>, idx: usize, value: String) {
    if fields.len() <= idx {
        fields.resize(idx + 1, String::new());
    }
    fields[idx] = value;
}

/// Parse a JabRef @Comment block and extract metadata into JabRefMeta.
pub fn parse_jabref_comment(raw_text: &str, meta: &mut JabRefMeta) {
    // JabRef metadata comments look like: @Comment{jabref-meta: key:value;}
    // Strip the @Comment{ prefix and } suffix
    let trimmed = raw_text.trim();
    let inner = if trimmed.starts_with("@Comment{") || trimmed.starts_with("@comment{") {
        let start = "@Comment{".len();
        if trimmed.ends_with('}') {
            &trimmed[start..trimmed.len() - 1]
        } else {
            return;
        }
    } else {
        return;
    };

    let inner = inner.trim();

    if !inner.starts_with("jabref-meta:") {
        return;
    }

    let meta_content = &inner["jabref-meta:".len()..].trim_start();

    // Split on first ':' to get key and value
    if let Some(colon_pos) = meta_content.find(':') {
        let key = meta_content[..colon_pos].trim();
        let value = meta_content[colon_pos + 1..].trim();
        // Remove trailing semicolon if present
        let value = value.strip_suffix(';').unwrap_or(value).trim();

        match key {
            "databaseType" => meta.database_type = Some(value.to_string()),
            "fileDirectory" => meta.file_directory = Some(value.to_string()),
            "protectedFlag" => meta.protected_flag = Some(value.to_string()),
            "grouping" => {
                // Multi-line group definitions — store the full raw content
                meta.unknown_meta
                    .insert("grouping".to_string(), value.to_string());
            }
            "groupsversion" => {
                meta.groups_version = Some(value.to_string());
            }
            "saveActions" => {
                meta.save_actions = Some(value.to_string());
            }
            "saveOrderConfig" => {
                meta.save_order_config = Some(value.to_string());
            }
            "keypatterndefault" => {
                meta.key_pattern_default = Some(value.to_string());
            }
            _ if key.starts_with("keypattern_") => {
                let type_name = key["keypattern_".len()..].to_lowercase();
                meta.key_patterns.insert(type_name, value.to_string());
            }
            _ => {
                meta.unknown_meta
                    .insert(key.to_string(), value.to_string());
            }
        }
    }
}

/// Build a GroupTree from parsed JabRef metadata.
pub fn build_group_tree(meta: &JabRefMeta) -> GroupTree {
    let mut tree = GroupTree::default();

    let grouping_text = match meta.unknown_meta.get("grouping") {
        Some(text) => text,
        None => return tree,
    };

    let mut stack: Vec<(usize, GroupNode)> = Vec::new();

    for line in grouping_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Lines look like: "0 AllEntriesGroup:;"
        // or: "1 StaticGroup:Name\;ctx\;expanded\;\;\;\;;"
        // or: "2 KeywordGroup:Name\;ctx\;field\;keyword\;cs\;rx\;expanded\;\;\;\;;"
        let (depth, rest) = match line.find(' ') {
            Some(pos) => {
                let depth: usize = line[..pos].parse().unwrap_or(0);
                (depth, &line[pos + 1..])
            }
            None => continue,
        };

        let node = parse_group_line(rest);

        // Insert node at the correct depth
        // Pop items from stack that are at same or deeper level
        while let Some((d, _)) = stack.last() {
            if *d >= depth {
                let (_, child) = stack.pop().unwrap();
                if let Some((_, parent)) = stack.last_mut() {
                    parent.children.push(child);
                } else {
                    // This was a top-level child of root
                    tree.root.children.push(child);
                }
            } else {
                break;
            }
        }

        if depth == 0 {
            // Replace root
            tree.root.group = node.group;
            tree.root.expanded = node.expanded;
        } else {
            stack.push((depth, node));
        }
    }

    // Flush remaining stack
    while let Some((_, child)) = stack.pop() {
        if let Some((_, parent)) = stack.last_mut() {
            parent.children.push(child);
        } else {
            tree.root.children.push(child);
        }
    }

    tree
}

fn parse_group_line(line: &str) -> GroupNode {
    // Split on first ':'
    let (type_str, rest) = match line.find(':') {
        Some(pos) => (&line[..pos], &line[pos + 1..]),
        None => {
            return GroupNode {
                group: Group {
                    name: line.to_string(),
                    group_type: GroupType::Static,
                },
                children: Vec::new(),
                expanded: true,
                original_fields: None,
            };
        }
    };

    // JabRef uses \; as a field separator within group definitions
    // First unescape: replace \; with a placeholder, split on real ;, then restore
    let fields: Vec<String> = split_jabref_fields(rest);
    // Keep the verbatim type token and split fields so serialization can
    // echo back tail fields (color, icon, description, ...) unchanged.
    let original_fields = Some((type_str.to_string(), fields.clone()));

    match type_str {
        "AllEntriesGroup" => GroupNode {
            group: Group {
                name: "All Entries".to_string(),
                group_type: GroupType::AllEntries,
            },
            children: Vec::new(),
            expanded: true,
            original_fields,
        },
        "StaticGroup" => {
            let name = fields.first().cloned().unwrap_or_default();
            let expanded = fields
                .get(2)
                .and_then(|s| s.parse::<u8>().ok())
                .map(|v| v == 1)
                .unwrap_or(true);
            GroupNode {
                group: Group {
                    name,
                    group_type: GroupType::Static,
                },
                children: Vec::new(),
                expanded,
                original_fields,
            }
        }
        "KeywordGroup" => {
            let name = fields.first().cloned().unwrap_or_default();
            let field = fields.get(2).cloned().unwrap_or_default();
            let search_term = fields.get(3).cloned().unwrap_or_default();
            let case_sensitive = fields
                .get(4)
                .and_then(|s| s.parse::<u8>().ok())
                .map(|v| v == 1)
                .unwrap_or(false);
            let regex = fields
                .get(5)
                .and_then(|s| s.parse::<u8>().ok())
                .map(|v| v == 1)
                .unwrap_or(false);
            let expanded = fields
                .get(6)
                .and_then(|s| s.parse::<u8>().ok())
                .map(|v| v == 1)
                .unwrap_or(true);
            GroupNode {
                group: Group {
                    name,
                    group_type: GroupType::Keyword {
                        field,
                        search_term,
                        case_sensitive,
                        regex,
                    },
                },
                children: Vec::new(),
                expanded,
                original_fields,
            }
        }
        _ => GroupNode {
            group: Group {
                name: fields.first().cloned().unwrap_or_default(),
                group_type: GroupType::Static,
            },
            children: Vec::new(),
            expanded: true,
            original_fields,
        },
    }
}

/// Split JabRef group field format: fields separated by \; (escaped semicolons)
/// The line ends with a bare ; which is the record terminator.
fn split_jabref_fields(s: &str) -> Vec<String> {
    let s = s.strip_suffix(';').unwrap_or(s);

    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if chars.peek() == Some(&';') {
                // This is a field separator \;
                chars.next();
                fields.push(current.clone());
                current.clear();
            } else {
                current.push(ch);
            }
        } else {
            current.push(ch);
        }
    }

    // Push last field
    if !current.is_empty() || !fields.is_empty() {
        fields.push(current);
    }

    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_jabref_fields() {
        let input = r"Markings\;2\;1\;\;\;\;;";
        let fields = split_jabref_fields(input);
        assert_eq!(fields[0], "Markings");
        assert_eq!(fields[1], "2");
        assert_eq!(fields[2], "1");
    }

    #[test]
    fn test_parse_keyword_group() {
        let line = r"KeywordGroup:Nuclear\;0\;keywords\;Nuclear\;0\;0\;1\;\;\;\;;";
        let (_type_str, rest) = line.split_once(':').unwrap();
        let fields = split_jabref_fields(rest);
        assert_eq!(fields[0], "Nuclear");
        assert_eq!(fields[2], "keywords");
        assert_eq!(fields[3], "Nuclear");
    }

    // ── parse_jabref_comment ──────────────────────────────────────────────────

    #[test]
    fn test_parse_jabref_comment_database_type() {
        let raw = "@Comment{jabref-meta: databaseType:bibtex;}";
        let mut meta = JabRefMeta::default();
        parse_jabref_comment(raw, &mut meta);
        assert_eq!(meta.database_type.as_deref(), Some("bibtex"));
    }

    #[test]
    fn test_parse_jabref_comment_file_directory() {
        let raw = "@Comment{jabref-meta: fileDirectory:/home/user/papers;}";
        let mut meta = JabRefMeta::default();
        parse_jabref_comment(raw, &mut meta);
        assert_eq!(meta.file_directory.as_deref(), Some("/home/user/papers"));
    }

    #[test]
    fn test_parse_jabref_comment_non_jabref_is_ignored() {
        let raw = "@Comment{This is just a regular comment}";
        let mut meta = JabRefMeta::default();
        parse_jabref_comment(raw, &mut meta);
        assert!(meta.database_type.is_none());
    }

    #[test]
    fn test_parse_jabref_comment_unknown_key_stored() {
        let raw = "@Comment{jabref-meta: someUnknownKey:someValue;}";
        let mut meta = JabRefMeta::default();
        parse_jabref_comment(raw, &mut meta);
        assert!(meta.unknown_meta.contains_key("someUnknownKey")
            || meta.unknown_meta.contains_key("someunknownkey"));
    }

    // ── serialize_group_tree ──────────────────────────────────────────────────

    #[test]
    fn test_serialize_all_entries_group() {
        let tree = GroupTree {
            root: GroupNode {
                group: Group { name: "All Entries".to_string(), group_type: GroupType::AllEntries },
                children: vec![],
                expanded: true,
                original_fields: None,
            },
        };
        let out = serialize_group_tree(&tree);
        assert!(out.contains("AllEntriesGroup:"), "got: {}", out);
    }

    #[test]
    fn test_serialize_static_group() {
        let tree = GroupTree {
            root: GroupNode {
                group: Group { name: "All Entries".to_string(), group_type: GroupType::AllEntries },
                children: vec![GroupNode {
                    group: Group { name: "MyGroup".to_string(), group_type: GroupType::Static },
                    children: vec![],
                    expanded: false,
                    original_fields: None,
                }],
                expanded: true,
                original_fields: None,
            },
        };
        let out = serialize_group_tree(&tree);
        assert!(out.contains("StaticGroup:MyGroup"), "got: {}", out);
    }

    #[test]
    fn test_serialize_keyword_group() {
        let tree = GroupTree {
            root: GroupNode {
                group: Group { name: "All Entries".to_string(), group_type: GroupType::AllEntries },
                children: vec![GroupNode {
                    group: Group {
                        name: "Nuclear".to_string(),
                        group_type: GroupType::Keyword {
                            field: "keywords".to_string(),
                            search_term: "Nuclear".to_string(),
                            case_sensitive: false,
                            regex: false,
                        },
                    },
                    children: vec![],
                    expanded: true,
                    original_fields: None,
                }],
                expanded: true,
                original_fields: None,
            },
        };
        let out = serialize_group_tree(&tree);
        assert!(out.contains("KeywordGroup:Nuclear"), "got: {}", out);
        assert!(out.contains("keywords"), "got: {}", out);
    }

    #[test]
    fn test_parse_jabref_comment_lowercase() {
        // @comment{ (lowercase) must be handled the same as @Comment{
        let raw = "@comment{jabref-meta: databaseType:bibtex;}";
        let mut meta = JabRefMeta::default();
        parse_jabref_comment(raw, &mut meta);
        assert_eq!(meta.database_type.as_deref(), Some("bibtex"));
    }

    #[test]
    fn test_parse_jabref_comment_unclosed() {
        // Missing closing } — should return early without panicking
        let raw = "@Comment{jabref-meta: databaseType:bibtex;";
        let mut meta = JabRefMeta::default();
        parse_jabref_comment(raw, &mut meta);
        assert!(meta.database_type.is_none());
    }

    #[test]
    fn test_parse_jabref_comment_non_comment_prefix() {
        // Input that doesn't start with @Comment{ or @comment{ at all
        let raw = "This is not a comment block";
        let mut meta = JabRefMeta::default();
        parse_jabref_comment(raw, &mut meta);
        assert!(meta.database_type.is_none());
    }

    #[test]
    fn test_parse_jabref_comment_meta_keys() {
        for (key, value) in &[
            ("protectedFlag", "true"),
            ("groupsversion", "3"),
            ("saveActions", "enabled;"),
            ("saveOrderConfig", "specified;"),
        ] {
            let raw = format!("@Comment{{jabref-meta: {}:{};}}",  key, value);
            let mut meta = JabRefMeta::default();
            parse_jabref_comment(&raw, &mut meta);
            match *key {
                "protectedFlag"     => assert!(meta.protected_flag.is_some()),
                "groupsversion"     => assert!(meta.groups_version.is_some()),
                "saveActions"       => assert!(meta.save_actions.is_some()),
                "saveOrderConfig"   => assert!(meta.save_order_config.is_some()),
                _ => {}
            }
        }
    }

    #[test]
    fn test_parse_jabref_comment_keypatterndefault() {
        let raw = "@Comment{jabref-meta: keypatterndefault:[auth][year];}";
        let mut meta = JabRefMeta::default();
        parse_jabref_comment(raw, &mut meta);
        assert_eq!(meta.key_pattern_default.as_deref(), Some("[auth][year]"));
    }

    #[test]
    fn test_parse_jabref_comment_keypattern_per_type() {
        let raw = "@Comment{jabref-meta: keypattern_article:[auth:lower][shortyear];}";
        let mut meta = JabRefMeta::default();
        parse_jabref_comment(raw, &mut meta);
        assert_eq!(
            meta.key_patterns.get("article").map(|s| s.as_str()),
            Some("[auth:lower][shortyear]")
        );
    }

    #[test]
    fn test_parse_jabref_comment_keypattern_case_insensitive() {
        let raw = "@Comment{jabref-meta: keypattern_InProceedings:[auth][year];}";
        let mut meta = JabRefMeta::default();
        parse_jabref_comment(raw, &mut meta);
        assert!(meta.key_patterns.contains_key("inproceedings"));
    }

    #[test]
    fn test_serialize_keyword_group_all_flags() {
        // Covers the case_sensitive=true, regex=true, expanded=false branches
        let tree = GroupTree {
            root: GroupNode {
                group: Group { name: "All Entries".to_string(), group_type: GroupType::AllEntries },
                children: vec![GroupNode {
                    group: Group {
                        name: "Fission".to_string(),
                        group_type: GroupType::Keyword {
                            field: "keywords".to_string(),
                            search_term: "fission".to_string(),
                            case_sensitive: true,
                            regex: true,
                        },
                    },
                    children: vec![],
                    expanded: false,
                    original_fields: None,
                }],
                expanded: true,
                original_fields: None,
            },
        };
        let out = serialize_group_tree(&tree);
        assert!(out.contains("KeywordGroup:Fission"), "got: {}", out);
        // case_sensitive=1, regex=1, expanded=0
        assert!(out.contains("\\;1\\;1\\;0\\;"), "got: {}", out);
    }

    // ── original-line fidelity ────────────────────────────────────────────────

    #[test]
    fn test_static_group_tail_fields_roundtrip_byte_identical() {
        // JabRef 5 StaticGroup line with color/icon/description tail fields.
        let line = r"1 StaticGroup:Physics\;2\;1\;0x8a8a8aff\;icon-atom\;My description\;;";
        let grouping = format!("0 AllEntriesGroup:;\n{}", line);
        let mut meta = JabRefMeta::default();
        meta.unknown_meta.insert("grouping".to_string(), grouping.clone());
        let tree = build_group_tree(&meta);
        let serialized = serialize_group_tree(&tree);
        assert_eq!(serialized, grouping, "unchanged tree must round-trip byte-identically");
    }

    #[test]
    fn test_static_group_expanded_toggle_changes_only_that_slot() {
        let line = r"1 StaticGroup:Physics\;2\;1\;0x8a8a8aff\;icon-atom\;My description\;;";
        let grouping = format!("0 AllEntriesGroup:;\n{}", line);
        let mut meta = JabRefMeta::default();
        meta.unknown_meta.insert("grouping".to_string(), grouping);
        let mut tree = build_group_tree(&meta);
        tree.root.children[0].expanded = false;
        let serialized = serialize_group_tree(&tree);
        let expected = format!(
            "0 AllEntriesGroup:;\n{}",
            r"1 StaticGroup:Physics\;2\;0\;0x8a8a8aff\;icon-atom\;My description\;;"
        );
        assert_eq!(serialized, expected, "only the expanded slot may change");
    }

    #[test]
    fn test_keyword_group_tail_fields_roundtrip_byte_identical() {
        let line =
            r"1 KeywordGroup:Nuclear\;0\;keywords\;fission\;0\;0\;1\;0x00ff00ff\;icon-x\;Desc\;;";
        let grouping = format!("0 AllEntriesGroup:;\n{}", line);
        let mut meta = JabRefMeta::default();
        meta.unknown_meta.insert("grouping".to_string(), grouping.clone());
        let tree = build_group_tree(&meta);
        let serialized = serialize_group_tree(&tree);
        assert_eq!(serialized, grouping);
    }

    #[test]
    fn test_unknown_group_type_roundtrips_verbatim() {
        // A group type bibtui does not model must be echoed back unchanged.
        let line = r"1 SearchGroup:Recent\;0\;year=2024\;0\;0\;1\;\;\;\;;";
        let grouping = format!("0 AllEntriesGroup:;\n{}", line);
        let mut meta = JabRefMeta::default();
        meta.unknown_meta.insert("grouping".to_string(), grouping.clone());
        let tree = build_group_tree(&meta);
        let serialized = serialize_group_tree(&tree);
        assert_eq!(serialized, grouping);
    }

    #[test]
    fn test_programmatic_group_uses_default_short_form() {
        // Groups created in bibtui (no original fields) keep the short form.
        let tree = GroupTree {
            root: GroupNode {
                group: Group { name: "All Entries".to_string(), group_type: GroupType::AllEntries },
                children: vec![GroupNode {
                    group: Group { name: "New".to_string(), group_type: GroupType::Static },
                    children: vec![],
                    expanded: true,
                    original_fields: None,
                }],
                expanded: true,
                original_fields: None,
            },
        };
        let out = serialize_group_tree(&tree);
        assert_eq!(out, "0 AllEntriesGroup:;\n1 StaticGroup:New\\;2\\;1\\;\\;\\;\\;;");
    }

    // ── build_group_tree ──────────────────────────────────────────────────────

    #[test]
    fn test_build_group_tree_empty_meta() {
        let meta = JabRefMeta::default();
        let tree = build_group_tree(&meta);
        // Should always produce at least a root AllEntriesGroup
        assert_eq!(tree.root.group.group_type, GroupType::AllEntries);
    }

    #[test]
    fn test_build_group_tree_roundtrip() {
        // Parse grouping text → build tree → serialize → compare
        let grouping = "0 AllEntriesGroup:;\n1 StaticGroup:Physics\\;2\\;1\\;\\;\\;\\;;";
        let mut meta = JabRefMeta::default();
        meta.unknown_meta.insert("grouping".to_string(), grouping.to_string());
        let tree = build_group_tree(&meta);
        let serialized = serialize_group_tree(&tree);
        assert!(serialized.contains("AllEntriesGroup:"));
        assert!(serialized.contains("StaticGroup:Physics"));
    }

    #[test]
    fn test_build_group_tree_line_without_space() {
        // A line with no space (no depth prefix) should be skipped
        let grouping = "0 AllEntriesGroup:;\nNOSPACELINE\n1 StaticGroup:X\\;2\\;1\\;\\;\\;\\;;";
        let mut meta = JabRefMeta::default();
        meta.unknown_meta.insert("grouping".to_string(), grouping.to_string());
        let tree = build_group_tree(&meta);
        // The "NOSPACELINE" entry is skipped; the static group is still parsed
        let names: Vec<_> = tree.root.children.iter().map(|c| c.group.name.as_str()).collect();
        assert!(names.contains(&"X"), "got: {:?}", names);
    }

    #[test]
    fn test_build_group_tree_non_numeric_depth() {
        // Non-numeric depth should default to 0 (child of root)
        let grouping = "abc AllEntriesGroup:;\n0 StaticGroup:Y\\;2\\;1\\;\\;\\;\\;;";
        let mut meta = JabRefMeta::default();
        meta.unknown_meta.insert("grouping".to_string(), grouping.to_string());
        // Should not panic
        let _tree = build_group_tree(&meta);
    }

    #[test]
    fn test_parse_group_line_no_colon() {
        // Line without ':' — should produce a Static group with the whole line as name
        let node = parse_group_line("JustAName");
        assert_eq!(node.group.name, "JustAName");
        assert_eq!(node.group.group_type, GroupType::Static);
    }

    #[test]
    fn test_parse_group_line_unknown_type() {
        // Unknown type string → falls back to Static with first field as name
        let node = parse_group_line("UnknownGroupType:MyGroup\\;2\\;1\\;\\;\\;\\;;");
        assert_eq!(node.group.name, "MyGroup");
        assert_eq!(node.group.group_type, GroupType::Static);
    }
}
