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
        let mut warnings = Vec::new();

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
            let item_start = self.pos;
            match self.parse_at_item() {
                Ok(item) => items.push(item),
                Err(e) => {
                    // Recover: skip the malformed item's bytes (preserving them
                    // as Preamble for byte-perfect round-trip) and continue at
                    // the next line that starts with '@'.
                    let line = self.line_at(item_start);
                    self.pos = item_start;
                    let skipped = self.skip_to_next_at_line();
                    items.push(RawItem::Preamble(skipped));
                    warnings.push(ParseWarning {
                        line,
                        message: e.to_string(),
                    });
                }
            }
        }

        Ok(RawBibFile { items, warnings })
    }

    /// Consume from the current position (assumed to be a malformed `@`-item)
    /// through the rest of its line and any following lines, stopping before
    /// the next line whose first non-blank character is '@' (so an indented
    /// entry after the malformed one still loads), or at end of input.
    /// Returns the consumed span so it can be preserved as `Preamble`.
    fn skip_to_next_at_line(&mut self) -> String {
        let start = self.pos;
        loop {
            // Consume the remainder of the current line.
            let _ = self.take_while(|c| c != '\n');
            if self.peek() == Some('\n') {
                self.advance(1);
            }
            let indent = self
                .remaining()
                .find(|c: char| c != ' ' && c != '\t')
                .unwrap_or(self.remaining().len());
            if self.at_end() || self.remaining()[indent..].starts_with('@') {
                break;
            }
        }
        self.input[start..self.pos].to_string()
    }

    /// Whether only spaces and tabs precede byte `pos` on its line.
    fn starts_line(&self, pos: usize) -> bool {
        self.input[..pos]
            .rsplit('\n')
            .next()
            .is_none_or(|before| before.chars().all(|c| c == ' ' || c == '\t'))
    }

    /// Consume an item's opening delimiter, `{` or `(`, and return the
    /// matching closing one.
    fn open_delimiter(&mut self) -> Option<char> {
        let close = match self.peek()? {
            '{' => '}',
            '(' => ')',
            _ => return None,
        };
        self.advance(1);
        Some(close)
    }

    fn parse_at_item(&mut self) -> Result<RawItem> {
        let entry_start = self.pos;

        // Consume '@'
        if self.peek() != Some('@') {
            anyhow::bail!("expected '@' at byte position {}", self.pos);
        }
        self.advance(1);

        // Read type name
        let type_name = self.take_while(|c| c.is_alphanumeric() || c == '_' || c == '-');
        let type_name_str = type_name.to_string();

        match type_name_str.to_lowercase().as_str() {
            "comment" => self.parse_comment(entry_start),
            "preamble" => self.parse_bib_preamble(entry_start),
            "string" => self.parse_string_def(entry_start),
            _ => {
                // A stray '@' in inter-entry text (e.g. an email address in a
                // `%` comment line) is not the start of an entry. Only treat it
                // as one when '{' or '(' follows the type name; otherwise pass
                // the rest of the line through as preamble text. An `@Type`
                // that starts a line looks like an entry, so say why it was not
                // read as one (its bytes are still preserved).
                let saved = self.pos;
                self.skip_whitespace();
                if matches!(self.peek(), Some('{' | '(')) {
                    self.pos = saved;
                    self.parse_entry(entry_start, type_name_str)
                } else if !type_name_str.is_empty() && self.starts_line(entry_start) {
                    bail!(
                        "@{} is not followed by '{{' or '(', so it was not read as an entry",
                        type_name_str
                    );
                } else {
                    self.pos = saved;
                    let _rest_of_line = self.take_while(|c| c != '\n');
                    if self.peek() == Some('\n') {
                        self.advance(1);
                    }
                    Ok(RawItem::Preamble(
                        self.input[entry_start..self.pos].to_string(),
                    ))
                }
            }
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

    fn parse_bib_preamble(&mut self, start: usize) -> Result<RawItem> {
        self.skip_whitespace();
        match self.open_delimiter() {
            Some('}') => {
                let content = self.take_braced_content()?;
                Ok(RawItem::BibPreamble {
                    content: content.to_string(),
                    raw_text: self.input[start..self.pos].to_string(),
                })
            }
            Some(close) => {
                // `@Preamble("...")`: the content is the value expression.
                self.skip_whitespace();
                let value_start = self.pos;
                self.parse_field_value()?;
                let content = self.input[value_start..self.pos].to_string();
                self.expect_close(close, "@Preamble")?;
                Ok(RawItem::BibPreamble {
                    content,
                    raw_text: self.input[start..self.pos].to_string(),
                })
            }
            None => bail!("Expected '{{' or '(' after @Preamble"),
        }
    }

    /// Skip whitespace and consume `close`, the end of `item`.
    fn expect_close(&mut self, close: char, item: &str) -> Result<()> {
        self.skip_whitespace();
        if self.peek() != Some(close) {
            bail!(
                "Expected '{}' to end {} at line {}",
                close,
                item,
                self.current_line()
            );
        }
        self.advance(1);
        Ok(())
    }

    fn parse_string_def(&mut self, start: usize) -> Result<RawItem> {
        self.skip_whitespace();
        let Some(close) = self.open_delimiter() else {
            bail!("Expected '{{' or '(' after @String");
        };
        self.skip_whitespace();

        let name = self
            .take_while(|c| c.is_alphanumeric() || c == '_' || c == '-')
            .to_string();
        self.skip_whitespace();

        if self.peek() != Some('=') {
            bail!(
                "Expected '=' in @String{{{}}} at line {}",
                name,
                self.current_line()
            );
        }
        self.advance(1);
        self.skip_whitespace();

        let value_start = self.pos;
        let value = self.parse_field_value()?;
        let raw_value = self.input[value_start..self.pos].to_string();
        self.expect_close(close, &format!("@String{{{}}}", name))?;
        Ok(RawItem::StringDef {
            name,
            raw_value,
            value,
            raw_text: self.input[start..self.pos].to_string(),
        })
    }

    fn parse_entry(&mut self, start: usize, entry_type: String) -> Result<RawItem> {
        self.skip_whitespace();

        // `@Type{...}` or `@Type(...)`, as BibTeX accepts both.
        let Some(close) = self.open_delimiter() else {
            bail!(
                "Expected '{{' or '(' after @{} at line {}",
                entry_type,
                self.line_at(start)
            );
        };

        // Read citation key (everything up to first comma or the closing delimiter)
        let citation_key = self
            .take_while(|c| c != ',' && c != close)
            .trim()
            .to_string();

        // Consume comma after citation key (if present)
        if self.peek() == Some(',') {
            self.advance(1);
        }

        // Parse fields. Whitespace and trailing commas are consumed but not
        // stored — formatting is preserved via raw_text passthrough.
        let mut fields = Vec::new();

        loop {
            // Skip indent whitespace
            self.take_while(|c| c == ' ' || c == '\t' || c == '\r' || c == '\n');

            // Check for end of entry
            if self.peek() == Some(close) {
                self.advance(1);
                break;
            }

            if self.at_end() {
                bail!(
                    "Unexpected end of input in entry {} at line {}",
                    citation_key,
                    self.current_line()
                );
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

            // Whitespace (including line breaks) before '='
            self.skip_whitespace();

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

            // Whitespace (including line breaks) after '='
            self.skip_whitespace();

            // Parse field value
            let value = self.parse_field_value()?;

            // Trailing comma, possibly on the next line
            let saved = self.pos;
            self.skip_whitespace();
            if self.peek() == Some(',') {
                self.advance(1);
            } else {
                self.pos = saved;
            }

            fields.push(RawField::new(field_name, value));
        }

        let raw_text = self.input[start..self.pos].to_string();

        Ok(RawItem::Entry(RawEntry {
            entry_type,
            citation_key,
            fields,
            raw_text,
        }))
    }

    fn parse_field_value(&mut self) -> Result<RawFieldValue> {
        let first = self.parse_single_value()?;

        // Check for concatenation with '#' (line breaks allowed around it)
        let mut parts = vec![first];
        loop {
            let saved = self.pos;
            self.skip_whitespace();
            if self.peek() == Some('#') {
                self.advance(1);
                self.skip_whitespace();
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
            other => bail!(
                "Unexpected character {:?} in field value at line {}",
                other,
                self.current_line()
            ),
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

        bail!(
            "Unterminated braced content starting at line {}",
            self.line_at(start)
        );
    }

    /// Read content inside quotes, handling escaped quotes. Consumes the
    /// closing '"'. As in BibTeX, a '"' inside braces (`"{"}quoted{"}"`) does
    /// not end the string.
    fn take_quoted_content(&mut self) -> Result<String> {
        let start = self.pos;
        let mut depth = 0usize;

        while !self.at_end() {
            match self.peek() {
                Some('{') => {
                    depth += 1;
                    self.advance(1);
                }
                Some('}') => {
                    depth = depth.saturating_sub(1);
                    self.advance(1);
                }
                Some('"') if depth == 0 => {
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

        bail!(
            "Unterminated quoted string starting at line {}",
            self.line_at(start)
        );
    }

    fn skip_whitespace(&mut self) {
        self.take_while(|c| c.is_whitespace());
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

/// The source text of an item, as written back.
fn item_text(item: &RawItem) -> &str {
    match item {
        RawItem::Preamble(text) => text,
        RawItem::BibPreamble { raw_text, .. }
        | RawItem::StringDef { raw_text, .. }
        | RawItem::Comment { raw_text } => raw_text,
        RawItem::Entry(entry) => &entry.raw_text,
    }
}

/// `@String` definitions, resolved as BibTeX reads them: in file order, the
/// later of two definitions of a name (compared without case) applying from
/// there on.
#[derive(Default)]
struct Macros {
    /// Lowercase name → (item index, line, resolved value), in file order.
    defs: std::collections::HashMap<String, Vec<(usize, usize, String)>>,
}

impl Macros {
    /// The definition of `name` in force at item `at`, or `None`.
    fn before(&self, name: &str, at: usize) -> Option<&(usize, usize, String)> {
        self.defs
            .get(&name.to_lowercase())?
            .iter()
            .rev()
            .find(|(index, _, _)| *index < at)
    }

    /// The first definition of `name` after item `at`, or `None`.
    fn after(&self, name: &str, at: usize) -> Option<&(usize, usize, String)> {
        self.defs
            .get(&name.to_lowercase())?
            .iter()
            .find(|(index, _, _)| *index > at)
    }
}

/// Resolve every `@String` definition and entry field of `raw` (setting each
/// field's `resolved` value), returning the problems found. A name used in
/// an entry above its only definitions resolves to the first one below, as
/// JabRef does, with a warning that BibTeX would leave it undefined;
/// definitions themselves see only definitions above them, so they cannot
/// form cycles (a definition naming itself, `@String{a = a # {x}}`, extends
/// the earlier one, with a warning). An undefined name keeps its name as its
/// value (BibTeX would make it empty), since names defined in another file,
/// such as IEEEabrv's journal names, are common.
fn resolve_strings(raw: &mut RawBibFile) -> Vec<ParseWarning> {
    let mut warnings = Vec::new();
    let mut macros = Macros::default();
    let mut lines = Vec::with_capacity(raw.items.len());
    let mut line = 1;
    for item in &raw.items {
        lines.push(line);
        line += item_text(item).matches('\n').count();
    }

    for (index, item) in raw.items.iter().enumerate() {
        let RawItem::StringDef { name, value, .. } = item else {
            continue;
        };
        let text = value.resolve(&mut |used| match macros.before(used, index) {
            Some((_, _, text)) => {
                if used.eq_ignore_ascii_case(name) {
                    warnings.push(ParseWarning {
                        line: lines[index],
                        message: format!(
                            "@String{{{name}}} uses '{used}' in its own definition; bibtui uses the earlier definition (BibTeX would treat it as undefined)"
                        ),
                    });
                }
                Some(text.clone())
            }
            None => {
                if !is_month_macro(used) {
                    warnings.push(ParseWarning {
                        line: lines[index],
                        message: format!(
                            "@String{{{name}}} uses undefined @String '{used}' (kept as its name)"
                        ),
                    });
                }
                None
            }
        });
        macros
            .defs
            .entry(name.to_lowercase())
            .or_default()
            .push((index, lines[index], text));
    }

    for (index, item) in raw.items.iter_mut().enumerate() {
        let RawItem::Entry(entry) = item else {
            continue;
        };
        let key = entry.citation_key.clone();
        for field in &mut entry.fields {
            let field_name = field.name.clone();
            field.resolved = field.value.resolve(&mut |used| {
                if let Some((_, _, text)) = macros.before(used, index) {
                    return Some(text.clone());
                }
                if let Some((_, def_line, text)) = macros.after(used, index) {
                    warnings.push(ParseWarning {
                        line: lines[index],
                        message: format!(
                            "{key}: field '{field_name}' uses @String '{used}' above its definition on line {def_line} (BibTeX would treat it as undefined)"
                        ),
                    });
                    return Some(text.clone());
                }
                if !is_month_macro(used) {
                    warnings.push(ParseWarning {
                        line: lines[index],
                        message: format!(
                            "{key}: field '{field_name}' uses undefined @String '{used}' (kept as its name)"
                        ),
                    });
                }
                None
            });
        }
    }
    warnings
}

/// Build a semantic Database from a parsed RawBibFile.
///
/// Field values are resolved: `@String` names are replaced by their
/// definitions and `#` concatenations joined exactly (see
/// [`resolve_strings`]). The raw file keeps the source expressions, which are
/// written back for every field whose value is unchanged.
pub fn build_database(mut raw: RawBibFile) -> Database {
    let value_warnings = resolve_strings(&mut raw);
    let mut entries = IndexMap::new();
    let mut jabref_meta = JabRefMeta::default();
    let mut duplicate_keys = Vec::new();

    // Every key that appears in the file is reserved up front, so a repair
    // suffix never takes a key owned by a later entry (which would silently
    // retarget crossrefs and other references to that key).
    let original_keys: std::collections::HashSet<&str> = raw
        .items
        .iter()
        .filter_map(|item| match item {
            RawItem::Entry(e) => Some(e.citation_key.as_str()),
            _ => None,
        })
        .collect();

    for (idx, item) in raw.items.iter().enumerate() {
        match item {
            RawItem::Entry(raw_entry) => {
                let entry_type = EntryType::parse(&raw_entry.entry_type);
                let mut fields = IndexMap::new();

                for field in &raw_entry.fields {
                    fields.insert(field.name.clone(), field.resolved.clone());
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

                // Uniquify duplicate citation keys: later copies are renamed
                // with a `_dupN` suffix and marked dirty so the rename is
                // written out on the next save. The original key is recorded
                // in duplicate_keys so the startup warning fires.
                let mut citation_key = raw_entry.citation_key.clone();
                let mut renamed = false;
                if entries.contains_key(&citation_key) {
                    duplicate_keys.push(citation_key.clone());
                    let mut n = 2usize;
                    loop {
                        let candidate = format!("{}_dup{}", raw_entry.citation_key, n);
                        if !entries.contains_key(&candidate)
                            && !original_keys.contains(candidate.as_str())
                        {
                            citation_key = candidate;
                            break;
                        }
                        n += 1;
                    }
                    renamed = true;
                }

                let entry = Entry {
                    entry_type,
                    citation_key: citation_key.clone(),
                    fields,
                    group_memberships,
                    raw_index: idx,
                    dirty: renamed,
                };

                entries.insert(citation_key, entry);
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
        duplicate_keys,
        value_warnings,
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

    #[test]
    fn test_parse_braced_comment() {
        // @Comment{...} — braced content
        let input = "@Comment{jabref-meta: databaseType:bibtex;}\n";
        let raw = parse_bib_file(input).unwrap();
        let comments: Vec<_> = raw
            .items
            .iter()
            .filter(|i| matches!(i, RawItem::Comment { .. }))
            .collect();
        assert_eq!(comments.len(), 1);
    }

    #[test]
    fn test_parse_bare_comment() {
        // @Comment without braces — rest of line is the comment
        let input = "@Comment This is a bare comment\n@Article{k, title = {T},}\n";
        let raw = parse_bib_file(input).unwrap();
        let comments: Vec<_> = raw
            .items
            .iter()
            .filter(|i| matches!(i, RawItem::Comment { .. }))
            .collect();
        assert_eq!(comments.len(), 1);
    }

    #[test]
    fn test_parse_preamble() {
        let input = "@Preamble{{Some preamble text}}\n";
        let raw = parse_bib_file(input).unwrap();
        let preambles: Vec<_> = raw
            .items
            .iter()
            .filter(|i| matches!(i, RawItem::BibPreamble { .. }))
            .collect();
        assert_eq!(preambles.len(), 1);
        if let RawItem::BibPreamble { content, .. } = &preambles[0] {
            // take_braced_content captures the inner {…} including braces
            assert!(content.contains("Some preamble text"), "got: {}", content);
        }
    }

    #[test]
    fn test_parse_string_def() {
        let input = "@String{jnl = {Journal of Testing}}\n";
        let raw = parse_bib_file(input).unwrap();
        let strings: Vec<_> = raw
            .items
            .iter()
            .filter(|i| matches!(i, RawItem::StringDef { .. }))
            .collect();
        assert_eq!(strings.len(), 1);
        if let RawItem::StringDef {
            name, raw_value, ..
        } = &strings[0]
        {
            assert_eq!(name, "jnl");
            assert!(
                raw_value.contains("Journal of Testing"),
                "got: {}",
                raw_value
            );
        }
    }

    #[test]
    fn test_string_and_preamble_byte_perfect_roundtrip() {
        // Case, spacing, no-space, and quoted variants must survive verbatim.
        let cases = [
            "@string{x = {y}}\n",
            "@STRING{X = {Y}}\n",
            "@String{x={y}}\n",
            "@String{jnl = \"Journal of Testing\"}\n",
            "@PREAMBLE{   {\\newcommand{\\x}{y}}   }\n",
        ];
        for input in cases {
            let raw = parse_bib_file(input).unwrap();
            let output = super::super::writer::write_bib_file(&raw);
            assert_eq!(input, output, "round-trip mismatch for {:?}", input);
        }
    }

    #[test]
    fn test_parse_string_def_missing_equals_recovers() {
        // A missing '=' must not silently mis-parse: the item is recorded as a
        // warning and its bytes are preserved for round-trip rather than being
        // rewritten into a bogus @String.
        let input = "@String{x {y}}\n";
        let raw = parse_bib_file(input).unwrap();
        assert_eq!(raw.warnings.len(), 1);
        assert!(!raw
            .items
            .iter()
            .any(|i| matches!(i, RawItem::StringDef { .. })));
        assert_eq!(super::super::writer::write_bib_file(&raw), input);
    }

    #[test]
    fn test_parse_concat_value() {
        // Value concatenated with '#'
        let input = "@Article{k, title = {Part A} # { Part B},}\n";
        let raw = parse_bib_file(input).unwrap();
        if let Some(RawItem::Entry(e)) = raw.items.iter().find(|i| matches!(i, RawItem::Entry(_))) {
            assert!(matches!(e.fields[0].value, RawFieldValue::Concat(_)));
        }
    }

    #[test]
    fn test_parse_unterminated_braces_recovers_with_warning() {
        // A malformed entry no longer aborts the parse: its bytes are preserved
        // and a warning is recorded so the rest of the file still loads.
        let input = "@Article{k, title = {unclosed\n";
        let raw = parse_bib_file(input).unwrap();
        assert_eq!(raw.warnings.len(), 1);
        // Byte-perfect round-trip of the skipped span.
        assert_eq!(super::super::writer::write_bib_file(&raw), input);
    }

    #[test]
    fn test_parse_unterminated_quoted_recovers_with_warning() {
        let input = "@Article{k, title = \"unclosed\n";
        let raw = parse_bib_file(input).unwrap();
        assert_eq!(raw.warnings.len(), 1);
        assert_eq!(super::super::writer::write_bib_file(&raw), input);
    }

    #[test]
    fn test_parse_recovers_malformed_entry_between_good_ones() {
        let input = "@Article{good1,\n  title = {A},\n}\n\
                     @Article{bad, title = {unclosed\n\
                     @Article{good2,\n  title = {B},\n}\n";
        let raw = parse_bib_file(input).unwrap();

        // Both good entries load.
        let db = build_database(raw.clone());
        assert!(db.entries.contains_key("good1"), "good1 must load");
        assert!(db.entries.contains_key("good2"), "good2 must load");

        // Exactly one warning, pointing at the malformed entry's line.
        assert_eq!(raw.warnings.len(), 1);
        assert_eq!(raw.warnings[0].line, 4);

        // Output round-trips byte-for-byte.
        assert_eq!(super::super::writer::write_bib_file(&raw), input);
    }

    #[test]
    fn test_parse_quoted_escaped_char() {
        // Escaped quote inside a quoted value
        let input = "@Article{k, title = \"say \\\"hi\\\"\",}\n";
        let raw = parse_bib_file(input).unwrap();
        if let Some(RawItem::Entry(e)) = raw.items.iter().find(|i| matches!(i, RawItem::Entry(_))) {
            assert!(matches!(e.fields[0].value, RawFieldValue::Quoted(_)));
        }
    }

    #[test]
    fn test_build_database_records_duplicate_keys() {
        let input = "@Article{k1,\n  title = {A},\n}\n\n@Article{k1,\n  title = {B},\n}\n";
        let raw = parse_bib_file(input).unwrap();
        let db = build_database(raw);
        // Both copies survive: the later one is renamed with a _dup suffix.
        assert_eq!(db.entries.len(), 2);
        assert_eq!(db.duplicate_keys, vec!["k1".to_string()]);
        assert_eq!(db.entries["k1"].fields["title"], "A");
        assert_eq!(db.entries["k1_dup2"].fields["title"], "B");
        // The renamed copy is dirty (rename persists on save) and points at
        // its own raw slot; the first copy is clean.
        assert!(!db.entries["k1"].dirty);
        assert!(db.entries["k1_dup2"].dirty);
        assert_ne!(db.entries["k1"].raw_index, db.entries["k1_dup2"].raw_index);
        assert!(matches!(
            &db.raw_file.items[db.entries["k1_dup2"].raw_index],
            RawItem::Entry(e) if e.fields[0].value.to_string_value() == "B"
        ));
    }

    #[test]
    fn test_build_database_uniquifies_triplicate_keys() {
        let input = "@Article{k1,\n  title = {A},\n}\n\n@Article{k1,\n  title = {B},\n}\n\n@Article{k1,\n  title = {C},\n}\n";
        let raw = parse_bib_file(input).unwrap();
        let db = build_database(raw);
        assert_eq!(db.entries.len(), 3);
        assert_eq!(db.duplicate_keys, vec!["k1".to_string(), "k1".to_string()]);
        assert_eq!(db.entries["k1"].fields["title"], "A");
        assert_eq!(db.entries["k1_dup2"].fields["title"], "B");
        assert_eq!(db.entries["k1_dup3"].fields["title"], "C");
    }

    #[test]
    fn test_build_database_dup_suffix_avoids_existing_key() {
        // A file that already contains k1_dup2 must not be clobbered by the
        // rename of a duplicate k1.
        let input = "@Article{k1,\n  title = {A},\n}\n\n@Article{k1_dup2,\n  title = {X},\n}\n\n@Article{k1,\n  title = {B},\n}\n";
        let raw = parse_bib_file(input).unwrap();
        let db = build_database(raw);
        assert_eq!(db.entries.len(), 3);
        assert_eq!(db.entries["k1_dup2"].fields["title"], "X");
        assert_eq!(db.entries["k1_dup3"].fields["title"], "B");
    }

    #[test]
    fn test_build_database_dup_suffix_skips_later_original_key() {
        // A later entry that already owns k1_dup2 keeps it; the duplicate k1
        // is repaired to a key nobody owns, and a crossref to k1_dup2 still
        // resolves to the entry it was written for.
        let input = "@Article{k1,\n  title = {A},\n}\n\n@Article{k1,\n  title = {Duplicate},\n}\n\n@Article{k1_dup2,\n  title = {Reserved},\n}\n\n@InBook{child,\n  crossref = {k1_dup2},\n}\n";
        let raw = parse_bib_file(input).unwrap();
        let db = build_database(raw);
        assert_eq!(db.entries.len(), 4);
        assert_eq!(db.duplicate_keys, vec!["k1".to_string()]);
        assert_eq!(db.entries["k1"].fields["title"], "A");
        assert_eq!(db.entries["k1_dup2"].fields["title"], "Reserved");
        assert!(!db.entries["k1_dup2"].dirty);
        assert_eq!(db.entries["k1_dup3"].fields["title"], "Duplicate");
        assert!(db.entries["k1_dup3"].dirty);
        let target = &db.entries["child"].fields["crossref"];
        assert_eq!(db.entries[target.as_str()].fields["title"], "Reserved");
    }

    #[test]
    fn test_build_database_dup_suffix_skips_multiple_reserved_keys() {
        // Several collisions with several reserved suffixes later in the file.
        let input = "@Misc{k,\n  title = {1},\n}\n\n@Misc{k,\n  title = {2},\n}\n\n@Misc{k,\n  title = {3},\n}\n\n@Misc{k_dup2,\n  title = {R2},\n}\n\n@Misc{k_dup4,\n  title = {R4},\n}\n";
        let db = build_database(parse_bib_file(input).unwrap());
        assert_eq!(db.entries.len(), 5);
        assert_eq!(db.entries["k"].fields["title"], "1");
        assert_eq!(db.entries["k_dup2"].fields["title"], "R2");
        assert_eq!(db.entries["k_dup4"].fields["title"], "R4");
        assert_eq!(db.entries["k_dup3"].fields["title"], "2");
        assert_eq!(db.entries["k_dup5"].fields["title"], "3");
    }

    #[test]
    fn test_stray_at_without_brace_is_preamble() {
        let input = "see jane@example.org for details\n@Article{k,\n  title = {T},\n}\n";
        let raw = parse_bib_file(input).unwrap();
        let output = super::super::writer::write_bib_file(&raw);
        assert_eq!(input, output);
        let db = build_database(raw);
        assert_eq!(db.entries.len(), 1);
    }

    #[test]
    fn test_parse_entry_skips_unexpected_char() {
        // An unexpected '!' before the field name should be skipped gracefully
        let input = "@Article{k,\n  !title = {T},\n}\n";
        // Should either parse (skipping '!') or error — either is acceptable, but must not panic
        let _ = parse_bib_file(input);
    }
}
