# Changelog

### 0.65.0

- **Yank attached files** (#61): the `yy` picker gains **Associated File(s)**, which puts the entry's attached file(s) themselves on the clipboard (all of them when there are several), so they paste into an email as attachments or into a file manager as copies. Missing attachments are skipped and counted in the status message. macOS writes file URLs to the general pasteboard; Linux writes `text/uri-list` via `wl-copy` or `xclip`. `yank_format: file` copies the files directly without the picker
- **Picker hotkeys**: in the `yy` picker, `f` formatted citation, `b` BibTeX entry, `a` associated file(s), and `c` citation key pick and copy immediately; the key is highlighted in each label and listed on the dialog's bottom border. Arrow keys / `j`/`k` + Enter still work
- **New default order**: Formatted citation, BibTeX entry, Associated File(s), Citation key
- **Most-used first**: picks from the `yy` picker are counted and persisted (`$XDG_STATE_HOME/bibtui/usage.yaml` on Linux, `~/Library/Application Support/bibtui/usage.yaml` on macOS), and the picker lists the most frequently used choices first; ties keep the default order

### 0.64.1

- **Windows CI**: JabRef round-trip test fixtures are checked out byte for byte on every platform (`.gitattributes` disables line-ending conversion for them), so the byte-exact JabRef compatibility tests no longer fail on Windows runners

### 0.64.0

- **Rename groups from the group pane**: select a group and press `e` to edit its name in place (prompt pre-filled with the current name). Renaming a static group also renames it in the `groups` field of every member entry, replacing only the name so each field keeps its separators and spacing; renaming a keyword group changes only its name. The rename is one undoable step and keeps JabRef-only group details (color, icon, description)
- **Safe group names**: names containing `,`, `;`, or `\` (JabRef separators) are refused when adding or renaming, as are names another group already uses. Renaming is refused when another static group shares the name and entries are assigned to it, since those entries cannot be attributed to either group
- **Help**: `?` lists the group-pane keys in a new Groups section
- **Fix**: the group tree is written back in JabRef's exact layout (closing brace on its own line), so group edits no longer change the block differently from JabRef and undo restores the file byte for byte; files written by older versions are still read correctly
- **Revert citation preview width-cap**: the citation preview uses the pre-v0.61.6 behavior of fixed-width fraction capped at 90 columns

### 0.63.0

- **Library settings stored in the `.bib` file**: settings in a library now take precedence over the YAML config (built-in defaults < YAML < library), so a library behaves the same on every machine. JabRef-shared capabilities use JabRef's own metadata keys: citation-key templates as `keypattern_<type>`, entry order on save as `saveOrderConfig`, and save actions JabRef also has as `saveActions` (so JabRef applies them too). Everything else is stored as `@Comment{jabref-meta: bibtui.<setting>:<json>;}`, which JabRef keeps when it saves; verified with JabRef 5.15, which rewrites bibtui-written settings byte for byte
- **Existing JabRef `saveActions` and `saveOrderConfig` now take effect**: a library that already has them overrides the matching YAML settings, and bibtui says so at startup. JabRef's `all-text-fields[identity]` placeholder is ignored; configurations bibtui cannot represent are read as far as possible and never rewritten
- **Settings editor**: `◆` marks settings that come from the library. Editing one updates the library as an undoable edit saved with `:w`; editing any other setting changes the YAML layer. `B` writes settings into the library after previewing every metadata line to be added, changed, or removed
- **Commands**: `:settings-export bib`, `:settings-export yaml [path]`, and `:settings-clear-bib` (removes bibtui's keys, keeps JabRef's)
- **YAML export (`E`) writes only the YAML layer**, so library settings never leak into a global config; YAML import still lets library settings win
- **Fix**: JabRef's `\;` escaping is now undone in `keypattern_*` values

### 0.62.1

- **Windows saves**: the library is replaced with a POSIX-semantics rename, so saving works even while another program holds the `.bib` open with delete sharing
- **Attachment paths keep their separators**: renaming an attachment replaces only the file name in the stored path, so `PDF/old.pdf` becomes `PDF/Smith2020.pdf` on Windows too instead of `PDF\Smith2020.pdf`
- **`~` expansion** uses the native separator after the home directory and accepts `~\` on Windows
- **CI**: fixed Clippy failures in Linux-only clipboard code

### 0.62.0

- **Safer saving**: saves write an exclusive temporary file next to the library and rename it into place, keep the existing file's permissions, follow symlinks without replacing them, and leave in-memory state untouched when a write fails so the save can be retried. A failed `:w`/`:wq` keeps bibtui open
- **External changes detected**: if the `.bib` file changed on disk since it was loaded or last saved, `:w` is refused instead of overwriting it. New `:w!` / `:wq!` overwrite deliberately, first copying the external version to `.bib.bak` even when backups are disabled
- **Attachment renames on save follow the final citation keys**: the rename preview is built from the fully normalized, rekeyed library and confirming executes exactly that plan. Renames never overwrite an existing file (atomic no-replace rename on macOS/Linux, with fallbacks for filesystems without hard links), case-only renames work on case-insensitive filesystems, and completed renames are reversed if the save fails
- **Missing or remote attachments no longer block saving**: attachments missing from disk or stored as `https://` links are skipped and listed in the status line instead of failing the whole save
- **Undo fixes**: editing after undoing past a save now marks the library unsaved; undoing a deletion or key change after a save is persisted correctly; keys regenerated during save are one undo step; partial attachment-undo failures are all reported
- **Key changes update `crossref`** fields that reference the old key, in the same undo step. Rekeyed entries keep `@String` references and `#` concatenation in unchanged fields
- **Blank lines inside entries, comments, and `@String`/`@Preamble` blocks are preserved** on save; only blank lines between items are normalized
- **Search**: space-separated terms each take their own qualifier (`author:smith year:2020`), `"quoted phrases"` match exactly, and search combines with the selected group. Libraries over 500 entries search in a background thread with cached entry text; current results stay visible while searching
- **Sorting** is a consistent order: numeric for `year`, `volume`, `number`, and `pages`, text for everything else
- **Imports**: one HTTP client with connect/read/request timeouts and a metadata size cap, using the OS trust store via native TLS (rustls on musl). PDF downloads stream to a temporary file with a configurable size limit (`import.max_pdf_size_mb`, default 100) and a separate 10-minute limit; re-importing reuses an identical file or picks a `_2` name instead of failing. Imported attachment paths escape `:` and `;` correctly
- **Unicode crash fixes** in PDF and publisher-page DOI scanning and in path tab completion
- **Input**: key-release events are ignored and held-key repeats only apply to navigation and typing, so terminals that report releases no longer double-fire actions
- **Groups** with the same name in different subtrees filter independently
- **Performance**: bulk citation-key regeneration ~50× faster and frame rendering ~4.6× faster on a 20,000-entry library; saves copy the database fewer times
- **Quality**: CI tests on Linux, macOS, Windows, and the minimum Rust version (1.88), enforces rustfmt, Clippy, and coverage floors; formatting commits are listed in `.git-blame-ignore-revs`

### 0.61.9

- **Fix citekey help regex example**: the Settings citekey template reference showed `[auth][year:regex("^..",...)]`, where `...` was a placeholder rather than valid syntax, so typing it literally silently did nothing. It now shows the working `[auth][year:regex("^..","")]` → `Smith24`, and a test asserts the documented output

### 0.61.8

- **Dialog list navigation fixed**: `gg`, `G`, `Home`, `End`, `PageUp` and `PageDown` in list dialogs (e.g. Assign Groups) moved the underlying detail-view field instead of the dialog selection. They now move the dialog selection, and a single `g` waits for the second `g` instead of jumping immediately

### 0.61.7

- **Correctly format name suffixes in IEEEtranN citations** (#60): `Doe, III, John` rendered as "I. J. Doe" in the citation preview and spacebar-copy. Suffixes (Jr., Sr., II–VI) are now recognised in `Last, Suffix, First`, `Last, First, Suffix`, and `First Last Suffix` forms and rendered as "J. Doe, III"

### 0.61.6

- **Citekey editor uses the full terminal width** (#59): the field editor overlay was capped at 70 columns, truncating long citekey templates. It now spans the terminal width (minus a 2-column margin each side), as does the citekey template reference panel beneath it
- **Other popups no longer width-capped**: the citation preview, name disambiguation, and validate results popups now use the full terminal width instead of fixed fractions capped at 90–110 columns
- **Citekey help panel shows examples in full**: the example pattern column is sized to the longest pattern (shrinking only when needed so results like `→ SmithJonesWilliams2020` stay visible), the left column no longer takes a fixed half of the width, and the panel is tall enough to list all examples

### 0.61.5

- **Sorting no longer clears an active group filter** (#57). `:sort` re-sorted the entry list correctly, but its filter refresh only accounted for an active search query — with a group filter (and no search) it reset `filtered_indices` and silently showed every entry. `:sort` now re-applies the active group filter against the newly sorted list instead
- **Ad-hoc sort-column preview** (#57): sorting by a field that isn't one of the configured columns now shows that field's values in a temporary column on the right, with a ↑/↓ direction indicator in the header, so the sort is visually confirmable. It's derived fresh from the current sort state on every render, so it disappears on its own once the sort changes to a visible field or is cleared

### 0.61.4

- **Fix stale key labels in the Detail View `?` help panel**: "add field" was shown under `a` instead of `A`, "add file attachment" under `A` instead of `f`, and "normalize names" under `N` (actually bound to jump-to-previous-search-match) instead of `a`. The README's Detail view table already had these right; only the in-app help text had drifted

### 0.61.3

- **`t` in the entry Detail view changes the entry's type** (#58), e.g. Article → InProceedings. Opens a type picker pre-selected to the entry's current type; changing it re-categorises required/optional fields in the detail view and is undoable with `u`. This capability already existed as of 0.34.0 but was undocumented — added to the README's Detail view table and the in-app `?` help panel, and covered with new tests (type picker pre-selection, applying a change, no-op on re-selecting the same type, undo)

### 0.61.2

- **Delete dialog sizes itself to its content** (#56). The confirmation shown when deleting an entry was pinned to 40 columns no matter how wide the terminal was, clipping the attached filename in `Delete entry + {file}` and long citation keys in `Delete '{key}'?`. Both variants now grow to fit their widest row and their title, capped at the terminal width. The multi-file checkbox variant also accounts for its `Delete '{key}'` title, which it previously ignored in favour of filename width alone
- **Dialog sizing is now testable**: the width and height rules moved out of the `render_dialog` match arms into pure `dialog_width(kind, area_width)` and `dialog_height(kind, width, area_height)` functions, so terminal-size behaviour can be asserted without constructing a `Frame`
- **Long confirm messages wrap instead of being cut off**: when a message still exceeds the terminal after the dialog has grown, the box grows vertically rather than clipping at its fixed 5 rows. Rows too long for a list dialog get an ellipsis via the existing `truncate_to` instead of a mid-character clip
- **Dialog polish**: the entry-type picker no longer leaves two empty rows below its last option (height is options + borders), and the yes/no confirm message gets the single column of horizontal padding its width already reserved, instead of sitting flush against the border
- **25 new tests**: 11 sizing cases in `dialog.rs` (content growth for message/title/options, terminal-width cap, minimum widths, vertical growth on wrap, tiny-terminal underflow), 3 render assertions that the full filename and full citation key actually appear in the rendered buffer, and 11 covering previously untested `main_screen` render branches — search-filtered lists including stale out-of-range indices, the hidden-sidebar layout, search and command modes, and the field-editor, citation-preview, validate-results, name-disambiguation, and help overlays. `main_screen.rs` coverage rises from 65% to 100%

### 0.61.1

- **New `trim_whitespace` save action** strips leading/trailing whitespace from field values (`{University of Texas }` → `{University of Texas}`). Unlike the other text actions it applies to **every** field, not just the title/name lists, so padding on `doi`, `file`, `isbn`, and custom fields is cleaned too. It runs last in the pipeline so it also removes padding the earlier actions leave behind (e.g. `latex_cleanup` collapsing the `"  "` of `"  Foo"` to a single leading space). Only the ends are touched — a wrapped multi-line value keeps its internal newlines and indentation. Enabled by default; disable with `save_action_trim_whitespace: false` or the toggle in the `S` settings editor. The `v` validate dry-run attributes a change to `trim_whitespace` only when the rest of the pipeline left the field alone
- **15 new tests**: 8 unit cases for the trim itself (leading, trailing, both sides, tabs/newlines, internal whitespace preserved, all-whitespace → empty, idempotence), 5 app-level (padding stripped on save, disabled config preserves padding, validate labels, validate predictions matching what save writes, ordering against space collapsing, end-to-end write to disk), and 2 settings-screen regression tests pinning the positional Save Actions row indices so an item inserted earlier can no longer silently repoint a row

### 0.61.0

Structural cleanups and UX polish, completing the full-codebase review begun in 0.60.2.

- **Batch undo**: bulk name disambiguation and regenerate-all-citekeys revert with a single `u` instead of one field or key at a time
- **Bare paths accepted in the `file` field** (`file = {paper.pdf}`), not just JabRef's `desc:path:TYPE` format
- **Deleting a group offers to strip its name from member entries** (undoable as one step); declining keeps the old leave-in-place behavior
- **Same-named groups in different subtrees** now filter correctly: sidebar selection resolves by tree path instead of first name match
- **JabRef 5 group metadata preserved**: color/icon/description fields and unknown group types (e.g. `SearchGroup`) round-trip verbatim instead of being dropped on any group edit
- **Imported DOIs keep their exact form**: URL cleanup (trailing-slash trim) applies to `url` only
- **Validate-results scrolling uses the real viewport height** instead of a hardcoded 24 rows
- **Validate and save share one pipeline**: the Validate popup's predictions now exactly match what a save produces, by construction
- **Numeric-aware sorting**: `year`, `volume`, `number`, and `pages` sort numerically ("9" before "10"; page ranges by leading number)
- **Panic hook restores the terminal**, so a crash no longer leaves the shell in raw mode on the alternate screen
- **Background DOI/import fetches no longer redraw the UI ~10×/s** while pending
- **Refactors**: `confirm_edit` if-chain converted to a match with per-action methods; name disambiguation extracted to its own module; duplicated helpers (entry-key lookup, entry-type list, rename planning) consolidated
- **Test suite grows to 1,556 tests**; clippy remains warning-free

### 0.60.3

Correctness fixes with smaller blast radius, continuing the full-codebase review.

- **`#` concatenation and `@String` references survive edits**: re-serializing a dirty entry reuses original source text for unchanged fields, so `journal = ieee_tps # {, Part B}` is no longer flattened by an unrelated field edit
- **`@String` and `@Preamble` items round-trip byte-perfectly** (case and spacing preserved); `@String` with a missing `=` now errors instead of silently mis-parsing
- **Malformed entries no longer abort loading**: the parser skips to the next `@`-item, preserves the bad span byte-for-byte, and reports a warning in the status bar
- **Duplicate citation keys are uniquified at load** (`_dup2`, …) instead of silently dropping earlier copies; the startup warning reports the renames
- **Saves are atomic**: written to a temp file and renamed into place, so a crash or full disk mid-save cannot truncate the library
- **`field_order` defaults to `jabref`** so a default-config save no longer rewrites the whole file; unrecognized `field_order`/`entry_sort_order` config values now produce startup warnings; the example config's `alpha` typo corrected to `alphabetical`
- **`:q` with unsaved changes opens a confirm dialog**; a dialog confirm with no pending action can no longer quit the app
- **Normalization edge cases**: escaped `\$` no longer flips math-mode underscore escaping, page normalization only converts simple ranges (and handles en dashes) instead of every hyphen, blank-line collapsing is CRLF-aware
- **`cleanup_url` documentation corrected** to match its behavior (trims a trailing slash; percent-encoding preserved)

### 0.60.2

Data-loss and data-corruption fixes from a full-codebase review.

- **Filename sync no longer overwrites existing files**: renaming attachments to match citation keys skips (and reports) targets that already exist instead of silently replacing them
- **Author-name normalization no longer corrupts names**: "von" particles attach to the last name (`van Rossum, Guido`), names with suffixes (`Jr.`, `III`) are left untouched instead of mangled, and brace-protected corporate names (`{U.S. Department of Energy}`) pass through unchanged
- **`file` field round-trips safely**: `:`, `;`, and `\` in descriptions and paths are escaped on write and unescaped on read
- **Add/Duplicate Entry no longer overwrite existing entries** on key collision: colliding keys get a numeric suffix (`New_Article_2`, `key_copy_2`)
- **`~/path` values now work** for the config `bib_file` and CLI argument

### 0.60.1

- **Fix titlecase skipping the first and last words of brace-wrapped fields**: a value enclosed in a single balanced brace pair (e.g. created by pasting into an empty field, which pre-fills protective braces) is now unwrapped, titlecased, and re-wrapped — previously the tokens carrying the outer braces (`{Discrimination`, `alanine}`) were mistaken for case-protected words and passed through unchanged. Interior protection groups (`{Monte Carlo}`), adjacent groups (`{MCNP} and {OpenMC}`), double wrapping, and unbalanced braces all keep their previous behavior
- **5 new tests** for the outer-brace unwrap (full wrap, inner protected group, recursive double wrap, adjacent-groups non-trigger, unbalanced fallback)

### 0.60.0

- **Opening a nonexistent .bib path now starts a blank library** instead of erroring: the status bar shows "New file: <path> (created on first save)" and the file is written on first save — works for both the CLI argument and the config-default path
- **Multi-line paste is now handled correctly**: bracketed paste is enabled, so pasting a multi-line string (e.g. a title copied from a PDF) arrives as a single event with newlines collapsed into single spaces — previously the first newline acted as Enter, confirming the edit with only part of the string. Pastes route to the active input (field editor, search, detail search, or command palette); the field editor's `p` clipboard paste applies the same newline collapsing
- **13 new tests**: blank-library open + save-creates-file; paste into Insert/Normal editor modes, search, and command palette; paste ignored in Normal mode; multi-line clipboard `p`; `collapse_newlines` unit cases (CRLF, bare CR, blank-line trimming, no-newline passthrough)

### 0.59.0

Full-codebase review release: correctness fixes, a large module refactor, testability abstractions, and expanded coverage.

- **Fix save corruption with `entry_sort_order: none`**: `sync_dirty_entries` left `raw_index` stale across saves — *add → save → edit → save* inserted a duplicate entry instead of updating in place, and *delete → save → delete → save* removed the wrong entry. The sync now runs in four phases (in-place updates, removals, insertions, then an unconditional `raw_index` rebuild). The default `citation_key` sort order masked the bug
- **Fix `duplicate_entry()` overwriting the original on save**: the copy shared the original's `raw_index`, so saving after duplicating replaced the original entry on disk; the copy now gets its own raw slot
- **Parser: stray `@` in inter-entry text no longer aborts loading**: a comment line like `% maintained by jane@example.org` previously failed the whole file; such text is now passed through byte-perfectly. Genuinely malformed entries (unterminated braces) still error
- **Duplicate citation keys are now detected**: recorded in `Database::duplicate_keys` and surfaced as a status-bar warning on load (the raw file keeps both copies, as before)
- **Explicit `--config` path that doesn't exist now errors** instead of silently falling back to the implicit search paths
- **Search: URLs are no longer misparsed as field filters**: `https://...` in a query no longer triggers `field:query` syntax
- **Removed the only `unsafe` block** (`writer.rs` UTF-8 conversion) and a dead fallback branch in keyword-group filtering
- **Module split**: `app/mod.rs` (8,742 lines) split into `editing.rs`, `save.rs`, `groups.rs`, `import.rs`, `completions.rs`, and `tests.rs` (mod.rs now 2,483 lines); no behavior change
- **Testable clipboard/opener**: new `Clipboard` and `Opener` traits with system impls; `App` holds injectable boxed instances, so yank/open logic is now covered by 14 mock-based tests
- **`main.rs` cleanup**: now consumes the lib crate instead of re-declaring every module (the crate was compiled twice and every lib test ran twice); bib-path resolution extracted into a tested `resolve_bib_path()` and clap parsing covered by tests
- **Clippy clean**: `cargo clippy --all-targets -- -D warnings` passes (was ~75 warnings); `EntryType::from_str` renamed to `EntryType::parse`, dead `RawEntry`/`RawField` formatting fields removed, manual clamp patterns fixed
- **Expanded test coverage**: 1405 → 1461 tests; line coverage ~81 % → ~87 % — save pipeline (previously 0 executions), group management round-trips, import-result handling, `sync_filenames`, and ratatui `TestBackend` render smoke tests (`settings_screen.rs` 0 % → 98 %, `dialog.rs` 65 % → 98.6 %)

### 0.58.1

- **Dependency security update**: `openssl` 0.10.78 → 0.10.80, `openssl-sys` 0.9.114 → 0.9.116 — fixes three rust-openssl advisories: undefined behavior in `X509Ref::ocsp_responders` for certificates with non-UTF-8 OCSP URLs (High), heap buffer overflow when encrypting with AES key-wrap-with-padding (Moderate), and potential out-of-bounds write in `CipherCtxRef::cipher_update_inplace` for AES-KW-PAD ciphers (Moderate)

### 0.58.0

- **Expanded test coverage**: 80.11 % → 82.10 % regions (78.98 % → 81.09 % lines, 87.46 % → 89.69 % functions); 1296 → 1405 tests
- **field_editor.rs** 74.43 % → 88.10 %: 37 new tests covering vim 3-key delete sequences (`dt{c}`, `df{c}`, `dT{c}`, `dF{c}`), `dw`, `t{c}`/`T{c}` find-to-char, `p` put with cursor clamping, undo stack (capped at 50, Normal-mode clamping), Replace mode push/backspace/append semantics, and the `clamp_normal` / `is_word_char` / ghost-text helpers
- **entry_detail.rs** 73.49 % → 82.22 %: 12 new tests covering the in-detail search (`/`, `n`, `N`) — `push_search_char`, `search_backspace`, `clear_search`, case-insensitive matching, field-name vs field-value matches, and `next_match` / `prev_match` wrap-around
- **settings.rs** 74.14 % → 80.21 %: 23 new tests covering `format_width_spec` / `parse_width_spec` round-trips and error-defaulting paths, column add / delete / set, and `current_section`
- **app/mod.rs** 60.78 % → 62.65 %: 18 new tests covering `parse_field_header`, `sort_field_candidates`, `action_label_for_field` (all priority branches), `collect_group_names` (skips `AllEntries`, includes nested), and `find_group_node` / `find_group_node_mut` (path navigation)

### 0.57.0

- **Manual filename sync** (`F` key in entry list): new Quality action that previews all files that would be renamed to match their citation keys in a scrollable `old → new` dialog before applying — works regardless of the `sync_filenames` config setting; shows "already in sync" status when nothing needs renaming
- **4 new tests** covering the `SyncFilenames` action: force-bypass of config guard, no-file-fields status message, pending-file dialog appearance, and `compute_sync_renames(false)` config-disabled path

### 0.56.3

- **Fix citekey uniqueness logic**: `regen_citekey` (single `c` key) now resolves collisions before inserting — previously a generated key that matched an existing entry would silently overwrite it; `regen_all_citekeys` (`C` key and auto-regen on save) no longer bumps an entry like `Key_2` to `Key_3` when `Key` is taken — the current entry's own slot is correctly recognised as free during the suffix search
- **6 new tests**: `unique_citekey` free-base, current-key-counts-as-free, collision-gets-suffix, suffix-slot-is-current-key (the exact bug scenario), `regen_citekey` collision resolved with suffix, `regen_all_citekeys` preserves suffix without spurious bump

### 0.56.2

- **Settings description wrapping**: description box now sizes itself to the text (1–4 inner lines) so long descriptions are shown in full rather than clipped; continuation lines are indented by one space to stay visually flush with the first line
- **6 new tests** for the `wrap_text` helper (single line, wrap at width, prefix on every line, empty input, max-line cap, zero width)

### 0.56.1

- **Fix false "modified" indicator on `dirty` column**: default header was `" "` (space) while configs use `""` (empty string); the Settings column row now correctly shows unmodified when both are blank
- **Settings column default hint**: column rows now show `default: <width>` (matching the item-row style) instead of the static format hint; falls back to the format hint for user-added columns with no default

### 0.56.0

- **Richer default citekey templates**: all entry types now use field-aware templates matching the `bibtui.yaml` reference config — `article` includes journal abbreviation, author list, and pages; `book` appends a camel-cased short title; `techreport` includes institution abbreviation and report number; `inproceedings`/`proceedings` include booktitle abbreviation; `mastersthesis` → `MS-Thesis_…`, `phdthesis` → `PhD-Thesis_…`; `misc` encodes howpublished and title; multi-author tokens use `[authors2]` (first two names + EtAl when more)
- **Default column layout**: replaced the `journal` column with `citekey` (15 % width) to match the reference config

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
