**Daily Candidates recovery — 2026-09-13, Europe/Istanbul**

Verified in the real Tauri desktop executable with production database C:\Users\husey\AppData\Roaming\com.footballpredictor.app\football-predictor.sqlite3. Database health reports that path and 25 applied migrations. Licensing, updater and prediction models were not modified by this recovery. Existing worktree changes were preserved. No production prediction, odds, candidate or coupon snapshots were rewritten.

The originally reported #79 / zero-row state is explained by the old committed implementation: Candidates sent runId=null and the backend selected ORDER BY generated_at DESC,id DESC. #79 shares #63's timestamp, has parent #63, declares LINEUP_AWARE, but contains zero matches, zero candidates, null lineup model/hash, and the SHA-256 of empty revision context. The old prediction loader compared prediction_runs.generated_at (the feature-information cutoff, often kickoff) with the candidate generation timestamp. Re-running that old filter in production returns **0**; the existing BASE snapshots reference prediction runs whose cutoffs are on September 13. The current loader uses predictions.created_at, a fix already present in the working tree when this task started.

At this session's first actual desktop inspection, the earlier explicit-publication fix was already running: both implicit and explicit API reads returned **#63 BASE / 59 rows**, so the original #79 screen was not falsely claimed as reproduced. Remaining observed defects were Candidates omitting BTTS #80, duplicate All rows, and policy-version filtering hiding three populated coupons. Before/after JSON and screenshots preserve that distinction.

| Tab | Reported empty #79 (derived from zero persisted rows) | First live inspection | Final desktop |
|---|---:|---:|---:|
| Tümü | 0 | 59 | 64 |
| Korner | 0 | 16 | 16 |
| 2.5 Üst | 0 | 3 | 3 |
| 3.5 Üst | 0 | 3 | 3 |
| KG Var | 0 | 1 | 17 |
| Yüksek Güven | 0 | 11 | 11 |
| Sürpriz | 0 | 8 | 8 |
| Katlama | 0 | 17 | 17 |

The final common publication identity is **2026-09-13:base:63:btts:80**. Today, Candidates and Coupons obtain the same atomic database snapshot through daily_selection_output. The explicit BASE publication stays **#63**, generated **2026-09-11T23:22:51.001116400+00:00**, model **arz-live-2026-09-06**, candidate policy **candidate_policy_v1**. BTTS is an explicitly named category revision **#80**, generated **2026-09-13T13:23:54.782770100+00:00**, policy **candidate_policy_v2_daily_goal_ranking**. A category revision is not a lineup revision, and does not replace other BASE categories. The manifest lists all category source IDs.

Persisted sources: #63 has **59** rows; #80 has **17**. The former one-row BTTS pool in #63 is superseded by #80, so the frontend DTO contains **75 category rows** and All renders **64 unique selections**, keyed by event + market + outcome + line. The ranked policy contributes **17 DAILY_RANKED rows**. The 3 legacy Over 2.5 rows remain explicitly STRICT_QUALIFIED with their original policy provenance; they are not falsely presented as a freshly regenerated ranking. Current-policy Over 2.5 generation and BTTS generation expose DAILY_RANKED without requiring the old strict gates, covered by tests. HIGH_CONFIDENCE and KATLAMA_ELIGIBLE remain separate source statuses.

| Category | Exact published source pool / candidate tab | Persisted coupon selections | Final coupon visibility |
|---|---:|---:|---|
| Corner | 16 | 7 (#133) | READY |
| Over 2.5 | 3 | 0 | Below the 5-selection minimum |
| Over 3.5 | 3 | 0 | Below the 5-selection minimum |
| BTTS | 17 | 7 (#139) | READY |
| High Confidence | 11 | 10 (#134) | READY |
| Surprise | 8 | 7 (#135) | READY |
| Katlama | 17 | 2 (#136, unbound daily draft) | Existing pending series step is shown separately |

There are **33 persisted coupon-source selections** across these source runs, including the 2-selection unbound Katlama draft; **31** are in ordinary READY daily coupons. The visible Katlama series card deliberately retains its already pending **2026-09-12** step with 2 selections. It is not miscounted as a new September 13 daily publication. The September 13 eligible pool and search remain 17. Coupons #133–135 were hidden solely because their frozen policy was v2 rather than the current v3; their explicit published flag and current structural validation now preserve visibility. No missing Over 2.5/3.5 coupon was fabricated. Counts and prices are frozen publication snapshots, not an assertion that every match is still pre-kickoff or every old price is current at inspection time.

Prediction lineage: #63 candidates reference prediction runs **252–274**; #80 references **283–299** (exact participating IDs are in database-trace.json). No LINEUP_AWARE prediction revision is selected. Authoritative complete official lineup snapshots: **0**. Publication selection rejects invalid lineup runs; the label additionally requires verified official data, a persisted available revision with matching schema and model hash, a linked candidate, and selected LINEUP_AWARE context. All three pages display BASE.

Six displayed selections were checked against persisted candidate, prediction, prediction-run and odds IDs; five also match actual coupon-source records. Every coupon-source candidate ID was checked for inclusion in its category pool. UI formatting was compared with exact backend values, including EV=p×odd−1 and confidence where stored. A dash represents missing confidence rather than an invented number.

| Category | Candidate | Prediction / run | Odds snapshot | Probability | Odds | EV | Confidence score | Coupon source |
|---|---:|---|---:|---:|---:|---:|---|---|
| CORNERS | 1075 | 14762 / 269 | 228564 | 59.139204% | 1.55 | -8.334234% | not stored | yes |
| HIGH_CONFIDENCE | 1091 | 14796 / 270 | 221812 | 96.311470% | 1.08 | 4.016388% | not stored | yes |
| SURPRISE | 1108 | 13861 / 253 | 209765 | 51.182460% | 3.17 | 62.248397% | not stored | yes |
| BTTS_YES | 1134 | 15519 / 287 | 271099 | 69.641442% | 1.42 | -1.109152% | 0.997256516 | yes |
| COMPOUND | 1058 | 14576 / 266 | 221993 | 89.463545% | 1.28 | 14.513337% | not stored | yes |
| OVER_25 | 1102 | 14801 / 270 | 221792 | 95.727865% | 1.10 | 5.300651% | not stored | pool only; coupon below minimum |

Cache audit: there is no React Query. useAsync uses a request sequence and dependency identity to suppress late responses and immediately conceal data from another date. Daily requests are keyed by business date and only coalesced while in flight; resolved results, including empty results, are not retained. Every refresh re-reads BASE and category revision IDs together. Categories are local views of one immutable response, so an empty tab cannot poison other tabs. The manifest contains the business date, BASE run and BTTS revision. Actual rapid tab and September 12 → 14 → 13 date changes passed.

Validation: frontend build and all **13 Vitest tests passed**; all **20 coupon-engine tests passed**; daily-selection/ranking/publication suite **11 passed**; empty-newer-BASE regression passed; BTTS pipeline tests passed. Archived daily-output test counts were corrected to reflect their zero-history goal inputs; tests now explicitly verify INSUFFICIENT_HISTORY and use the populated Surprise pool for category/cache isolation. Model thresholds were not changed.

Desktop acceptance artifacts: [final screen](.tmp-dataflow/candidates-recovery/final.png), [all eight tab counts and six cross-checks](.tmp-dataflow/candidates-recovery/acceptance.json), [atomic frontend DTO](.tmp-dataflow/candidates-recovery/output.json), [database lineage](.tmp-dataflow/candidates-recovery/database-trace.json), [Today](.tmp-dataflow/candidates-recovery/today.png), [Coupons](.tmp-dataflow/candidates-recovery/coupons.png). A separate Model Performance navigation stalled desktop calls during inspection; restarting cleared it and the complete acceptance rerun passed. That unrelated page was not changed.
