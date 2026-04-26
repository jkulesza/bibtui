# Changelog

### 0.55.3

- **Expanded import-fetcher test coverage**: 8 new tests; `util/import/ans.rs` 86.98% → 90.05% (DC.Identifier meta-tag path, non-DOI value fall-through to href, candidate dedup); `util/import/pdf.rs` 83.58% → 92.34% (real PDF file with header/tail DOI extraction, non-PDF magic rejection, no-DOI-anywhere failure, `.PDF` extension acceptance); overall line coverage 79.61% → 79.71%

### 0.55.2

- **Remove unused dead code**: removed unused `make_entry` test helper in `src/util/export.rs` to eliminate compiler warning
- **Dependency update**: `openssl` 0.10.77 → 0.10.78, `openssl-sys` 0.9.113 → 0.9.114
- **Fix custom skill invocation**: moved `.claude/skills/commit.md` to `.claude/skills/commit/SKILL.md` so the `/commit` slash command is correctly recognized by Claude Code

### 0.55.1

- **Dependency security update**: `rustls-webpki` 0.103.12 → 0.103.13 (RUSTSEC-2026-0104 — reachable panic in certificate revocation list parsing via malformed CRL BIT STRING)
- **Fix Windows compiler warning**: `copy_to_clipboard` parameter `text` was unused on non-macOS/Linux targets; suppress with `let _ = text` in the unsupported-platform branch

### 0.55.0

- **Dependency security updates**: `rustls-webpki` 0.103.10 → 0.103.12 (fixes two name-constraints advisories: wildcard names and URI names accepted incorrectly); `rand` 0.8.5 → 0.8.6 and assorted other dependency updates via `cargo update`
- **Bug fixes found by testing**: popup dialogs (`CitationPreviewState`, `NameDisambigState`, `ValidateResultsState`) no longer panic on terminals narrower than the popup's minimum width — popup width is now clamped to the available terminal area (same fix applied to `help.rs` in 0.54.0)
- **Expanded test coverage**: 41 new tests; `citation_preview.rs` 0% → 100%, `validate_results.rs` 61% → 100%, `name_disambig.rs` 57% → 99%, `keybindings.rs` 92% → 99%, `export.rs` 90% → 97%; overall line coverage 77.1% → 79.6%
  - `citation_preview`: 9 tests for `estimate_wrapped_lines` edge cases + 4 render smoke-tests
  - `validate_results`: 4 render smoke-tests including scroll-clamping in render path
  - `name_disambig`: 5 render smoke-tests including preview overlay and scroll-to-focus
  - `keybindings`: all 23 named special keys, `ctrl-`/`shift-`/`alt-` prefixes, exhaustive `action_from_name` coverage, all 9 mode names via `build_user_bindings`, `None`-sentinel skip, unknown-mode skip
  - `export`: `csl_type`/`ris_type` for all remaining entry types, `parse_authors` single-word and empty, RIS editor lines, RIS single-page (no EP tag), CSL-JSON `booktitle` container, CSL-JSON editor, non-numeric year omits `issued`

### 0.54.0

- **Quality section in entry-list help**: `C` (regenerate all cite keys), `M` (name disambiguator), and `v` (validate) are now grouped under a dedicated **Quality** section in the `?` help overlay, separate from general navigation keys
- Fixed a latent bug: the help popup no longer panics when the terminal is smaller than the popup's minimum size — dimensions are clamped to the available area
- Expanded test coverage: 11 new tests for the help component (render smoke-tests, section/key content checks, tiny-terminal robustness, `build_column` edge cases, `HelpContext` clone); `help.rs` line coverage 0% → 98.87%; overall 76.32% → 77.09%

### 0.53.0

- **`F` key in detail view — sync filename to citation key**: renames the attached file(s) on disk so their stem matches the current citation key, updates the `file` field, and marks the entry dirty; works regardless of the `sync_filenames` config setting; supports undo (`u` reverts both the field value and the on-disk rename)
- Expanded test coverage: 6 new tests covering the no-detail, no-file, already-matches, disk-rename, absent-file, and undo paths; overall line coverage 75.73% → 76.32%

### 0.52.0

- **Context-sensitive help modal**: pressing `?` in the entry list shows entry-list navigation, command-palette, and citation-preview keys; pressing `?` from the detail view shows detail-view navigation and the full vim field-editor key reference (insert/replace modes, find/motion/delete operators); the dialog title reflects the active context
- Fixed an incorrect binding in the previous combined help (`a` was listed twice in the detail section; corrected to `N` for normalize names)

### 0.51.1

- **macOS signing identity configurable**: `APPLE_DEVELOPER_NAME` is now a separate repository secret used to construct the codesign identity string, replacing the previously hardcoded name

### 0.51.0

- **Signed macOS binaries**: release builds for macOS (Apple Silicon and Intel) are now code-signed with a Developer ID Application certificate and notarized with Apple's notary service; Gatekeeper will no longer block the binary on first launch

### 0.50.0

- **LaTeX symbol rendering**: `\textregistered` → ®, `\textcopyright` → ©, `\texttrademark` → ™ in all three LaTeX forms (braced, bare with `{}`, and bare); `\textsuperscript{\textregistered}` collapses to just ® (e.g., `MCNP\textsuperscript{\textregistered}` → `MCNP®`)
- **Escaped ampersand rendering**: `\&` now displays as `&` when LaTeX rendering is enabled
- Expanded test coverage: 1234 tests, ~76% line coverage

### 0.49.0

- **Name disambiguator** (`M` on main screen): scans all person-name fields (author, editor, translator, etc.) for similar names using normalized last-name + first-initial grouping and nucleo fuzzy matching, then presents clusters of likely-duplicate names in a scrollable overlay
- **Disambiguator merge workflow**: `Tab`/`Shift-Tab` to cycle the merge target within a cluster, `Enter` to apply all merges (replaces variant names with the selected canonical form across all entries, with full undo support)
- **Disambiguator preview** (`Space`): shows all entries associated with the currently selected name variant, with j/k scrolling; press `Space` or `Esc` to close the preview
- **Disambiguator remove** (`x`): removes the selected variant from a cluster to exclude incorrect matches; clusters with fewer than 2 remaining variants are auto-removed
- Center-scroll behavior in the disambiguator keeps the focused cluster vertically centered

### 0.48.0

- **Shift-Tab reverse cycling**: `Shift-Tab` now cycles backward through tab-completion candidates in field editors, path dialogs, and the `:sort` command palette
- **Smarter file-add autocomplete**: when adding a file attachment (`f`), Tab completion now sorts candidates with directories first, then files whose names do not match an existing citation key, then files that do — within each group, most recently modified files appear first
- **Paste sanitization**: pasting multi-line text into the field editor now converts newlines, tabs, and other control characters to spaces so the text flows into a single line

### 0.46.0

- **JabRef-compatible citation key patterns**: the `[token:modifier]` system now matches JabRef's documented behavior at https://docs.jabref.org/setup/citationkeypatterns; this is a **breaking change** — `[authN]` now means the first N characters of the first author's last name (was first N authors), and `[title]` now capitalizes all significant words and concatenates them (was first significant word only)
- **Three-level template precedence**: citation key patterns are resolved in order: (1) per-type patterns from JabRef metadata in the `.bib` file (`@Comment{jabref-meta: keypattern_article:...;}`), (2) default pattern from `.bib` metadata (`keypatterndefault`), (3) per-type patterns from YAML config, (4) hardcoded default `EntryType_[year]_[auth]`
- **New author tokens**: `[auth.etal]`, `[authEtAl]`, `[auth.auth.ea]`, `[authshort]`, `[authorLast]`, `[authForeIni]`, `[authorLastForeIni]`, `[authorIni]`, `[authIniN]`, `[authN_M]`, `[authorsN]` — all with editor fallback (use `[pureauth*]` variants to skip editor fallback)
- **Editor tokens**: `[edtr]`, `[editors]`, `[edtrN]`, `[edtrN_M]`, `[edtrshort]`, `[edtrForeIni]`, `[editorLast]`, `[editorIni]` — mirror auth tokens but read the `editor` field only
- **New field tokens**: `[entrytype]`, `[lastpage]`, `[pageprefix]`, `[keywordN]`, `[keywordsN]`, `[fulltitle]`, `[camelN]`, `[booktitle]`, `[volume]`, `[number]` (with `report-number` fallback), `[ALLCAPS]` raw field access
- **New modifiers**: `capitalize`, `titlecase`, `sentencecase`, `truncateN`, `(fallback text)` when value is empty
- **Expanded function words**: the skip list for title tokens now uses JabRef's full 50-word list (was 12 words)
- Expanded test coverage: 1193 tests, ~76% line coverage; `citekey.rs` at 97% line coverage

### 0.45.0

- **`\textsuperscript` and `\textsubscript` rendering**: when LaTeX rendering is enabled (`L`), `\textsuperscript{...}` and `\textsubscript{...}` are converted to Unicode superscript/subscript characters in all displayed fields (e.g. `8\textsuperscript{th}` → `8ᵗʰ`)
- **Fix `?` help in detail view**: the help overlay now renders correctly from the detail view; `CloseHelp` restores the previous input mode instead of always returning to Normal

### 0.44.0

- **`Esc` clears confirmed search filter**: after pressing `Enter` to lock search results, pressing `Esc` from the entry list now clears the search filter and restores the full list; a second `Esc` then resets the sort to the configured default as before
- **`:sort none` restores file order**: the special field name `none` skips sorting entirely and returns entries in the order they appear in the `.bib` file (IndexMap insertion order); any active search is re-evaluated against the new ordering so filtered indices stay consistent
- Any `:sort` command executed while a search filter is active now re-runs the search against the new `sorted_keys`, keeping filtered indices valid
- Expanded test coverage: 6 new tests; `app/mod.rs` line coverage 53% → 55%, overall 75.54% → 75.78%

### 0.42.0

- **Vim delete-to / find-to operators**: `t{c}` / `T{c}` move the cursor to just before/after the next/previous occurrence of `c`; `dt{c}` deletes from cursor to (not including) the next `c`; `df{c}` deletes through (including) the next `c`; `dT{c}` / `dF{c}` mirror these backward — all consistent with standard vim behaviour
- Three-key sequences are tracked via a new `second_last_key` field on `App`; non-character keys reset the chain; 3-key matches take priority over 2-key matches in the dispatch table
- `t` and `T` added to the pending-key set so they never fire as single keystrokes

### 0.41.0

- **ESC resets sort in Normal mode**: pressing `Esc` from the entry list restores the sort field and direction to whatever was configured at startup (i.e. the `display.default_sort` value); a status message confirms the reset

### 0.40.0

- **Vim Replace mode (`R`)**: pressing `R` in the field editor's Normal mode enters Replace mode, which overwrites characters in place rather than inserting; each overwritten character is individually reversible with `Backspace` (the original characters are stored on a per-replacement undo stack); `Esc` exits Replace mode and returns to Normal
- The field editor title bar now shows `— REPLACE` when in Replace mode (alongside the existing `— INSERT` indicator)

### 0.39.0

- **Normalize person-name fields** (`a` in detail view): the normalization command now applies to all person-name fields (`author`, `editor`, `editora`, `editorb`, `editorc`, `bookauthor`, `afterword`, `translator`) rather than `author` alone
- Renamed internal action `NormalizeAuthor` → `NormalizeNames`; the keybinding (`a` in detail mode) is unchanged

### 0.38.0

- **Empty `title` / `booktitle` pre-filled with `{}`**: opening an empty title or booktitle field now pre-populates it with `{}` and places the cursor inside the braces in Insert mode, so case-protection is applied automatically without extra keystrokes

### 0.37.0

- **Sort entries by citation key on save**: the `save.entry_sort_order` config option (default `citation_key`) controls the order of entries in the written `.bib` file; this keeps the file consistently ordered regardless of when entries were added or edited

### 0.36.0

- **INSERT mode indicator for blank fields and Add Field**: opening a field that has no existing text now starts directly in Insert mode (rather than Normal mode); the editor title bar shows `— INSERT` to indicate this; the Add Field name-entry step also shows `— INSERT` since it is always in Insert mode

### 0.35.1

- **`sync_filenames` applies to all entries**: previously only dirty (modified) entries had their attached files renamed on save; now all entries with a `file` field are processed on every save, keeping the database consistent regardless of whether the entry was edited in the current session

### 0.33.0

- **CSL-JSON and RIS export**: new `:export-json [path]` and `:export-ris [path]` commands (and bindable `ExportJson` / `ExportRis` actions) serialize all entries to Citation Style Language JSON or RIS format; path dialogs with `Tab` completion are shown when no path is given inline
- **Dirty-entry roundtrip integration test**: new test edits a field via the `Database` API, serializes with `serialize_entry`, rewrites `raw_file`, and verifies the changed field is present in the re-parsed output while all other bytes are identical
- **Expanded test fixtures**: `special_chars.bib` (accents, math, ampersands), `string_macros.bib` (`@String` macro definitions), `multi_file_a.bib` / `multi_file_b.bib` (two-file scenario with overlapping citekeys); 10 new fixture-based roundtrip tests
- **`FieldEditorState::render()`**: the free function `render_field_editor` has been moved into the `FieldEditorState` impl block; call sites updated to `editor_state.render(f, area, theme)`
- Expanded test coverage: 1014 tests, ~77% line coverage

### 0.32.0

- **User-configurable keybindings**: add a `keybindings:` section to `bibtui.yaml` to override or add key bindings on a per-mode basis; use `"None"` to intentionally unbind a built-in key; all action names are documented in `bibtui.yaml.example`
- **JabRef regex keyword group filtering**: keyword groups with `regex: true` in the JabRef `@Comment` block now use real `regex::Regex` matching; previously the flag was parsed but silently ignored
- **App module split**: `src/app.rs` (6 000+ lines) reorganized into `src/app/mod.rs` and `src/app/actions.rs`; no behavior change
- **Library panic fix**: the BibTeX parser no longer panics with `assert_eq!` on unexpected input; the bad byte position is returned as an `anyhow` error instead
- Expanded test coverage: 13 new tests for citekey modifiers, regex group filtering, and keybinding configuration

### 0.31.0

- **Symmetric centered-cursor scrolling in the field editor**: the text cursor now starts at the right edge of the field when editing a long value; pressing `←` moves the cursor left toward the visual centre, then the text scrolls while the cursor stays fixed at the midpoint; the same behaviour applies from the left edge when pressing `→`; this keeps context visible on both sides of the cursor at all times
- **Author initial spacing**: the `a` (normalize author) command now separates run-together initials — e.g. `G.H. Smith` → `Smith, G. H.`
- **Expanded test coverage**: 940 tests, ~77% line coverage; new tests cover `@Comment`/`@Preamble`/`@String` parsing, unterminated-content errors, concatenated field values, JabRef group edge cases (unknown type, no-colon lines, non-numeric depth, lowercase `@comment`), display/unclosed math, trailing script triggers, unclosed text commands, and all keybinding modes

### 0.30.0

- See 0.31.0 (0.30.0 and 0.31.0 were developed together and released as 0.31.0)

### 0.29.0

- **Incremental search in the entry detail view**: press `/` to open a search bar that filters field names and values in real time; matching fields are highlighted; `n` / `N` jump to the next / previous match; `Esc` clears the search (second `Esc` closes the detail view)
- **Keybinding changes in the entry detail view**: `a` now normalizes author names (was `N`); `A` adds a new field (was `a`); `f` adds a file attachment (was `A`)
- **`number` added as an optional field for Book entries**
- **Empty fields in the detail view are now blank** instead of showing a placeholder dot
- **`regex()` modifier requires quoted arguments**: citekey template regex modifiers now require double-quoted pattern and replacement strings, e.g. `[field:regex("\d+$", "")]`; backslash-escaped quotes within strings are supported
- **Citekey template syntax updated to `[token]` form**: all built-in defaults now use the JabRef-compatible `[token]` syntax; legacy `{token}` syntax is still accepted for backward compatibility
- Expanded test coverage (905 tests, ~76% line coverage)

### 0.28.0

- **Auto-regenerate citation key on field edit**: editing any field in the detail view now immediately regenerates the citation key from the configured template — no need to press `c` manually
- **Dirty-flag cleared on full revert**: if a field is edited and then restored to its original value, the entry is no longer marked as modified
- **Citation key sanitization**: generated citation keys now contain only alphanumeric characters, hyphens, periods, and underscores — tildes, apostrophes, colons, and other problematic characters are removed
- **Filename sanitization for sync-filenames**: when `save.sync_filenames` renames attached files to match the citation key, the filename passes through the same sanitizer so keys with special characters produce clean filenames
- **Settings cursor restored on reopen**: the settings view (`S`) remembers the cursor row and restores it the next time the screen is opened
- **Author column width reduced**: default author column is now 20% / max 20 characters (was 25% / 40)
- **ISBN normalization**: the `normalize_isbn` save action now produces properly hyphenated output using registration-agency range data (e.g. `9780374528379` → `978-0-374-52837-9`); ISBNs with invalid checksums are returned unchanged
- Expanded test coverage (905 tests)

### 0.27.0

- **Optional fields always visible in detail view**: all optional fields for an entry type are now shown in the detail view even when not yet populated, making it easy to fill them in without using the add-field dialog
- **New entry dialog shows optional fields**: creating a new entry now displays optional fields alongside required ones in the detail view immediately after creation
- **`type` field added to TechReport**: `type` is now an optional field for TechReport entries (e.g. "Technical Report", "NISTIR")
- **`doi` field added universally**: `doi` is now an optional field for all entry types that previously lacked it (Booklet, InBook, InCollection, Manual, MastersThesis, Misc, PhdThesis, Proceedings, TechReport, Unpublished)
- Expanded test coverage for TUI detail-view component (FileEntry paths, custom group dedup, move-selection edge cases)
