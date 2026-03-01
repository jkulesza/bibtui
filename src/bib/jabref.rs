use super::model::*;

/// Serialize a GroupTree back to JabRef grouping text format.
/// The returned string is the content between `grouping:\n` and the closing `}`.
pub fn serialize_group_tree(tree: &GroupTree) -> String {
    let mut lines = Vec::new();
    serialize_node(&tree.root, 0, &mut lines);
    lines.join("\n")
}

fn serialize_node(node: &GroupNode, depth: usize, lines: &mut Vec<String>) {
    let line = match &node.group.group_type {
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
    };
    lines.push(line);
    for child in &node.children {
        serialize_node(child, depth + 1, lines);
    }
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
            };
        }
    };

    // JabRef uses \; as a field separator within group definitions
    // First unescape: replace \; with a placeholder, split on real ;, then restore
    let fields: Vec<String> = split_jabref_fields(rest);

    match type_str {
        "AllEntriesGroup" => GroupNode {
            group: Group {
                name: "All Entries".to_string(),
                group_type: GroupType::AllEntries,
            },
            children: Vec::new(),
            expanded: true,
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
            }
        }
        _ => GroupNode {
            group: Group {
                name: fields.first().cloned().unwrap_or_default(),
                group_type: GroupType::Static,
            },
            children: Vec::new(),
            expanded: true,
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
}
