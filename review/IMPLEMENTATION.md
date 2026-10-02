Implementation of the October 1 review. Each item is committed separately after the full test suite passes.

- R01: save errors are returned internally; both save-and-quit paths remain open on failure. Backup/write failures and retry are tested.

- R02: invalidate saved undo branches before new edits; regression, quit-confirmation, new save-point, and cap-eviction tests pass.

Remaining: R03–R14, P01–P03, Q01–Q02.
