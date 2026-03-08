# bibtui

A terminal UI BibTeX manager written in Rust. Designed as a lightweight, keyboard-driven replacement for JabRef.

## Features

- Fast fuzzy search across all fields with field-specific syntax (`author:smith year:2020`)
- JabRef-compatible group tree (static and keyword groups)
- Byte-perfect BibTeX round-tripping — formatting is preserved for unmodified entries
- Vim-style navigation throughout
- Entry CRUD: add, edit, duplicate, delete with undo (`u`)
- Template-based citation key generation
- Clipboard yank of citation keys
- Per-entry status indicators: `●` unsaved change, `⎘` file attachment, `⎋` DOI/URL
- Open attached files (`o`) or DOI/URL links (`w`) with OS default applications
- Citation preview popup (`Space`) formatted in IEEEtranN style
- LaTeX markup rendered to Unicode for display (`L` to toggle)
- Configurable columns, sort, theme, and citekey templates via YAML
- In-TUI settings editor (`S`) with live config import and export

## Requirements

- Rust toolchain (stable, 1.70+): https://rustup.rs

## Build

```sh
cargo build --release
```

The binary is placed at `target/release/bibtui`.

Optionally copy it to somewhere on your `$PATH`:

```sh
cp target/release/bibtui ~/.local/bin/
```

## Usage

```
bibtui [OPTIONS] [BIB_FILE]

Arguments:
  [BIB_FILE]  Path to .bib file

Options:
  -c, --config <CONFIG>  Path to config file
  -h, --help             Print help
  -V, --version          Print version
```

If no file is given on the command line, bibtui looks for `bib_file` in the config file.

```sh
bibtui references.bib
bibtui --config ~/dotfiles/bibtui.yaml references.bib
```

## Keyboard Reference

### Entry List (Normal mode)

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `gg` | Jump to top |
| `G` | Jump to bottom |
| `Ctrl-D` / `Ctrl-U` | Page down / up |
| `Enter` | Open entry detail |
| `a` | Add new entry |
| `dd` | Delete selected entry |
| `D` | Duplicate selected entry |
| `yy` | Yank formatted citation to clipboard (IEEEtranN style) |
| `/` | Start fuzzy search |
| `h` / `←` | Focus group tree |
| `l` / `→` | Focus entry list |
| `Tab` | Toggle group sidebar |
| `Space` | Select focused group |
| `o` | Open attached file(s) in OS default viewer |
| `w` | Open DOI / URL in default browser |
| `Space` | Citation preview popup (IEEEtranN format) |
| `B` | Toggle case-protecting brace display |
| `L` | Toggle LaTeX rendering (accents, math, dashes) |
| `S` | Open settings editor |
| `u` | Undo last change |
| `:` | Open command palette |
| `?` | Show help |
| `q` | Quit |

### Search mode

| Key | Action |
|-----|--------|
| Type | Append to search query |
| `Enter` | Confirm search / lock results |
| `Esc` | Clear search and exit |

Search syntax:
- Plain text — fuzzy match across all fields
- `field:query` — restrict to a specific field, e.g. `author:smith`, `year:2024`, `title:neural`

### Entry Detail view

| Key | Action |
|-----|--------|
| `j` / `k` | Move field selection |
| `e` / `Enter` | Edit selected field |
| `a` | Add new field |
| `d` | Delete selected field |
| `T` | Convert selected field to title case |
| `N` | Normalize author names to "Last, First" form |
| `o` | Open attached file(s) in OS default viewer |
| `w` | Open DOI / URL in default browser |
| `g` | Edit entry's groups |
| `c` | Regenerate citation key from template |
| `B` | Toggle case-protecting brace display |
| `L` | Toggle LaTeX rendering |
| `u` | Undo last change |
| `Esc` / `q` | Close detail, return to list |

### Field editor (Editing mode)

| Key | Action |
|-----|--------|
| Type | Insert character |
| `←` / `→` | Move cursor |
| `Ctrl-A` / `Home` | Jump to start |
| `Ctrl-E` / `End` | Jump to end |
| `Backspace` / `Delete` | Delete character |
| `Enter` | Confirm edit |
| `Esc` | Cancel edit |

### Settings editor (`S`)

Opens a full-screen view of all configuration options. Changes apply immediately to the running session.

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate settings |
| `Enter` / `Space` | Toggle boolean setting |
| `e` | Edit string setting |
| `E` | Export current config to a YAML file |
| `I` | Import config from a YAML file |
| `Esc` / `q` | Close settings |

Settings marked with `●` differ from their default value.

### Command palette

Open with `:` from the entry list.

| Command | Description |
|---------|-------------|
| `:w` / `:write` / `:save` | Save the file |
| `:wq` | Save and quit |
| `:q` | Quit (warns if unsaved changes) |
| `:q!` | Force quit without saving |
| `:sort <field>` | Sort by field (repeat to toggle direction) |
| `:sort` | Toggle sort direction |
| `:group <name>` | Filter to a named group |
| `:search <query>` | Apply a search query |

Example: `:sort year`, `:sort author`, `:sort title`, `:sort citation_key`

### Group tree

| Key | Action |
|-----|--------|
| `j` / `k` | Move selection |
| `Space` | Apply selected group filter |
| `h` / `l` | Switch focus between groups and entry list |

## Configuration

bibtui looks for a config file in this order:

1. Path given with `--config`
2. `./bibtui.yaml` or `./bibtui.yml` (next to the working directory)
3. `$XDG_CONFIG_HOME/bibtui/config.yaml` (typically `~/.config/bibtui/config.yaml`)

If no file is found, built-in defaults are used.

Copy the annotated example to get started:

```sh
cp bibtui.yaml.example ~/.config/bibtui/config.yaml
```

### Key config options

```yaml
general:
  bib_file: ~/documents/references.bib   # default file when none given on CLI
  backup_on_save: true                    # write .bib.bak before every save

display:
  show_groups: true
  group_sidebar_width: 30
  show_braces: true                       # show/hide case-protecting {braces}; toggle with B
  render_latex: false                     # render LaTeX → Unicode for display; toggle with L
  abbreviate_authors: true                # abbreviate author lists in entry list
  default_sort:
    field: citation_key
    ascending: true

save:
  align_fields: true                      # align field values to a column on save
  field_order: jabref                     # jabref | alphabetical
  sync_filenames: false                   # rename attached files to match citation key on save

titlecase:
  ignore_words: [MCNP, OpenMC]           # words kept verbatim by the T title-case command

theme:
  selected_bg: "#3b4261"
  selected_fg: "#c0caf5"
  header_bg:   "#1a1b26"
  header_fg:   "#7aa2f7"
  search_match: "#ff9e64"
  border_color: "#565f89"
```

```yaml
# Custom field groups in the detail view (fields pulled out of the "Other" section)
field_groups:
  - name: Identifiers
    fields: [isbn, issn, lccn, eprint, archiveprefix, primaryclass]
```

See `bibtui.yaml.example` for all options including columns, citekey templates, and save normalization.

## Running Tests

```sh
cargo test
```

All 80 tests should pass (unit tests, round-trip, parser edge cases, JabRef compatibility, citekey generation).
