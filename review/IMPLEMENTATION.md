Implementation of the October 1 review. Each item is committed separately after the full test suite passes.

- R01: save errors are returned internally; both save-and-quit paths remain open on failure. Backup/write failures and retry are tested.

- R02: invalidate saved undo branches before new edits; regression, quit-confirmation, new save-point, and cap-eviction tests pass.

- R03: restored entries rebind to the current raw document; key undo retains the current binding and marks it dirty. Automatic save-time renames are undo batches, preserving older field history.

Remaining: R04–R14, P01–P03, Q01–Q02.

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
