use unicode_width::UnicodeWidthStr;

/// Truncate a string to fit within a given display width, accounting for
/// Unicode character widths. Adds ".." if truncated.
#[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_truncation_needed() {
        assert_eq!(truncate_to_width("hello", 10), "hello");
    }

    #[test]
    fn test_exact_fit() {
        assert_eq!(truncate_to_width("hello", 5), "hello");
    }

    #[test]
    fn test_truncation_ascii() {
        assert_eq!(truncate_to_width("hello world", 7), "hello..");
    }

    #[test]
    fn test_max_width_zero() {
        assert_eq!(truncate_to_width("hi", 0), "");
    }

    #[test]
    fn test_max_width_one() {
        assert_eq!(truncate_to_width("hi", 1), ".");
    }

    #[test]
    fn test_max_width_two() {
        assert_eq!(truncate_to_width("hi", 2), "hi");
    }

    #[test]
    fn test_max_width_two_longer() {
        assert_eq!(truncate_to_width("hello", 2), "..");
    }

    #[test]
    fn test_multibyte_unicode() {
        // "café" = width 4; target = 8-2 = 6; fits "café w" then truncates
        assert_eq!(truncate_to_width("café world", 8), "café w..");
        // max_width 6: target=4, fits "café" exactly
        assert_eq!(truncate_to_width("café world", 6), "café..");
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(truncate_to_width("", 5), "");
    }
}
