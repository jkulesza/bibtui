use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

/// One cluster of similar author names that the user can merge.
#[derive(Debug, Clone)]
pub struct NameCluster {
    /// The canonical (longest / most complete) form chosen as default.
    pub canonical: String,
    /// All variant forms found in the database (includes canonical).
    pub variants: Vec<NameVariant>,
    /// Which variant is currently selected as the merge target (index into `variants`).
    pub selected_variant: usize,
}

/// A single author-name variant with usage count.
#[derive(Debug, Clone)]
pub struct NameVariant {
    /// The exact author-name string as it appears in the database.
    pub name: String,
    /// Number of entries that use this variant.
    pub count: usize,
}

/// Preview showing entries that use a particular name variant.
#[derive(Debug, Clone)]
pub struct NamePreview {
    /// The variant name being previewed.
    pub variant_name: String,
    /// "citekey — title" summaries of entries using this variant.
    pub entries: Vec<String>,
    /// Scroll offset within the preview.
    pub scroll: usize,
}

pub struct NameDisambigState {
    pub clusters: Vec<NameCluster>,
    /// Which cluster is focused.
    pub cursor: usize,
    /// Scroll offset for the viewport.
    pub scroll: usize,
    /// Optional preview of entries for the selected variant.
    pub preview: Option<NamePreview>,
}

impl NameDisambigState {
    pub fn new(clusters: Vec<NameCluster>) -> Self {
        NameDisambigState {
            clusters,
            cursor: 0,
            scroll: 0,
            preview: None,
        }
    }

    pub fn move_down(&mut self) {
        if !self.clusters.is_empty() && self.cursor < self.clusters.len() - 1 {
            self.cursor += 1;
        }
    }

    pub fn move_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Cycle the selected variant within the focused cluster.
    pub fn cycle_variant(&mut self) {
        if let Some(cluster) = self.clusters.get_mut(self.cursor) {
            if !cluster.variants.is_empty() {
                cluster.selected_variant =
                    (cluster.selected_variant + 1) % cluster.variants.len();
                cluster.canonical = cluster.variants[cluster.selected_variant].name.clone();
            }
        }
    }

    /// Cycle variant backward.
    pub fn cycle_variant_reverse(&mut self) {
        if let Some(cluster) = self.clusters.get_mut(self.cursor) {
            if !cluster.variants.is_empty() {
                let len = cluster.variants.len();
                cluster.selected_variant =
                    (cluster.selected_variant + len - 1) % len;
                cluster.canonical = cluster.variants[cluster.selected_variant].name.clone();
            }
        }
    }

    /// Remove the currently selected variant from the focused cluster.
    /// If the cluster drops to fewer than 2 variants, remove the cluster entirely.
    /// Returns `true` if a variant was removed.
    pub fn remove_variant(&mut self) -> bool {
        let cluster = match self.clusters.get_mut(self.cursor) {
            Some(c) => c,
            None => return false,
        };
        if cluster.variants.len() <= 2 {
            // Removing one would leave ≤1 variant — drop the whole cluster.
            self.clusters.remove(self.cursor);
            if !self.clusters.is_empty() && self.cursor >= self.clusters.len() {
                self.cursor = self.clusters.len() - 1;
            }
            return true;
        }
        // Remove the selected variant.
        cluster.variants.remove(cluster.selected_variant);
        if cluster.selected_variant >= cluster.variants.len() {
            cluster.selected_variant = 0;
        }
        cluster.canonical = cluster.variants[cluster.selected_variant].name.clone();
        true
    }

    pub fn page_down(&mut self) {
        if !self.clusters.is_empty() {
            self.cursor = (self.cursor + 10).min(self.clusters.len() - 1);
        }
    }

    pub fn page_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(10);
    }
}

pub fn render_name_disambig(
    f: &mut Frame,
    area: Rect,
    state: &mut NameDisambigState,
    theme: &Theme,
) {
    let width = (area.width * 9 / 10).min(110).max(50);
    let height = (area.height.saturating_sub(4)).max(8).min(area.height);

    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, popup_area);

    let title = if state.clusters.is_empty() {
        " Name Disambiguator: no similar names found ".to_string()
    } else {
        format!(
            " Name Disambiguator: {} cluster{} of similar names ",
            state.clusters.len(),
            if state.clusters.len() == 1 { "" } else { "s" },
        )
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(title)
        .title_bottom(Line::from(Span::styled(
            " j/k: navigate  Tab/S-Tab: cycle target  Space: preview  x: remove  Enter: apply  Esc: close ",
            theme.label,
        )));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if state.clusters.is_empty() {
        let para = Paragraph::new("All author names are unique — nothing to merge.")
            .style(theme.value);
        f.render_widget(para, inner);
        return;
    }

    let bold = Style::default().add_modifier(Modifier::BOLD);
    let selected_style = Style::default().fg(Color::Green).add_modifier(Modifier::BOLD);
    let dim = theme.label;
    let count_style = Style::default().fg(Color::Yellow);

    let max_w = inner.width.saturating_sub(4) as usize;

    let mut lines: Vec<Line> = Vec::new();
    for (ci, cluster) in state.clusters.iter().enumerate() {
        let is_focused = ci == state.cursor;
        let marker = if is_focused { "▶ " } else { "  " };
        let header_style = if is_focused {
            bold.fg(Color::Cyan)
        } else {
            bold
        };

        // Cluster header: merge target
        let target_display: String = cluster.canonical.chars().take(max_w.saturating_sub(20)).collect();
        lines.push(Line::from(vec![
            Span::styled(marker, header_style),
            Span::styled("Merge to: ", dim),
            Span::styled(target_display, selected_style),
        ]));

        // Variants
        for (vi, variant) in cluster.variants.iter().enumerate() {
            let is_target = vi == cluster.selected_variant;
            let prefix = if is_target { "  ✓ " } else { "    " };
            let name_display: String = variant.name.chars().take(max_w.saturating_sub(15)).collect();
            let name_style = if is_target { selected_style } else { theme.value };
            lines.push(Line::from(vec![
                Span::styled(prefix, name_style),
                Span::styled(name_display, name_style),
                Span::styled(format!("  ({})", variant.count), count_style),
            ]));
        }
        lines.push(Line::from(""));
    }

    let total_lines = lines.len();

    // Ensure focused cluster is visible.
    // Compute line offset of the focused cluster.
    let mut cursor_line = 0usize;
    for ci in 0..state.cursor {
        cursor_line += state.clusters[ci].variants.len() + 2; // header + variants + blank
    }
    let visible = inner.height as usize;
    // Center the focused cluster vertically, clamped to [0, max_scroll].
    let half = visible / 2;
    let ideal = cursor_line.saturating_sub(half);
    let max_scroll = total_lines.saturating_sub(visible);
    state.scroll = ideal.min(max_scroll);

    let para = Paragraph::new(lines).scroll((state.scroll as u16, 0));
    f.render_widget(para, inner);

    // ── Preview overlay ─────────────────────────────────────────────────
    if let Some(ref preview) = state.preview {
        let pw = (inner.width.saturating_sub(4)).max(30);
        let ph = (inner.height.saturating_sub(2)).max(4);
        let px = inner.x + (inner.width.saturating_sub(pw)) / 2;
        let py = inner.y + (inner.height.saturating_sub(ph)) / 2;
        let preview_area = Rect::new(px, py, pw, ph);

        f.render_widget(Clear, preview_area);

        let preview_title = format!(
            " Entries for: {} ({}) ",
            preview.variant_name,
            preview.entries.len(),
        );
        let preview_block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border)
            .title(preview_title)
            .title_bottom(Line::from(Span::styled(
                " j/k: scroll  Space/Esc: close ",
                theme.label,
            )));
        let preview_inner = preview_block.inner(preview_area);
        f.render_widget(preview_block, preview_area);

        let preview_lines: Vec<Line> = preview.entries
            .iter()
            .map(|s| Line::from(Span::styled(s.as_str(), theme.value)))
            .collect();
        let preview_para = Paragraph::new(preview_lines)
            .scroll((preview.scroll as u16, 0));
        f.render_widget(preview_para, preview_inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cluster(names: &[(&str, usize)]) -> NameCluster {
        let variants: Vec<NameVariant> = names
            .iter()
            .map(|(n, c)| NameVariant { name: n.to_string(), count: *c })
            .collect();
        let canonical = variants[0].name.clone();
        NameCluster {
            canonical,
            variants,
            selected_variant: 0,
        }
    }

    #[test]
    fn test_new_empty() {
        let s = NameDisambigState::new(vec![]);
        assert!(s.clusters.is_empty());
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn test_move_down_up() {
        let mut s = NameDisambigState::new(vec![
            make_cluster(&[("Smith, John", 3), ("J. Smith", 1)]),
            make_cluster(&[("Jones, Alice", 2), ("A. Jones", 1)]),
        ]);
        assert_eq!(s.cursor, 0);
        s.move_down();
        assert_eq!(s.cursor, 1);
        s.move_down(); // clamped
        assert_eq!(s.cursor, 1);
        s.move_up();
        assert_eq!(s.cursor, 0);
        s.move_up(); // clamped
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn test_cycle_variant() {
        let mut s = NameDisambigState::new(vec![
            make_cluster(&[("Smith, John", 3), ("J. Smith", 1), ("Smith, J.", 2)]),
        ]);
        assert_eq!(s.clusters[0].canonical, "Smith, John");
        s.cycle_variant();
        assert_eq!(s.clusters[0].canonical, "J. Smith");
        s.cycle_variant();
        assert_eq!(s.clusters[0].canonical, "Smith, J.");
        s.cycle_variant(); // wraps
        assert_eq!(s.clusters[0].canonical, "Smith, John");
    }

    #[test]
    fn test_cycle_variant_reverse() {
        let mut s = NameDisambigState::new(vec![
            make_cluster(&[("Smith, John", 3), ("J. Smith", 1)]),
        ]);
        assert_eq!(s.clusters[0].canonical, "Smith, John");
        s.cycle_variant_reverse(); // wraps to last
        assert_eq!(s.clusters[0].canonical, "J. Smith");
        s.cycle_variant_reverse();
        assert_eq!(s.clusters[0].canonical, "Smith, John");
    }

    #[test]
    fn test_remove_variant_from_3_variant_cluster() {
        let mut s = NameDisambigState::new(vec![
            make_cluster(&[("Smith, John", 3), ("J. Smith", 1), ("Smith, J.", 2)]),
        ]);
        // selected_variant starts at 0 ("Smith, John")
        assert_eq!(s.clusters[0].variants.len(), 3);
        assert!(s.remove_variant());
        assert_eq!(s.clusters.len(), 1); // cluster still exists
        assert_eq!(s.clusters[0].variants.len(), 2);
        // "Smith, John" was removed; selected_variant should reset
        assert_ne!(s.clusters[0].canonical, "Smith, John");
    }

    #[test]
    fn test_remove_variant_from_2_variant_cluster_removes_cluster() {
        let mut s = NameDisambigState::new(vec![
            make_cluster(&[("Smith, John", 3), ("J. Smith", 1)]),
            make_cluster(&[("Jones, Alice", 2), ("A. Jones", 1)]),
        ]);
        assert_eq!(s.clusters.len(), 2);
        assert!(s.remove_variant()); // removes Smith cluster entirely
        assert_eq!(s.clusters.len(), 1);
        assert_eq!(s.clusters[0].canonical, "Jones, Alice");
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn test_remove_variant_last_cluster() {
        let mut s = NameDisambigState::new(vec![
            make_cluster(&[("Smith, John", 3), ("J. Smith", 1)]),
        ]);
        assert!(s.remove_variant());
        assert!(s.clusters.is_empty());
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn test_remove_variant_empty_state() {
        let mut s = NameDisambigState::new(vec![]);
        assert!(!s.remove_variant());
    }

    #[test]
    fn test_remove_variant_cursor_at_end() {
        let mut s = NameDisambigState::new(vec![
            make_cluster(&[("Smith, John", 3), ("J. Smith", 1)]),
            make_cluster(&[("Jones, Alice", 2), ("A. Jones", 1)]),
        ]);
        s.cursor = 1; // focus last cluster
        assert!(s.remove_variant()); // removes Jones cluster
        assert_eq!(s.clusters.len(), 1);
        assert_eq!(s.cursor, 0); // cursor clamped
    }

    #[test]
    fn test_preview_state() {
        let mut s = NameDisambigState::new(vec![
            make_cluster(&[("Smith, John", 3), ("J. Smith", 1)]),
        ]);
        assert!(s.preview.is_none());
        s.preview = Some(NamePreview {
            variant_name: "Smith, John".to_string(),
            entries: vec!["k1 — Paper A".to_string(), "k2 — Paper B".to_string()],
            scroll: 0,
        });
        assert!(s.preview.is_some());
        assert_eq!(s.preview.as_ref().unwrap().entries.len(), 2);
        // Scroll
        s.preview.as_mut().unwrap().scroll = 1;
        assert_eq!(s.preview.as_ref().unwrap().scroll, 1);
        // Close
        s.preview = None;
        assert!(s.preview.is_none());
    }

    #[test]
    fn test_remove_selected_variant_at_end_of_list() {
        // When selected_variant is the last index in a 3+ variant cluster
        let mut s = NameDisambigState::new(vec![
            make_cluster(&[("A", 1), ("B", 2), ("C", 3)]),
        ]);
        s.clusters[0].selected_variant = 2; // select last
        s.clusters[0].canonical = "C".to_string();
        assert!(s.remove_variant());
        // selected_variant should wrap to 0
        assert_eq!(s.clusters[0].selected_variant, 0);
        assert_eq!(s.clusters[0].canonical, "A");
        assert_eq!(s.clusters[0].variants.len(), 2);
    }

    #[test]
    fn test_cycle_variant_single_variant() {
        // Edge case: cluster with 1 variant (shouldn't happen normally, but test it)
        let mut s = NameDisambigState::new(vec![NameCluster {
            canonical: "Solo".to_string(),
            variants: vec![NameVariant { name: "Solo".to_string(), count: 5 }],
            selected_variant: 0,
        }]);
        s.cycle_variant();
        assert_eq!(s.clusters[0].selected_variant, 0);
        s.cycle_variant_reverse();
        assert_eq!(s.clusters[0].selected_variant, 0);
    }

    #[test]
    fn test_move_down_empty() {
        let mut s = NameDisambigState::new(vec![]);
        s.move_down(); // should not panic
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn test_page_down_empty() {
        let mut s = NameDisambigState::new(vec![]);
        s.page_down(); // should not panic
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn test_page_down_up() {
        let clusters: Vec<NameCluster> = (0..20)
            .map(|i| make_cluster(&[(&format!("Name{}", i), 1)]))
            .collect();
        let mut s = NameDisambigState::new(clusters);
        s.page_down();
        assert_eq!(s.cursor, 10);
        s.page_down();
        assert_eq!(s.cursor, 19); // clamped
        s.page_up();
        assert_eq!(s.cursor, 9);
    }
}
