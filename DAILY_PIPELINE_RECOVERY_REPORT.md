# ARZ daily pipeline recovery

Acceptance: 2026-09-12, approximately 00:20 Europe/Istanbul. Real desktop Tauri app, production AppData SQLite database. September 12 is the current business date; September 13 and 14 are future supported dates. No additional future supported date was present in the available bulletin. All three dates were regenerated and inspected in the desktop UI.

## Verified output

| Business date | Scheduled Iddaa matches | Supported | Resolved | Prediction ready | Odds ready | Published run | Candidates | Usable coupons |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 2026-09-12 | 651 | 50 | 49 | 39 | 39 | 44 | 15 | 3 |
| 2026-09-13 | 396 | 29 | 28 | 23 | 23 | 47 | 13 | 3 |
| 2026-09-14 | 92 | 9 | 9 | 8 | 8 | 50 | 4 | 2 |

Candidate counts are category-qualified entries, not distinct fixtures. A market can qualify for multiple coupon categories; category labels now distinguish these entries in the all-category view. All coupon selections reference actual candidates from the published run.

| Date | Coupon type | Selections | Combined decimal odds | System columns |
|---|---|---:|---:|---:|
| 2026-09-12 | DAILY_SURPRISE_SYSTEM | 7 | Per-column odds | 29 |
| 2026-09-12 | DAILY_OVER_35 | 1 | 2.19 | — |
| 2026-09-12 | DAILY_OVER_25 | 2 | 2.5665 | — |
| 2026-09-13 | DAILY_SURPRISE_SYSTEM | 7 | Per-column odds | 29 |
| 2026-09-13 | DAILY_OVER_35 | 3 | 6.53058 | — |
| 2026-09-13 | DAILY_OVER_25 | 3 | 2.2946 | — |
| 2026-09-14 | DAILY_OVER_35 | 1 | 2.61 | — |
| 2026-09-14 | DAILY_OVER_25 | 1 | 1.66 | — |

All accumulator products and every system-column product were checked against selection odds. Surprise systems have 29 columns from seven selections; a single combined price would misrepresent this coupon. Corner, BTTS, High Confidence and Compound remain honestly unavailable when their policies cannot be satisfied; Surprise is also unavailable on September 14. No thresholds were loosened to fill them. The previous verified 15/13/4 candidates and 3/3/2 usable coupons are preserved.

## Root causes and fixes

1. **Date-request race:** a slower response for the previous date could replace the selected date's output. The asynchronous hook now rejects obsolete responses and errors, cancels on unmount, and masks previous-key data while a new date loads. Rapid September 11 → 13 → 14 changes pass on both pages.
2. **Unclear run publication:** database insertion order mixed BASE, LINEUP_AWARE comparison and replay runs. The daily publication table explicitly identifies the intended candidate run. Candidate, coupon and Today reads use that same publication. Candidate generation, coupon generation and publication commit in one transaction.
3. **Replay/live confusion:** historical replay formerly depended on mutable match status and timestamp string ordering. Explicit replay uses its normalized UTC as-of instant, predictions/features/odds available at that cutoff, and kickoff-based status at that instant. Latest revisions use timestamp then ID. Finished/live current status does not retroactively invalidate a pre-kickoff replay. Requests for past dates without an explicit as-of return the archived publication; when none exists, they require an explicit replay timestamp.
4. **Category cache contamination:** category-only generation could persist a partial run under the full-run fingerprint. Generation now persists the complete result and applies category filtering to its returned view.
5. **Timezone error:** Iddaa import used the UTC calendar date. It now derives Europe/Istanbul business dates. Migration 0022 corrected 178 stored match dates, including 83 scheduled matches, with audit entries. Supported fixtures did not change dates. Today's schedule corrected from 644 to 651.
6. **Navigation cost:** daily pages loaded/classified the entire 1,151-match bulletin; navigation repeated Data Center work and coupon loads triggered lineup-impact computation. Daily queries are date-specific, unsupported coverage exits before history queries, relevant indexes were added, odds lookup is shared across category evaluation, and metadata parsing is cached by current file digest. Readiness is shared and lineup impact runs only on explicit request.

Audit evidence does not support claiming that later empty September 13/14 database runs had erased their valid output at task start: the valid outputs were still stored. The UI race and ambiguous default reads explain the disappearance; explicit replay/live publication also prevents future silent replacement.

## Deterministic selection and publication

- Default reads resolve the business date's daily_output_publications row, not MAX(id) across all contexts.
- Migration 0021 initializes existing publications from completed BASE runs ordered by actual generated_at time, then ID.
- An explicit REPLAY can initialize an unpublished date but cannot replace an existing publication, even when it inserts a newer empty run.
- An intentional LIVE generation can supersede the publication only at an equal or later as-of instant. A legitimately empty live result may supersede old output; stale candidates are not retained just to keep counts positive.
- Coupon reads and regeneration use the selected candidate run. Empty drafts and cancelled drafts are excluded from usable daily output; finalized historical coupons remain preserved.
- Fingerprints include the normalized as-of instant, model/calibration/policy configuration, selected prediction and odds inputs, and relevant match/data-quality state. Identical replay inputs return the identical persisted response, including IDs.
- The regression runtime used 2026-09-11T20:36:00Z and equivalent 2026-09-11T23:36:00+03:00. Both returned identical results. Early empty replays at 2026-09-11T00:00:00Z left each prior publication and its coupons unchanged.

| Date | Replay run | Replay candidates | Empty replay run | Final live run | Live generated_at (UTC) |
|---|---:|---:|---:|---:|---|
| 2026-09-12 | 42 | 15 | 43 | 44 | 2026-09-11T21:15:09.677848700+00:00 |
| 2026-09-13 | 45 | 13 | 46 | 47 | 2026-09-11T21:15:14.816983300+00:00 |
| 2026-09-14 | 48 | 4 | 49 | 50 | 2026-09-11T21:15:17.437027100+00:00 |

After all UI navigation and rapid-date tests, the database still contained exactly 50 candidate runs, maximum ID 50. Navigation did not generate new runs. Published runs remained 44/47/50, with 15/13/4 candidates and 3/3/2 usable coupons. The archived September 11 publication #26 remained 0/0; its recorded exclusions are listed below rather than regenerated against today's clock.

## Performance

Measured through the actual desktop DOM until correct date/run/count content was present. These are local single-run observations, not percentile claims.

| Page/date | Final load/change time |
|---|---:|
| candidates-2026-09-12 | 2467 ms |
| candidates-2026-09-13 | 453 ms |
| candidates-2026-09-14 | 156 ms |
| coupons-2026-09-12 | 1756 ms |
| coupons-2026-09-13 | 254 ms |
| coupons-2026-09-14 | 91 ms |
| today | 1545 ms |

Before: the global bulletin request blocking daily navigation took 16,257 ms. After: 516 ms for that same global request, while daily pages now request only their date. The final candidate read took 308 ms and coupon read 16 ms. Full explicit Data Center status remains approximately 4.13 seconds; it is shared/cached for ordinary navigation instead of repeated on every tab. Today is 1.55 seconds in the final acceptance run. No global import, prediction generation or calibration is triggered by page navigation.

## UI acceptance

- Today: visually verified 651 matches, 15 candidates and 3 selection-bearing coupons; counts match production publication #44.
- Candidates: visually verified dates September 12, 13 and 14 with real names, markets, probabilities, odds and category labels; runs #44/#47/#50. Rapid date-change race passed.
- Coupons: visually verified all three dates, real selections and correct displayed prices; unavailable categories show unavailable state. No empty draft was counted as usable. Rapid date-change race passed.
- Screenshots and DOM transcripts: [.tmp-dataflow/daily-ui](.tmp-dataflow/daily-ui). [UI measurements](.tmp-dataflow/daily-ui/results.json).

## Validation

Passed cargo fmt --check, cargo check and cargo build. Targeted Rust suites passed: candidate engine (3), coupon engine (14), daily output regressions (3), Data Center (5), migrations (7), changed-byte metadata cache validation (1), and Istanbul bulletin business-date rollover (1). The relevant migration/daily suites were repeated after migration 0022. Frontend tests: 8 passed; npm build passed after the final category-label change.

New daily regression coverage proves: empty replay cannot hide a publication; coupon source run agrees; historical status and timezone-equivalent replay are deterministic; category-first requests cannot poison the full run; zero-selection drafts are unusable; and the real public September 12/13/14 snapshot preserves 15/13/4 and 3/3/2. The fixture contains public match/prediction/odds input only, no production license or settings state. No model training, full calibration or Phase 13/14 acceptance was performed.

## Files changed for this daily recovery

- src-tauri/migrations/0021_daily_output_publications.sql; 0022_iddaa_business_dates.sql; src-tauri/src/database/migrations.rs and tests.rs: publication, indexes, audited business-date repair.
- src-tauri/src/repositories/candidate_engine.rs; coupon_engine.rs; current_flow.rs: deterministic as-of evaluation, publication and atomic daily generation.
- src-tauri/src/repositories/iddaa.rs; model_coverage.rs; data_center.rs; src-tauri/src/commands.rs; lib.rs: date-scoped queries and consistent current publication.
- src-tauri/src/providers/iddaa/orchestrator.rs; providers/football_data/cache.rs: Istanbul date semantics and digest-keyed parsed metadata cache.
- src-tauri/src/repositories/daily_output_tests.rs; candidate_engine_acceptance_tests.rs; coupon_engine_tests.rs; mod.rs; src-tauri/tests/fixtures/daily_2026_09_12_14.json: regression coverage.
- src/hooks/useAsync.ts; src/lib/latestRequest.ts and latestRequest.test.ts: obsolete-request protection.
- src/App.tsx; src/services/tauri.ts; src/types.ts; src/pages/CandidatesPage.tsx; CouponsPage.tsx; TodayPage.tsx; src/components/MatchRow.tsx: date-specific reads, shared readiness, explicit lineup impact and category labeling.
- DAILY_PIPELINE_RECOVERY_REPORT.md and .tmp-dataflow daily audit/runtime/UI scripts and evidence.

The working tree already contained earlier product-recovery changes. This list describes the daily recovery work, not a claim that every current git modification originated here. Licensing, updater/release, License Admin and model training/calibration architecture were not changed for this task.

## Data safety and remaining limitations

Production backup before migrations: .tmp-dataflow/production-2026-09-11T20-50-36-934Z.sqlite3. License state, settings, historical prediction/performance records and audit history were retained. Migrations are versioned; business-date corrections append audit records.

No external credential or data-source blocker remains for this daily-output acceptance. This does not expand statistical coverage to every Iddaa competition or supply an absent lineup provider. Unsupported fixtures and insufficient history remain explicit exclusions. Live counts can legitimately change with kickoff, fresh odds, predictions or policy inputs; the guarantee is consistent evaluation and publication, not fixed future counts.

## Complete run audit and exact exclusions

The separate [coupon audit](.tmp-dataflow/daily-coupon-audit-final.json) lists every coupon record for these dates, ordered by date, creation timestamp and ID, with source candidate run, generation cutoff, model/calibration/policy, status, selection count and combined odds. This preserves the distinction between an empty historical container and usable output.

The machine-readable [full audit](.tmp-dataflow/daily-audit-final.json) includes each run's model/calibration/policy, generated_at, odds cutoff, context, parent, counts, plus every published candidate's selected prediction ID, odds snapshot ID, captured_at, probability, edge and EV. Odds are selected per market as of each run, not from one fictitious global snapshot. Runs below are ordered by insertion ID to expose late inserts with older as-of times. Coupon counts distinguish persisted containers from selection-bearing coupons.

### 2026-09-11

| Run | As-of/generated_at | Odds cutoff | Context | Purpose | Candidates | Coupon containers | Usable coupons | Published |
|---:|---|---|---|---|---:|---:|---:|---|
| 4 | 2026-09-11T18:56:21.358420500+00:00 | 2026-09-11T18:56:21.358420500+00:00 | BASE | Legacy | 2 | 5 | 0 |  |
| 5 | 2026-09-11T19:08:23.625558200+00:00 | 2026-09-11T19:08:23.625558200+00:00 | BASE | Legacy | 0 | 5 | 0 |  |
| 6 | 2026-09-11T18:56:21.358420500+00:00 | 2026-09-11T18:56:21.358420500+00:00 | LINEUP_AWARE | Legacy | 0 | 0 | 0 |  |
| 7 | 2026-09-11T19:08:23.625558200+00:00 | 2026-09-11T19:08:23.625558200+00:00 | LINEUP_AWARE | Legacy | 0 | 0 | 0 |  |
| 8 | 2026-09-11T19:54:45.094329900+00:00 | 2026-09-11T19:54:45.094329900+00:00 | BASE | Legacy | 0 | 0 | 0 |  |
| 12 | 2026-09-11T20:05:04.850228+00:00 | 2026-09-11T20:05:04.850228+00:00 | BASE | Legacy | 0 | 0 | 0 |  |
| 17 | 2026-09-11T20:17:24.147687600+00:00 | 2026-09-11T20:17:24.147687600+00:00 | BASE | Legacy | 0 | 0 | 0 |  |
| 21 | 2026-09-11T20:29:28.291618400+00:00 | 2026-09-11T20:29:28.291618400+00:00 | BASE | Legacy | 0 | 0 | 0 |  |
| 26 | 2026-09-11T20:34:43.930680600+00:00 | 2026-09-11T20:34:43.930680600+00:00 | BASE | Legacy | 0 | 0 | 0 | Yes |

Published exclusions (a match can fail different categories for different reasons; these are not disjoint match totals):

| Reason | Evaluations | Distinct matches |
|---|---:|---:|
| MATCH_STARTED | 126 | 2 |
| MODEL_UNSUPPORTED | 231 | 33 |
| ODDS_UNAVAILABLE | 644 | 2 |

### 2026-09-12

| Run | As-of/generated_at | Odds cutoff | Context | Purpose | Candidates | Coupon containers | Usable coupons | Published |
|---:|---|---|---|---|---:|---:|---:|---|
| 9 | 2026-09-11T19:54:46.489358800+00:00 | 2026-09-11T19:54:46.489358800+00:00 | BASE | Legacy | 41 | 3 | 3 |  |
| 13 | 2026-09-11T20:05:06.840734800+00:00 | 2026-09-11T20:05:06.840734800+00:00 | BASE | Legacy | 15 | 3 | 3 |  |
| 16 | 2026-09-11T20:05:06.840734800+00:00 | 2026-09-11T20:05:06.840734800+00:00 | LINEUP_AWARE | Legacy | 15 | 0 | 0 |  |
| 18 | 2026-09-11T20:17:26.184483100+00:00 | 2026-09-11T20:17:26.184483100+00:00 | BASE | Legacy | 15 | 3 | 3 |  |
| 22 | 2026-09-11T20:29:30.341674200+00:00 | 2026-09-11T20:29:30.341674200+00:00 | BASE | Legacy | 15 | 3 | 3 |  |
| 25 | 2026-09-11T20:29:30.341674200+00:00 | 2026-09-11T20:29:30.341674200+00:00 | LINEUP_AWARE | Legacy | 15 | 0 | 0 |  |
| 27 | 2026-09-11T20:34:46.082942400+00:00 | 2026-09-11T20:34:46.082942400+00:00 | BASE | Legacy | 15 | 3 | 3 |  |
| 30 | 2026-09-11T20:36:00+00:00 | 2026-09-11T20:36:00+00:00 | BASE | REPLAY | 15 | 3 | 3 |  |
| 31 | 2026-09-11T00:00:00+00:00 | 2026-09-11T00:00:00+00:00 | BASE | REPLAY | 0 | 0 | 0 |  |
| 32 | 2026-09-11T21:06:44.179724100+00:00 | 2026-09-11T21:06:44.179724100+00:00 | BASE | LIVE | 15 | 3 | 3 |  |
| 39 | 2026-09-11T21:14:53.575333600+00:00 | 2026-09-11T21:14:53.575333600+00:00 | BASE | LIVE | 15 | 3 | 3 |  |
| 42 | 2026-09-11T20:36:00+00:00 | 2026-09-11T20:36:00+00:00 | BASE | REPLAY | 15 | 3 | 3 |  |
| 43 | 2026-09-11T00:00:00+00:00 | 2026-09-11T00:00:00+00:00 | BASE | REPLAY | 0 | 0 | 0 |  |
| 44 | 2026-09-11T21:15:09.677848700+00:00 | 2026-09-11T21:15:09.677848700+00:00 | BASE | LIVE | 15 | 3 | 3 | Yes |

Published exclusions (a match can fail different categories for different reasons; these are not disjoint match totals):

| Reason | Evaluations | Distinct matches |
|---|---:|---:|
| CORRELATION_REDUCTION | 3 | 3 |
| INSUFFICIENT_HISTORY | 70 | 10 |
| LOW_DATA_QUALITY | 1053 | 39 |
| LOW_EDGE | 16 | 14 |
| LOW_PUBLIC_PROBABILITY | 395 | 39 |
| MODEL_UNSUPPORTED | 4207 | 601 |
| ODDS_UNAVAILABLE | 12558 | 39 |
| RESOLUTION_FAILED | 7 | 1 |
| UNSUPPORTED_MARKET | 975 | 39 |

### 2026-09-13

| Run | As-of/generated_at | Odds cutoff | Context | Purpose | Candidates | Coupon containers | Usable coupons | Published |
|---:|---|---|---|---|---:|---:|---:|---|
| 10 | 2026-09-11T19:55:12.314478500+00:00 | 2026-09-11T19:55:12.314478500+00:00 | BASE | Legacy | 29 | 3 | 3 |  |
| 14 | 2026-09-11T20:05:43.798917600+00:00 | 2026-09-11T20:05:43.798917600+00:00 | BASE | Legacy | 13 | 3 | 3 |  |
| 19 | 2026-09-11T20:18:05.930278500+00:00 | 2026-09-11T20:18:05.930278500+00:00 | BASE | Legacy | 13 | 3 | 3 |  |
| 23 | 2026-09-11T20:30:09.961207300+00:00 | 2026-09-11T20:30:09.961207300+00:00 | BASE | Legacy | 13 | 3 | 3 |  |
| 28 | 2026-09-11T20:35:28.363002100+00:00 | 2026-09-11T20:35:28.363002100+00:00 | BASE | Legacy | 13 | 3 | 3 |  |
| 33 | 2026-09-11T20:36:00+00:00 | 2026-09-11T20:36:00+00:00 | BASE | REPLAY | 13 | 3 | 3 |  |
| 34 | 2026-09-11T00:00:00+00:00 | 2026-09-11T00:00:00+00:00 | BASE | REPLAY | 0 | 0 | 0 |  |
| 35 | 2026-09-11T21:06:50.492676600+00:00 | 2026-09-11T21:06:50.492676600+00:00 | BASE | LIVE | 13 | 3 | 3 |  |
| 40 | 2026-09-11T21:14:54.954493100+00:00 | 2026-09-11T21:14:54.954493100+00:00 | BASE | LIVE | 13 | 3 | 3 |  |
| 45 | 2026-09-11T20:36:00+00:00 | 2026-09-11T20:36:00+00:00 | BASE | REPLAY | 13 | 3 | 3 |  |
| 46 | 2026-09-11T00:00:00+00:00 | 2026-09-11T00:00:00+00:00 | BASE | REPLAY | 0 | 0 | 0 |  |
| 47 | 2026-09-11T21:15:14.816983300+00:00 | 2026-09-11T21:15:14.816983300+00:00 | BASE | LIVE | 13 | 3 | 3 | Yes |

Published exclusions (a match can fail different categories for different reasons; these are not disjoint match totals):

| Reason | Evaluations | Distinct matches |
|---|---:|---:|
| INSUFFICIENT_HISTORY | 35 | 5 |
| LOW_DATA_QUALITY | 615 | 23 |
| LOW_EDGE | 14 | 11 |
| LOW_PUBLIC_PROBABILITY | 224 | 23 |
| MODEL_UNSUPPORTED | 2569 | 367 |
| ODDS_UNAVAILABLE | 7420 | 23 |
| RESOLUTION_FAILED | 7 | 1 |
| UNSUPPORTED_MARKET | 569 | 23 |

### 2026-09-14

| Run | As-of/generated_at | Odds cutoff | Context | Purpose | Candidates | Coupon containers | Usable coupons | Published |
|---:|---|---|---|---|---:|---:|---:|---|
| 11 | 2026-09-11T19:55:28.191560400+00:00 | 2026-09-11T19:55:28.191560400+00:00 | BASE | Legacy | 10 | 3 | 3 |  |
| 15 | 2026-09-11T20:06:05.587729100+00:00 | 2026-09-11T20:06:05.587729100+00:00 | BASE | Legacy | 4 | 2 | 2 |  |
| 20 | 2026-09-11T20:18:31.072356300+00:00 | 2026-09-11T20:18:31.072356300+00:00 | BASE | Legacy | 4 | 2 | 2 |  |
| 24 | 2026-09-11T20:30:32.812767700+00:00 | 2026-09-11T20:30:32.812767700+00:00 | BASE | Legacy | 4 | 2 | 2 |  |
| 29 | 2026-09-11T20:35:52.195989600+00:00 | 2026-09-11T20:35:52.195989600+00:00 | BASE | Legacy | 4 | 2 | 2 |  |
| 36 | 2026-09-11T20:36:00+00:00 | 2026-09-11T20:36:00+00:00 | BASE | REPLAY | 4 | 2 | 2 |  |
| 37 | 2026-09-11T00:00:00+00:00 | 2026-09-11T00:00:00+00:00 | BASE | REPLAY | 0 | 0 | 0 |  |
| 38 | 2026-09-11T21:06:55.052641400+00:00 | 2026-09-11T21:06:55.052641400+00:00 | BASE | LIVE | 4 | 2 | 2 |  |
| 41 | 2026-09-11T21:14:55.864084900+00:00 | 2026-09-11T21:14:55.864084900+00:00 | BASE | LIVE | 4 | 2 | 2 |  |
| 48 | 2026-09-11T20:36:00+00:00 | 2026-09-11T20:36:00+00:00 | BASE | REPLAY | 4 | 2 | 2 |  |
| 49 | 2026-09-11T00:00:00+00:00 | 2026-09-11T00:00:00+00:00 | BASE | REPLAY | 0 | 0 | 0 |  |
| 50 | 2026-09-11T21:15:17.437027100+00:00 | 2026-09-11T21:15:17.437027100+00:00 | BASE | LIVE | 4 | 2 | 2 | Yes |

Published exclusions (a match can fail different categories for different reasons; these are not disjoint match totals):

| Reason | Evaluations | Distinct matches |
|---|---:|---:|
| INSUFFICIENT_HISTORY | 7 | 1 |
| LOW_DATA_QUALITY | 216 | 8 |
| LOW_EDGE | 4 | 3 |
| LOW_PUBLIC_PROBABILITY | 80 | 8 |
| MODEL_UNSUPPORTED | 581 | 83 |
| ODDS_UNAVAILABLE | 2576 | 8 |
| UNSUPPORTED_MARKET | 200 | 8 |

ARZ DAILY PIPELINE RECOVERY COMPLETE
