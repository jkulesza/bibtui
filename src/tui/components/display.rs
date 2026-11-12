//! How field values are shown, shared by the entry list and detail view.

use crate::util::latex::render_latex;
use crate::util::titlecase::strip_case_braces;

/// Display settings passed to the widgets that show field values, so a new
/// display flag is one field here rather than a parameter on every render
/// function.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisplayOptions {
    /// Show case-protecting braces instead of stripping them (`B`).
    pub show_braces: bool,
    /// Render LaTeX markup as Unicode (`L`).
    pub render_latex: bool,
    /// Shorten author lists in the entry list ("Smith et al.").
    pub abbreviate_authors: bool,
    /// Show the ISO 4 abbreviation in the entry list's journal column.
    pub abbreviate_journal: bool,
}

impl DisplayOptions {
    /// `value` as it is shown: see [`apply_display_pipeline`].
    pub fn show(&self, value: &str) -> String {
        apply_display_pipeline(value, self.show_braces, self.render_latex)
    }
}

/// Apply the display pipeline: optionally render LaTeX, then optionally strip braces.
/// LaTeX must run first because it needs the `{...}` accent patterns.
pub fn apply_display_pipeline(
    value: &str,
    show_braces: bool,
    render_latex_enabled: bool,
) -> String {
    let s = if render_latex_enabled {
        render_latex(value)
    } else {
        value.to_string()
    };
    if show_braces {
        s
    } else {
        strip_case_braces(&s)
    }
}
