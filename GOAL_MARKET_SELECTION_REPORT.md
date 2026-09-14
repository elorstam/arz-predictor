# Dedicated goal-market ranking evaluation

Research only. No application tables, candidate rules or coupons were modified.

Historical odds: exact provider fixture hash matches to cached football-data.co.uk **B365 pre-closing Over 2.5** columns. They are not Iddaa odds. Closing and maximum-bookmaker columns are not used. Missing Over 3.5 odds are never derived from model probabilities. [Provider collection methodology](https://www.football-data.co.uk/downloadm.php).

Public goal probabilities use the existing OOS-validated raw fallback, because the active goal calibration adjustment is not beneficial. No probability transform is fitted here. Chronological day split: first 70% development, last 30% holdout. Whole dates stay together. All weights/floors/cutoffs are chosen on development only; the holdout cannot retune them. The historical confidence feature uses only outcomes from earlier days.

Nine score inputs (weighted sum, 0–100): public probability, expected-goals total/6, clipped 0.5+EV, inverse decimal odds, combined recent attack/5, combined concessions/5, BTTS Yes probability, league goals/4, reliability/sample quality. Goal distribution and probability are correlated, so the bounded weight family limits their influence. Recent profiles use each side’s last ten matches. No post-match stats enter the score.

Safety requires finite inputs, valid prematch provenance, five historical matches per team, odds (1,8], fresh non-future live odds ≤24h, and the market-specific probability/EV floors. Ranking then applies one learned score cutoff, selecting at most seven and publishing only when at least five remain. Research proposals are explicitly separate from published coupons.

Acceptance was fixed before holdout evaluation: ≥150 holdout bets, ≥30 coupons, positive lower 95% day-block bootstrap ROI bound, ROI no worse than old thresholds, calibration gap ≤10 percentage points and nonnegative actual accumulator ROI. Passing proxy bookmaker evidence still requires Iddaa validation before live adoption. These evidence gates do not reward simply increasing coupon counts.

## Over 2.5

**OOS_REJECTED**

Probability floor 0.5; EV floor 0.0; score cutoff 50.0/100; target 5 selections (minimum 5). OOS-tuned experimental policy, not adopted.

| Component | Weight |
| --- | --- |
| probability | 0.3100 |
| goal_distribution | 0.1200 |
| ev | 0.1500 |
| odds | 0.0800 |
| attack | 0.1000 |
| concession | 0.0500 |
| btts | 0.0800 |
| league | 0.0600 |
| quality | 0.0500 |

OOS date boundary: 2026-01-07. Development: 5242 rows; holdout: 2028 rows. Parameter trials: 1872. Rejection reasons: POSITIVE_ROI_NOT_SUPPORTED_BY_DAY_BLOCK_INTERVAL, COUPON_RETURNS_NOT_ACCEPTABLE
Single-bet ROI 95% day-block interval: [-0.05308333333333333, 0.15620833333333334].

| EV floor | Best development ROI | Bets | Probability floor | Target selections |
| --- | --- | --- | --- | --- |
| -0.1 | -0.0311 | 819 | 0.4500 | 7 |
| -0.05 | -0.0312 | 917 | 0.4500 | 7 |
| 0.0 | -0.0061 | 540 | 0.5000 | 5 |
| 0.02 | -0.0215 | 555 | 0.4500 | 5 |

Large OOS Saturdays (≥20 model-ready): 20; ranking produces a coupon on 19, old thresholds on 11. This volume improvement does not establish profitable coupons.

| Approach | Bets | Hit rate | ROI | Average odds | Brier | Calibration gap | 5–7 coupons | Coupon ROI |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Old thresholds (single bets) | 248 | 0.6573 | 0.0050 | 1.5627 | 0.2222 | 0.0926 | 0 | N/A |
| Old thresholds, 5–7 rule | 105 | 0.6000 | -0.0739 | 1.5810 | 0.2464 | 0.1494 | 17 | 0.2538 |
| Dedicated rank, 5–7 rule | 240 | 0.6458 | 0.0519 | 1.6624 | 0.2266 | 0.0633 | 48 | -0.1180 |

| Date | Model-ready | Old qualified | Safety survivors | Score survivors | Research 5–7 | Published |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-09-12 | 39 | 2 | 3 | 3 | 0 | 0 |
| 2026-09-13 | 23 | 3 | 3 | 3 | 0 | 0 |
| 2026-09-14 | 8 | 1 | 1 | 1 | 0 | 0 |

### 2026-09-12: ranked top 10

Rejections: `{"EV_SAFETY_FLOOR": 19, "PROBABILITY_SAFETY_FLOOR": 17}`. Research selection IDs: []. Published: none (adoption gate).

| Rank | Match | Probability | Odds | EV | Goal score | Proposed |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Strasbourg — Monaco | 0.7369 | 1.4500 | 0.0685 | 70.8003 | False |
| 2 | Swansea — Burnley | 0.6432 | 1.7700 | 0.1385 | 62.3762 | False |
| 3 | Tottenham — Everton | 0.6128 | 1.7600 | 0.0785 | 60.7404 | False |

Top 10 scored model-ready fixtures, **including excluded diagnostics** (not a coupon):

| Rank | Match | Probability | Odds | EV | Goal score | Safety rejection |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Strasbourg — Monaco | 0.7369 | 1.4500 | 0.0685 | 70.8003 | SURVIVES |
| 2 | For Sittard — Ajax | 0.7294 | 1.2700 | -0.0737 | 67.2644 | EV_SAFETY_FLOOR |
| 3 | Augsburg — Leverkusen | 0.6648 | 1.3100 | -0.1292 | 65.4409 | EV_SAFETY_FLOOR |
| 4 | Southampton — Bristol City | 0.6500 | 1.4800 | -0.0381 | 63.0294 | EV_SAFETY_FLOOR |
| 5 | Swansea — Burnley | 0.6432 | 1.7700 | 0.1385 | 62.3762 | SURVIVES |
| 6 | Tottenham — Everton | 0.6128 | 1.7600 | 0.0785 | 60.7404 | SURVIVES |
| 7 | Real Madrid — Vallecano | 0.6472 | 1.1800 | -0.2363 | 60.3415 | EV_SAFETY_FLOOR |
| 8 | FC Koln — Werder Bremen | 0.5876 | 1.4600 | -0.1420 | 59.1367 | EV_SAFETY_FLOOR |
| 9 | Hoffenheim — Stuttgart | 0.5700 | 1.2400 | -0.2932 | 59.0173 | EV_SAFETY_FLOOR |
| 10 | Liverpool — Fulham | 0.6194 | 1.3500 | -0.1639 | 58.3940 | EV_SAFETY_FLOOR |

### 2026-09-13: ranked top 10

Rejections: `{"EV_SAFETY_FLOOR": 13, "PROBABILITY_SAFETY_FLOOR": 7}`. Research selection IDs: []. Published: none (adoption gate).

| Rank | Match | Probability | Odds | EV | Goal score | Proposed |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | PSV Eindhoven — Sparta Rotterdam | 0.9573 | 1.1000 | 0.0530 | 84.2966 | False |
| 2 | Benfica — Gil Vicente | 0.7869 | 1.4000 | 0.1017 | 69.4106 | False |
| 3 | Galatasaray — Kocaelispor | 0.7773 | 1.4900 | 0.1581 | 68.5898 | False |

Top 10 scored model-ready fixtures, **including excluded diagnostics** (not a coupon):

| Rank | Match | Probability | Odds | EV | Goal score | Safety rejection |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | PSV Eindhoven — Sparta Rotterdam | 0.9573 | 1.1000 | 0.0530 | 84.2966 | SURVIVES |
| 2 | Benfica — Gil Vicente | 0.7869 | 1.4000 | 0.1017 | 69.4106 | SURVIVES |
| 3 | Galatasaray — Kocaelispor | 0.7773 | 1.4900 | 0.1581 | 68.5898 | SURVIVES |
| 4 | Zwolle — Feyenoord | 0.7382 | 1.2600 | -0.0698 | 68.0360 | EV_SAFETY_FLOOR |
| 5 | RB Leipzig — Hamburg | 0.6750 | 1.3000 | -0.1225 | 64.0874 | EV_SAFETY_FLOOR |
| 6 | Club Brugge — Antwerp | 0.6501 | 1.3300 | -0.1353 | 62.6525 | EV_SAFETY_FLOOR |
| 7 | Brest — Paris Saint Germain | 0.6488 | 1.3000 | -0.1566 | 62.5891 | EV_SAFETY_FLOOR |
| 8 | Levante — Barcelona | 0.6330 | 1.1700 | -0.2594 | 60.2889 | EV_SAFETY_FLOOR |
| 9 | Manchester United — Man City | 0.5731 | 1.4200 | -0.1863 | 58.9063 | EV_SAFETY_FLOOR |
| 10 | Excelsior — Utrecht | 0.5410 | 1.4900 | -0.1940 | 58.6935 | EV_SAFETY_FLOOR |

### 2026-09-14: ranked top 10

Rejections: `{"PROBABILITY_SAFETY_FLOOR": 4, "EV_SAFETY_FLOOR": 3}`. Research selection IDs: []. Published: none (adoption gate).

| Rank | Match | Probability | Odds | EV | Goal score | Proposed |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Gaziantep — Fenerbahce | 0.7463 | 1.6600 | 0.2388 | 70.4988 | False |

Top 10 scored model-ready fixtures, **including excluded diagnostics** (not a coupon):

| Rank | Match | Probability | Odds | EV | Goal score | Safety rejection |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Gaziantep — Fenerbahce | 0.7463 | 1.6600 | 0.2388 | 70.4988 | SURVIVES |
| 2 | Inter — Udinese | 0.6708 | 1.4000 | -0.0609 | 63.2242 | EV_SAFETY_FLOOR |
| 3 | Villarreal — Betis | 0.5695 | 1.4600 | -0.1685 | 58.1861 | EV_SAFETY_FLOOR |
| 4 | Torino — Roma | 0.5673 | 1.6600 | -0.0583 | 57.9387 | EV_SAFETY_FLOOR |
| 5 | Leeds — Newcastle | 0.4730 | 1.6600 | -0.2149 | 50.0792 | PROBABILITY_SAFETY_FLOOR |
| 6 | Rio Ave — Estrela | 0.4167 | 1.9600 | -0.1833 | 46.4925 | PROBABILITY_SAFETY_FLOOR |
| 7 | Como — Parma | 0.4085 | 1.5600 | -0.3627 | 42.9231 | PROBABILITY_SAFETY_FLOOR |
| 8 | Sp Braga — Estoril | 0.3851 | 1.5900 | -0.3876 | 42.2842 | PROBABILITY_SAFETY_FLOOR |

## Over 3.5

**BLOCKED_MISSING_HISTORICAL_MARKET_ODDS**

Probability floor 0.35; EV floor 0.0; score cutoff 55.0/100; target 7 selections (minimum 5). **Untuned provisional policy**: historical 3.5 prices are absent. These weights/floors have no ROI validation.

| Component | Weight |
| --- | --- |
| probability | 0.2600 |
| goal_distribution | 0.1800 |
| ev | 0.1500 |
| odds | 0.0600 |
| attack | 0.1200 |
| concession | 0.1000 |
| btts | 0.0300 |
| league | 0.0600 |
| quality | 0.0400 |

OOS date boundary: 2026-01-07. Development: 5242 rows; holdout: 2028 rows. Parameter trials: 0. Rejection reasons: 

Probability-only Over 3.5 cohort above 0.52: 131 fixtures, hit rate 0.5344, Brier 0.2665. This omits the EV gate because prices are missing; it is **not** the old betting policy. Full-policy bet count, ROI and mean odds cannot be established.

| Approach | Bets | Hit rate | ROI | Average odds | Brier | Calibration gap | 5–7 coupons | Coupon ROI |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Old thresholds (single bets) | N/A | N/A | N/A | N/A | N/A | N/A | N/A | N/A |
| Old thresholds, 5–7 rule | N/A | N/A | N/A | N/A | N/A | N/A | N/A | N/A |
| Dedicated rank, 5–7 rule | N/A | N/A | N/A | N/A | N/A | N/A | N/A | N/A |

| Date | Model-ready | Old qualified | Safety survivors | Score survivors | Research 5–7 | Published |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-09-12 | 39 | 1 | 3 | 2 | 0 | 0 |
| 2026-09-13 | 23 | 3 | 3 | 3 | 0 | 0 |
| 2026-09-14 | 8 | 1 | 1 | 1 | 0 | 0 |

### 2026-09-12: ranked top 10

Rejections: `{"PROBABILITY_SAFETY_FLOOR": 29, "EV_SAFETY_FLOOR": 7}`. Research selection IDs: []. Published: none (adoption gate).

| Rank | Match | Probability | Odds | EV | Goal score | Proposed |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Strasbourg — Monaco | 0.5338 | 2.1900 | 0.1691 | 65.2901 | False |
| 2 | Swansea — Burnley | 0.4226 | 3.0200 | 0.2761 | 56.5363 | False |
| 3 | Tottenham — Everton | 0.3897 | 2.9900 | 0.1653 | 53.7262 | False |

Top 10 scored model-ready fixtures, **including excluded diagnostics** (not a coupon):

| Rank | Match | Probability | Odds | EV | Goal score | Safety rejection |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Strasbourg — Monaco | 0.5338 | 2.1900 | 0.1691 | 65.2901 | SURVIVES |
| 2 | For Sittard — Ajax | 0.5244 | 1.8000 | -0.0562 | 58.8599 | EV_SAFETY_FLOOR |
| 3 | Augsburg — Leverkusen | 0.4468 | 1.8700 | -0.1646 | 57.5481 | EV_SAFETY_FLOOR |
| 4 | Swansea — Burnley | 0.4226 | 3.0200 | 0.2761 | 56.5363 | SURVIVES |
| 5 | Southampton — Bristol City | 0.4301 | 2.2800 | -0.0195 | 55.1760 | EV_SAFETY_FLOOR |
| 6 | Tottenham — Everton | 0.3897 | 2.9900 | 0.1653 | 53.7262 | SURVIVES |
| 7 | FC Koln — Werder Bremen | 0.3637 | 2.2200 | -0.1926 | 50.8088 | EV_SAFETY_FLOOR |
| 8 | Real Madrid — Vallecano | 0.4270 | 1.5800 | -0.3254 | 50.5616 | EV_SAFETY_FLOOR |
| 9 | Hoffenheim — Stuttgart | 0.3460 | 1.6800 | -0.4188 | 50.1080 | PROBABILITY_SAFETY_FLOOR |
| 10 | Liverpool — Fulham | 0.3967 | 1.9800 | -0.2145 | 49.3641 | EV_SAFETY_FLOOR |

### 2026-09-13: ranked top 10

Rejections: `{"PROBABILITY_SAFETY_FLOOR": 15, "EV_SAFETY_FLOOR": 5}`. Research selection IDs: []. Published: none (adoption gate).

| Rank | Match | Probability | Odds | EV | Goal score | Proposed |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | PSV Eindhoven — Sparta Rotterdam | 0.8888 | 1.4000 | 0.2444 | 85.7004 | False |
| 2 | Benfica — Gil Vicente | 0.6007 | 2.1300 | 0.2794 | 65.1512 | False |
| 3 | Galatasaray — Kocaelispor | 0.5873 | 2.1900 | 0.2862 | 63.4518 | False |

Top 10 scored model-ready fixtures, **including excluded diagnostics** (not a coupon):

| Rank | Match | Probability | Odds | EV | Goal score | Safety rejection |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | PSV Eindhoven — Sparta Rotterdam | 0.8888 | 1.4000 | 0.2444 | 85.7004 | SURVIVES |
| 2 | Benfica — Gil Vicente | 0.6007 | 2.1300 | 0.2794 | 65.1512 | SURVIVES |
| 3 | Galatasaray — Kocaelispor | 0.5873 | 2.1900 | 0.2862 | 63.4518 | SURVIVES |
| 4 | Zwolle — Feyenoord | 0.5356 | 1.7600 | -0.0574 | 60.4467 | EV_SAFETY_FLOOR |
| 5 | RB Leipzig — Hamburg | 0.4585 | 1.8700 | -0.1426 | 56.1514 | EV_SAFETY_FLOOR |
| 6 | Club Brugge — Antwerp | 0.4302 | 1.9500 | -0.1610 | 54.3612 | EV_SAFETY_FLOOR |
| 7 | Brest — Paris Saint Germain | 0.4287 | 1.8700 | -0.1983 | 53.8980 | EV_SAFETY_FLOOR |
| 8 | Excelsior — Utrecht | 0.3179 | 2.3100 | -0.2658 | 50.9203 | PROBABILITY_SAFETY_FLOOR |
| 9 | Levante — Barcelona | 0.4114 | 1.5700 | -0.3542 | 50.4970 | EV_SAFETY_FLOOR |
| 10 | Manchester United — Man City | 0.3490 | 2.1300 | -0.2566 | 49.2692 | PROBABILITY_SAFETY_FLOOR |

### 2026-09-14: ranked top 10

Rejections: `{"PROBABILITY_SAFETY_FLOOR": 6, "EV_SAFETY_FLOOR": 1}`. Research selection IDs: []. Published: none (adoption gate).

| Rank | Match | Probability | Odds | EV | Goal score | Proposed |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Gaziantep — Fenerbahce | 0.5460 | 2.6100 | 0.4250 | 65.6803 | False |

Top 10 scored model-ready fixtures, **including excluded diagnostics** (not a coupon):

| Rank | Match | Probability | Odds | EV | Goal score | Safety rejection |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Gaziantep — Fenerbahce | 0.5460 | 2.6100 | 0.4250 | 65.6803 | SURVIVES |
| 2 | Inter — Udinese | 0.4537 | 2.1300 | -0.0337 | 55.4808 | EV_SAFETY_FLOOR |
| 3 | Torino — Roma | 0.3433 | 2.7400 | -0.0593 | 49.6912 | PROBABILITY_SAFETY_FLOOR |
| 4 | Villarreal — Betis | 0.3455 | 2.2200 | -0.2330 | 49.4127 | PROBABILITY_SAFETY_FLOOR |
| 5 | Leeds — Newcastle | 0.2566 | 2.7100 | -0.3046 | 40.6100 | PROBABILITY_SAFETY_FLOOR |
| 6 | Rio Ave — Estrela | 0.2105 | 3.5400 | -0.2548 | 38.6580 | PROBABILITY_SAFETY_FLOOR |
| 7 | Sp Braga — Estoril | 0.1864 | 2.5900 | -0.5171 | 33.4782 | PROBABILITY_SAFETY_FLOOR |
| 8 | Como — Parma | 0.2042 | 2.5200 | -0.4855 | 33.1113 | PROBABILITY_SAFETY_FLOOR |

## Reproduction and limits

`python tools/goal_market_selection.py --output .tmp-dataflow/goal-selection`

The output JSON includes source file hashes, feature/prediction cutoffs, live odds IDs, score inputs, chosen weights and excluded reasons. `oos-bets.csv` records each historical evaluated bet and actual unit return. Historical CSV prices lack an exact collection timestamp per fixture: they support a bookmaker-proxy research backtest, not an exact replay of the live Iddaa feed. The old comparison under a common 5–7 rule uses the existing general score to truncate to seven; this is explicitly a standardized comparison, since the old goal coupon generator had no seven-selection cap. Brier/gap comparisons use different selected cohorts; they are not a claim of model recalibration.

The OOS model has already been studied previously. An initial diagnostic also evaluated a seven-selection-only policy on the later dates before the research family included 5/6/7 lengths and mild negative-EV floors. The optimizer never uses holdout labels, but this is retrospective analysis, not a claim of a pristine prospective trial. No profitable 5–7-leg product is assumed from single-bet results.

Ten targeted unit tests cover publication bounds, stale/future/missing prices, score inputs, diagnostics, determinism, chronological separation and the missing-price adoption block. No Rust/frontend or application code changed, so application builds were not rerun for this research-only implementation.
