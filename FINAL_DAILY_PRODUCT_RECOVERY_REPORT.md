# Final daily product recovery — 14 September 2026

Production desktop inspected through its real Tauri WebView, using C:/Users/husey/AppData/Roaming/com.footballpredictor.app/football-predictor.sqlite3. Current Istanbul date: 2026-09-14. Today, Candidates, Coupons and Populars share publication **2026-09-14:base:93:btts:93**, BASE candidate run 93. No synthetic scores, odds or probabilities were injected into production.

## Populars

| Measure | Before | After |
|---|---:|---:|
| Visible / linked provider identities | 21 | 20 |
| Unresolved | 0 | 0 |
| No exact market model | 3 | 3 |
| Model supported | 6 | 1 |

The provider list refreshed during verification. The previous per-selection snapshot query retained obsolete entries and duplicate ranks (24 rows during the intermediate audit). The final page contains the current 20 provider selections, ranks 1–20. All 20 identity mappings and all available odds/probabilities were checked against persisted provider mappings, exact odds rows, candidates and predictions. Complete row-level evidence: [.tmp-dataflow/final-product/acceptance.json](.tmp-dataflow/final-product/acceptance.json), including prediction_trace.

Final states: {"MODEL_BELOW_POLICY":9,"MODEL_UNSUPPORTED":7,"MARKET_NOT_SUPPORTED":3,"MODEL_SUPPORTED":1}. A daily ranked candidate with negative EV is no longer automatically called model-supported. Existing policy exclusions remain explicit. No-market-model, no prediction, unresolved identity and invalid odds are distinct. Provider quote odds and display names are retained even for markets that odds normalization does not support. No fuzzy identity resolution was introduced. Prediction fallback is bounded by the selected publication cutoff.

## Exact corner selections

Seven unique current-day selections were inspected in both Candidates and Coupons: **14 rendered rows**, all seven current choices, today's pool contains seven distinct choices. Each line/outcome and odd matched the persisted selected odds row and prediction. Five examples were requested; all seven are included below.

| Match | Exact selection | Odds | Odds row | Prediction row |
|---|---|---:|---:|---:|
| Torino — Roma | Toplam Korner 8.5 Üst | 1.94 | 284889 | 15147 |
| Moreirense — Maritimo | Toplam Korner 8.5 Üst | 1.92 | 287611 | 15566 |
| Rio Ave — Estrela | Toplam Korner 8.5 Üst | 1.77 | 285313 | 15257 |
| Como — Parma | Toplam Korner 8.5 Üst | 1.66 | 287455 | 15092 |
| Gaziantep — Fenerbahce | Toplam Korner 8.5 Üst | 1.63 | 286900 | 15202 |
| Villarreal — Betis | Toplam Korner 9.5 Üst | 1.60 | 283985 | 15424 |
| Inter — Udinese | Toplam Korner 9.5 Üst | 1.83 | 283603 | 15314 |

Screenshots: [Candidates](.tmp-dataflow/final-product/corner-candidates.png), [Coupons](.tmp-dataflow/final-product/coupons.png), [Populars](.tmp-dataflow/final-product/populars.png).

## Goal coupons

Policy coupon_policy_v4_goals_3_to_5 sets both goal coupon minimums to 3 and maximums to 5. Ranking/quality/model probabilities are unchanged. Tests cover 2, 3, 4, 5, 7 and 8 available choices, including 3 READY and 2 insufficient. Historical policies and immutable historical selections retain their original values. Daily output deduplicates coupon types so a replacement policy cannot double Today's ready count.

| Type | Current pool | Published selections | Combined odds | State |
|---|---:|---:|---:|---|
| DAILY_OVER_25 | 9 | 5 | 11.9577321792 | READY |
| DAILY_OVER_35 | 1 | 0 | — (no published coupon) | INSUFFICIENT |

Today shows **4 ready coupons**. Other rules remain Corner 5–7, BTTS existing minimum 5, High Confidence existing strict policy, Surprise unchanged, Katlama 2–3 with existing 1.50–1.80 target and 1/7 progression. The new O2.5 coupon contains five of the nine existing pool choices. O3.5 has only one qualified choice today; no coupon was forced.

## Settlement and realized performance

| Measure | Count |
|---|---:|
| Total persisted coupons before / after | 170 / 171 |
| Raw storage DRAFT | 171 |
| Publication READY across retained records | 60 |
| Current-day READY | 4 |
| Published production coupons pending results | 75 |
| WON / LOST / VOID | 0 / 0 / 0 |
| settled_at populated | 0 |
| All retained selection rows | 770 |
| Production selection rows under automatic settlement | 512 |
| Past selections PENDING_DATA: FINAL_RESULT_UNAVAILABLE | 287 |
| Future selections PENDING: AWAITING_MATCH_COMPLETION | 225 |
| Result-known selection still pending | 0 |
| Published suggestions with no recorded stake | 74 |

READY is a publication state, and PENDING is a result state; these counts are deliberately not presented as mutually exclusive storage statuses. The raw DRAFT storage flag previously prevented all automatic settlement. Published production records are now processed automatically without manual finalization. Results are collected in bounded batches (256 selections, 64 completed coupons), atomically settled and audited. Replay records and unpublished drafts are excluded. Confirmed outcomes for suggestions without recorded stake can be persisted, but no monetary settlement is fabricated and they are excluded from realized financial metrics.

The single staked coupon is the active Katlama step: 100000 cents, step 1, still waiting for results. Tested automatic outcomes advance WON to the next step, reset LOST to 1, reset a step-7 win to 1, and leave missing-data cases unchanged.

Result ingestion invokes settlement after commit; normal Iddaa refresh invokes it; the startup worker resumes and rechecks every minute. Only past unsettled selections cause source refreshes, one due league per minute, with persisted 15-minute retry dates. The original final-result importer now acquires an immediate transaction to prevent read-to-write snapshot races with background writers. Its existing alias/resolver behavior is unchanged.

## Result source audit

The existing football-data.co.uk CSV provider supplies FTHG/FTAG for Match Result, totals and BTTS, and HC/AC for final corners. The primary endpoint timed out on this network. Automatic fallback to the project's existing public GitHub mirror successfully imported current-season rows. The source URL remains on the import run. A successful refresh means the supplied file was imported; it does not claim the provider has every recent result.

| Dataset | Last recorded refresh state | Detail |
|---|---|---|
| B1 / 2627 | REFRESHED | 45 rows; 45 updated; 0 failed |
| D1 / 2627 | PENDING_DATA | RESULT_DATASET_NOT_PUBLISHED |
| E0 / 2627 | REFRESHING | 30 rows; 20 updated; 0 failed |
| E1 / 2627 | REFRESHING | 69 rows; 48 updated; 0 failed |
| F1 / 2627 | REFRESHED | 27 rows; 19 updated; 0 failed |
| I1 / 2627 | REFRESHING | timeout: error sending request for url (https://www.football-data.co.uk/mmz4281/2627/I1.csv): client error (Connect): tcp connect error: deadline has elapsed |
| N1 / 2627 | PENDING_DATA | database is locked |
| P1 / 2627 | REFRESHED | 44 rows; 33 updated; 0 failed |
| SP1 / 2627 | REFRESHED | 41 rows; 31 updated; 0 failed |
| T1 / 2627 | REFRESHED | 36 rows; 27 updated; 0 failed |

These are timestamped audit states; failed/interrupted jobs retain retry dates and may advance after this report. The observed Dutch refresh encountered SQLITE_BUSY before the immediate-transaction fix. The Bundesliga 2026/27 dataset is absent from the configured verified catalog and remains PENDING_DATA. No unsupported dataset was invented.

Latest final result in the audited DB: **2026-09-10T19:15:00Z** (8060 final matches). Published production coupons await matches dated September 12–14. A direct score check and an exact canonical competition/home/away/date result lookup both find zero settleable pending coupon selections. Therefore the production Performance page is legitimately empty, now with the exact pending counts/reasons. A bulletin-removal 'cancelled' flag is never treated as proof of VOID. Corners without actual final corner counts remain PENDING_DATA even when a final score exists.

Realized metrics use persisted coupon/column odds and actual recorded stake, never backtests. Count, wins/losses/void, hit rate, stake, gross return, P&L, ROI, average odds and win/loss streaks are calculated. Current realized counts/stake/return/profit are 0; hit rate, ROI and average odds are null. Performance window option values were corrected, so the visible filter matches its backend request. [Actual Performance screenshot](.tmp-dataflow/final-product/performance.png).

## Preservation and validation

The pre-task backup is .tmp-dataflow/production-2026-09-13T20-58-24-984Z.sqlite3. All **765 pre-existing coupon selection snapshots** were compared field-by-field, excluding their allowed settlement status field; their odds, line/outcome, predictions and source references are unchanged. Only a new five-leg goal coupon was added. Licensing, updater, License Admin, BASE architecture, resolver architecture and logo pipeline were not changed.

- cargo check: PASS (two existing warnings).
- Targeted coupon tests: 34 PASS; additional automatic-outcome tests: 4 PASS (overlapping suite; includes two additional settlement tests).
- Populars tests: 10 PASS.
- Final-result provider tests: 26 PASS, 4 explicitly ignored controlled/network tests. Obsolete catalog expectations were updated to the already configured 95/65 dataset counts; catalog configuration was not changed.
- Frontend: 17 PASS; npm run build PASS.
- Rust formatting for every changed Rust file: PASS.
- Repository-wide cargo fmt --check: **FAIL** on 13 pre-existing files outside this change, including assets.rs and logo_discovery.rs. Full output: [.tmp-dataflow/final-fmt-check.txt](.tmp-dataflow/final-fmt-check.txt). The logo pipeline was explicitly excluded from this task, so its existing formatting was preserved.

Desktop functional acceptance passed. The requested all-checks-clean completion remains blocked by the existing repository-wide formatting gate. The live source coverage limits and seven-unique-corner sample limit are disclosed above.
