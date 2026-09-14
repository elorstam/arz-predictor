# ARZ product rules recovery — final acceptance

Accepted against the real production AppData database and running Tauri desktop on 2026-09-11T22:26:38.044Z (Europe/Istanbul business date: September 12). September 13 and 14 supply the required future-date coverage. No additional supported future date is available in this bulletin. This report supersedes the underfilled-coupon acceptance in DAILY_PIPELINE_RECOVERY_REPORT.md.

**Production output**

| Date | Published run | Candidates | Corner | Over 2.5 | Over 3.5 | BTTS | High Confidence | Katlama | Surprise | READY coupons |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 2026-09-12 | 54 | 63 | 16 | 2 | 1 | 0 | 25 | 1 | 18 | 3 |
| 2026-09-13 | 55 | 47 | 16 | 3 | 3 | 0 | 11 | 2 | 12 | 3 |
| 2026-09-14 | 56 | 13 | 0 | 1 | 1 | 0 | 7 | 1 | 3 | 1 |

Counts are category-qualified entries, not unique fixtures across categories. Before this recovery the dates had 15/13/4 candidates; earlier 3/3/2 coupon counts included underfilled Over coupons. The current 3/3/1 READY counts enforce the restored rules. Predictions and odds remain real; thresholds were not relaxed.

| Date | Published type | Selections | Combined odds | System columns |
|---|---|---:|---:|---:|
| 2026-09-12 | DAILY_SURPRISE_SYSTEM | 7 | Per-column prices | 29 |
| 2026-09-12 | DAILY_HIGH_CONFIDENCE | 8 | 3.13358846 | — |
| 2026-09-12 | DAILY_CORNERS | 7 | 33.36645786 | — |
| 2026-09-13 | DAILY_SURPRISE_SYSTEM | 7 | Per-column prices | 29 |
| 2026-09-13 | DAILY_HIGH_CONFIDENCE | 10 | 3.25995773 | — |
| 2026-09-13 | DAILY_CORNERS | 7 | 31.69069600 | — |
| 2026-09-14 | DAILY_HIGH_CONFIDENCE | 7 | 3.20218784 | — |

Corner publishes 5–7 selections. Over 2.5, Over 3.5, BTTS Yes and High Confidence require at least five; High Confidence adds the highest-confidence qualified distinct fixtures until reaching at least 3.00 where possible. Surprise retains its existing 6/7-selection system rules. Katlama requires 2–3 selections in the 1.50–1.80 range and optimizes toward 1.80. Every published accumulator and every Surprise column was checked against the product of its actual selection odds.

**Exact shortages**

- September 12: Over 2.5 has 2/5, Over 3.5 1/5, BTTS 0/5, Katlama 1/2.
- September 13: Over 2.5 has 3/5, Over 3.5 3/5, BTTS 0/5. Katlama has two selections, but 1.28 × 1.16 = 1.4848, below 1.50.
- September 14: Corner 0/5 because none of the eight prediction-ready matches has the required corner odds. Over 2.5 1/5, Over 3.5 1/5, BTTS 0/5, Katlama 1/2, Surprise 3/6.
- All 70 prediction-ready BTTS Yes selections have usable joined odds, but all 70 fail the existing 0.64 public-probability requirement. This is a policy result, not a missing join.

**Supported coverage and lineup**

| Date | Supported | Safely resolved | Prediction/odds ready | Supported unresolved | Resolved, insufficient history |
|---|---:|---:|---:|---:|---:|
| 2026-09-12 | 50 | 49 | 39 | 1 | 10 |
| 2026-09-13 | 29 | 28 | 23 | 1 | 5 |
| 2026-09-14 | 9 | 9 | 8 | 0 | 1 |

Supported resolution is **86/88 = 97.73%** across 10 current supported competitions. The bulletin contains 211 competitions and 1,151 scheduled records; 88 are supported and 1,063 are outside coverage. Today’s live upcoming count was 647 at UI acceptance, down from the archived 651 as kickoffs passed. The 2,372 retained Iddaa events in Data Center are a different, historical-inclusive universe: 176 supported, 171 resolved, 2,196 unsupported. The two current unresolved cases need canonical historical identities for Paderborn and Elversberg. No unsafe aliases or lower confidence thresholds were introduced.

BASE is operational. Lineup implementation exists and its tests pass. Production has zero authoritative lineup snapshots, zero lineup feature sets and zero trained lineup adjustment artifacts, so no artifact can truthfully be generated from the existing production inputs. The live provider is unconfigured. Data Center now states these three facts separately and says BASE remains usable. No lineups were fabricated; no model training or calibration architecture was changed.

**Root causes and repairs**

1. Provider full-time corner market 2_48 was left UNKNOWN. It is now mapped deterministically to full-time total corners; first-half 2_49 remains separate. The immutable odds view also bridges BOTH_TEAMS_TO_SCORE → BTTS. Raw odds records and timestamps remain unchanged. Source: [Iddaa public market configuration](https://sportsbookv2.iddaa.com/sportsbook/get_market_config).
2. The quality gate read current-competition-season history counts of zero despite immutable feature snapshots already containing ten prior matches. Candidate evaluation now uses the recorded historical sample, and newly generated quality metadata uses actual prior history. This restored Corner and High Confidence pools without changing thresholds.
3. Non-bettable 1.00 prices entered candidate pools. They are now excluded as INVALID_ODDS. All published selection prices exceed 1.00.
4. Daily coupon generation lacked product minimums. Publication status now distinguishes DRAFT, INSUFFICIENT, READY and SETTLED. Generated coupons need the current coupon policy, publication marker and type compliance; underfilled manual drafts cannot finalize. The internal editing lifecycle still uses the historical DRAFT column for generated snapshots, while publication_status is the authoritative user-facing readiness value. Old audit rows remain intact.
5. Katlama now verifies persisted settlement, preserves pending steps across candidate publications, and resets after a loss or seventh win. Its current pending coupon is loaded independently of the date filter, with the original business date visible.
6. Readiness navigation and refresh repeated expensive checks. Pages share a readiness result; footer refresh consumes that same result. Candidate caches check the authoritative publication ID on every read; date-specific match caches expire after 60 seconds and explicit refresh/import invalidates them. Navigation does not generate candidates or coupons.

**Run selection and deterministic replay**

The business date selects daily_output_publications.candidate_run_id. Replay/comparison insertions cannot silently replace that publication. Intended live refreshes may supersede it, including a genuinely empty live result; stale selections are not retained to preserve counts. Candidate/coupon generation and publication use one transaction. Exact as-of replay considers only predictions/features/odds available at that instant and orders revisions by actual timestamp, then ID. The implementation fingerprint is daily-v6; coupon policy is coupon_policy_v2_minimums. Published production runs remain 54/55/56 after navigation and restarts; total persisted candidate runs remain 56.

**Katlama persistence and settlement**

The real production state is unstarted, displayed as **Adım 1 / 7**. No current date has a compliant Katlama combination, and no artificial step or settlement was inserted. The existing series/step tables retain coupon links, selected legs, odds, result and timestamps. Migration 0023 adds an idempotent transition audit with from/to steps and WON_ADVANCE, LOSS_RESET or SEVEN_STEPS_COMPLETED reasons. Transitions and settlement commit transactionally.

Tests verify pending cannot advance; actual WON advances; LOSS resets immediately; seventh WON resets to step 1 for a new cycle; reopening the database retains state; empty runs/refreshes do not reset it; an attached pending coupon remains visible after publication replacement and across dates. Seven-step and loss tests settle actual fixture selections, rather than calling an unchecked success flag.

**Logos**

| READY | PENDING | RATE_LIMITED | UNAVAILABLE | INVALID | Known teams |
|---:|---:|---:|---:|---:|---:|
| 34 | 2790 | 112 | 33 | 0 | 2969 |

Before: approximately 3 ready. Final validated cache size: 1,132,017 bytes. PENDING includes unrequested teams; it does not mean thousands of queued downloads. Only a bounded set of current supported teams is considered (32); a single worker processes at most 24 items, with spacing and visible-team priority. Requests are deduplicated. Cached validated PNGs return immediately. Retry-After and exponential backoff persist across restart, provider rate limits pause that provider, and invalid/unavailable files use initials.

A small catalog contains provider team-name/crest metadata, not bundled logos. Exact identities come from [football-data.org public documentation](https://www.football-data.org/documentation/quickstart). The public crest host supplies validated PNG images; documented SVG IDs also have PNG renditions (Arsenal and Bayern responses verified). A missing rendition remains unavailable. Wikipedia fallback is rate-limited and is not hammered. This source is useful for documented major teams, not a claim of universal 3,000-team coverage. Real desktop screenshots show Augsburg, Leverkusen, Burnley and Inter crests; unsupported/missing teams show clean initials.

**Performance**

Single-run desktop DOM measurements, not percentile benchmarks. The earlier recovery pass measured Candidates 4,709 ms, Coupons 5,404 ms, Today 3,081 ms and Data Center 19,928 ms. After caching and removing repeated work:

| Page | Initial final acceptance / first entry | Warm navigation |
|---|---:|---:|
| Bugün | 250 ms | 241 ms |
| Günün Adayları | 2370 ms | 115 ms |
| Günün Kuponları | 2110 ms | 68 ms |
| Veri Merkezi | 2 ms | 17 ms |

Explicit full readiness refresh: 9856 ms, reduced from 24,090 ms after removing the duplicate footer audit. This diagnostic remains relatively expensive; ordinary page navigation uses the shared result. First-entry measurements include coming from the Populars page and are not directly comparable to warm navigation. The previous task’s initial Candidates/Coupons figures were 2,467/1,756 ms with much smaller candidate pools; the current result is 2,370/2,110 ms with 63 entries. No blanket cold-load speedup is claimed.

**Real desktop acceptance**

- Today: 647 upcoming matches, 63 candidates, exactly three READY coupons; footer and Data Center both Kısmen hazır, BASE capabilities true.
- Candidates: September 12/13/14 show runs 54/55/56 and 63/47/13 entries with real names, markets, probability, odds, edge and EV. Rapid date changes retain the intended result.
- Coupons: all three dates inspected, minimums enforced, actual selections visible, total and system-column odds verified. September 14 High Confidence displays seven selections at 3.20; Katlama displays step 1 and the real shortage.
- Data Center: separate implementation/artifact/provider lineup facts, supported resolution universe, five disjoint logo states, optional limitations separate from core readiness.
- Evidence: [desktop screenshots and text](.tmp-dataflow/rules-ui), [UI timings](.tmp-dataflow/rules-ui/results.json), [final navigation/refresh timings](.tmp-dataflow/rules-ui/final-inspection.json), [runtime assertions](.tmp-dataflow/rules-runtime.json), [complete market audit](.tmp-dataflow/product-rules-audit.json).

**Market audit**

Pred/public/calibrated counts refer to selected latest prediction rows at each published as-of, not unique matches. High Confidence/Katlama inspect multiple markets per match. Public probabilities can legitimately use CALIBRATION_NOT_BENEFICIAL fallback; these are not counted as CALIBRATED_V1. Odds-ready rows require a real mapped price above 1.00 within the cutoff window. Exclusions below are evaluated rows, and several rows can belong to one match. Coverage/history exclusions were listed separately above and remain fully persisted in the JSON audit.

| Date | Category | Predicted matches | Prediction rows | Public rows | Calibrated rows | Odds-ready rows | Qualified | Selected | Exclusions on model rows |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|
| 2026-09-12 | Corner | 39 | 195 | 195 | 195 | 39 | 16 | 7 | LOW_PUBLIC_PROBABILITY=23, ODDS_UNAVAILABLE=156 |
| 2026-09-12 | Over 2.5 | 39 | 39 | 39 | 0 | 39 | 2 | 0 | LOW_EDGE=4, LOW_PUBLIC_PROBABILITY=33 |
| 2026-09-12 | Over 3.5 | 39 | 39 | 39 | 0 | 39 | 1 | 0 | LOW_EDGE=1, LOW_PUBLIC_PROBABILITY=37 |
| 2026-09-12 | BTTS Yes | 39 | 39 | 39 | 39 | 39 | 0 | 0 | LOW_PUBLIC_PROBABILITY=39 |
| 2026-09-12 | High Confidence | 39 | 2145 | 2145 | 1599 | 499 | 25 | 8 | CORRELATION_REDUCTION=1, INVALID_ODDS=8, LOW_PUBLIC_PROBABILITY=473, ODDS_UNAVAILABLE=1638 |
| 2026-09-12 | Katlama | 39 | 2145 | 2145 | 1599 | 499 | 1 | 0 | CALIBRATION_INSUFFICIENT=30, INVALID_ODDS=8, LOW_PUBLIC_PROBABILITY=468, ODDS_UNAVAILABLE=1638 |
| 2026-09-13 | Corner | 23 | 115 | 115 | 115 | 23 | 16 | 7 | LOW_PUBLIC_PROBABILITY=7, ODDS_UNAVAILABLE=92 |
| 2026-09-13 | Over 2.5 | 23 | 23 | 23 | 0 | 23 | 3 | 0 | LOW_EDGE=4, LOW_PUBLIC_PROBABILITY=16 |
| 2026-09-13 | Over 3.5 | 23 | 23 | 23 | 0 | 23 | 3 | 0 | LOW_EDGE=1, LOW_PUBLIC_PROBABILITY=19 |
| 2026-09-13 | BTTS Yes | 23 | 23 | 23 | 23 | 23 | 0 | 0 | LOW_PUBLIC_PROBABILITY=23 |
| 2026-09-13 | High Confidence | 23 | 1265 | 1265 | 943 | 291 | 11 | 10 | CORRELATION_REDUCTION=6, INVALID_ODDS=6, LOW_PUBLIC_PROBABILITY=274, ODDS_UNAVAILABLE=968 |
| 2026-09-13 | Katlama | 23 | 1265 | 1265 | 943 | 291 | 2 | 0 | CALIBRATION_INSUFFICIENT=22, INVALID_ODDS=6, LOW_PUBLIC_PROBABILITY=267, ODDS_UNAVAILABLE=968 |
| 2026-09-14 | Corner | 8 | 40 | 40 | 40 | 0 | 0 | 0 | ODDS_UNAVAILABLE=40 |
| 2026-09-14 | Over 2.5 | 8 | 8 | 8 | 0 | 8 | 1 | 0 | LOW_EDGE=1, LOW_PUBLIC_PROBABILITY=6 |
| 2026-09-14 | Over 3.5 | 8 | 8 | 8 | 0 | 8 | 1 | 0 | LOW_PUBLIC_PROBABILITY=7 |
| 2026-09-14 | BTTS Yes | 8 | 8 | 8 | 8 | 8 | 0 | 0 | LOW_PUBLIC_PROBABILITY=8 |
| 2026-09-14 | High Confidence | 8 | 440 | 440 | 328 | 88 | 7 | 7 | CORRELATION_REDUCTION=1, LOW_PUBLIC_PROBABILITY=80, ODDS_UNAVAILABLE=352 |
| 2026-09-14 | Katlama | 8 | 440 | 440 | 328 | 88 | 1 | 0 | CALIBRATION_INSUFFICIENT=7, LOW_PUBLIC_PROBABILITY=80, ODDS_UNAVAILABLE=352 |

**Validation and changed files**

Passed cargo fmt --check, cargo check, desktop cargo build, npm build and all nine frontend tests. Targeted Rust suites passed: coupon engine 16; daily output 3; assets 3; Data Center 5; feature-related 4; migrations 11; lineup tests; the new provider Corner/BTTS/cross-season/invalid-odds regression. Existing unused-variable warnings remain. No licensing/updater acceptance, model training or full calibration was rerun.

- src-tauri/src/repositories/candidate_engine.rs, features.rs, coupon_engine.rs, data_center.rs: quality, odds joins, publication, series state, readiness.
- src-tauri/src/assets.rs and crest_catalog.json: bounded cache-first downloads, public metadata, validation and durable backoff.
- src-tauri/src/providers/iddaa/markets.rs; migrations/0023_asset_queue_and_series_audit.sql and 0024_iddaa_corner_market.sql; database/migrations.rs: deterministic corner mapping, immutable odds compatibility view and durable queue/series audit.
- src-tauri/src/commands.rs and lib.rs: publication lookup, transactional settlement/step creation and asset commands.
- src/App.tsx, pages/CandidatesPage.tsx, CouponsPage.tsx, TodayPage.tsx, DataCenterPage.tsx, components/MatchRow.tsx, StatusBadge.tsx, services/tauri.ts, types.ts: authoritative publication display, shortages, Katlama controls/history, background logo refresh and shared readiness.
- coupon_engine_tests.rs, daily_output_tests.rs, candidate_engine.rs tests, assets.rs tests, providers/iddaa/markets.rs tests, services/dailyCache.test.ts; migration-version expectations in database/tests.rs, calibration_tests.rs, prediction_engine_tests.rs and features_tests.rs. Existing broader recovery changes were preserved.

Production backup before this task’s migrations/reprocessing: .tmp-dataflow/production-2026-09-11T21-27-16-509Z.sqlite3. License state, settings, prediction/performance history and audits were preserved. New migrations are additive; original odds snapshots remain immutable. Licensing, release/updater and ARZ License Admin architecture were not modified.

ARZ PRODUCT RULES RECOVERY COMPLETE
