Implementation of the October 1 review. Each item is committed separately after the full test suite passes.

- R01: save errors are returned internally; both save-and-quit paths remain open on failure. Backup/write failures and retry are tested.

- R02: invalidate saved undo branches before new edits; regression, quit-confirmation, new save-point, and cap-eviction tests pass.

- R03: restored entries rebind to the current raw document; key undo retains the current binding and marks it dirty. Automatic save-time renames are undo batches, preserving older field history.

Remaining: Q02 build/quality gates and final measurements.

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
