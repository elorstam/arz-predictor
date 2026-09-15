# Katlama manual series reset

Verified on 2026-09-15 with the real debug Tauri desktop and isolated acceptance databases. The installed user's database was not modified.

## Behavior

- “Katlama serisini sıfırla” in the daily coupon status area opens explicit confirmation.
- One immediate SQLite transaction archives the old progression and creates a new active series at step 1 with the original starting stake. It preserves every coupon, selection, settlement, series step and existing financial/audit record.
- The archived series records the manual reset reason and timestamp; its successor records the predecessor ID. Repeated requests for the same predecessor and repeated resets before publication are idempotent.
- Reset performs no candidate generation or coupon publication. Normal publication clears the manual-reset display state.
- Coupon Performance includes manually archived series, including when Katlama is the only recorded history.
- Selection rules, settlement calculations and prediction models are unchanged.

## Automated verification

- `cargo test manual_reset -- --nocapture`: step 4 and pending/settled cases, unchanged history and performance report, repeated reset, database reopen, normal publication, repeated refresh, no valid combination, stale request and transaction rollback.
- `cargo test coupon_engine_tests -- --nocapture`: existing coupon rules and settlement regression suite passed (29 tests at the initial run).
- `cargo test coupon_performance_tests`: 5 passed.
- `npm test`: 34 passed.
- `npm run build`: passed.

## Desktop evidence

Artifacts are in `.tmp-dataflow/manual-reset/` (local acceptance output, not release data).

1. `navigate.json`, `confirm.png`: active step 4, historical winning step 3 and pending step 4; confirmation shown and clicked with WebView mouse input.
2. `reset.json`, `reset.png`: new series ID 2, step 1, no latest coupon, manual reset message.
3. `automatic-restart.json`, `restart.png`: desktop process closed and reopened; the reset remains persisted without publication.
4. `db-before.json`, `db-reset.json`, `db-restart.json`: coupon/selection/series-step/transition records unchanged. The independent settlement scheduler continues updating pending audit check timestamps; audit record identities remain present. Unit tests compare all audit values around the reset transaction itself.
5. `history.json`, `history.png`: Coupon Performance retains the winning step 3 and historical step 4.
6. A full provider refresh completed without a qualifying combination; no coupon was invented.
7. In a fresh controlled fixture, reset was clicked in the desktop again. The existing `candidate_engine_generate_daily` command (the normal publication pipeline, called through desktop IPC) published coupon 7 at step 1, with two selections and combined odds 1.69. `candidate-refresh-run.json`, `candidate-refresh.json` and `published.png` show this. The UI's Yenile button then displayed normal active-series status and both the old step-4 coupon and the new step-1 coupon.

The valid-publication desktop scenario uses controlled candidates; it does not claim that current live provider data always contains a qualifying combination.
