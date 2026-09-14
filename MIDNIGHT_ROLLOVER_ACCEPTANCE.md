# Shared business clock and real desktop midnight acceptance

Date: 2026-09-14. Tested the actual desktop executable with production AppData. Windows time was never changed.

## Implementation

`src-tauri/src/business_clock.rs` supplies business datetime/date and a serialized clock snapshot. The automatic scheduler's business-date detection, daily publication validation and current production flow use this source. Frontend daily views share the backend clock snapshot through `businessClock`; its normal clock advances between snapshots, while an injected test datetime remains fixed until explicitly advanced. The legacy frontend business-date helper now reads that same shared clock.

Production defaults to real Europe/Istanbul time. Override storage exists only in debug builds and is process memory only. Mutation additionally requires `ARZ_TEST_BUSINESS_CLOCK=1` in the launched application's environment. There is no settings control or release UI for the hook. The release command rejects mutation unconditionally; release compilation was checked. A normal debug session without the opt-in also rejects it.

Debug-only test procedure: launch the debug executable with the opt-in, then invoke `business_clock_test_set` with `{ "datetime": "2026-09-14T23:59:50+03:00" }`, followed by `{ "datetime": "2026-09-15T00:00:10+03:00" }`. Invoke with `{ "datetime": null }` to clear. `business_clock_get` reports `utc_ms`, `business_date`, `overridden` and `revision`. Invalid input is rejected; setting an identical value does not increment revision. Mutations are rejected while an automatic refresh is running. Nothing is written to clock settings on disk.

Audit, ingestion, settlement and licensing timestamps continue to use real wall time. No migration, historical timestamp rewrite, model calculation change, or licensing/updater behavior change was made.

## Desktop finding and correction

The first real rollover exposed a race: the frontend's `daily_ensure_publication` and the automatic refresh independently generated new-day runs #123 and #124. Only #123 became the selected publication, but the second run was unnecessary. Those audit records were retained.

The ensure command now waits, outside the database connection and UI thread, for the enabled scheduler to finish the current-date transition. It does not request another refresh. A 60-second bound prevents an indefinitely waiting request. Future-date explicit ensure and disabled-scheduler behavior retain their prior path. A regression test covers old-date IDLE, new-date RUNNING, completed IDLE, and disabled states.

## Final real desktop acceptance

The corrected build was tested again with the exact requested datetimes.

| Check | Observed result |
|---|---|
| Shared date change | Exactly one observed DOM date change to 2026-09-15 |
| Immediate refresh | RUNNING observed 876 ms after advancing the clock |
| Refresh count | 1 → 2; remained 2 after repeating the same clock value |
| Completion | IDLE observed after 4,287 ms, no error |
| New-day run count | Increased by exactly one; no second ensure-generated run |
| Today | 2026-09-15, 17 matches, zero eligible selections |
| Candidates | 2026-09-15, all 17 matches reported outside model scope |
| Coupons | 2026-09-15; no yesterday READY cards carried over |
| Yesterday publication | Complete before/after daily output JSON identical |
| Historical coupons and selections | SHA-256 content hashes identical |
| Settled audit outcomes | SHA-256 content hash identical |
| Model-performance history | 799,484 persisted observations before and after |

The new date legitimately had no supported selections; no probability or eligibility rules were changed to populate it. The prior day's four READY coupons remained stored. The empty new-day publication created in the initial attempt was retained under existing explicit publication semantics; the corrected attempt added only one generation audit run, not another active publication.

Evidence: `.tmp-dataflow/midnight-final-before-clock.json`, `midnight-final-rollover.json`, `midnight-final-before-db.json`, `midnight-final-after-db.json`, `midnight-final-coupons.png`. The initial failure evidence remains in `midnight-rollover.json` and `midnight-after-db.json`.

## Clear and restart verification

Clearing the override returned frontend and backend to the real Istanbul date 2026-09-14 and `overridden: false`; the scheduler returned IDLE without error. The application was then restarted without `ARZ_TEST_BUSINESS_CLOCK`. The snapshot reset to revision 0 with no override. A direct attempt to set the test clock returned `TEST_BUSINESS_CLOCK_DISABLED`; the date stayed unchanged. The normal desktop again displayed its four READY coupons and scheduler interval 300 seconds, enabled.

Evidence: `.tmp-dataflow/midnight-final-cleared.json`, `midnight-normal-restart.json`, `midnight-normal-restart.png`. Final normal desktop PID: 33396.

## Checks

- `cargo fmt --check`: PASS.
- `cargo check`: PASS.
- `cargo check --release --lib`: PASS.
- Clock tests: 2 passed.
- Automatic refresh tests, including midnight ownership: 5 passed.
- Current-flow regression test: 1 passed.
- Frontend suite: 22 passed in 9 files.
- `npm run build`: PASS.
- Embedded desktop build: PASS.
- `git diff --check`: PASS.

Existing unrelated compiler warnings remain. Earlier network-failure and process-sleep/resume desktop acceptance is documented in `AUTOMATIC_REFRESH_DESKTOP_ACCEPTANCE.md`; this test closes its previously blocked midnight scenario.
