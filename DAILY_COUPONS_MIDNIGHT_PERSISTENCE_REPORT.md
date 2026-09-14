# Daily coupons persist until Istanbul midnight

Date: 2026-09-15. Changes are local on top of `c38eec10c85e99e304a718385f028774fe5ece85`; previously prepared release artifacts have not been rebuilt or published.

## Root cause and change

The daily read query tied ordinary published coupons to the selected candidate run; BTTS additionally required the latest category run. Candidate generation correctly excludes started matches, but a newer pool could therefore make an already published coupon disappear. Separately, the frontend filtered out every publication whose state was no longer READY, including settled coupons.

`coupon_engine::get_daily` now retains published non-replay snapshots for the requested business date across candidate-run changes. Cancellation and coupon validity checks remain. Live ordinary coupon generation reuses the published snapshot once any selection has started, including when later eligible candidates exist. Existing pre-kickoff updates remain possible. Katlama can publish its next step while its earlier same-day settled card remains visible; no progression or settlement code changed.

The page filters published READY and SETTLED coupons by the shared business date, labels the section “Yayımlanan kuponlar”, and displays the persisted settlement result. It uses the existing `useBusinessDate` / shared backend clock and one-second clock synchronization. No second date system, midnight deletion, migration, model change, or selection-rule change was introduced.

## Controlled real desktop evidence

Evidence is retained locally under `.tmp-dataflow/midnight-persistence/`: desktop JSON snapshots, screenshots, database observations and the acceptance scripts. These are real debug Tauri/WebView2 sessions with isolated fixture AppData and the existing opt-in business-clock hook; Windows time and production AppData were not changed. The fixture is not a first-run or release-license acceptance. Final scores were supplied only at the match-result boundary; the application's existing worker performed settlement.

The fixed-date scenario publishes at 18:00 Istanbul with 20:00 kickoffs. `scheduled-cycle.json` records the uninterrupted five-minute scheduler cycle, with start times 1789419119 and 1789419419 exactly 300 seconds apart. At virtual 20:05 the original visible coupon IDs 1, 2 and 3 remained. This particular cycle reused unchanged inputs.

For a separate automatic regeneration check, the current-date fixture uses publication cutoff two minutes before export and kickoff one minute before export. `live-start.json` records an unmodified wall clock, automatic production UPDATED, a new candidate run #2 with **zero candidates**, and **three usable coupons**, retaining IDs 1, 2 and 3. `live-db-after-kickoff.json` records the immutable original source run and the new run. No manual generation or settlement command was used in this current-date scenario.

`before-midnight.json` / `.png` show all three original cards at 23:59: O2.5 WON, BTTS LOST, High Confidence WON. `restart-confirmed.json` and `db-restart.json` show the same IDs and unchanged coupon count after closing and relaunching the application at virtual 23:30. The existing debug clock is ephemeral, so it was reapplied after restart.

The initial historical next-day fixture hit the existing historical-replay guard, producing an error view instead of a normal empty page. This is not counted as successful normal midnight UI acceptance. The current-date scenario is used for the final midnight UI check.

Final current-date verification:

| Case | Desktop observation | Evidence |
| --- | --- | --- |
| A: after kickoff | Automatic generation created run #2 with zero candidates; IDs 1, 2, 3 remained READY. | `live-start.json`, `live-db-after-kickoff.json` |
| B: settled, before midnight | The worker settled O2.5 WON, BTTS LOST and High Confidence WON. All three remained visible at 23:59. | `live-settled.json`, `live-before-midnight.json/png` |
| D: restart at 23:30 | Same three published IDs, same four total database rows including the unattached compound draft; no duplicate coupon. | `live-restart.json`, `live-db-restart.json` |
| C: midnight | The shared clock switched September 15 to September 16. The completed page shows September 16, zero coupons and its normal empty state. Old rows and results remain persisted, with the old publication still pointing to run #1 and a separate new-date publication pointing to run #3. | `live-midnight-completed.json/png`, `live-db-midnight.json` |

The current-date startup ingestion and production generation succeeded. A later upstream refresh failed during restart/rollover (`UPSTREAM_FAILED`); existing publication recovery still produced the correct new-date page. This acceptance proves coupon visibility, date separation and preservation, not uninterrupted provider availability or generation of a new valid coupon set.

`performance-history.json` confirms the three settled published coupons remain in Coupon Performance: two won, one lost, total stake 300 cents. The unattached compound draft is excluded from the visible published set; its database row is also retained.

## Regression validation

- Rust suite: 221 passed, 0 failed, 11 ignored. The ignored desktop exporter was run explicitly.
- Frontend: 33 tests passed; TypeScript/Vite production build passed.
- Actual debug Tauri executable build passed and was used for desktop acceptance.
- `cargo fmt --check` and `git diff --check` passed.
- After adding the current-date exporter variant, its explicit export and the shared same-day/reopen/date-switch regression passed again.

Regression coverage includes empty pools after kickoff, later valid candidates without replacement, WON and LOST retention, reopening the persisted database without duplicate coupons, date switching with historical records retained, and settled Katlama plus its next same-day step without duplicate progression. The previous BTTS test expecting disappearance was corrected to require retention while preserving its stale/invalid-input rejection assertions.

No release installer was produced for this change. Existing compiler warnings remain unrelated to this scope.
