Implementation of the [October 1 review](CODEBASE_REVIEW.md). All 19 numbered review items are implemented, with separate commits per item and additional commits for follow-up fixes. The full test suite passes before committing.

- R01: save errors are returned internally; both save-and-quit paths remain open on failure. Backup/write failures and retry are tested.

- R02: invalidate saved undo branches before new edits; regression, quit-confirmation, new save-point, and cap-eviction tests pass.

- R03: restored entries rebind to the current raw document; key undo retains the current binding and marks it dirty. Automatic save-time renames are undo batches, preserving older field history.

- R09 (part 1): owned atomic temporary files, permission/symlink preservation, injectable persistence, and in-memory rollback on failure.

- R09 (part 2): compare exact saved bytes before backup and replacement; reject external edits/deletion and newly occupied paths, preserving backups.

- R04 (part 1): shared rename execution records partial successes, leaves missing paths intact, makes manual bulk sync undoable, and uses exclusive destination creation in both directions. Undo failure retains accurate paths and dirty state.

- R04 (part 2): immutable save plans normalize and rekey before planning attachment moves; preview and execution share the plan. Backups precede moves, failed persistence reverses moves, and stale previews/shared attachments are rejected.

- R06: preserve raw-expression baselines through manual, automatic, and duplicate-repair key changes; consecutive-save tests assert parsed variants.

- R07: centralized key mutation and simultaneous exact crossref mapping, recorded together in undo; external citation consequences documented.

- R08: normalize only whitespace-only raw separators, preserving opaque content and CRLF blank lines; insertion/deletion spacing tests retained.

- R10: per-field total ordering with cached sort keys, including signed page numbers; mixed-value order laws and ascending/descending tests.

- R11: AND terms with individual qualifiers, quoted phrases, URL/DOI compatibility, aliases, and documented incomplete-input behavior.

- R12: ASCII DOI-label folding, downward UTF-8 cutoff, and valid shared completion prefixes; generated Unicode cases exercise both invariants.

- R13: ignore releases before key history/bindings; allow repeats only for navigation/text actions, without advancing command chains; event-boundary regressions.

- R05: centralized sorting/filter refresh and selection preservation after mutations; search/group intersection and active tree paths (including sibling deletion/undo) are tested.

- P02: one reserved-key/map rebuild for bulk rekeying, cached collision suffix search, stable file order, one view refresh; collision/selection/save-reload tests.

- R14 (transport): injectable shared Agent with explicit native TLS selection, bounded request/import budgets and metadata sizes, local-PDF OA skip, deterministic routing/metadata/redirect/timeout tests.

- R14 (downloads): stream PDFs into owned temporary files with configurable caps, signature validation and exclusive persistence; local PDF scanning uses bounded Read/Seek head/tail reads.

- R12 follow-up: the same offset bug also occurred in ANS/Taylor & Francis metadata scraping; ASCII-fold tag labels and cover Unicode before/inside tags.

- P03: window-only entry gathering and row construction with separate global/local table state; row counts and TestBackend navigation/resize/empty-filter checks.

- P01: batch search paste; one cancellable worker above 500 entries, combined query/document generations, immutable per-entry text/range cache, stale-result rejection and responsiveness/full-field tests.

- Q01: event-driven workflow helpers and save/reload/failure sequences, generated semantic round-trip cases, tracked JabRef fixtures and isolated config roots; existing injected filesystem/HTTP/order/Unicode tests retained.

- P01 follow-up: release benchmarks exposed excess copying in synchronous qualified search; borrow selected fields and share compiled-query scoring with the worker.

- Q02 (formatting): apply rustfmt as a separate mechanical commit, with the full suite rerun before committing.

- R03 follow-up: restoration excludes raw slots already claimed by a live entry. A reproduced delete → key reuse → save → undo twice → save case now retains both entries.

- R14 follow-up: serialize imported attachment paths through the shared escaping routine; use escaped absolute paths in native-platform test fixtures and test semicolon filenames through save/reload.

- R04 follow-up: aggregate failed reversals across undo batches so later successful changes cannot hide attachment recovery errors.

- Q02 (quality gates): declare and test Rust 1.88.0, bump the minor version to 0.62.0, replace drain/collect with mem::take, and add native Linux/macOS/Windows CI tests. Pin format/Clippy/coverage to Rust 1.93.1 and cargo-llvm-cov 0.8.4; enforce reviewed overall/application coverage floors and export existing reports without rerunning tests.

Validation on x86_64 macOS (October 3, 2026):

- 1,676 Rust tests pass on Rust 1.88.0, 1.93.1 (instrumented coverage), and 1.98.1. The two Python coverage-gate tests pass.
- All 18 archived review regressions pass. The comparator probe now checks ordering laws under the documented lexical policy rather than requiring natural ordering.
- rustfmt checks and strict Clippy pass; the release binary reports `bibtui 0.62.0`.
- Overall LLVM line coverage is 90.77% (review: 88.44%). App editing is 34.29% (22.97%), App state/dispatch is 73.06% (62.56%), and saving is 95.10% (90.71%). Overall counts include inline unit tests, and formatting changed the line denominator; module values are more useful for planning further coverage work.
- Coverage floors are rounded down deliberately; the worker has a wider floor for scheduler-dependent cancellation branches. Baseline changes are explicit edits reviewed with the measurements.

Native Linux and Windows jobs are configured but were not executed locally. The review's separately scoped macro-resolution/export compatibility project remains future work. Persistence detects ordinary external edits and recovers ordinary failures; it does not lock other processes or provide filesystem-wide crash atomicity.

The [final coverage report](coverage-after.txt) and [18 passing review probes](regression-after.txt) retain the detailed results. The remaining low coverage in application editing merits further event-driven workflow tests, particularly confirmation, settings, and attachment actions.

Release measurements use the same synthetic bibliography sizes, complete abstracts, 120×40 TestBackend, Rust 1.98.1, and x86_64 macOS as the review. Timings below are for 20,000 entries (44.22 MB of BibTeX); compare the [original output](performance-results.txt) with the [final output](performance-after.txt).

| Operation | Review (ms) | Implemented (ms) |
|---|---:|---:|
| Bulk citation-key regeneration | 4,052.258 | 81.286 |
| Render one frame | 36.004 | 7.768 |
| Synchronous full-field search | 1,204.835 | 1,640.394 |
| Synchronous author-qualified search | 7.078 | 7.316 |
| Matcher over cached text | 1,169.793 | 1,183.183 |
| Sort by title | 8.708 | 8.155 |
| Parse and build database | 246.068 | 252.346 |

Bulk rekeying is about 50× faster and frame rendering about 4.6× faster in this sample. Large searches now leave matching to the background worker: first dispatch, including cache construction, takes 57.649 ms; subsequent dispatch takes 9.788 ms. First-result completion takes 1,246.882 ms, including dispatch and polling. Matching CPU time has not materially improved; the synchronous full-field path is slower in this run. Qualified search avoids copying unrelated abstracts.

The reference searchable-text payload is 43,380,000 bytes (41.4 MiB), before keys, field offsets, allocations, and worker/result metadata. This is a text-size measurement, not process RSS. The worker retains one copy of searchable text per entry and shares unchanged snapshots across requests.

Parse timings are medians of three runs; synchronous search, cached matching, sorting, rendering, and cached dispatch use five. Regeneration, first dispatch, and first-result completion are single measurements. The benchmark polls at 1 ms intervals. Results vary with machine load, particularly at smaller sizes; these are local observations rather than CI performance thresholds. Reproduce with `python3 review/run_review.py performance`; run the original regressions with `python3 review/run_review.py regressions`.
