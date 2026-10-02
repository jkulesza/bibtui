Review of **bibtui 0.61.9, revision `912414e`**, performed October 1, 2026.

The code has a useful separation between raw BibTeX, semantic entries, UI components, and application actions. Pure transformations and widget operations have extensive tests. The highest-priority improvements are in saving, undo, entry identity, and filesystem operations: individually tested operations become incorrect when combined. Address those before adding features or chasing a higher aggregate coverage percentage.

Production source was left unchanged. The accompanying probes run in a disposable source copy. Existing untracked bibliographies, PDFs, and configuration files were left untouched.

**Measured baseline.**

| Check | Result |
|---|---|
| `cargo test --locked` | 1,611 passed: 1,493 library, 8 binary, 110 integration; no ignored tests |
| Fresh `cargo llvm-cov --workspace --summary-only` | 88.44% lines, 89.15% regions, 92.29% functions |
| Branch coverage | Not collected; the report's branch columns are empty |
| `cargo fmt --all -- --check` | Fails across multiple files |
| `cargo clippy --locked --all-targets -- -D warnings` | Fails at `src/app/save.rs:423`, `clippy::drain_collect` |
| Added review regression probes | All 18 fail against this revision, demonstrating the defects below |

Tests, Clippy, and release measurements used Rust 1.98.1 on x86_64 macOS. Coverage used the already-installed Rust 1.93.1 with its matching LLVM 21 tools; the default Homebrew toolchain lacked `llvm-tools-preview`. Both toolchains passed the existing suite. The README's 1,598-test count is stale. No live publisher-service validation, interactive terminal session, or Windows/Linux execution was performed. HTTP findings below come from implementation and locked dependency source inspection.

The [complete coverage summary](coverage-summary.txt) is included. Aggregate coverage includes inline test code; it is not a production-only percentage. Dedicated `app/tests.rs` is absent from the report. High module percentages therefore need to be interpreted alongside application-path coverage:

| Area | Line coverage | Interpretation |
|---|---:|---|
| `app/editing.rs` | 22.97% | Many confirmation paths, attachments, settings, and editor action routing are not exercised |
| `app/mod.rs` | 62.56% | `handle_key` and the real event loop have no hits |
| `app/completions.rs` | 67.33% | Path completion application is untested |
| `app/import.rs` | 68.20% | Background import execution is untested |
| `app/groups.rs` | 72.70% | Several group UI workflows are untested |
| `app/save.rs` | 90.71% | Good line coverage, but missing operation sequences and partial failures |
| `bib/parser.rs` / `bib/writer.rs` | 96.89% / 98.01% | Strong fixture coverage; application save behavior still has fidelity defects |
| `tui/components/field_editor.rs` | 95.43% | Widget tests do not establish correctness of application integration |
| `tui/mod.rs` / `tui/event.rs` | 0% / 0% | Terminal setup, restoration, and polling need a small system test layer |

**Implementation tasks, in priority order.** P1 means potential data loss, broken persisted references, or filesystem damage; P2 means functional correctness or responsiveness. Locations refer to the reviewed revision. Each task is intended to be implemented and reviewed separately. Use the named probes from [regression_probes.rs](regression_probes.rs) as starting tests, then add the acceptance cases listed below. Do not merely change the assertions to match current behavior.

**R01 — P1: Keep the app open when saving fails.** Locations: [save.rs](../src/app/save.rs), `request_save` at line 245 and `save` at 263; [mod.rs](../src/app/mod.rs), `PendingAction::SaveAndQuit` at 1874.

`save()` returns `()`, while both save-and-quit paths unconditionally set `should_quit`. A failed write or backup therefore discards the user's opportunity to recover unsaved edits.

1. Return a typed success/error result from saving; let the UI turn errors into status messages.
2. Set `should_quit` only after successful persistence, including the filename-preview confirmation path.
3. Keep the document dirty after failure and allow retry or export to another path.

Acceptance: `review_failed_wq_must_stay_open`; additionally test backup failure, final replacement failure, preview-confirmed failure, successful `wq`, and a successful retry. Inject I/O failures instead of relying exclusively on Unix permissions.

**R02 — P1: Fix the saved-state marker after branching undo history.** Location: [mod.rs](../src/app/mod.rs), `push_undo` at 2007 and `undo` at 2018.

Reproduction: edit → save → undo → make a different edit. The new undo depth equals the saved depth, so `dirty` becomes false despite different content. `:q` can then exit without confirmation.

Before pushing a new undo item, invalidate `save_generation` if the current depth is below its saved value: the saved branch has been abandoned. A subsequent successful save establishes a new marker. Preserve the existing capped-stack eviction behavior. Keep this initial fix small; explicit document revision IDs are a possible later design improvement.

Acceptance: `review_edit_after_undo_saved_state_must_be_dirty`; also test returning to the saved state by undo, multiple branch edits, batch undo, stack-cap eviction, and failed saves. Assert quit confirmation as well as `dirty`.

**R03 — P1: Restore valid raw bindings and dirty state when undoing across saves.** Location: [mod.rs](../src/app/mod.rs), `undo_apply` at 2056, especially `EntryDeleted` and `CitekeyChanged`; [save.rs](../src/app/save.rs), `sync_dirty_entries` at 370.

Deleting A, saving, undoing, and saving again displays A in memory but omits it from the file. Restored snapshots retain an obsolete `raw_index` and can be clean. Citation-key undo after a save likewise restores the key in memory but leaves the saved key on disk. Further edits to a restored entry can target another entry's former slot.

1. On restoring a deleted entry, find its current raw slot by identity/key if it still exists; otherwise use the new-entry sentinel and mark it dirty.
2. On undoing a key change, retain the current entry's valid raw binding and mark the restored snapshot dirty relative to the persisted document.
3. Reconcile raw bindings after every layout change. Do not trust a historical array index as an entry identity.
4. Cover save-time key regeneration interacting with older field-change undo records; either remap their keys or introduce stable entry IDs in a separate follow-up. A popped undo item must not silently do nothing because its key changed.

Acceptance: `review_delete_save_undo_save_must_restore_entry`, `review_key_change_save_undo_save_must_restore_key`; add deletion → save → undo → edit → save, sorted and unsorted saves, adjacent deletions, and repeated reloads. Assert every entry's contents and count, not just its in-memory presence.

**R04 — P1: Make attachment renames and undo consistent with actual filesystem results.** Locations: [save.rs](../src/app/save.rs), `sync_filenames` at 14, `sync_entry_filename` at 83, save ordering at 263; [mod.rs](../src/app/mod.rs), `FilenamesSynced` at 2157.

Three reproduced failures:

- For `one.pdf; two.pdf`, if `A_2.pdf` already exists, the first rename succeeds but the detail action returns before updating the file field or recording undo. The entry still points at missing `one.pdf`.
- After syncing `old.pdf` to `A.pdf`, create a different `old.pdf` and undo. The unchecked reverse rename overwrites that new file on macOS.
- With sync and key regeneration enabled, saving renames to the old key and only then generates the final key. The file is named `A.pdf` while the entry becomes `New2020`.

Implement this in three small changes. First, record each successful rename and update its field path even when a later rename fails; retain errors alongside successful results. Second, apply no-overwrite conflict handling during undo, and update each reverse path only on successful reversal. Third, construct one save plan in the order normalize fields → assign final unique keys → plan attachment renames. Use that exact plan for preview and execution. Do not silently retarget missing source files. Maintain global dirty state for manual bulk sync, and do not replace error messages with unconditional success text.

Before any file mutation, complete serialization and backup preparation. If bibliography persistence subsequently fails, reverse successful renames when possible and report any recovery failure without pretending disk and memory agree. Filesystem-wide crash atomicity is a separate, larger problem; these changes should at least handle ordinary failures correctly.

Acceptance: `review_partial_attachment_failure_must_keep_paths_consistent`, `review_attachment_undo_must_not_overwrite_new_file`, `review_save_filename_must_match_final_citekey`; add missing sources, shared attachments, failed reverse renames, destination conflicts, backup/write failure after planning, and preview-to-execution equality. Retain unrelated destination bytes in every case.

**R05 — P1: Recompute visible entries after mutations.** Locations: [mod.rs](../src/app/mod.rs), `update_search` at 1149, selection at 1211, deletion at 1418, duplication at 1435; [groups.rs](../src/app/groups.rs), filtering; import and key-regeneration completion paths.

`filtered_indices` indexes `sorted_keys`, but mutations replace `sorted_keys` without rebuilding the filter. With A/B/C sorted and `key:B` active, deleting B makes C appear as the matching entry. Subsequent operations can target that unrelated entry.

Create one `refresh_view` operation that rebuilds sort and filters after every mutation, preserves selection by entry key when possible, and otherwise clamps it. Explicitly represent the active group and search query and apply both consistently. Preserve group identity by tree path/ID: storing only the name also loses which identically named group was selected when sorting reapplies it. A vector of visible entry identities is safer than persistent offsets into a changing vector.

Acceptance: `review_filter_must_remain_valid_after_delete`; repeat for add, duplicate, import, rename, undo, changed searchable fields, active keyword groups, same-named groups, and empty results. Assert the selected key and displayed result count.

**R06 — P1: Preserve raw field expressions when a citation key changes.** Location: [save.rs](../src/app/save.rs), original-entry lookup around line 393.

The raw entry is used only when its old citation key equals the current one. A key change therefore discards the original field forms: `journal = j` becomes `journal = {j}`, and concatenations become braced literals. Automatic key regeneration is enabled by default, so an ordinary save can trigger this.

Preserve the original raw-entry association through a key change and use it for unchanged-field comparisons. Changing the key must not invalidate the field-expression baseline. Fix R03 first so removing a key-equality guard does not conceal an invalid raw index. Keep changed fields serialized normally.

Acceptance: `review_key_rename_must_preserve_raw_field_expressions`; add manual/automatic rename, duplicate-key repair, quoted values, concatenation, and two consecutive saves. Reparse and assert `RawFieldValue` variants, not only displayed text.

**R07 — P1: Update internal bibliography references during key changes.** Location: [editing.rs](../src/app/editing.rs), `regen_citekey` at 624 and `regen_all_citekeys_impl` at 665.

Renaming a proceedings entry from `Parent` to `New2020` leaves a child's `crossref = {Parent}` unchanged. Save-time regeneration can break existing libraries on first save.

Centralize renaming. Build an old-to-final-key map, then update exact `crossref` references once using the original values, mark changed entries dirty, and include those updates in the same undo batch. Handle collisions deterministically. Do not apply general string replacement to titles, notes, or arbitrary field contents. Treat BibLaTeX list-valued references as a separately specified extension.

Acceptance: `review_key_regeneration_must_update_crossref`; add chained renames, colliding generated keys, references to unchanged entries, undo, and save/reload. External documents containing citation commands cannot be updated by this operation; make automatic regeneration's consequences explicit in the README.

**R08 — P2: Limit blank-line cleanup to separators between entries.** Locations: [save.rs](../src/app/save.rs), line 297; [writer.rs](../src/bib/writer.rs), `normalize_blank_lines` at 37.

Saving an entirely clean entry containing `abstract={First\n\n\nSecond}` changes its bytes even with every save action disabled. The global string pass also reaches comments, preambles, and macro contents.

Normalize only whitespace-only inter-item `RawItem::Preamble` separators, or generate correct separators at insertion/deletion time and remove global normalization. Preserve untouched entry bodies and non-whitespace raw items verbatim.

Acceptance: `review_blank_lines_inside_fields_must_survive_save`; add CRLF, comments, quoted/braced values, preambles, and macro definitions containing blank lines. Keep the existing insertion/deletion spacing tests.

**R09 — P1: Use owned temporary files and protect persistence boundaries.** Location: [save.rs](../src/app/save.rs), lines 297–316.

The predictable `library.bib.tmp` path is opened with truncation and then renamed away. An unrelated existing file at that path is destroyed; concurrent app instances also share it. Failed-save cleanup can remove a path the operation did not create. Raw state/deletion queues are mutated before persistence succeeds.

1. Promote `tempfile` to a runtime dependency and create a unique sibling file using exclusive creation. Only clean up the owned file.
2. Write and flush/sync it before replacement; preserve original permissions and define symlink handling explicitly. Test platform replacement behavior.
3. Stage the raw-file changes and commit the new in-memory baseline only after replacement succeeds.
4. As a follow-up, store a load/save content fingerprint and refuse an ordinary save when the bibliography changed externally. Check before replacing the backup as well as the original.

Acceptance: `review_temp_path_must_not_clobber_existing_file`; add failed writes/replacements, retry, permissions, symlinks, two unique temporary writers, and external modification. Do not claim protection against every concurrent-write race from an mtime-only check.

**R10 — P2: Give sorting a total order.** Location: [save.rs](../src/app/save.rs), `compare_sort_values` at 822.

The fallback comparator switches between numeric and lexical ordering depending on the pair. For titles, `2 < 10`, `10 < 1a`, but `2 > 1a`. Sorting therefore has no consistent order; particular inputs can also trigger sort's comparator-consistency checks.

Choose a sort policy once per field/pass. Retain numeric ordering for numeric fields; for other fields either use lexical ordering or a consistently defined natural-sort key. Do not select the policy independently for each pair. Precompute keys once per entry rather than cloning/parsing values in each comparison.

Acceptance: `review_sort_comparator_must_be_transitive`; add antisymmetry/transitivity over numeric/text/empty/Unicode values, signed integers, leading zeroes, overflow, and ascending/descending equivalence. A panic from sorting was not needed to establish this defect and was not reproduced in this review.

**R11 — P2: Implement the advertised multi-field query syntax.** Location: [engine.rs](../src/search/engine.rs), `parse_query` at 65; README line 12.

`author:smith year:2020` is parsed as field `author` with the entire remaining text as one pattern, producing no result for the documented example.

Parse a query into terms with optional field qualifiers and require every term to match its assigned field. Combine term scores deterministically. Define quoted phrases, unknown fields, URLs/DOIs containing colons, and empty terms before extending the parser. Preserve existing single-field and plain-text behavior with explicit compatibility cases.

Acceptance: `review_documented_multi_field_search_must_match`; add a matching author with the wrong year, mixed qualified/plain terms, URLs, field aliases, and malformed query input. Share parsed queries with the later search-performance work.

**R12 — P2: Fix three Unicode offset errors.** Locations: [pdf.rs](../src/util/import/pdf.rs), `labeled_doi` around 96 and `extract_doi_at` at 135; [completions.rs](../src/app/completions.rs), `longest_common_prefix` at 509.

PDF scanning slices the original string using offsets from `to_lowercase()`, although lowercasing can change byte length. Ten Kelvin signs before `doi:` reproduce a panic. The fallback 200-byte DOI cutoff can split a multibyte character and panic independently. Completion rounds a byte prefix up to a character end: candidates `é.pdf` and `ê.pdf` incorrectly produce `é` as their common prefix.

Use ASCII case folding for ASCII DOI labels so offsets remain stable; clamp any byte limit downward to a UTF-8 boundary. For completion, decrease the common byte length until `first.is_char_boundary(len)` is true. Do not round up.

Acceptance: `review_pdf_lowercase_offsets_must_not_panic`, `review_pdf_doi_scan_must_not_panic_on_unicode`, `review_completion_must_return_a_prefix_of_every_candidate`; add Unicode property tests asserting no panic and that the result prefixes every candidate. The PDF panic occurs in a worker in normal use, so it fails the import; the installed global panic hook may also restore the terminal from that worker.

**R13 — P2: Ignore key-release events.** Location: [mod.rs](../src/app/mod.rs), `handle_event` at 406 and `handle_key` at 455.

Every key event advances key history and dispatches an action regardless of `KeyEventKind`. A synthetic release of `j` moves the selection. Backends that supply releases can therefore execute actions twice and accidentally complete `dd`, `gg`, or `yy` sequences.

Filter release events before key history and user bindings. Define whether repeat events are accepted for navigation and editing; keep press behavior unchanged. Test at the event boundary, not only `map_key`.

Acceptance: `review_key_release_must_not_perform_actions`; add press/release sequences for `d`, `g`, `y`, ordinary text, and user bindings. Add native Windows CI coverage; this review did not run Windows.

**R14 — P2: Centralize HTTP transport, trust configuration, and download limits.** Locations: [Cargo.toml](../Cargo.toml), target-specific ureq features; [import/mod.rs](../src/util/import/mod.rs), `download_pdf` at 69; fetchers in `src/util/import/`.

Code/dependency inspection establishes these gaps:

- The README promises automatic trust of corporate CAs. In locked ureq 2.12.1, `native-tls` merely enables an optional adapter; crate-level `ureq::get` still uses rustls. The `native-certs` feature is separate and is not enabled. The cached dependency's `src/lib.rs` lines 165–176 document this behavior.
- Every fetch uses an ad hoc request. Locked ureq's default connect timeout is 30 seconds, but read/write timeouts are unset (`src/agent.rs:256`). A stalled response can occupy the single import slot indefinitely.
- PDF downloads read the entire body into memory, then truncate the DOI-derived destination with `std::fs::write`. Reimports or sanitized-name collisions can replace an existing attachment. There is no size cap.
- Local PDF DOI extraction reads the entire file despite scanning only the first 200 KB and last 50 KB.

Split implementation into transport and download tasks. Build one injectable `Agent` with an explicit trust policy and bounded connect/read/overall timeouts. Choose rustls plus native certificates or explicitly configure the native TLS connector on supported targets; retain certificate verification. Thread the transport through fetchers. Then stream PDFs into an owned temporary file with a configurable size limit, validate the signature, and persist without overwriting existing files. Use `Read`/`Seek` for local head/tail scanning. Give optional OA lookup a bounded budget, especially when a local PDF is already available.

Acceptance: deterministic responses for success, non-PDF, oversize, interrupted transfer, timeout, redirects, and existing destination; metadata remains usable when an optional download fails. Test transport configuration without relying on the developer's trust store. Remove the default-suite live Crossref routing request and test fetcher selection directly.

**Performance findings and follow-up tasks.** The [performance probe](performance_probe.rs) uses release mode, a 120×40 `TestBackend`, reversed unique keys, four fields, and approximately 2 KB of abstract per entry. Parse timings are medians of three runs; search/sort/render are medians of five; regeneration is one pass changing every key. [Raw timing results](performance-results.txt) are included. These are local scaling measurements, not representative measurements of every bibliography or terminal. No PDFs or network calls are involved.

| Entries | Input MB | Parse + database ms | Global search ms | Author-only search ms | Render ms | Regenerate all keys ms |
|---:|---:|---:|---:|---:|---:|---:|
| 1,000 | 2.21 | 10.5 | 59.5 | 0.30 | 8.6 | 9.1 |
| 5,000 | 11.06 | 51.0 | 300.7 | 1.57 | 13.4 | 187.3 |
| 10,000 | 22.11 | 113.9 | 596.2 | 3.42 | 19.1 | 744.4 |
| 20,000 | 44.22 | 246.1 | 1,204.8 | 7.08 | 36.0 | 4,052.3 |

**P01 — P2: Keep large searches off the UI thread.** Locations: [engine.rs](../src/search/engine.rs), `search` at 28; [mod.rs](../src/app/mod.rs), `update_search` at 1149 and paste handling at 418.

Global search rebuilds and fuzzy-scores every entry on every keystroke. Pasting calls `SearchChar` once per character and performs repeated full searches. Prebuilding all search strings reduced the 20,000-entry scoring pass only to 1,169.8 ms: matching dominates this workload, so a cache alone is insufficient.

First batch paste into one query update. Then use a cancellable worker or chunked search with query/document generation IDs; discard outdated results and keep the event loop responsive. Cache search strings by entry revision to reduce allocation, but preserve all-field semantics and benchmark memory overhead. Do not silently truncate abstracts or cap results as a performance shortcut. Integrate R05 and R11 before sharing this state.

Acceptance: simulate slow searches and verify navigation/cancellation continue, stale results never replace a newer query, mutations invalidate cached entries, and paste starts one search. Report latency and throughput separately; use benchmark comparisons rather than fixed millisecond assertions in ordinary CI.

**P02 — P2: Rebuild the entry map once during bulk key regeneration.** Location: [editing.rs](../src/app/editing.rs), `regen_all_citekeys_impl` at 665.

Repeated `IndexMap::shift_remove` shifts remaining entries for each rename, producing quadratic work. The 5,000 → 10,000 doubling increases regeneration time approximately fourfold; 20,000 entries take four seconds on this machine, on the UI thread during save.

Move the map out, reserve existing keys in a hash set, and iterate once to build the replacement map. Remove each entry's old key from the reservation set before choosing and reserving its final unique key, preserving deterministic collision behavior. Keep the old-to-new mapping for R07 and refresh view state once. Preserve entry order explicitly when file-order sorting is selected.

Acceptance: identical results to current collision rules for unchanged keys, suffix collisions, empty generated keys, and batch undo; near-linear scaling on this probe rather than repeated shifts. Keep raw-field preservation and reference updates intact.

**P03 — P2: Make rendering proportional to the visible rows.** Locations: [main_screen.rs](../src/tui/screens/main_screen.rs), line 27; [entry_list.rs](../src/tui/components/entry_list.rs), row construction at 124.

Expensive cell formatting is culled, which is good, but every redraw still gathers all entries and allocates a `Row` plus a cell vector for every offscreen entry. It is not fully virtualized. Rendering rises to 36 ms with an unchanged 40-row terminal. Visible attachment indicators also call `exists()` during rendering, which can block on slow/network storage.

Track global selection/offset separately, pass only the visible slice to the table, and translate its local selection back to the global row. Retain total counts for scrolling/status. Cache attachment existence with explicit refresh/invalidation if profiling shows it matters; do not do new filesystem work for offscreen rows.

Acceptance: constant viewport cell/row allocation counts as library size grows, plus snapshots for `G`, `gg`, page movement, resizing, filters, and empty results. Retain render scaling measurements. Cached sort keys are worthwhile cleanup after R10, but sorting was only 8.7 ms at 20,000 entries here and is a lower priority than search and regeneration.

**Q01 — P2: Test complete user workflows and failure sequences.** Start with [app/tests.rs](../src/app/tests.rs) and the coverage gaps above.

Keep the existing pure-function and raw round-trip tests. Add a small event-driven test helper that sends real `Event::Key` and paste events through `handle_event`; use the existing mock clipboard/opener and `TestBackend`. Prioritize open → edit → confirm → save → undo → save → reload; create-new-library; attachment dialogs; settings import/export; and type/group changes. Check database state, dirty state, serialized output, and visible selection together.

Introduce an injectable filesystem boundary for failure tests and an injectable HTTP transport for import tests. Add property tests for comparator order, UTF-8 slicing, parse/write preservation, and save/reload semantic equivalence. Pure parse/write identity alone is insufficient because opaque malformed input also round-trips.

Two production-file round-trip tests silently return when untracked `jabref.bib` is absent. The config fallback test returns when the tracked `bibtui.yaml` exists. Replace these with checked-in fixtures and injected config search roots or isolated subprocess working directories. A passing count should not hide skipped assertions. The live Crossref routing test accepts network errors, so passing it does not establish successful import behavior.

**Q02 — P2: Make the supported build and quality gates explicit.** Locations: [Cargo.toml](../Cargo.toml), [README.md](../README.md), [.github/workflows/ci.yml](../.github/workflows/ci.yml), [.gitlab-ci.yml](../.gitlab-ci.yml).

The README advertises Rust 1.70+, but locked ratatui 0.30.0 declares 1.86 and darling 0.23.0 declares 1.88. Verify the full dependency graph at a chosen minimum, add `package.rust-version`, and test that compiler with `--locked`. The review established that 1.70 is unsupported; it did not establish the exact minimum compiler by building every candidate.

Land formatting as a separate mechanical change, replace the flagged `drain(..).collect()` with `std::mem::take`, then add formatting and strict Clippy CI jobs. Run native tests on Linux, macOS, and Windows; cross-compiling release artifacts does not execute platform-sensitive code. Record coverage with a fixed toolchain, publish the module summary, and add a no-regression gate with deliberate baseline updates. Improve application-path coverage before raising an overall target. Use `cargo llvm-cov report --summary-only` to print an already-generated report; the current CI summary command reruns the suite.

**Follow-up scope after these corrections.** Review BibTeX semantic resolution and export fidelity separately: `RawFieldValue::Concat::to_string_value` joins parts with spaces, and `build_database` does not resolve `@String` definitions. Display/search/export can therefore contain macro identifiers instead of their values. Export has its own simplified author parser that splits on every ` and `, including within braced organization names. Define a shared resolved-value/name representation without losing raw expressions, then test corporate authors, name suffixes, accents, macros, and inherited fields against expected exported values. This is a compatibility project, not a prerequisite for the small data-loss fixes above.

**Suggested execution order.** R01 and R02 are small first fixes. Follow with R03, R09, and R04 for persistence; R06/R07 for key changes; R05 for consistent views; then R10–R14 and R08. Implement P01–P03 after their related correctness changes. Add each task's regressions immediately; grow Q01/Q02 alongside those fixes. Avoid a broad App rewrite until these behaviors are protected by tests.

To reproduce the review assertions without modifying production sources:

```sh
python3 review/run_review.py regressions
python3 review/run_review.py performance
```

The regression command intentionally exits nonzero on the reviewed revision: its assertions specify the corrected behavior. The runner copies sources into a temporary directory and reuses `target/` only for compilation. For implementation, move the relevant probe/helper into the normal tests and make that subset pass before running the full suite. The performance command runs only the release performance probe, not the intentionally failing regressions.
