//! Duplicate review (list of clusters) and merge (field-by-field reconcile)
//! popups.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::bib::model::Entry;
use crate::tui::theme::Theme;
use crate::util::open::{parse_file_field, serialize_file_field, ParsedFile};

/// Pseudo field name of the entry-type row in the merge view.
pub const TYPE_FIELD: &str = "(type)";

// ── Review ───────────────────────────────────────────────────────────────────

/// One cluster of possible duplicates, as shown in the review list.
#[derive(Debug, Clone)]
pub struct ReviewCluster {
    /// Member citation keys, in library order.
    pub keys: Vec<String>,
    /// Distinct match reasons, e.g. "same DOI".
    pub reasons: Vec<String>,
    /// One "title (year)" line per member, aligned with `keys`.
    pub summaries: Vec<String>,
}

pub struct DedupReviewState {
    pub clusters: Vec<ReviewCluster>,
    pub cursor: usize,
    pub scroll: usize,
}

impl DedupReviewState {
    pub fn new(clusters: Vec<ReviewCluster>) -> Self {
        DedupReviewState {
            clusters,
            cursor: 0,
            scroll: 0,
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor + 1 < self.clusters.len() {
            self.cursor += 1;
        }
    }

    pub fn move_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_to_bottom(&mut self) {
        self.cursor = self.clusters.len().saturating_sub(1);
    }

    pub fn page_down(&mut self) {
        self.cursor = (self.cursor + 10).min(self.clusters.len().saturating_sub(1));
    }

    pub fn page_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(10);
    }

    pub fn current(&self) -> Option<&ReviewCluster> {
        self.clusters.get(self.cursor)
    }

    /// Remove the focused cluster (merged or marked not duplicates).
    pub fn remove_current(&mut self) {
        if self.cursor < self.clusters.len() {
            self.clusters.remove(self.cursor);
        }
        if self.cursor >= self.clusters.len() {
            self.cursor = self.clusters.len().saturating_sub(1);
        }
    }
}

// ── Merge ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RowKind {
    /// Every member has the same value.
    Identical,
    /// Only one distinct value, missing from some members; it is kept.
    Single,
    /// Members disagree; the user picks one.
    Conflict,
    /// Combined across members (`groups`, `file`).
    Union,
}

#[derive(Debug, Clone)]
pub struct MergeRow {
    pub field: String,
    /// Each member's value, aligned with `DedupMergeState::keys`.
    pub values: Vec<Option<String>>,
    /// Member whose value is kept (ignored for `Union` rows).
    pub choice: usize,
    /// True once the user picked a value; untouched rows follow the survivor.
    pub touched: bool,
    pub kind: RowKind,
}

impl MergeRow {
    pub fn chosen(&self) -> Option<&String> {
        self.values.get(self.choice).and_then(|v| v.as_ref())
    }
}

/// Field values compare equal when they differ only in whitespace.
fn same_value(a: &str, b: &str) -> bool {
    a.split_whitespace().eq(b.split_whitespace())
}

pub struct DedupMergeState {
    /// Member citation keys.
    pub keys: Vec<String>,
    /// Member that is kept (and receives the merged fields); others are deleted.
    pub survivor: usize,
    /// A key regenerated from the template; overrides `keys[survivor]`.
    pub generated_key: Option<String>,
    pub rows: Vec<MergeRow>,
    /// Index into `rows`; always a `Conflict` row when there is one.
    pub cursor: usize,
    pub scroll: usize,
    /// Why the members were matched.
    pub reasons: Vec<String>,
    /// Return to the review list after merging or cancelling.
    pub from_review: bool,
}

impl DedupMergeState {
    pub fn new(entries: &[&Entry], reasons: Vec<String>, from_review: bool) -> Self {
        let survivor = default_survivor(entries);
        let mut state = DedupMergeState {
            keys: entries.iter().map(|e| e.citation_key.clone()).collect(),
            survivor,
            generated_key: None,
            rows: build_rows(entries, survivor),
            cursor: 0,
            scroll: 0,
            reasons,
            from_review,
        };
        state.cursor = state.conflict_rows().first().copied().unwrap_or(0);
        state
    }

    /// The citation key the merged entry will have.
    pub fn final_key(&self) -> &str {
        self.generated_key
            .as_deref()
            .unwrap_or(&self.keys[self.survivor])
    }

    pub fn conflict_rows(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.kind == RowKind::Conflict)
            .map(|(i, _)| i)
            .collect()
    }

    /// Move to the next (`delta > 0`) or previous conflict row.
    pub fn move_conflict(&mut self, delta: isize) {
        let conflicts = self.conflict_rows();
        let Some(pos) = conflicts.iter().position(|&r| r == self.cursor) else {
            return;
        };
        let target = (pos as isize + delta).clamp(0, conflicts.len() as isize - 1) as usize;
        self.cursor = conflicts[target];
    }

    pub fn move_to_first_conflict(&mut self) {
        if let Some(&first) = self.conflict_rows().first() {
            self.cursor = first;
        }
    }

    pub fn move_to_last_conflict(&mut self) {
        if let Some(&last) = self.conflict_rows().last() {
            self.cursor = last;
        }
    }

    /// Keep `member`'s value for the focused conflict row (if it has one).
    pub fn pick(&mut self, member: usize) {
        let Some(row) = self.rows.get_mut(self.cursor) else {
            return;
        };
        if row.kind == RowKind::Conflict && matches!(row.values.get(member), Some(Some(_))) {
            row.choice = member;
            row.touched = true;
        }
    }

    /// Step the focused conflict row's choice to the next member with a value.
    pub fn pick_step(&mut self, delta: isize) {
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        if row.kind != RowKind::Conflict {
            return;
        }
        let n = row.values.len() as isize;
        let mut member = row.choice as isize;
        for _ in 0..n {
            member = (member + delta).rem_euclid(n);
            if row.values[member as usize].is_some() {
                self.pick(member as usize);
                return;
            }
        }
    }

    /// Make the next member the survivor (its key and raw position are kept).
    /// Rows the user has not picked follow the new survivor.
    pub fn cycle_survivor(&mut self) {
        self.survivor = (self.survivor + 1) % self.keys.len();
        self.generated_key = None;
        let survivor = self.survivor;
        for row in &mut self.rows {
            if !row.touched && row.values[survivor].is_some() {
                row.choice = survivor;
            }
        }
    }

    /// The merged value of each non-union field (`None` drops the field),
    /// in display order. The entry type is under [`TYPE_FIELD`].
    pub fn merged_fields(&self) -> Vec<(String, Option<String>)> {
        self.rows
            .iter()
            .filter(|r| r.kind != RowKind::Union)
            .map(|r| (r.field.clone(), r.chosen().cloned()))
            .collect()
    }
}

/// The member with the most non-empty fields (first one on a tie).
fn default_survivor(entries: &[&Entry]) -> usize {
    let mut best = 0;
    let mut best_count = 0;
    for (i, entry) in entries.iter().enumerate() {
        let count = entry
            .fields
            .values()
            .filter(|v| !v.trim().is_empty())
            .count();
        if count > best_count {
            best = i;
            best_count = count;
        }
    }
    best
}

fn classify(values: &[Option<String>]) -> RowKind {
    let present: Vec<&String> = values.iter().flatten().collect();
    let mut distinct: Vec<&String> = Vec::new();
    for v in &present {
        if !distinct.iter().any(|d| same_value(d, v)) {
            distinct.push(v);
        }
    }
    match distinct.len() {
        0 | 1 if present.len() == values.len() => RowKind::Identical,
        0 | 1 => RowKind::Single,
        _ => RowKind::Conflict,
    }
}

fn build_rows(entries: &[&Entry], survivor: usize) -> Vec<MergeRow> {
    let mut names: Vec<String> = Vec::new();
    for entry in std::iter::once(&entries[survivor]).chain(entries.iter()) {
        for (name, value) in &entry.fields {
            if !value.trim().is_empty() && !names.contains(name) {
                names.push(name.clone());
            }
        }
    }
    if entries.iter().any(|e| !e.group_memberships.is_empty())
        && !names.iter().any(|n| n == "groups")
    {
        names.push("groups".into());
    }

    let mut rows = vec![{
        let values: Vec<Option<String>> = entries
            .iter()
            .map(|e| Some(e.entry_type.display_name().to_string()))
            .collect();
        MergeRow {
            field: TYPE_FIELD.into(),
            kind: classify(&values),
            values,
            choice: survivor,
            touched: false,
        }
    }];
    for name in names {
        let values: Vec<Option<String>> = entries
            .iter()
            .map(|e| {
                if name == "groups" {
                    (!e.group_memberships.is_empty()).then(|| e.group_memberships.join(", "))
                } else {
                    e.fields
                        .get(&name)
                        .filter(|v| !v.trim().is_empty())
                        .cloned()
                }
            })
            .collect();
        let kind = if name == "groups" || name == "file" {
            RowKind::Union
        } else {
            classify(&values)
        };
        let choice = if values[survivor].is_some() {
            survivor
        } else {
            values.iter().position(|v| v.is_some()).unwrap_or(survivor)
        };
        rows.push(MergeRow {
            field: name,
            values,
            choice,
            touched: false,
            kind,
        });
    }
    rows
}

/// Group memberships of all members, survivor first, without repeats.
pub fn union_groups(entries: &[&Entry], survivor: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for entry in std::iter::once(&entries[survivor]).chain(entries.iter()) {
        for group in &entry.group_memberships {
            if !out.contains(group) {
                out.push(group.clone());
            }
        }
    }
    out
}

/// Attached files of all members, survivor first, without repeated paths.
/// Returns `None` when the survivor already has exactly these files.
pub fn union_files(entries: &[&Entry], survivor: usize) -> Option<String> {
    let parse = |e: &Entry| -> Vec<ParsedFile> {
        e.fields
            .get("file")
            .map(|v| parse_file_field(v))
            .unwrap_or_default()
    };
    let own = parse(entries[survivor]);
    let mut out: Vec<ParsedFile> = Vec::new();
    for entry in std::iter::once(&entries[survivor]).chain(entries.iter()) {
        for file in parse(entry) {
            if !out.iter().any(|f| f.path == file.path) {
                out.push(file);
            }
        }
    }
    (out.len() != own.len()).then(|| serialize_file_field(&out))
}

// ── Rendering ────────────────────────────────────────────────────────────────

fn popup(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).max(50_u16.min(area.width));
    let height = area.height.saturating_sub(4).max(8).min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

fn truncate(s: &str, max: usize) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let mut t: String = flat.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// Scroll so `focus_line` sits near the middle of a `visible`-line viewport.
fn centered_scroll(focus_line: usize, total: usize, visible: usize) -> usize {
    focus_line
        .saturating_sub(visible / 2)
        .min(total.saturating_sub(visible))
}

pub fn render_dedup_review(f: &mut Frame, area: Rect, state: &mut DedupReviewState, theme: &Theme) {
    let popup_area = popup(area);
    f.render_widget(Clear, popup_area);

    let n = state.clusters.len();
    let title = if n == 0 {
        " Duplicates: none found ".to_string()
    } else {
        format!(
            " Duplicates: {} possible cluster{} ",
            n,
            if n == 1 { "" } else { "s" }
        )
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(title)
        .title_bottom(Line::from(Span::styled(
            " j/k: navigate  Enter: reconcile  x: not duplicates  Esc: close ",
            theme.label,
        )));
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if n == 0 {
        f.render_widget(
            Paragraph::new("No possible duplicates — every entry looks unique.").style(theme.value),
            inner,
        );
        return;
    }

    let bold = Style::default().add_modifier(Modifier::BOLD);
    let reason_style = Style::default().fg(Color::Yellow);
    let max_w = inner.width.saturating_sub(4) as usize;

    let mut lines: Vec<Line> = Vec::new();
    let mut focus_line = 0;
    for (ci, cluster) in state.clusters.iter().enumerate() {
        let focused = ci == state.cursor;
        if focused {
            focus_line = lines.len();
        }
        let header_style = if focused { bold.fg(Color::Cyan) } else { bold };
        lines.push(Line::from(vec![
            Span::styled(if focused { "▶ " } else { "  " }, header_style),
            Span::styled(format!("{} entries", cluster.keys.len()), header_style),
            Span::styled(" — ", theme.label),
            Span::styled(cluster.reasons.join("; "), reason_style),
        ]));
        for (key, summary) in cluster.keys.iter().zip(&cluster.summaries) {
            let key_w = key.chars().count().min(max_w / 2);
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    truncate(key, key_w),
                    theme.value.add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ", theme.label),
                Span::styled(
                    truncate(summary, max_w.saturating_sub(key_w + 2)),
                    theme.label,
                ),
            ]));
        }
        lines.push(Line::from(""));
    }

    state.scroll = centered_scroll(focus_line, lines.len(), inner.height as usize);
    f.render_widget(
        Paragraph::new(lines).scroll((state.scroll as u16, 0)),
        inner,
    );
}

pub fn render_dedup_merge(f: &mut Frame, area: Rect, state: &mut DedupMergeState, theme: &Theme) {
    let popup_area = popup(area);
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(format!(" Merge {} entries ", state.keys.len()))
        .title_bottom(Line::from(Span::styled(
            " j/k: conflict  h/l or 1-9: pick  Tab: keep other  c: regen key  Enter: merge  Esc: back ",
            theme.label,
        )));
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let bold = Style::default().add_modifier(Modifier::BOLD);
    let keep_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let warn = Style::default().fg(Color::Yellow);
    let width = inner.width as usize;
    let name_w = state
        .rows
        .iter()
        .map(|r| r.field.chars().count())
        .max()
        .unwrap_or(8)
        .clamp(6, 16);
    let value_w = width.saturating_sub(name_w + 12).max(10);

    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("Key: ", theme.label),
            Span::styled(state.final_key().to_string(), keep_style),
            Span::styled(
                if state.generated_key.is_some() {
                    "  (regenerated)"
                } else {
                    ""
                },
                theme.label,
            ),
        ]),
        Line::from(vec![
            Span::styled("Matched: ", theme.label),
            Span::styled(state.reasons.join("; "), warn),
        ]),
    ];
    for (i, key) in state.keys.iter().enumerate() {
        let keep = i == state.survivor;
        lines.push(Line::from(vec![
            Span::styled(format!("  ({}) ", i + 1), theme.label),
            Span::styled(
                truncate(key, width.saturating_sub(18)),
                if keep { keep_style } else { theme.value },
            ),
            Span::styled(if keep { "  ★ keep" } else { "  ✗ delete" }, theme.label),
        ]));
    }
    lines.push(Line::from(Span::styled("─".repeat(width), theme.border)));

    let conflicts = state.conflict_rows();
    if conflicts.is_empty() {
        lines.push(Line::from(Span::styled(
            "No conflicts — Enter merges.",
            theme.label,
        )));
    }

    let mut focus_line = 0;
    for (ri, row) in state.rows.iter().enumerate() {
        let name = format!("{:<w$}", truncate(&row.field, name_w), w = name_w);
        match row.kind {
            RowKind::Identical => {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {} ", name), theme.label),
                    Span::styled("✓ ", keep_style),
                    Span::styled(
                        truncate(row.chosen().map(String::as_str).unwrap_or(""), value_w),
                        theme.value,
                    ),
                ]));
            }
            RowKind::Single => {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {} ", name), theme.label),
                    Span::styled("+ ", keep_style),
                    Span::styled(
                        truncate(row.chosen().map(String::as_str).unwrap_or(""), value_w),
                        theme.value,
                    ),
                    Span::styled(format!("  ({})", row.choice + 1), theme.label),
                ]));
            }
            RowKind::Union => {
                let parts: Vec<String> = row
                    .values
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| v.as_ref().map(|v| format!("({}) {}", i + 1, v)))
                    .collect();
                lines.push(Line::from(vec![
                    Span::styled(format!("  {} ", name), theme.label),
                    Span::styled("∪ ", keep_style),
                    Span::styled(truncate(&parts.join("  "), value_w), theme.value),
                ]));
            }
            RowKind::Conflict => {
                let focused = ri == state.cursor;
                if focused {
                    focus_line = lines.len();
                }
                for (mi, value) in row.values.iter().enumerate() {
                    let chosen = mi == row.choice;
                    let lead = match (mi == 0, focused) {
                        (true, true) => format!("▶ {} ", name),
                        (true, false) => format!("  {} ", name),
                        _ => " ".repeat(name_w + 3),
                    };
                    let lead_style = if focused { bold.fg(Color::Cyan) } else { warn };
                    let text = value.as_deref().unwrap_or("—");
                    lines.push(Line::from(vec![
                        Span::styled(lead, lead_style),
                        Span::styled(format!("({}) ", mi + 1), theme.label),
                        Span::styled(
                            truncate(text, value_w),
                            if chosen { keep_style } else { theme.value },
                        ),
                        Span::styled(if chosen { " ◀" } else { "" }, keep_style),
                    ]));
                }
            }
        }
    }

    state.scroll = centered_scroll(focus_line, lines.len(), inner.height as usize);
    f.render_widget(
        Paragraph::new(lines).scroll((state.scroll as u16, 0)),
        inner,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::model::EntryType;
    use pretty_assertions::assert_eq;

    fn entry(key: &str, ty: EntryType, fields: &[(&str, &str)], groups: &[&str]) -> Entry {
        Entry {
            entry_type: ty,
            citation_key: key.into(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            group_memberships: groups.iter().map(|g| g.to_string()).collect(),
            raw_index: 0,
            dirty: false,
        }
    }

    fn sample() -> (Entry, Entry) {
        (
            entry(
                "A",
                EntryType::Article,
                &[
                    ("title", "Same  Title"),
                    ("journal", "Phys. Rev. D"),
                    ("pages", "1--10"),
                ],
                &["G1"],
            ),
            entry(
                "B",
                EntryType::Article,
                &[
                    ("title", "Same Title"),
                    ("journal", "Physical Review D"),
                    ("doi", "10.1/x"),
                    ("year", "2020"),
                ],
                &["G2", "G1"],
            ),
        )
    }

    fn row<'a>(state: &'a DedupMergeState, field: &str) -> &'a MergeRow {
        state.rows.iter().find(|r| r.field == field).unwrap()
    }

    #[test]
    fn rows_are_classified() {
        let (a, b) = sample();
        let state = DedupMergeState::new(&[&a, &b], vec![], false);
        // B has more fields, so it survives by default.
        assert_eq!(state.survivor, 1);
        assert_eq!(state.final_key(), "B");
        assert_eq!(row(&state, TYPE_FIELD).kind, RowKind::Identical);
        assert_eq!(row(&state, "title").kind, RowKind::Identical);
        assert_eq!(row(&state, "journal").kind, RowKind::Conflict);
        assert_eq!(row(&state, "pages").kind, RowKind::Single);
        assert_eq!(row(&state, "pages").chosen().unwrap(), "1--10");
        assert_eq!(row(&state, "groups").kind, RowKind::Union);
        assert_eq!(state.rows[state.cursor].field, "journal");
    }

    #[test]
    fn picking_and_survivor_cycling() {
        let (a, b) = sample();
        let mut state = DedupMergeState::new(&[&a, &b], vec![], false);
        assert_eq!(
            row(&state, "journal").chosen().unwrap(),
            "Physical Review D"
        );
        state.pick_step(1);
        assert_eq!(row(&state, "journal").chosen().unwrap(), "Phys. Rev. D");
        // A touched row keeps the user's pick when the survivor changes...
        state.cycle_survivor();
        assert_eq!(state.final_key(), "A");
        assert_eq!(row(&state, "journal").chosen().unwrap(), "Phys. Rev. D");
        // ...and untouched rows follow the survivor.
        assert_eq!(row(&state, "title").chosen().unwrap(), "Same  Title");
        state.pick(1);
        assert_eq!(
            row(&state, "journal").chosen().unwrap(),
            "Physical Review D"
        );
    }

    #[test]
    fn conflicting_types_are_a_conflict_row() {
        let a = entry("A", EntryType::Article, &[("title", "T")], &[]);
        let b = entry("B", EntryType::Book, &[("title", "T")], &[]);
        let mut state = DedupMergeState::new(&[&a, &b], vec![], false);
        assert_eq!(state.rows[state.cursor].field, TYPE_FIELD);
        state.pick(1);
        let merged = state.merged_fields();
        assert_eq!(
            merged[0],
            (TYPE_FIELD.to_string(), Some("Book".to_string()))
        );
    }

    #[test]
    fn unions_dedupe_groups_and_files() {
        let (a, b) = sample();
        assert_eq!(union_groups(&[&a, &b], 0), vec!["G1", "G2"]);
        let fa = entry("A", EntryType::Article, &[("file", ":PDF/a.pdf:PDF")], &[]);
        let fb = entry(
            "B",
            EntryType::Article,
            &[("file", ":PDF/a.pdf:PDF;:PDF/b.pdf:PDF")],
            &[],
        );
        assert_eq!(
            union_files(&[&fa, &fb], 0).as_deref(),
            Some(":PDF/a.pdf:PDF;:PDF/b.pdf:PDF")
        );
        assert_eq!(union_files(&[&fa, &fb], 1), None);
    }

    #[test]
    fn review_remove_current_clamps_cursor() {
        let cluster = ReviewCluster {
            keys: vec!["A".into(), "B".into()],
            reasons: vec!["same DOI".into()],
            summaries: vec!["T".into(), "T".into()],
        };
        let mut state = DedupReviewState::new(vec![cluster.clone(), cluster]);
        state.move_down();
        state.remove_current();
        assert_eq!(state.cursor, 0);
        state.remove_current();
        assert!(state.clusters.is_empty());
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn rendering_does_not_panic() {
        let (a, b) = sample();
        let mut merge = DedupMergeState::new(&[&a, &b], vec!["same DOI".into()], true);
        let mut review = DedupReviewState::new(vec![ReviewCluster {
            keys: vec!["A".into(), "B".into()],
            reasons: vec!["same DOI".into()],
            summaries: vec!["Same Title (2020)".into(), "Same Title".into()],
        }]);
        let theme = Theme::default();
        for (w, h) in [(120, 40), (20, 8)] {
            let mut term =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            term.draw(|f| render_dedup_review(f, f.area(), &mut review, &theme))
                .unwrap();
            term.draw(|f| render_dedup_merge(f, f.area(), &mut merge, &theme))
                .unwrap();
        }
        let mut empty = DedupReviewState::new(vec![]);
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20)).unwrap();
        term.draw(|f| render_dedup_review(f, f.area(), &mut empty, &theme))
            .unwrap();
    }
}
