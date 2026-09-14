# Test suite cleanup

## Original failures and classification

The seven migration failures were classified before editing expectations. Migration definitions are ordered, unique and contiguous from 1 through 29. Tests stopped at migration 27; migrations 28 (`coupon_settlement_audit`) and 29 (`popularity_selection_quotes`) add tables/indexes without resetting historical data. Production AppData reports all 29 matching migration names.

| Test | File under src-tauri/src | Expected before | Actual before | Classification / cause |
|---|---|---|---|---|
| migration_versions_are_tracked_and_not_reapplied | database/tests.rs | Named versions 1–27; reopen count 27 | Named versions 1–29 | A: stale list/count, missing migrations 28–29 |
| migration_0002_applies_after_existing_0001_database | database/tests.rs | Versions 1–27 | Versions 1–29 | A: stale upgrade expectation |
| migration_0003_applies_after_existing_0002_database | database/tests.rs | Versions 1–27; count 27 | Versions 1–29 | A: stale upgrade expectation |
| migration_0004_applies_after_existing_0003_database | database/tests.rs | Count 27 | Count 29 | A: stale upgrade expectation |
| migration_0005_applies_after_existing_0004_database | database/tests.rs | Count 27 | Count 29 | A: stale upgrade expectation |
| migration_0007_applies_after_existing_0006_database | database/tests.rs | Maximum version 27 | Maximum version 29 | A: stale upgrade expectation |
| migration_0008_applies_after_existing_0007_database | database/tests.rs | Maximum version 27 | Maximum version 29 | A: stale upgrade expectation |
| phase_4_0_1_full_acceptance | repositories/resolution.rs | 16 Iddaa event mappings | 15 | B: real ingestion collision; additional E: test lookup assumed separate provider team mappings |

## Resolver delta and production fix

The missing event is **910014, Manchester United FC–Chelsea**, competition **642 / England Premier League**, kickoff Unix **1792958400**. Import diagnostics report `UNIQUE constraint failed: teams.normalized_name, teams.country` for this event alone.

The historical fixture has both `Manchester United` and `Manchester United FC` in England and the same competition. Both normalize to `manchester united`. Their tied normalized scores correctly fail the existing winner-margin rule, but the fallback attempted to insert the already-existing literal `Manchester United FC` identity. The savepoint rolled back this event. This was not a supported-scope count change and the expected count remains **16**.

The only production change is in `repositories/incremental_resolution.rs`: after existing provider/scoped mappings, before normalized/fuzzy comparisons, reuse a literal, case-sensitive team name with an equal known country, a football-data canonical mapping, and membership in this exact competition. No normalized alias is learned from this branch, since suffix-normalized names can be ambiguous. Null country is not accepted by this branch. Existing thresholds, winner margins, historical merges and schema semantics are unchanged.

A new regression test proves that two literal names sharing a normalized key retain their distinct IDs, no team or ambiguous alias is duplicated, and a third ambiguous spelling is not assigned to either canonical ID. Existing four-day tests retain unrelated-competition and ambiguity protection.

The acceptance test now asserts zero import failures and obtains raw provider spelling from its bulletin fixture. Canonical identity reuse does not require an extra Iddaa team mapping. All existing count, match safety, merge, snapshot-preservation and idempotence assertions remain.

## Changed files and scope

- `database/tests.rs`: test-only migration expectations updated to explicit version 29.
- `database/migrations.rs`: a `#[cfg(test)]` upgrade test for versions 27 and 28; production migration definitions and implementation unchanged. It checks existing rows/bookkeeping, new tables, integrity, and repeated application.
- `repositories/resolution.rs`: acceptance-test diagnostics and raw-name lookup only.
- `repositories/incremental_resolution.rs`: the literal identity bug fix described above, plus its regression test.
- `repositories/calibration_tests.rs`, `repositories/features_tests.rs`, `repositories/prediction_engine_tests.rs`: three additional stale schema-version assertions, 24 to 29, discovered by the full suite. No model or feature logic changed.

No fixture input files changed. No tests were deleted, disabled, or newly ignored. Earlier working-tree changes were preserved. Per-file initial snapshots are in `.tmp-dataflow/test-cleanup/`.

Production AppData was opened read-only: schema bookkeeping was readable and `PRAGMA quick_check` returned `ok`. No production database writes or destructive reset were performed.

## Additional failures exposed by the full suite

The initial full run passed 195 tests, failed 4, and intentionally ignored 7. All eight original failures passed. Each additional failure was classified A before updating its expectation:

| Test | File | Expected | Actual | Cause |
|---|---|---|---|---|
| migration_0010_and_oos_identity_are_present | repositories/calibration_tests.rs | Max version 24 | 29 | Stale schema assertion |
| conservative_cutoff_missing_stats_and_unfinished_are_safe | repositories/features_tests.rs | Max version 24 | 29 | Shared fixture schema assertion |
| phase_5_feature_engine_and_leakage_acceptance | repositories/features_tests.rs | Max version 24 | 29 | Same shared fixture assertion |
| migration_0009_and_prediction_run_constraints_exist | repositories/prediction_engine_tests.rs | Max version 24 | 29 | Stale schema assertion |

## Final validation

- `cargo fmt --check`: PASS.
- `cargo check`: PASS; existing unused-variable/dead-code warnings remain.
- `git diff --check`: PASS.
- Full `cargo test`: **199 passed, 0 failed, 7 ignored** (library); binary and documentation targets also pass with zero tests. This includes all originally failing tests and all migration, resolver, and incremental-resolution tests. Library runtime: 54.31 seconds.
- Seven existing ignored tests are explicitly controlled local/live provider verification jobs, including production-AppData reports; their ignore annotations were not changed.
- Final full output: `.tmp-dataflow/test-cleanup/full-tests-final.txt`.

Seven Rust files changed in this task. Only the 11-line literal canonical identity branch changes production behavior, fixing the proven lost-event bug. All other changes are tests. Prediction, coupon, licensing/updater behavior, resolver thresholds, and database schema/migration semantics were not changed.
