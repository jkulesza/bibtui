use anyhow::{bail, Result};
use indexmap::IndexMap;

use super::model::*;

/// Parse a complete BibTeX file, preserving all formatting for round-trip fidelity.
pub fn parse_bib_file(input: &str) -> Result<RawBibFile> {
    let mut parser = Parser::new(input);
    parser.parse_file()
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser { input, pos: 0 }
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn advance(&mut self, n: usize) {
        self.pos += n;
    }

    fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// Consume characters while the predicate holds
    fn take_while<F: Fn(char) -> bool>(&mut self, pred: F) -> &'a str {
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if !pred(ch) {
                break;
            }
            self.advance(ch.len_utf8());
        }
        &self.input[start..self.pos]
    }

    /// Consume everything up to (but not including) the next '@' or end of input.
    /// This captures inter-entry text: whitespace, bare comments (%), semicolons, etc.
    fn take_preamble_text(&mut self) -> &'a str {
        let start = self.pos;
        while !self.at_end() {
            if self.peek() == Some('@') {
                break;
            }
            self.advance(self.peek().unwrap().len_utf8());
        }
        &self.input[start..self.pos]
    }

    fn parse_file(&mut self) -> Result<RawBibFile> {
        let mut items = Vec::new();

        while !self.at_end() {
            // Consume any text before the next '@'
            let preamble = self.take_preamble_text();
            if !preamble.is_empty() {
                items.push(RawItem::Preamble(preamble.to_string()));
            }

            if self.at_end() {
                break;
            }

            // We should be at '@'
            let item = self.parse_at_item()?;
            items.push(item);
        }

        Ok(RawBibFile { items })
    }

    fn parse_at_item(&mut self) -> Result<RawItem> {
        let entry_start = self.pos;

        // Consume '@'
        assert_eq!(self.peek(), Some('@'));
        self.advance(1);

        // Read type name
        let type_name = self.take_while(|c| c.is_alphanumeric() || c == '_' || c == '-');
        let type_name_str = type_name.to_string();

        match type_name_str.to_lowercase().as_str() {
            "comment" => self.parse_comment(entry_start),
            "preamble" => self.parse_bib_preamble(entry_start),
            "string" => self.parse_string_def(entry_start),
            _ => self.parse_entry(entry_start, type_name_str),
        }
    }

    fn parse_comment(&mut self, start: usize) -> Result<RawItem> {
        // @Comment may be followed by {braced content} or just text to end of line
        self.skip_whitespace();

        if self.peek() == Some('{') {
            self.advance(1);
            let _content = self.take_braced_content()?;
            Ok(RawItem::Comment {
                raw_text: self.input[start..self.pos].to_string(),
            })
        } else {
            // Bare comment — take to end of line
            let _text = self.take_while(|c| c != '\n');
            if self.peek() == Some('\n') {
                self.advance(1);
            }
            Ok(RawItem::Comment {
                raw_text: self.input[start..self.pos].to_string(),
            })
        }
    }

    fn parse_bib_preamble(&mut self, _start: usize) -> Result<RawItem> {
        self.skip_whitespace();
        if self.peek() == Some('{') {
            self.advance(1);
            let content = self.take_braced_content()?;
            Ok(RawItem::BibPreamble(content.to_string()))
        } else {
            bail!("Expected '{{' after @Preamble");
        }
    }

    fn parse_string_def(&mut self, _start: usize) -> Result<RawItem> {
        self.skip_whitespace();
        if self.peek() != Some('{') {
            bail!("Expected '{{' after @String");
        }
        self.advance(1);
        self.skip_whitespace();

        let name = self.take_while(|c| c.is_alphanumeric() || c == '_' || c == '-').to_string();
        self.skip_whitespace();

        if self.peek() == Some('=') {
            self.advance(1);
        }
        self.skip_whitespace();

        let raw_value = self.take_braced_content()?;
        Ok(RawItem::StringDef {
            name,
            raw_value: raw_value.to_string(),
        })
    }

    fn parse_entry(&mut self, start: usize, entry_type: String) -> Result<RawItem> {
        self.skip_whitespace();

        if self.peek() != Some('{') {
            bail!(
                "Expected '{{' after @{} at line {}",
                entry_type,
                self.line_at(start)
            );
        }
        self.advance(1);

        // Read citation key (everything up to first comma or '}')
        let citation_key = self
            .take_while(|c| c != ',' && c != '}')
            .trim()
            .to_string();

        // Consume comma after citation key (if present)
        if self.peek() == Some(',') {
            self.advance(1);
        }

        // Parse fields
        let mut fields = Vec::new();
        let mut trailing_comma = false;

        loop {
            // Capture indent whitespace
            let indent = self.take_while(|c| c == ' ' || c == '\t' || c == '\r' || c == '\n');
            let indent_str = indent.to_string();

            // Check for end of entry
            if self.peek() == Some('}') {
                self.advance(1);
                break;
            }

            if self.at_end() {
                bail!("Unexpected end of input in entry {} at line {}", citation_key, self.current_line());
            }

            // Read field name
            let field_name = self
                .take_while(|c| c.is_alphanumeric() || c == '_' || c == '-')
                .to_lowercase();

            if field_name.is_empty() {
                // Skip unexpected character
                if let Some(ch) = self.peek() {
                    self.advance(ch.len_utf8());
                }
                continue;
            }

            // Whitespace before '='
            let pre_eq = self.take_while(|c| c == ' ' || c == '\t').to_string();

            // Expect '='
            if self.peek() != Some('=') {
                bail!(
                    "Expected '=' after field name '{}' in entry {} at line {}",
                    field_name,
                    citation_key,
                    self.current_line()
                );
            }
            self.advance(1);

            // Whitespace after '='
            let post_eq = self.take_while(|c| c == ' ' || c == '\t').to_string();

            // Parse field value
            let value = self.parse_field_value()?;

            // Trailing: comma
            let mut trailing = String::new();
            if self.peek() == Some(',') {
                self.advance(1);
                trailing.push(',');
                trailing_comma = true;
            } else {
                trailing_comma = false;
            }

            fields.push(RawField {
                name: field_name,
                value,
                indent: indent_str,
                pre_eq,
                post_eq,
                trailing,
            });
        }

        let raw_text = self.input[start..self.pos].to_string();

        // Compute alignment width: max field name length
        let align_width = fields.iter().map(|f| f.name.len()).max().unwrap_or(0);

        Ok(RawItem::Entry(RawEntry {
            entry_type,
            citation_key,
            fields,
            align_width,
            trailing_comma,
            raw_text,
        }))
    }

    fn parse_field_value(&mut self) -> Result<RawFieldValue> {
        let first = self.parse_single_value()?;

        // Check for concatenation with '#'
        let mut parts = vec![first];
        loop {
            let saved = self.pos;
            self.skip_inline_whitespace();
            if self.peek() == Some('#') {
                self.advance(1);
                self.skip_inline_whitespace();
                parts.push(self.parse_single_value()?);
            } else {
                self.pos = saved;
                break;
            }
        }

        if parts.len() == 1 {
            Ok(parts.into_iter().next().unwrap())
        } else {
            Ok(RawFieldValue::Concat(parts))
        }
    }

    fn parse_single_value(&mut self) -> Result<RawFieldValue> {
        match self.peek() {
            Some('{') => {
                self.advance(1);
                let content = self.take_braced_content()?;
                Ok(RawFieldValue::Braced(content.to_string()))
            }
            Some('"') => {
                self.advance(1);
                let content = self.take_quoted_content()?;
                Ok(RawFieldValue::Quoted(content.to_string()))
            }
            Some(c) if c.is_alphanumeric() || c == '-' || c == '_' => {
                let bare = self
                    .take_while(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
                    .to_string();
                Ok(RawFieldValue::Bare(bare))
            }
            other => bail!("Unexpected character {:?} in field value at line {}", other, self.current_line()),
        }
    }

    /// Read content inside braces, handling nested braces. Consumes the closing '}'.
    fn take_braced_content(&mut self) -> Result<String> {
        let mut depth = 1;
        let start = self.pos;

        while !self.at_end() {
            match self.peek() {
                Some('{') => {
                    depth += 1;
                    self.advance(1);
                }
                Some('}') => {
                    depth -= 1;
                    if depth == 0 {
                        let content = self.input[start..self.pos].to_string();
                        self.advance(1); // consume '}'
                        return Ok(content);
                    }
                    self.advance(1);
                }
                Some('\\') => {
                    // Skip escaped character
                    self.advance(1);
                    if !self.at_end() {
                        self.advance(self.peek().unwrap().len_utf8());
                    }
                }
                Some(c) => {
                    self.advance(c.len_utf8());
                }
                None => break,
            }
        }

        bail!("Unterminated braced content starting at line {}", self.line_at(start));
    }

    /// Read content inside quotes, handling escaped quotes. Consumes the closing '"'.
    fn take_quoted_content(&mut self) -> Result<String> {
        let start = self.pos;

        while !self.at_end() {
            match self.peek() {
                Some('"') => {
                    let content = self.input[start..self.pos].to_string();
                    self.advance(1); // consume '"'
                    return Ok(content);
                }
                Some('\\') => {
                    self.advance(1);
                    if !self.at_end() {
                        self.advance(self.peek().unwrap().len_utf8());
                    }
                }
                Some(c) => {
                    self.advance(c.len_utf8());
                }
                None => break,
            }
        }

        bail!("Unterminated quoted string starting at line {}", self.line_at(start));
    }

    fn skip_whitespace(&mut self) {
        self.take_while(|c| c.is_whitespace());
    }

    fn skip_inline_whitespace(&mut self) {
        self.take_while(|c| c == ' ' || c == '\t');
    }

    /// Return the 1-based line number corresponding to a byte offset in the input.
    fn line_at(&self, byte_offset: usize) -> usize {
        self.input[..byte_offset.min(self.input.len())]
            .chars()
            .filter(|&c| c == '\n')
            .count()
            + 1
    }

    fn current_line(&self) -> usize {
        self.line_at(self.pos)
    }
}

/// Build a semantic Database from a parsed RawBibFile.
pub fn build_database(raw: RawBibFile) -> Database {
    let mut entries = IndexMap::new();
    let mut jabref_meta = JabRefMeta::default();

    for (idx, item) in raw.items.iter().enumerate() {
        match item {
            RawItem::Entry(raw_entry) => {
                let entry_type = EntryType::from_str(&raw_entry.entry_type);
                let mut fields = IndexMap::new();

                for field in &raw_entry.fields {
                    fields.insert(field.name.clone(), field.value.to_string_value());
                }

                let groups_field = fields.get("groups").cloned().unwrap_or_default();
                let group_memberships: Vec<String> = if groups_field.is_empty() {
                    Vec::new()
                } else {
                    groups_field
                        .split(',')
                        .map(|s: &str| s.trim().to_string())
                        .filter(|s: &String| !s.is_empty())
                        .collect()
                };

                let entry = Entry {
                    entry_type,
                    citation_key: raw_entry.citation_key.clone(),
                    fields,
                    group_memberships,
                    raw_index: idx,
                    dirty: false,
                };

                entries.insert(raw_entry.citation_key.clone(), entry);
            }
            RawItem::Comment { raw_text } => {
                // Parse JabRef metadata from @Comment blocks
                super::jabref::parse_jabref_comment(raw_text, &mut jabref_meta);
            }
            _ => {}
        }
    }

    let groups = super::jabref::build_group_tree(&jabref_meta);

    Database {
        entries,
        groups,
        jabref_meta,
        raw_file: raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_entry() {
        let input = r#"@Article{key2024,
  author = {Jane Doe},
  title  = {A Title},
  year   = {2024},
}
"#;
        let raw = parse_bib_file(input).unwrap();
        // Should have preamble (empty or not) and one entry
        let entries: Vec<_> = raw
            .items
            .iter()
            .filter(|i| matches!(i, RawItem::Entry(_)))
            .collect();
        assert_eq!(entries.len(), 1);
        if let RawItem::Entry(e) = &entries[0] {
            assert_eq!(e.citation_key, "key2024");
            assert_eq!(e.entry_type, "Article");
            assert_eq!(e.fields.len(), 3);
        }
    }

    #[test]
    fn test_parse_bare_month() {
        let input = "@Article{k,\n  month = apr,\n}\n";
        let raw = parse_bib_file(input).unwrap();
        if let RawItem::Entry(e) = &raw.items.last().unwrap() {
            assert_eq!(e.fields[0].name, "month");
            assert!(matches!(e.fields[0].value, RawFieldValue::Bare(ref s) if s == "apr"));
        }
    }

    #[test]
    fn test_roundtrip_simple() {
        let input = "@Article{key2024,\n  author = {Jane Doe},\n  title  = {A Title},\n}\n";
        let raw = parse_bib_file(input).unwrap();
        let output = super::super::writer::write_bib_file(&raw);
        assert_eq!(input, output);
    }
}
