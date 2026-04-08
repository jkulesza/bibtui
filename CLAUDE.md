# bibtui — Claude Code Instructions

## Project Overview

Terminal UI BibTeX manager written in Rust. Keyboard-driven replacement for JabRef.

## Build & Test

```sh
# Build release binary
cargo build --release
# Binary at: target/release/bibtui

# Run all tests
cargo test

# Run specific test module
cargo test parser_edge_cases
cargo test jabref_compat

# Coverage (requires cargo-llvm-cov)
cargo llvm-cov --workspace --summary-only
```

All tests must pass before committing. The shell needs `export PATH="$HOME/.cargo/bin:$PATH"`.

## Architecture

```
src/
  main.rs          — CLI parsing (clap), config loading, terminal setup
  app.rs           — App state, event loop, action dispatch
  bib/
    parser.rs      — Custom recursive-descent BibTeX parser (byte-perfect round-trip)
    model.rs       — RawBibFile + Database dual representation
    writer.rs      — BibTeX serialization (preserves formatting)
    citekey.rs     — Template-based citation key generation
    entry_types.rs — Required/optional field definitions per entry type
    jabref.rs      — JabRef group parsing (@Comment blocks)
    normalize.rs   — Field normalization (month, pages)
  config/          — YAML config loading (serde_yaml)
  tui/
    components/    — Reusable UI widgets (dialogs, editors, lists)
    screens/       — Full-screen views (entry list, detail, settings)
    event.rs       — crossterm event handling
    keybindings.rs — Key → Action mapping
    theme.rs       — Color palette
  util/
    author.rs      — Author parsing, abbreviation, normalization
    latex.rs       — LaTeX → Unicode rendering (accents, math, dashes)
    titlecase.rs   — Title case conversion
    citation.rs    — IEEEtranN citation formatting
    clipboard.rs   — Clipboard yank
    open.rs        — OS file/URL opener
  search/          — nucleo-matcher fuzzy search with field:query syntax
tests/
  parser_roundtrip.rs   — Round-trip fidelity tests
  parser_edge_cases.rs  — Edge case BibTeX parsing
  jabref_compat.rs      — JabRef-specific format compatibility
  citekey_generation.rs — Citekey template tests
  fixtures/             — Sample .bib files for testing
```

## Key Design Decisions

- **Dual representation**: `RawBibFile` (byte-perfect passthrough) + `Database` (semantic layer). Unmodified entries are written back byte-for-byte; only dirty entries are re-serialized.
- **Parser**: Hand-written recursive descent, not a library. Preserves all formatting including bare month tokens (`month = apr,`), case-protecting braces, and JabRef `@Comment` group blocks.
- **LaTeX rendering**: Must run BEFORE `strip_case_braces`. In `MATH_SYMBOLS` table, longer patterns must come before their prefix-patterns (e.g. `\infty` before `\in`).
- **TUI**: ratatui + crossterm with a component architecture. Event-driven rendering with viewport culling for performance.
- **`sync_filenames`**: When enabled, renames attached files to match the citation key on every save — applies to **all** entries with a `file` field, not just dirty ones. An entry is marked dirty if its `file` path changed, so it will be re-serialized. The rename target preserves the original subdirectory (e.g. `PDF/old.pdf` → `PDF/citekey.pdf`).

## Config File

Loaded in order: `--config` arg → `./bibtui.yaml` → `$XDG_CONFIG_HOME/bibtui/config.yaml`.

The annotated example is at `bibtui.yaml.example`. A local `bibtui.yaml` in the project root is used for development testing.

## Workflow Rules

- **NEVER commit** unless explicitly told to do so.
- **Before committing**, update `README.md` to reflect any relevant changes.
- **NEVER push** even if I tell you to.
- **Version bumps**: bump the minor version (e.g., 0.1.x → 0.2.0) unless instructed to do otherwise.

## Code Conventions

- Zero compiler warnings expected. Fix all warnings before committing.
- Use `anyhow::Result` for fallible functions; `thiserror` for typed errors in library code.
- `indexmap` for field maps (preserves insertion order for round-tripping).
- Tests use `pretty_assertions` for readable diffs and `tempfile` for temp dirs.
- No `unwrap()` in library code; use `?` or explicit error handling.

## Test Fixtures

- `jabref.bib` — 557-entry production file (5887 lines, 260 KB). Do not commit changes to this file unless intentional.
- `jabref_small.bib` — Smaller subset for faster tests.
- `test.bib` — Scratch file for ad-hoc testing.
- `tests/fixtures/` — Checked-in fixture files for integration tests.

## Vim Modal Editing (field editor)

The field editor (`FieldEditorState`) implements a vim-like modal editor with Normal, Insert, and Replace modes. All vim operations are consistent with standard vim behavior:

- **Mode transitions**: `i`/`a`/`A`/`I` enter Insert; `R` enters Replace; `Esc` returns to Normal (in Insert/Replace → Normal; in Normal → CancelEdit)
- **2-key sequences** (tracked via `last_key: Option<char>`):
  - `r{c}` — replace char at cursor with c
  - `f{c}` / `F{c}` — find char forward/backward (inclusive, cursor lands ON c)
  - `t{c}` / `T{c}` — find to char forward/backward (exclusive, cursor stops BEFORE/AFTER c)
  - `dw` — delete word forward; `yy` — yank whole field value
- **3-key sequences** (tracked via `second_last_key: Option<char>` in addition to `last_key`):
  - `dt{c}` — delete from cursor to (but NOT including) next c
  - `df{c}` — delete from cursor through (including) next c
  - `dT{c}` — delete from (but NOT including) prev c backward to cursor
  - `dF{c}` — delete from (including) prev c backward to cursor
- Key history is maintained in `App.last_key` and `App.second_last_key`. Only `KeyCode::Char` keys advance the chain; any non-char key resets both to `None`.
- The routing in `map_editing_normal_key` checks `(second_last_key, last_key)` tuples for 3-key sequences FIRST, then `last_key` for 2-key sequences, then single-key actions.
- Pending keys (`r`, `f`, `F`, `t`, `T`, `d`, `y`) return `None` from `map_key` to wait for the next keypress.

## Interactive Testing

```sh
./target/release/bibtui jabref.bib
./target/release/bibtui --config bibtui_test.yaml jabref_small.bib
```
