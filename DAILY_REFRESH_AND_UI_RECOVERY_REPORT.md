# Daily refresh and UI recovery — 2026-09-14

## Environment and scope

Measured the real debug desktop executable and its Tauri WebView against production `AppData/Roaming/com.footballpredictor.app/football-predictor.sqlite3`. Existing unrelated working-tree changes were retained. No licensing/updater changes were made in this task. Historical metrics and prediction algorithms were not changed.

## Model Performance

| Measurement | Before | After |
|---|---:|---:|
| LAST_30 all-market/all-context Tauri command round trip | 40,858.9 ms | 570.1 ms cold; 34.6 ms warm |
| Actual page navigation to painted primary content | Not separately captured | 683.8 ms cold; 175.9 ms warm |
| Rapid market filter changes to final BTTS content | Not separately captured | 220.3 ms |
| Raw base query, SQLite reader including row materialization | 5,375.1 ms / 799,484 rows | 76.2 ms / 23,100 rows |
| Backend source loading, including indexed historical odds lookups | Not separately instrumented before | 371.1 ms |
| Backend aggregation | Not separately instrumented before | 126.8 ms |
| Response serialization (debug measurement) | Not separately instrumented before | 5.1 ms / 95,119 bytes |
| DOM ready through two animation frames | Not captured before | 31.2 ms in a separate warm navigation |

The animation-frame measurement is a browser paint scheduling measurement, not React CPU profiling. Cold source loading and aggregation remain the operations over 100 ms. Their results are cached.

Additional real invoke measurements: LAST_7 56.7 ms, BTTS 89.7 ms, BASE context 35.2 ms, single nonempty Premier League filter 77.3 ms / 1,480 samples. LAST_30 retains 15,540 samples. Full serialized metric responses compared exactly equal before/after optimization.

SQL filtering now happens before decoding the entire historical corpus. EXPLAIN identifies indexed match/backtest lookup and an expression index for the exact historical odds market/outcome/cutoff. The old odds temporary sort is removed. A final ordering temporary B-tree remains to preserve response order. Cold historical odds lookups remain per observation, with cached prepared statements and the new index; warm requests avoid them entirely. Lineup and candidate queries were also explained and measured (about 3.0 and 2.3 ms). Epoch lookup is about 0.03 ms.

The read-only command runs outside the shared application database mutex. A bounded backend snapshot cache and frontend versioned promise cache key all request dimensions and Istanbul date. Database triggers invalidate relevant settled-data/model changes; timestamp-only writes and future odds do not invalidate historical metrics. Concurrent identical frontend requests coalesce; stale filter responses cannot overwrite the latest filter. Primary summary/table paint before secondary breakdowns. Navigation invokes no ingestion, resolution, training, candidate, coupon, or logo work.

Evidence: `.tmp-dataflow/model-performance-before.json`, `model-performance-query-plan.json`, `model-performance-query-plan-after.json`, `final-performance-timings.json`, `verified-ui-acceptance.json`, `ui-filter-final.json`, `model-performance-final.png`.

## Daily coupon grid

The grid filters persisted publication status strictly to READY and orders Corner, Over 2.5, Over 3.5, BTTS, High Confidence, Surprise, Katlama. CSS auto-fit creates only occupied columns. Non-ready types are concise diagnostics in a closed-by-default details section.

Actual production UI: Corner, Over 2.5, BTTS and High Confidence are visible, four cards total; Today also reports four ready coupons. Over 3.5, Surprise and Katlama are insufficient and appear only in diagnostics. Two equal-width columns wrap naturally, with no reserved type slots or oversized unavailable cards. Tests cover one/multiple cards, ordering and exclusion of non-ready states.

Evidence: `.tmp-dataflow/verified-ui-acceptance.json`, `dynamic-grid-desktop.png`, `dynamic-grid-desktop.json`.

## Automatic refresh implementation

Default enabled; persisted interval 300 seconds with minimum 300. One background worker owns RUNNING/PENDING_RERUN state; repeated manual requests coalesce into one subsequent cycle. Startup, due time, stale focus/resume and Istanbul date changes request work. Cached UI remains usable. Manual bulletin/popularity/current-pipeline controls enter the same orchestrator.

Each cycle refreshes bulletin/odds and popularity, processes a bounded current/future unresolved batch, compares relevant production fingerprints, conditionally produces predictions/candidates/coupons, settles available results, and refreshes readiness audit state. Stable unchanged odds are skipped. Active model/calibration versions and settled-history epoch are fingerprint inputs. Existing eligible predictions are reused unless relevant inputs require refresh. No retraining or historical resolver rebuild is scheduled.

Historical maintenance examines supported teams' required historical inputs and active artifact metadata. At most one known current-season URL is HEAD-checked each cycle; a changed previously observed ETag can schedule one import. The first observed ETag establishes a baseline and does not prove byte equality with an older local cache. Missing required datasets use bounded repair/backoff. Logos only wake the existing bounded queue. The existing bounded results scheduler remains responsible for acquiring final results; settlement also runs every orchestrator cycle.

During development an overly broad maintenance denominator imported legitimate E0:2324 and E0:2223 historical datasets. This was narrowed to current/future supported teams lacking required historical samples; final observed required-missing lists are empty. Historical data was retained, not reset or deleted.

Empty/malformed/partial bulletin responses do not deactivate last-known-good events or replace the publication. A real startup write contention was observed: 142 event writes failed, generation was skipped, and existing four READY coupons remained visible. The next cycle recovered. Import now reserves an IMMEDIATE transaction before identity reads to avoid WAL snapshot read-to-write upgrade contention; network work stays outside that transaction.

### Observed real cycles

| Start epoch (UTC) | Next start | Interval | Duration | Production |
|---:|---:|---:|---:|---|
| 1789339688 | 1789339988 | 300 s | 3,083 ms | Updated |
| 1789339988 | — | — | 1,626 ms | Skipped: UNCHANGED_INPUTS |

A separate changed-data cycle at 1789340324 took 2,870 ms: freshness 10 ms, bulletin/odds 1,428 ms, popularity 138 ms, incremental resolution under 1 ms, combined prediction/candidate/coupon phase 956 ms, settlement 30 ms, readiness 33 ms. It persisted 156 changed odds, skipped 7,013 unchanged odds, resolved no new identities because none were pending, and reused existing MODEL_READY predictions. Prediction/candidate/coupon timing is combined, not separately measured. The no-change sample is one observed cycle, not a statistically meaningful multi-cycle average.

Append-only production evidence: `automatic-refresh.jsonl` in production AppData. UI displays actual last-success and next-check values without periodic toast spam.

## Validation and remaining acceptance limits

Final full Rust suite after the transaction-lock adjustment: 204 passed, 0 failed, 7 intentionally ignored (58.68 seconds). Frontend: 21 passed in 9 files. `cargo fmt --check`, `cargo check`, `git diff --check`, `npm run build` and the embedded desktop build pass. Existing compiler warnings remain. Regression coverage includes metric equivalence and settled-result cache invalidation, cached invoke coalescing, latest-filter behavior, READY rendering, startup/interval/resume/midnight scheduler logic, coalescing, unchanged fingerprints, and invalid/empty feed preservation. Existing resolver and settlement suites also pass.

Final desktop PID 32884 started successfully with production schema 32. Its automatic startup cycle at epoch 1789340727 completed in 1,676 ms with no errors, no unresolved events and no duplicate publication. Actual desktop manual stress verification then submitted 12 requests while RUNNING: all returned PENDING_RERUN, only one follow-up ran, cycles advanced from 1 to 3 and returned IDLE without errors. Those two cycles took 2,105 and 2,033 ms and both skipped unchanged production. Mean of these three final-build no-change cycles is 1,938 ms (startup/manual mix, not three scheduled ticks). Evidence: `.tmp-dataflow/final-startup-verification.json`, `desktop-coalescing.json`, `final-data-center.png`, `recovery-final-validated-tests.txt`, `recovery-final-frontend.txt`.

Another observed successful automatic pair started at epochs 1789340324 and 1789340624, also exactly 300 seconds apart. The final lock adjustment was startup/manual verified; the scheduled pairs above were measured before that last adjustment. Real Data Center screenshot shows automatic refresh active and general readiness 10/10 READY.

Real physical Windows sleep/resume, an actual Istanbul midnight transition, and backend network outage injection have not been performed. These paths have clock/fixture regression coverage, which is not a substitute for the requested desktop fault simulations. Do not represent these acceptance cases as completed.
