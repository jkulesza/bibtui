#![allow(dead_code)]
use unicode_width::UnicodeWidthStr;

/// Truncate a string to fit within a given display width, accounting for
/// Unicode character widths. Adds "..." if truncated.
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    let width = UnicodeWidthStr::width(s);
    if width <= max_width {
        return s.to_string();
    }

    if max_width <= 2 {
        return ".".repeat(max_width);
    }

    let target = max_width - 2; // Leave room for ".."
    let mut current_width = 0;
    let mut end_idx = 0;

    for (idx, ch) in s.char_indices() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width + ch_width > target {
            break;
        }
        current_width += ch_width;
        end_idx = idx + ch.len_utf8();
    }

    format!("{}..", &s[..end_idx])
}
