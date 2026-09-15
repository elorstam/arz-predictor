# Office bootstrap/readiness recovery

Verified 2026-09-15. The reported error was reproduced verbatim in the real Tauri desktop using an isolated recovered-state fixture, then recovered automatically using the fixed backend. No office database was supplied or remotely inspected; the evidence below is a controlled reproduction of its reported state.

## 1. Exact root cause

Three backend decisions disagreed:

1. `features::persist` uses `INSERT OR IGNORE` with the unique key `(match_id, feature_engine_version, cutoff_at)`. Feature snapshots are immutable. A snapshot produced before history recovery therefore survives subsequent inference at the same cutoff.
2. `current_flow` generates a fresh feature vector in memory and can persist valid predictions from it even when saving the feature snapshot is ignored. It also reuses existing BASE predictions without previously repairing their missing/stale feature snapshot.
3. `data_center::status` combined current production coverage with the global historical `features::quality_summary`. If all retained snapshots reported insufficient history, features stayed PARTIAL even with valid predictions and a readable production publication.

This is a snapshot/progression dependency inconsistency, not a UI cache failure. The reproduction has valid same-date BASE predictions and a canonical publication. The displayed “candidate/coupon source could not be verified” is a cascading capability error; in this case it did not establish a broken source/run ID.

## 2. Why readiness stayed 7/10

Features PARTIAL makes `can_generate_predictions=false`. That blocks candidates, which blocks coupons: exactly three failed core checks. The other seven remain READY. Unrelated historical feature quality was effectively a permanent veto over current production readiness.

## 3. Why bootstrap stayed 12/13

The manifest had recorded every stage except `readiness`. `run_attempt` skipped completed stages and repeatedly reran only the read-only readiness check. The five-minute scheduler also waits for bootstrap completion. Thus neither retry repaired the dependency that prevented completion.

## 4. Fix

- Evaluate production coverage using the existing `model_coverage::classify` interpretation shared with inference. No supported league rules or minimum history thresholds were changed.
- Validate current feature ownership, engine version, cutoff and history sufficiency instead of applying the global historical quality total as a gate. Conservative date-only cutoffs remain supported.
- Migration 0033 adds immutable `production_feature_revisions`. A recovered future fixture gets an appended current revision when its original unique snapshot already exists. Original snapshots, training labels and historical rows remain intact.
- The normal flow repairs feature evidence before reusing an existing BASE prediction.
- The final bootstrap stage reconciles current dependencies before evaluating readiness. Existing valid predictions/publications are reused; missing predictions or publication state go through the normal production pipeline.
- Data Center reads `daily_selections::get`, the same canonical publication/view/coupon source used by the daily screens. Prediction existence checks use the active model and the actual supported production fixtures.
- A failed refresh audit does not invalidate a still-readable, valid publication; a missing publication still blocks readiness and is repaired by the production flow.
- A fully completed manifest is idempotent. Completion is persisted and startup does not rerun bootstrap.

No readiness UI labels, prediction models, thresholds, coupon selection rules, settlement calculations, supported-scope rules, licensing or updater behavior were changed for this task. Earlier manual Katlama reset work in the workspace is preserved.

## 5. Before and after

| State | Old desktop, including retry | Fixed desktop | Restart |
|---|---:|---:|---:|
| Bootstrap | 12/13, RETRY_WAIT | 13/13, COMPLETED | 13/13, COMPLETED |
| Core readiness | 7/10 | 10/10 | 10/10 |
| Predictions enabled | false | true | true |
| Candidates enabled | false | true | true |
| Coupons enabled | false | true | true |
| Bootstrap error | READINESS_NOT_COMPLETE | none | none |

The old desktop emitted the exact reported feature/candidate/coupon error text in both `before.json` and `before-retry.json`.

## 6. Real desktop acceptance

The fixture contains the shipped BASE and calibration artifacts, repaired ten-league scope, 120 historical matches, ten current supported fixtures, ten stale immutable feature snapshots, 550 real inference outputs, a canonical candidate publication, coupons, fresh odds/popularity and a 12-stage manifest.

The old debug executable remained stuck. The fixed debug executable opened the same database and completed automatically without a Data Center action. Its normal post-bootstrap refresh also completed. Actual WebView screenshots and IPC state are stored under `.tmp-dataflow/office-readiness/`:

- `before.png`, `before.json`, `before-retry.json`
- `after.png`, `after.json`
- `restart.png`, `restart.json`
- `db-after.json`, `db-restart.json`

This is the real Tauri desktop/backend, not a browser mock. The existing explicit debug-license mode was used. The periodic timer was disabled in the fixture to isolate restart idempotency; the normal post-bootstrap refresh still ran once. Scheduler behavior is additionally covered by the unchanged five-minute scheduling regression tests.

## 7. Restart acceptance

The app process was closed and reopened against the same SQLite database and bootstrap manifest. A fresh WebView profile was used after the first profile failed to reopen in the test runner; this did not reset application data.

Before and after restart: 86 competitions, 572 teams, 447 matches, 550 predictions, 3 candidate runs, 1 publication, 2 coupons, 10 original snapshots and 10 appended revisions. The post-bootstrap provider refresh accounts for the live entities added before this comparison.

Counts, publication rows, original snapshot hash and the complete bootstrap manifest match exactly. Bootstrap attempts remain 7; the restarted scheduler has zero cycles. SQLite integrity is `ok`, with no foreign-key violations. The isolated unit test also verifies that bootstrap repair itself creates no duplicate prediction, run, coupon or entity records.

## 8. Tests and builds

- Full Rust regression run: 224 passed; four tests failed only because they still expected schema version 32. Their schema expectations were updated to 33, and all affected tests passed on rerun.
- Three office recovery tests pass: exact 7/10 reproduction and 13/13 recovery/reopen; missing prediction/publication repair and idempotency; snapshot ownership, date-only cutoff and immutability.
- Schema upgrades from 27, 28 and 32 pass with preserved rows and bookkeeping.
- Existing coverage for scope recovery, midnight coupon persistence, Katlama progression/settlement, refresh scheduling and licensing passed in the regression run.
- Frontend: 34 tests passed; `npm run build` passed.
- Debug build and `cargo build --release` passed. Compiler warnings reported are pre-existing unused/dead-code warnings.
- `git diff --check` passed.

The release executable was built locally. This task did not publish an updater release or install a new version on the remote office machine.
