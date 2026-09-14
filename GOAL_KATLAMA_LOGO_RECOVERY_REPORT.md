ARZ goal / Katlama / logo recovery — production acceptance

Evidence captured 2026-09-11T23:19:22.286Z. Production AppData SQLite and actual Tauri WebView; selected dates September 12–14, 2026. Backup: `.tmp-dataflow/production-2026-09-11T22-34-41-491Z.sqlite3`. Historical runs, the existing pending Katlama step and original calibration artifact are retained.

**Findings and changes**

Calibration fitting previously partitioned matches by database ID, mixing earlier and later seasons. It now partitions by kickoff, keeps every outcome of a match together and verifies the temporal boundary. Only BTTS and TOTAL_GOALS were revalidated using the existing walk-forward OOS run; no BASE training or non-goal calibration changes. The new active artifact is `calibration-goals-chrono-v2.json`, version `arz-live-cal-goals-chrono-v2`. The older artifact remains intact.

The BTTS adjustment worsened chronological holdout loss, so public probabilities now correctly use BASE with `CALIBRATION_NOT_BENEFICIAL`. Goal thresholds are unchanged. Compound eligibility now accepts this validated fallback with the same sample/history/confidence requirements as before. Odds freshness uses seconds, fixing acceptance of odds aged 24:01–24:59. Daily generation and step mutations acquire the SQLite write transaction before reading, fixing the snapshot-upgrade collision exposed by the independent logo worker.

**Goal funnels**

Counts are immutable run-as-of counts, not a mixture with later wall-clock state. Public probability means an OOS-evaluated public output: all these goal rows deliberately use raw fallback, not a beneficial fitted adjustment. The fitted-adjustment count is zero. Goal coupons need five qualified distinct matches; none of these dates meets that minimum.

| Date | Market | Supported | Predictions | Public p | Adjusted | Odds | p pass | Odds/fresh pass | EV pass | Confidence pass | Qualified |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-09-12 | OVER_25 | 50 | 39 | 39 | 0 | 39 | 6 | 6 | 2 | 2 | 2 |
| 2026-09-12 | OVER_35 | 50 | 39 | 39 | 0 | 39 | 2 | 2 | 1 | 1 | 1 |
| 2026-09-12 | BTTS_YES | 50 | 39 | 39 | 0 | 39 | 3 | 3 | 1 | 1 | 1 |
| 2026-09-13 | OVER_25 | 29 | 23 | 23 | 0 | 23 | 7 | 7 | 3 | 3 | 3 |
| 2026-09-13 | OVER_35 | 29 | 23 | 23 | 0 | 23 | 4 | 4 | 3 | 3 | 3 |
| 2026-09-13 | BTTS_YES | 29 | 23 | 23 | 0 | 23 | 2 | 2 | 1 | 1 | 1 |
| 2026-09-14 | OVER_25 | 9 | 8 | 8 | 0 | 8 | 2 | 2 | 1 | 1 | 1 |
| 2026-09-14 | OVER_35 | 9 | 8 | 8 | 0 | 8 | 1 | 1 | 1 | 1 | 1 |
| 2026-09-14 | BTTS_YES | 9 | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 |
There is no additional confidence threshold for these three goal categories; the confidence stage passes the EV survivors. Model availability, exact market identity and numeric probability validity were checked for all model-ready rows. None has an unavailable model, missing calibrated/public outcome, non-finite fraction, wrong goal line or started fixture at its run timestamp.

| Date | Supported | Resolved | Prediction-ready | History missing | Resolution failed | Unsupported (outside funnel) |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-09-12 | 50 | 49 | 39 | 10 | 1 | 601 |
| 2026-09-13 | 29 | 28 | 23 | 5 | 1 | 367 |
| 2026-09-14 | 9 | 9 | 8 | 1 | 0 | 83 |
**Every rejection rule**

Rejected counts are independent and overlap. “Only” means it fails that rule and no other listed rule. Availability/market identity/extra confidence failures are zero on all audited goal rows. Rows with missing model inputs are counted separately above.

| Date | Market | Rule | Threshold | Rejected | Only |
| --- | --- | --- | --- | --- | --- |
| 2026-09-12 | OVER_25 | ODDS_MISSING | Exact market/outcome/line snapshot required | 0 | 0 |
| 2026-09-12 | OVER_25 | ODDS_INVALID | Decimal odds > 1 | 0 | 0 |
| 2026-09-12 | OVER_25 | STALE_ODDS | Age ≤ 24 hours | 0 | 0 |
| 2026-09-12 | OVER_25 | EV_NONPOSITIVE | p × odds − 1 > 0 (equivalent to positive edge) | 36 | 4 |
| 2026-09-12 | OVER_25 | LOW_PROBABILITY | p ≥ 0.64 | 33 | 1 |
| 2026-09-12 | OVER_25 | STARTED | Kickoff after run as-of | 0 | 0 |
| 2026-09-12 | OVER_35 | ODDS_MISSING | Exact market/outcome/line snapshot required | 0 | 0 |
| 2026-09-12 | OVER_35 | ODDS_INVALID | Decimal odds > 1 | 0 | 0 |
| 2026-09-12 | OVER_35 | STALE_ODDS | Age ≤ 24 hours | 0 | 0 |
| 2026-09-12 | OVER_35 | EV_NONPOSITIVE | p × odds − 1 > 0 (equivalent to positive edge) | 36 | 1 |
| 2026-09-12 | OVER_35 | LOW_PROBABILITY | p ≥ 0.52 | 37 | 2 |
| 2026-09-12 | OVER_35 | STARTED | Kickoff after run as-of | 0 | 0 |
| 2026-09-12 | BTTS_YES | ODDS_MISSING | Exact market/outcome/line snapshot required | 0 | 0 |
| 2026-09-12 | BTTS_YES | ODDS_INVALID | Decimal odds > 1 | 0 | 0 |
| 2026-09-12 | BTTS_YES | STALE_ODDS | Age ≤ 24 hours | 0 | 0 |
| 2026-09-12 | BTTS_YES | EV_NONPOSITIVE | p × odds − 1 > 0 (equivalent to positive edge) | 36 | 2 |
| 2026-09-12 | BTTS_YES | LOW_PROBABILITY | p ≥ 0.64 | 36 | 2 |
| 2026-09-12 | BTTS_YES | STARTED | Kickoff after run as-of | 0 | 0 |
| 2026-09-13 | OVER_25 | ODDS_MISSING | Exact market/outcome/line snapshot required | 0 | 0 |
| 2026-09-13 | OVER_25 | ODDS_INVALID | Decimal odds > 1 | 0 | 0 |
| 2026-09-13 | OVER_25 | STALE_ODDS | Age ≤ 24 hours | 0 | 0 |
| 2026-09-13 | OVER_25 | EV_NONPOSITIVE | p × odds − 1 > 0 (equivalent to positive edge) | 20 | 4 |
| 2026-09-13 | OVER_25 | LOW_PROBABILITY | p ≥ 0.64 | 16 | 0 |
| 2026-09-13 | OVER_25 | STARTED | Kickoff after run as-of | 0 | 0 |
| 2026-09-13 | OVER_35 | ODDS_MISSING | Exact market/outcome/line snapshot required | 0 | 0 |
| 2026-09-13 | OVER_35 | ODDS_INVALID | Decimal odds > 1 | 0 | 0 |
| 2026-09-13 | OVER_35 | STALE_ODDS | Age ≤ 24 hours | 0 | 0 |
| 2026-09-13 | OVER_35 | EV_NONPOSITIVE | p × odds − 1 > 0 (equivalent to positive edge) | 20 | 1 |
| 2026-09-13 | OVER_35 | LOW_PROBABILITY | p ≥ 0.52 | 19 | 0 |
| 2026-09-13 | OVER_35 | STARTED | Kickoff after run as-of | 0 | 0 |
| 2026-09-13 | BTTS_YES | ODDS_MISSING | Exact market/outcome/line snapshot required | 0 | 0 |
| 2026-09-13 | BTTS_YES | ODDS_INVALID | Decimal odds > 1 | 0 | 0 |
| 2026-09-13 | BTTS_YES | STALE_ODDS | Age ≤ 24 hours | 0 | 0 |
| 2026-09-13 | BTTS_YES | EV_NONPOSITIVE | p × odds − 1 > 0 (equivalent to positive edge) | 22 | 1 |
| 2026-09-13 | BTTS_YES | LOW_PROBABILITY | p ≥ 0.64 | 21 | 0 |
| 2026-09-13 | BTTS_YES | STARTED | Kickoff after run as-of | 0 | 0 |
| 2026-09-14 | OVER_25 | ODDS_MISSING | Exact market/outcome/line snapshot required | 0 | 0 |
| 2026-09-14 | OVER_25 | ODDS_INVALID | Decimal odds > 1 | 0 | 0 |
| 2026-09-14 | OVER_25 | STALE_ODDS | Age ≤ 24 hours | 0 | 0 |
| 2026-09-14 | OVER_25 | EV_NONPOSITIVE | p × odds − 1 > 0 (equivalent to positive edge) | 7 | 1 |
| 2026-09-14 | OVER_25 | LOW_PROBABILITY | p ≥ 0.64 | 6 | 0 |
| 2026-09-14 | OVER_25 | STARTED | Kickoff after run as-of | 0 | 0 |
| 2026-09-14 | OVER_35 | ODDS_MISSING | Exact market/outcome/line snapshot required | 0 | 0 |
| 2026-09-14 | OVER_35 | ODDS_INVALID | Decimal odds > 1 | 0 | 0 |
| 2026-09-14 | OVER_35 | STALE_ODDS | Age ≤ 24 hours | 0 | 0 |
| 2026-09-14 | OVER_35 | EV_NONPOSITIVE | p × odds − 1 > 0 (equivalent to positive edge) | 7 | 0 |
| 2026-09-14 | OVER_35 | LOW_PROBABILITY | p ≥ 0.52 | 7 | 0 |
| 2026-09-14 | OVER_35 | STARTED | Kickoff after run as-of | 0 | 0 |
| 2026-09-14 | BTTS_YES | ODDS_MISSING | Exact market/outcome/line snapshot required | 0 | 0 |
| 2026-09-14 | BTTS_YES | ODDS_INVALID | Decimal odds > 1 | 0 | 0 |
| 2026-09-14 | BTTS_YES | STALE_ODDS | Age ≤ 24 hours | 0 | 0 |
| 2026-09-14 | BTTS_YES | EV_NONPOSITIVE | p × odds − 1 > 0 (equivalent to positive edge) | 7 | 0 |
| 2026-09-14 | BTTS_YES | LOW_PROBABILITY | p ≥ 0.64 | 8 | 1 |
| 2026-09-14 | BTTS_YES | STARTED | Kickoff after run as-of | 0 | 0 |
**Distributions and near-threshold rows**

p and EV are fractions, not percentages. Quantiles use observed ordered rows (no interpolation). Near-threshold means ±0.05 probability around the unchanged threshold.

| Date | Market | Variable | N | Min | P10 | Median | P90 | Max |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-09-12 | OVER_25 | p | 39 | 0.3430 | 0.4097 | 0.5050 | 0.6472 | 0.7369 |
| 2026-09-12 | OVER_25 | EV | 39 | -0.3928 | -0.2946 | -0.1878 | -0.0401 | 0.1385 |
| 2026-09-12 | OVER_35 | p | 39 | 0.1563 | 0.2051 | 0.2846 | 0.4270 | 0.5338 |
| 2026-09-12 | OVER_35 | EV | 39 | -0.5250 | -0.4078 | -0.2575 | -0.0195 | 0.2761 |
| 2026-09-12 | BTTS_YES | p | 39 | 0.4119 | 0.4499 | 0.5282 | 0.6003 | 0.6988 |
| 2026-09-12 | BTTS_YES | EV | 39 | -0.3004 | -0.2830 | -0.1536 | -0.0445 | 0.0235 |
| 2026-09-13 | OVER_25 | p | 23 | 0.4407 | 0.4725 | 0.5410 | 0.7382 | 0.9573 |
| 2026-09-13 | OVER_25 | EV | 23 | -0.2928 | -0.2226 | -0.1401 | -0.0481 | 0.1581 |
| 2026-09-13 | OVER_35 | p | 23 | 0.2297 | 0.2562 | 0.3179 | 0.5356 | 0.8888 |
| 2026-09-13 | OVER_35 | EV | 23 | -0.4035 | -0.3101 | -0.1905 | -0.0574 | 0.2862 |
| 2026-09-13 | BTTS_YES | p | 23 | 0.4099 | 0.4693 | 0.5213 | 0.5714 | 0.6964 |
| 2026-09-13 | BTTS_YES | EV | 23 | -0.3029 | -0.2416 | -0.1241 | -0.0683 | 0.0813 |
| 2026-09-14 | OVER_25 | p | 8 | 0.3851 | 0.3851 | 0.4730 | 0.6708 | 0.7463 |
| 2026-09-14 | OVER_25 | EV | 8 | -0.3876 | -0.3876 | -0.1833 | -0.0609 | 0.2388 |
| 2026-09-14 | OVER_35 | p | 8 | 0.1864 | 0.1864 | 0.2566 | 0.4537 | 0.5460 |
| 2026-09-14 | OVER_35 | EV | 8 | -0.5153 | -0.5153 | -0.2548 | -0.0292 | 0.4250 |
| 2026-09-14 | BTTS_YES | p | 8 | 0.4130 | 0.4130 | 0.4983 | 0.5694 | 0.6339 |
| 2026-09-14 | BTTS_YES | EV | 8 | -0.2408 | -0.2408 | -0.1763 | -0.0543 | 0.0649 |

| Date | Market | Match ID | p | Odds | EV | Failed rules |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-09-12 | OVER_25 | 9058 | 0.6648 | 1.3100 | -0.1292 | EV_NONPOSITIVE |
| 2026-09-12 | OVER_25 | 9200 | 0.6500 | 1.4800 | -0.0381 | EV_NONPOSITIVE |
| 2026-09-12 | OVER_25 | 9442 | 0.6432 | 1.7700 | 0.1385 | PASS |
| 2026-09-12 | OVER_25 | 9581 | 0.6194 | 1.3500 | -0.1639 | EV_NONPOSITIVE, LOW_PROBABILITY |
| 2026-09-12 | OVER_25 | 9319 | 0.6128 | 1.7600 | 0.0785 | LOW_PROBABILITY |
| 2026-09-12 | OVER_25 | 9421 | 0.6472 | 1.1800 | -0.2363 | EV_NONPOSITIVE |
| 2026-09-12 | OVER_35 | 9361 | 0.5338 | 2.1900 | 0.1691 | PASS |
| 2026-09-12 | OVER_35 | 8933 | 0.5244 | 1.8000 | -0.0562 | EV_NONPOSITIVE |
| 2026-09-12 | BTTS_YES | 9058 | 0.6433 | 1.3300 | -0.1445 | EV_NONPOSITIVE |
| 2026-09-12 | BTTS_YES | 9361 | 0.6874 | 1.3900 | -0.0445 | EV_NONPOSITIVE |
| 2026-09-12 | BTTS_YES | 9148 | 0.6003 | 1.4000 | -0.1596 | EV_NONPOSITIVE, LOW_PROBABILITY |
| 2026-09-12 | BTTS_YES | 9319 | 0.6279 | 1.6300 | 0.0235 | LOW_PROBABILITY |
| 2026-09-13 | OVER_25 | 9652 | 0.6501 | 1.3300 | -0.1353 | EV_NONPOSITIVE |
| 2026-09-13 | OVER_25 | 9303 | 0.6750 | 1.3000 | -0.1225 | EV_NONPOSITIVE |
| 2026-09-13 | OVER_25 | 8826 | 0.6330 | 1.1700 | -0.2594 | EV_NONPOSITIVE, LOW_PROBABILITY |
| 2026-09-13 | OVER_25 | 8834 | 0.6488 | 1.3000 | -0.1566 | EV_NONPOSITIVE |
| 2026-09-13 | OVER_35 | 8892 | 0.5356 | 1.7600 | -0.0574 | EV_NONPOSITIVE |
| 2026-09-13 | BTTS_YES | 9507 | 0.5960 | 1.3800 | -0.1776 | EV_NONPOSITIVE, LOW_PROBABILITY |
| 2026-09-13 | BTTS_YES | 8834 | 0.6436 | 1.6800 | 0.0813 | PASS |
| 2026-09-14 | OVER_25 | 8811 | 0.6708 | 1.4000 | -0.0609 | EV_NONPOSITIVE |
| 2026-09-14 | OVER_35 | 9644 | 0.5460 | 2.6100 | 0.4250 | PASS |
| 2026-09-14 | BTTS_YES | 9644 | 0.6339 | 1.6800 | 0.0649 | LOW_PROBABILITY |
**Outcome, snapshot and revision checks**

Over/Under and BTTS Yes/No complement error: zero. Over 3.5 > Over 2.5 violations: 0. Stored qualified candidate versus selected prediction/odds/public-probability mismatches: 0. OOS outcome labels reconstructed from actual final goals: zero mismatches. The odds view maps normalized TOTAL_GOALS OVER with line 2.5/3.5 and BTTS YES; aliases are normalized before lookup. Selection uses the newest capture ≤ run as-of, ordered by parsed timestamp then ID. Prediction selection uses the active model and newest created_at ≤ as-of, then ID. These runs use BASE; there are no authoritative live lineup revisions replacing them. Full row IDs, raw/public probabilities, snapshot IDs and timestamps are in `goal-audit-final.json`.

| Date | Run | As-of UTC | Odds capture earliest | Odds capture latest | Max age hours |
| --- | --- | --- | --- | --- | --- |
| 2026-09-12 | 59 | 2026-09-11T22:57:23.959499+00:00 | 2026-09-11T19:55:58.855Z | 2026-09-11T20:17:01.313Z | 3.024 |
| 2026-09-13 | 60 | 2026-09-11T22:57:25.990434300+00:00 | 2026-09-11T20:17:01.313Z | 2026-09-11T20:17:01.313Z | 2.674 |
| 2026-09-14 | 61 | 2026-09-11T22:57:27.307406500+00:00 | 2026-09-11T20:17:01.313Z | 2026-09-11T20:17:01.313Z | 2.674 |
**Chronological OOS evidence**

Fit ends 2025-08-23T15:00:00Z; holdout starts 2025-08-23T15:15:00Z. Evaluation uses 3,635 later fixtures. Loss below compares both outcomes and all fitted lines within each family. Lower is better.

| Family | Raw log loss | Adjusted log loss | Raw Brier | Adjusted Brier | Selected output |
| --- | --- | --- | --- | --- | --- |
| BTTS | 0.690599 | 0.692600 | 0.248677 | 0.249726 | Validated raw fallback |
| TOTAL_GOALS | 0.610187 | 0.683673 | 0.209430 | 0.245265 | Validated raw fallback |

| Selection | Threshold | OOS N above threshold | Mean predicted | Observed wins | Brier |
| --- | --- | --- | --- | --- | --- |
| OVER_25 | 0.64 | 471 | 73.11% | 64.54% | 0.231623 |
| OVER_35 | 0.52 | 189 | 64.74% | 49.74% | 0.283121 |
| BTTS_YES | 0.64 | 114 | 67.75% | 60.53% | 0.246993 |
The high-probability groups are already overconfident on this later holdout. This supplies no defensible evidence to lower probability gates merely to fill coupons. OOS Brier/log loss and observed wins are not an ROI claim: the stored calibration evaluation does not provide matched historical Iddaa odds for an EV/ROI threshold optimization. No numeric threshold was changed.

**Katlama search and actual state**

All qualified COMPOUND rows are searched (bounded at 256; exceeding the bound is explicitly reported, never called “no combination”). Exhaustive distinct-match pairs; triples are also searched unless an exact 1.80 pair has already been found. The permissible product range remains 1.50–1.80 with numerical tolerance. Preference: exact 1.80, then quality among products ≥1.75, then quality within the remaining band; ties use higher odds and stable candidate-ID order. Quality is mean existing candidate score. Thus Sep 13/14 may choose a higher-quality set rather than the numerically closest set.

| Date | All category candidates | Eligible pool | Pairs | Triples | Valid combinations | Chosen IDs | Product |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-09-12 | 85 | 28 | 378 | 0 | 150 | 811, 820 | 1.800000 |
| 2026-09-13 | 59 | 17 | 136 | 680 | 397 | 896, 897 | 1.792000 |
| 2026-09-14 | 18 | 7 | 21 | 35 | 30 | 954, 955, 957 | 1.772016 |

| Date | Target | Closest product | Candidate IDs |
| --- | --- | --- | --- |
| 2026-09-12 | 1.50 | 1.500000 | 811, 830 |
| 2026-09-12 | 1.60 | 1.600800 | 813, 837 |
| 2026-09-12 | 1.70 | 1.699200 | 814, 820 |
| 2026-09-12 | 1.80 | 1.800000 | 811, 820 |
| 2026-09-13 | 1.50 | 1.497600 | 896, 909 |
| 2026-09-13 | 1.60 | 1.598385 | 901, 902, 908 |
| 2026-09-13 | 1.70 | 1.695456 | 899, 903, 910 |
| 2026-09-13 | 1.80 | 1.797768 | 908, 910, 911 |
| 2026-09-14 | 1.50 | 1.493400 | 954, 960 |
| 2026-09-14 | 1.60 | 1.591200 | 956, 959 |
| 2026-09-14 | 1.70 | 1.702476 | 954, 958, 960 |
| 2026-09-14 | 1.80 | 1.798464 | 956, 957, 958 |

| Date | Chosen fixture | Market / outcome / line | p | Odds |
| --- | --- | --- | --- | --- |
| 2026-09-12 | Atalanta — Cagliari | TOTAL_GOALS UNDER 3.5 | 84.37% | 1.25 |
| 2026-09-12 | Westerlo — Standard | TOTAL_GOALS UNDER 3.5 | 74.03% | 1.44 |
| 2026-09-13 | Galatasaray — Kocaelispor | MATCH_RESULT HOME  | 89.46% | 1.28 |
| 2026-09-13 | PSV Eindhoven — Sparta Rotterdam | TOTAL_GOALS OVER 3.5 | 88.88% | 1.40 |
| 2026-09-14 | Gaziantep — Fenerbahce | TOTAL_GOALS OVER 1.5 | 90.05% | 1.14 |
| 2026-09-14 | Sp Braga — Estoril | TOTAL_GOALS UNDER 3.5 | 81.36% | 1.34 |
| 2026-09-14 | Inter — Udinese | MATCH_RESULT HOME  | 75.74% | 1.16 |
Actual persisted series 1, coupon #122: step 1/7, UNSETTLED, odds 1.800000. It survived restarts unchanged. Sep 13/14 are real candidate-backed previews, not fabricated future settled steps. The UI labels both the active coupon date and selected search date. Generation is idempotent while a step is pending; only real WON/LOST settlement advances/resets, with reset after winning step 7.

**Every eligible Katlama candidate**

| Date | Candidate ID | Fixture | Market / outcome / line | p | Odds | Score |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-09-12 | 811 | Atalanta — Cagliari | TOTAL_GOALS UNDER 3.5 | 84.37% | 1.25 | 0.4932 |
| 2026-09-12 | 812 | Swansea — Burnley | TOTAL_GOALS OVER 1.5 | 84.30% | 1.19 | 0.4748 |
| 2026-09-12 | 813 | Casa Pia — Porto | MATCH_RESULT AWAY  | 75.35% | 1.16 | 0.4645 |
| 2026-09-12 | 814 | Le Havre — Angers | TOTAL_GOALS UNDER 3.5 | 82.44% | 1.18 | 0.4634 |
| 2026-09-12 | 815 | Tottenham — Everton | TOTAL_GOALS OVER 1.5 | 82.42% | 1.18 | 0.4633 |
| 2026-09-12 | 816 | Sunderland — Arsenal | TOTAL_GOALS UNDER 3.5 | 79.52% | 1.19 | 0.4474 |
| 2026-09-12 | 817 | Lazio — Milan | TOTAL_GOALS UNDER 3.5 | 79.49% | 1.15 | 0.4472 |
| 2026-09-12 | 818 | Osasuna — Espanol | TOTAL_GOALS UNDER 3.5 | 79.22% | 1.16 | 0.4457 |
| 2026-09-12 | 819 | Nacional — Alverca FC | TOTAL_GOALS UNDER 3.5 | 78.10% | 1.19 | 0.4395 |
| 2026-09-12 | 820 | Westerlo — Standard | TOTAL_GOALS UNDER 3.5 | 74.03% | 1.44 | 0.4385 |
| 2026-09-12 | 821 | Strasbourg — Monaco | TOTAL_GOALS OVER 2.5 | 73.69% | 1.45 | 0.4373 |
| 2026-09-12 | 822 | Bournemouth — Brentford | TOTAL_GOALS UNDER 3.5 | 73.91% | 1.44 | 0.4373 |
| 2026-09-12 | 823 | Alanyaspor — Goztep | TOTAL_GOALS UNDER 3.5 | 77.66% | 1.22 | 0.4371 |
| 2026-09-12 | 824 | Auxerre — Nice | TOTAL_GOALS UNDER 3.5 | 77.15% | 1.24 | 0.4343 |
| 2026-09-12 | 825 | Charlton — Portsmouth | TOTAL_GOALS UNDER 3.5 | 77.00% | 1.16 | 0.4335 |
| 2026-09-12 | 826 | Derby — Birmingham | TOTAL_GOALS UNDER 3.5 | 76.90% | 1.18 | 0.4330 |
| 2026-09-12 | 827 | West Brom — QPR | TOTAL_GOALS OVER 1.5 | 76.49% | 1.16 | 0.4307 |
| 2026-09-12 | 828 | Watford — Stoke | TOTAL_GOALS UNDER 3.5 | 75.97% | 1.25 | 0.4278 |
| 2026-09-12 | 829 | Ath Bilbao — Elche | TOTAL_GOALS UNDER 3.5 | 75.09% | 1.34 | 0.4250 |
| 2026-09-12 | 830 | Aston Villa — Nott'm Forest | TOTAL_GOALS OVER 1.5 | 75.31% | 1.20 | 0.4242 |
| 2026-09-12 | 831 | Blackburn — Millwall | TOTAL_GOALS OVER 1.5 | 75.09% | 1.18 | 0.4230 |
| 2026-09-12 | 832 | Eyupspor — Rizespor | TOTAL_GOALS OVER 1.5 | 75.03% | 1.21 | 0.4227 |
| 2026-09-12 | 833 | Konyaspor — Trabzonspor | TOTAL_GOALS OVER 1.5 | 74.92% | 1.12 | 0.4220 |
| 2026-09-12 | 834 | Oud-Heverlee Leuven — Cercle Brugge | TOTAL_GOALS UNDER 3.5 | 73.32% | 1.40 | 0.4220 |
| 2026-09-12 | 835 | Lorient — Toulouse | TOTAL_GOALS OVER 1.5 | 74.90% | 1.20 | 0.4219 |
| 2026-09-12 | 836 | Crystal Palace — Ipswich | TOTAL_GOALS OVER 1.5 | 74.66% | 1.16 | 0.4206 |
| 2026-09-12 | 837 | Paris — Lyon | TOTAL_GOALS UNDER 3.5 | 73.18% | 1.38 | 0.4158 |
| 2026-09-12 | 838 | For Sittard — Ajax | TOTAL_GOALS OVER 2.5 | 72.94% | 1.27 | 0.4112 |
| 2026-09-13 | 896 | Galatasaray — Kocaelispor | MATCH_RESULT HOME  | 89.46% | 1.28 | 0.5922 |
| 2026-09-13 | 897 | PSV Eindhoven — Sparta Rotterdam | TOTAL_GOALS OVER 3.5 | 88.88% | 1.40 | 0.5791 |
| 2026-09-13 | 898 | Benfica — Gil Vicente | TOTAL_GOALS OVER 2.5 | 78.69% | 1.40 | 0.4762 |
| 2026-09-13 | 899 | Club Brugge — Antwerp | MATCH_RESULT HOME  | 73.33% | 1.16 | 0.4533 |
| 2026-09-13 | 900 | Coventry — Brighton | TOTAL_GOALS OVER 1.5 | 77.99% | 1.14 | 0.4389 |
| 2026-09-13 | 901 | Arouca — Santa Clara | TOTAL_GOALS UNDER 3.5 | 77.03% | 1.13 | 0.4337 |
| 2026-09-13 | 902 | Lecce — Monza | TOTAL_GOALS UNDER 3.5 | 76.39% | 1.15 | 0.4301 |
| 2026-09-13 | 903 | Famalicao — Sp Lisbon | TOTAL_GOALS OVER 1.5 | 75.28% | 1.16 | 0.4240 |
| 2026-09-13 | 904 | Sociedad — Ath Madrid | TOTAL_GOALS OVER 1.5 | 75.26% | 1.11 | 0.4239 |
| 2026-09-13 | 905 | Waregem — Charleroi | TOTAL_GOALS OVER 1.5 | 75.22% | 1.13 | 0.4237 |
| 2026-09-13 | 906 | Sheffield United — Wolves | TOTAL_GOALS OVER 1.5 | 74.85% | 1.13 | 0.4217 |
| 2026-09-13 | 907 | Sassuolo — Juventus | TOTAL_GOALS OVER 1.5 | 74.49% | 1.16 | 0.4197 |
| 2026-09-13 | 908 | RAAL La Louviere — Kortrijk | TOTAL_GOALS UNDER 3.5 | 74.38% | 1.23 | 0.4191 |
| 2026-09-13 | 909 | Genclerbirligi — Kasimpasa | TOTAL_GOALS UNDER 3.5 | 74.13% | 1.17 | 0.4177 |
| 2026-09-13 | 910 | Zwolle — Feyenoord | TOTAL_GOALS OVER 2.5 | 73.82% | 1.26 | 0.4160 |
| 2026-09-13 | 911 | Genk — Gent | TOTAL_GOALS OVER 1.5 | 73.71% | 1.16 | 0.4154 |
| 2026-09-13 | 912 | Napoli — Bologna | TOTAL_GOALS UNDER 3.5 | 73.47% | 1.17 | 0.4141 |
| 2026-09-14 | 954 | Gaziantep — Fenerbahce | TOTAL_GOALS OVER 1.5 | 90.05% | 1.14 | 0.5151 |
| 2026-09-14 | 955 | Sp Braga — Estoril | TOTAL_GOALS UNDER 3.5 | 81.36% | 1.34 | 0.4878 |
| 2026-09-14 | 956 | Como — Parma | TOTAL_GOALS UNDER 3.5 | 79.58% | 1.36 | 0.4752 |
| 2026-09-14 | 957 | Inter — Udinese | MATCH_RESULT HOME  | 75.74% | 1.16 | 0.4665 |
| 2026-09-14 | 958 | Torino — Roma | TOTAL_GOALS OVER 1.5 | 79.45% | 1.14 | 0.4470 |
| 2026-09-14 | 959 | Rio Ave — Estrela | TOTAL_GOALS UNDER 3.5 | 78.95% | 1.17 | 0.4442 |
| 2026-09-14 | 960 | Leeds — Newcastle | TOTAL_GOALS UNDER 3.5 | 74.34% | 1.31 | 0.4189 |
Compound exclusions below count evaluated prediction/outcome rows, not distinct matches. Multiple missing odds often belong to unsupported market lines; coverage/history exclusions have no prediction row. Correlation reduction retains one best qualified selection per fixture before search.

| Date | Exclusion | Rows |
| --- | --- | --- |
| 2026-09-12 | CORRELATION_REDUCTION | 3 |
| 2026-09-12 | INSUFFICIENT_HISTORY | 10 |
| 2026-09-12 | INVALID_ODDS | 8 |
| 2026-09-12 | LOW_PUBLIC_PROBABILITY | 460 |
| 2026-09-12 | MODEL_UNSUPPORTED | 601 |
| 2026-09-12 | ODDS_OUTSIDE_COMPOUND_RANGE | 8 |
| 2026-09-12 | ODDS_UNAVAILABLE | 1638 |
| 2026-09-12 | RESOLUTION_FAILED | 1 |
| 2026-09-13 | CORRELATION_REDUCTION | 7 |
| 2026-09-13 | INSUFFICIENT_HISTORY | 5 |
| 2026-09-13 | INVALID_ODDS | 6 |
| 2026-09-13 | LOW_PUBLIC_PROBABILITY | 261 |
| 2026-09-13 | MODEL_UNSUPPORTED | 367 |
| 2026-09-13 | ODDS_OUTSIDE_COMPOUND_RANGE | 6 |
| 2026-09-13 | ODDS_UNAVAILABLE | 968 |
| 2026-09-13 | RESOLUTION_FAILED | 1 |
| 2026-09-14 | CORRELATION_REDUCTION | 1 |
| 2026-09-14 | INSUFFICIENT_HISTORY | 1 |
| 2026-09-14 | LOW_PUBLIC_PROBABILITY | 77 |
| 2026-09-14 | MODEL_UNSUPPORTED | 83 |
| 2026-09-14 | ODDS_OUTSIDE_COMPOUND_RANGE | 3 |
| 2026-09-14 | ODDS_UNAVAILABLE | 352 |
**Publication minimums**

Corner 5–7; Over 2.5, Over 3.5, BTTS and High Confidence minimum 5. No underfilled goal coupon is READY. The UI shows the exact qualified shortage and an expandable rejection explanation. Today shows 85 candidate rows and 4 usable coupons including the existing pending Katlama step; Sep 13 has 3 daily published coupons plus the separately dated pending step, Sep 14 has 1 plus that pending step. A pending Sep 12 coupon is never counted as a newly published Sep 13/14 coupon.

**Logo scheduler**

Ready 34 before → 50 now. Every one of 2,969 known teams has a persistent job; unrequested = 0.

| State | Teams |
| --- | --- |
| READY | 50 |
| QUEUED | 2895 |
| DOWNLOADING | 0 |
| RETRY_LATER | 0 |
| RATE_LIMITED | 13 |
| UNAVAILABLE | 11 |
| INVALID | 0 |
Single worker, at most 24 dispatches per batch, 1.5 seconds between jobs, 30-second wakeups independent of page navigation. Priorities: visible 100, teams in supported competitions 20, remaining teams lower. The initial all-team enrollment writes metadata only; it does not scan all image files or start 3,000 HTTP requests. Exact catalog hints are filled in batches of 32. Cached validated images are not downloaded again. Interrupted downloads are recovered after restart. Retry-After is persisted and suppresses requests to that provider, including across restart; other transient errors use 5/30/120-minute backoff and bounded attempts. Definitive missing/unsafe identity is UNAVAILABLE; corrupt images are INVALID.

The fallback URL previously contained `/summary//`; it now has one correctly escaped title segment. Failed conclusions from that provider revision are retried once using the corrected version, preserving genuine rate-limit cooldowns. Therefore some formerly unavailable rows correctly return to QUEUED. The remaining backlog is scheduled work, not a terminal “unrequested” state.

Runtime evidence: ready 45 just before restart → 48 after restart. On Settings, scheduler hint counts increased at each 30-second wakeup while the recorded rate-limited job’s last_attempt_at stayed unchanged. Its advertised cooldown was until 2026-09-11 23:25:13 UTC; it was not bypassed for acceptance. Automatic expiry/requeue and persistence are also exercised by the asset regression test. Actual cached logos, including Burnley on Today, were visually inspected. The public fallback provider can delay completion; no claim is made that every known team has a discoverable logo.

The background scheduler subsequently raised READY from 48 to 50 without another app restart, while the Wikimedia cooldown remained in force. This confirms that other available sources continue to progress during a provider-specific pause.

**Validation and files**

Passed: cargo fmt --check; cargo check; coupon_engine_tests (18); calibration_tests (7); daily_output_tests (3, including three-date production replay); assets::tests (4); Compound validated-fallback and exact 24-hour odds boundary tests; frontend goal-explanation/daily-cache/latest-request tests (5); npm build. Existing unrelated warnings remain. No Phase 13/14 acceptance or full model training was run.

Measured desktop navigation to loaded content, final run: Today 496 ms, Candidates 191 ms, Coupons 493 ms, Data Center 37 ms (warm readiness view). The preceding run measured 731/202/565/40 ms respectively. These are observed navigation timings, not a cold-start benchmark. Navigation did not regenerate the daily pipeline.

The final visual check also caught unrelated market exclusions leaking into goal-category summaries. `src/lib/goalExclusions.ts` now filters exact goal outcome/line in both Candidates and Coupons; regression tests cover BTTS Yes versus No, unrelated missing odds, exact Over lines and pre-prediction coverage failures. September 14 BTTS now correctly shows 83 outside coverage, 1 insufficient history and 8 below probability policy, without falsely claiming its eight goal odds are missing.

Real desktop: changed candidate categories and business dates for all three days, inspected shortages, all search alternatives, real pending step and Today counts; restarted and confirmed identical step/coupon; inspected Data Center and visible logos. Screenshots and runtime evidence: `.tmp-dataflow/goal-ui/`.

Task changes: `src-tauri/src/repositories/{calibration.rs,calibration_tests.rs,candidate_engine.rs,coupon_engine.rs,coupon_engine_tests.rs,data_center.rs}`, `src-tauri/src/{assets.rs,commands.rs,lib.rs}`, `src/{types.ts,services/tauri.ts,pages/CandidatesPage.tsx,pages/CouponsPage.tsx,pages/PopularsPage.tsx,pages/DataCenterPage.tsx,lib/goalExclusions.ts,lib/goalExclusions.test.ts}`, `vite.config.ts`. Existing broader recovery changes were left in place. Vite now ignores audit output to avoid Windows EBUSY watcher crashes during desktop verification.

Raw evidence: `.tmp-dataflow/goal-audit.json` (before), `goal-audit-final.json` (after, every prediction/odds row), `goal-chronological.json`, `goal-flow-final.json`, `goal-ui/evidence.json`, `logo-idle-monitor.json`.
