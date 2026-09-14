# Goal Market Model V2 — offline OOS evaluation

**REJECTED; production remains unchanged.**

Midnight Istanbul decision; history embargo 24h; stage labels embargo 48h; four expanding date-block folds; no outcome/price ranking leakage.

Production Poisson goal architecture causally refit on the same past fitting rows; not the present artifact applied backward.

B365 non-closing prices lack capture timestamps. All ROI is retrospective price-proxy research, not verified executable return. Odds prior masked; no closing prices used.

Historical matches: 7370; minimum-five-history ready: 6722.

| Market / variant | OOS n | Brier | Log loss | ECE | Bet n | Bet ROI proxy | Coupon n | Coupon ROI proxy | Coupon max DD | Longest loss streak |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| O2.5 baseline | 3962 | 0.24691 | 0.68756 | 0.04017 | 374 | -6.80% | 46 | 24.32% | 19.593600000000002 | 16 |
| O2.5 dedicated | 3962 | 0.25011 | 0.69398 | 0.04411 | 181 | -0.77% | 46 | -2.28% | 25.0 | 25 |
| O2.5 calibrated | 3962 | 0.24673 | 0.68653 | 0.01374 | 14 | 1.43% | 46 | -2.28% | 25.0 | 25 |
| O2.5 ensemble | 3962 | 0.24494 | 0.68356 | 0.01135 | 174 | -8.57% | 46 | 12.28% | 14.5936 | 11 |
| O2.5 nominated | 3962 | 0.24649 | 0.68680 | 0.02362 | 266 | -8.41% | 46 | 2.70% | 19.0 | 19 |
| O3.5 baseline | 3962 | 0.20895 | 0.61024 | 0.02955 | 0 | N/A | 0 | N/A | None | None |
| O3.5 dedicated | 3962 | 0.20681 | 0.60400 | 0.01935 | 0 | N/A | 0 | N/A | None | None |
| O3.5 calibrated | 3962 | 0.20663 | 0.60310 | 0.01489 | 0 | N/A | 0 | N/A | None | None |
| O3.5 ensemble | 3962 | 0.20720 | 0.60459 | 0.01887 | 0 | N/A | 0 | N/A | None | None |
| O3.5 nominated | 3962 | 0.20776 | 0.60602 | 0.01846 | 0 | N/A | 0 | N/A | None | None |

Rejection reasons:
- Over 2.5: no demonstrated positive five-leg coupon return improvement.
- Over 2.5: paired Saturday bootstrap does not establish a coupon advantage.
- Over 3.5: no demonstrated positive five-leg coupon return improvement.
- Independent nominated models violate P(4+) <= P(3+) on 4 OOS matches; a joint coherent model is required before activation.
- Historical odds lack verified pre-decision capture timestamps; Over 3.5 has no matched historical prices. Activation evidence is incomplete.

Full probabilities, reliability bins, hit rates, individual drawdown, exact five selections and combined odds for every Saturday, split manifests and fitted models are in the adjacent JSON/joblib artifacts. Early Saturdays are explicitly TRAINING_WARMUP; missing prices are PRICE_MISSING, never imaginary settlements.

## Hit rates and drawdown

| Market / variant | Single-bet hit rate | Mean bet odds | Single-bet max drawdown (u) | Coupon hit rate |
|---|---:|---:|---:|---:|
| O2.5 baseline | 59.89% | 1.6108823529411767 | 32.319999999999986 | 19.57% |
| O2.5 dedicated | 61.33% | 1.6506629834254145 | 14.899999999999997 | 13.04% |
| O2.5 calibrated | 64.29% | 1.6199999999999999 | 2.7 | 13.04% |
| O2.5 ensemble | 57.47% | 1.6507471264367817 | 24.2 | 17.39% |
| O2.5 nominated | 57.14% | 1.6610526315789476 | 31.679999999999975 | 15.22% |
| O3.5 baseline | N/A | None | None | N/A |
| O3.5 dedicated | N/A | None | None | N/A |
| O3.5 calibrated | N/A | None | None | N/A |
| O3.5 ensemble | N/A | None | None | N/A |
| O3.5 nominated | N/A | None | None | N/A |

## Scope and interpretation

- Fixed one-unit stakes. Coupon returns settle all five legs together; combined odds are the product of five real recorded prices. No commission, taxes, limits or execution slippage are modeled.
- Best-five probability ranking is a fixed evaluation rule, not a production publication policy or recommendation. Saturday selection is frozen before the day; no minimum coupon count is forced.
- Odds-implied prior is a reserved, entirely missing feature because no qualifying capture timestamps exist. It is not synthesized from closing prices. Opponent adjustment uses prior opponent Elo relative to the league scoring baseline.
- Labels and historical statistics are reconstructed from the current database. The embargo prevents event-time leakage; original publication/correction vintages are unavailable, so this is not an ingestion-vintage replay.
- Baseline uses the exact production goal estimator and preprocessing, refitted on all eligible past fit+tune rows for a fair data comparison. The present production artifact and present calibration are never applied backward.
- Model hyperparameters are chosen on tune rows; Platt calibration on a later separate block; ensemble weights and the nominated variant on another later block. None use that fold's test labels. Earlier test blocks can become training history in later folds.
- The nominated variant is selected before each test block. It avoids claiming the best variant chosen after viewing pooled OOS results.
- No production model, policy, database row, coupon or frontend was changed.

Over 3.5 five-pick outcome-only checks (prices missing; these are not settled monetary coupons): baseline: 0/46; dedicated: 0/46; calibrated: 0/46; ensemble: 0/46; nominated: 0/46.

## Historical odds audit

- Local football-data CSV audit and exact fixture mappings are recorded with SHA-256 hashes in evaluation.json. Over 3.5 has zero usable matched prices.
- [Football-data methodology](https://www.football-data.co.uk/downloadm.php): non-C quotes are distinct from closing prices; local files contain no per-quote capture timestamp.
- [Public Club Football Match Data](https://github.com/xgabora/Club-Football-Match-Data): the documented free schema provides Over25/Under25, not the needed Over35 history.
- [Footiqo public database](https://footiqo.com/database/): advertises 1xBet closing goal-line prices. Two bounded advertised-export requests returned empty bodies; the second explicitly recorded HTTP 200. No history was imported or inferred from these responses.
- [Footiqo premium sample](https://footiqo.com/premium/premium-football-database/) requires email/marketing consent; no subscription or purchase was made. Closing-only data would still not establish pre-day executable coupon prices.

## Uncertainty

Paired bootstrap resamples complete days for Brier differences and complete Saturdays for coupon-profit differences (5,000 deterministic draws). Negative Brier differences favor V2; positive coupon differences favor V2. Intervals are exploratory and do not correct for all research choices.

| Market / variant | Coupon ROI difference, percentage points | 95% lower | 95% upper |
|---|---:|---:|---:|
| O2.5 baseline | 0.00 | 0.00 | 0.00 |
| O2.5 dedicated | -26.60 | -143.37 | 97.45 |
| O2.5 calibrated | -26.60 | -143.37 | 97.45 |
| O2.5 ensemble | -12.03 | -60.78 | 32.98 |
| O2.5 nominated | -21.61 | -73.05 | 29.82 |

## Validation and changed files

- `cargo test --example goal_v2_export`: 1 passed, proving the frozen snapshot ignores target/same-day/future outcome changes.
- `python -m unittest discover -s tools -p test_goal_model_v2.py`: 10 passed, including actual saved Saturday selection uniqueness, five-leg counts, combined odds and settlement arithmetic.
- `cargo fmt --check`, `cargo check --example goal_v2_export`, and `cargo check`: passed. Existing unrelated lineup warnings remain.
- No frontend changes; frontend build/tests and desktop publication were unnecessary for this rejected offline model.
- New files: `src-tauri/examples/goal_v2_export.rs`, `tools/goal_model_v2.py`, `tools/test_goal_model_v2.py`, `tools/requirements-goal-v2.txt`, `tools/GOAL_MODEL_V2.md`, and this report. Local research artifacts are in `.tmp-dataflow/goal-v2/`.
- Single-bet drawdown uses kickoff/id order; coupon drawdown uses Saturday order, one unit per stake. These are historical evaluation conventions, not an intraday cash-flow simulation.

GOAL MARKET MODEL V2 REJECTED — no improved five-leg coupon evidence; Over 3.5 historical prices are missing and independently fitted goal-line probabilities require coherence before activation.
