//! Filesystem path helpers.

/// Expand a leading `~` (alone or as `~/...`) to the user's home directory.
///
/// Returns the input unchanged when it does not start with a tilde or when
/// no home directory can be determined.
pub fn expand_tilde(s: &str) -> String {
    let rest = if s == "~" {
        Some("")
    } else {
        s.strip_prefix("~/").or_else(|| {
            if cfg!(windows) {
                s.strip_prefix("~\\")
            } else {
                None
            }
        })
    };
    if let Some(rest) = rest {
        if let Some(home) = dirs::home_dir() {
            // Join so the separator after the home directory is native.
            let expanded = if rest.is_empty() {
                home
            } else {
                home.join(rest)
            };
            return expanded.to_string_lossy().into_owned();
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde_prefix() {
        if let Some(home) = dirs::home_dir() {
            assert_eq!(
                expand_tilde("~/x.bib"),
                home.join("x.bib").to_string_lossy().into_owned()
            );
            assert_eq!(expand_tilde("~"), home.to_string_lossy().into_owned());
        }
    }

    #[test]
    fn test_expand_tilde_non_tilde_unchanged() {
        assert_eq!(expand_tilde("/abs/path.bib"), "/abs/path.bib");
        assert_eq!(expand_tilde("relative/path.bib"), "relative/path.bib");
        // A tilde that is not a leading `~/` component is untouched.
        assert_eq!(expand_tilde("~user/x.bib"), "~user/x.bib");
    }
}
