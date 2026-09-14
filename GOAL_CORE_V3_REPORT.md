# GOAL CORE V3

**REJECTED. Production and coupon rules are unchanged.**

Four expanding date-block folds; fit/tune/calibration/selection/test blocks separated by 48-hour label embargoes. Match features are frozen 24 hours before midnight Istanbul. September 12–14 never enter fitting, calibration, threshold selection, or OOS scoring.

A is a causal refit of production Poisson + logistic BTTS architecture, its tune-selected BTTS blend, and its family Platt public calibration with log-loss/ECE acceptance and raw fallback. The calibration block is split chronologically into fit/validation halves with a 48-hour embargo. It is not a replay of archived deployment artifacts. B is the shared Poisson matrix; C calibrates its joint event masses; D mixes C with a coherent projection of A. N is nominated before each OOS block using average Over 2.5/BTTS log loss.

History: 7370 matches; ready: 6722; OOS: 3962 matches; 70 Saturdays including empty days.

## Probability quality

| Market / model | Brier | Log loss | ECE | Calibration gap | Probability >=64% |
|---|---:|---:|---:|---:|---:|
| over25 / A_production | 0.24665 | 0.68687 | 0.03920 | -0.01342 | 469 |
| over25 / B_joint | 0.24564 | 0.68425 | 0.02670 | -0.02670 | 99 |
| over25 / C_calibrated | 0.24490 | 0.68271 | 0.00909 | -0.00909 | 139 |
| over25 / D_ensemble | 0.24447 | 0.68182 | 0.02223 | -0.00854 | 311 |
| over25 / N_nominated | 0.24465 | 0.68221 | 0.02133 | -0.01113 | 297 |
| btts / A_production | 0.24842 | 0.69018 | 0.04697 | -0.03830 | 113 |
| btts / B_joint | 0.24673 | 0.68656 | 0.01888 | -0.01888 | 18 |
| btts / C_calibrated | 0.24630 | 0.68568 | 0.01132 | -0.01132 | 33 |
| btts / D_ensemble | 0.24660 | 0.68632 | 0.02666 | -0.02416 | 49 |
| btts / N_nominated | 0.24667 | 0.68647 | 0.02331 | -0.02105 | 54 |

## Verified betting evidence

There are no qualifying timestamped historical quotes. For every model and both markets: qualified bets = 0; Saturdays with >=5 valid selections = 0; hit rate, single/coupon ROI, average odds, drawdown, and losing streak are N/A. Missing evidence is not a zero return.

## Over 2.5 retrospective price proxy

Recorded non-closing B365 prices lack quote capture times. These results are descriptive and do not count as verified valid selections for adoption. BTTS has no matched historical prices, so its monetary metrics remain N/A.

| Model | Qualified bets | Hit rate | ROI | Mean odds | Max DD (u) | Longest losses |
|---|---:|---:|---:|---:|---:|---:|
| A_production | 344 | 0.61337 | -0.04387 | 1.61305 | 21.97000 | 6 |
| B_joint | 17 | 0.76471 | 0.20765 | 1.62824 | 2.47000 | 2 |
| C_calibrated | 28 | 0.64286 | 0.03000 | 1.65143 | 3.90000 | 3 |
| D_ensemble | 154 | 0.65584 | 0.02812 | 1.62097 | 10.45000 | 4 |
| N_nominated | 151 | 0.66225 | 0.03662 | 1.61980 | 10.45000 | 4 |

| Model | Saturdays tested | >=5 proxy-qualified | Coupon hit rate | Coupon ROI | Mean combined odds | Max DD (u) | Longest losses |
|---|---:|---:|---:|---:|---:|---:|---:|
| A_production | 70 | 12 | 0.16667 | 0.40478 | 10.14755 | 7.00000 | 7 |
| B_joint | 70 | 0 | N/A | N/A | N/A | N/A | None |
| C_calibrated | 70 | 0 | N/A | N/A | N/A | N/A | None |
| D_ensemble | 70 | 3 | 0.33333 | 2.28917 | 14.33390 | 2.00000 | 2 |
| N_nominated | 70 | 3 | 0.33333 | 2.28917 | 14.33390 | 2.00000 | 2 |

3 proxy-priced coupons qualify for the nominated strategy, versus 12 for A. A larger observed coupon ROI is not sufficient evidence of an advantage with this sample. The paired all-Saturday profit bootstrap includes zero stakes on inactive days and is stored in evaluation.json; it is a profit-per-calendar-Saturday difference, not a difference in ROI per coupon.

Coupon evaluation uses the top five distinct fixtures only after each passes the unchanged 0.64 probability and positive-EV requirements. Fewer than five means no coupon. Markets are evaluated separately; no double counting a fixture inside a coupon. One unit per single/coupon, ordered by kickoff/id and Saturday respectively. Fees/taxes/slippage are excluded.

## Adoption

Predeclared gate: >=1% relative improvement in both Brier and log loss for each market, paired day-bootstrap Brier upper bound below zero, >=10% more verified usable selections, and nondegrading verified coupon availability, ROI and drawdown. The bootstrap resamples day means (5,000 draws); it is exploratory because V2 already examined these dates.

- over25: material probability improvement is not established.
- over25: improved usable selection availability is not established.
- over25: probability-floor availability falls from 469 to 297 matches before requiring a price.
- over25: nondegrading coupon performance cannot be established.
- btts: material probability improvement is not established.
- btts: improved usable selection availability is not established.
- btts: probability-floor availability falls from 113 to 54 matches before requiring a price.
- btts: nondegrading coupon performance cannot be established.
- Historical results end before timestamped Iddaa odds begin; verified historical BTTS/Over 2.5 execution and coupon evidence is unavailable.
- These historical OOS periods were examined in V2 research; they are temporal OOS, not a fresh research holdout. Production deployment-vintage replay is unavailable.

## September 12/13/14 after selection

Historical nomination: **C_calibrated**. Retained production: **arz-live-2026-09-06**. Daily rows below use retained production public probabilities; challenger tables are in sep-12-13-14.json.

Daily snapshots use midnight Istanbul and the available stored data. Prices older than 24 hours are shown for audit, with descriptive EV, but cannot qualify. September 14 is a frozen-data forecast, not evidence that future quotes will be available.

| Date | Feature ready | Production prediction ready | Over 2.5 qualified | BTTS qualified |
|---|---:|---:|---:|---:|
| 2026-09-12 | 39 | 39 | 2 | 0 |
| 2026-09-13 | 24 | 23 | 3 | 1 |
| 2026-09-14 | 8 | 8 | 0 | 0 |

| Date | Total fixtures | Challenger prediction ready | Challenger Over 2.5 qualified | Challenger BTTS qualified |
|---|---:|---:|---:|---:|
| 2026-09-12 | 654 | 39 | 0 | 0 |
| 2026-09-13 | 399 | 24 | 0 | 1 |
| 2026-09-14 | 92 | 8 | 0 | 0 |

### 2026-09-12 over25 top 10 — retained production

| Match | Probability | Odds | EV | Valid quote | Qualified |
|---|---:|---:|---:|---|---|
| Strasbourg – Monaco | 0.73686 | 1.45000 | 0.06845 | True | True |
| For Sittard – Ajax | 0.72940 | 1.27000 | -0.07366 | True | False |
| Augsburg – Leverkusen | 0.66476 | 1.31000 | -0.12917 | True | False |
| Southampton – Bristol City | 0.64996 | 1.48000 | -0.03805 | True | False |
| Real Madrid – Vallecano | 0.64719 | 1.18000 | -0.23631 | True | False |
| Swansea – Burnley | 0.64321 | 1.77000 | 0.13848 | True | True |
| Liverpool – Fulham | 0.61937 | 1.35000 | -0.16386 | True | False |
| Tottenham – Everton | 0.61278 | 1.76000 | 0.07849 | True | False |
| Casa Pia – Porto | 0.58887 | 1.63000 | -0.04014 | True | False |
| FC Koln – Werder Bremen | 0.58764 | 1.46000 | -0.14204 | True | False |

### 2026-09-12 btts top 10 — retained production

| Match | Probability | Odds | EV | Valid quote | Qualified |
|---|---:|---:|---:|---|---|
| For Sittard – Ajax | 0.51052 | 1.44000 | -0.26486 | True | False |
| Strasbourg – Monaco | 0.50985 | 1.39000 | -0.29131 | True | False |
| Augsburg – Leverkusen | 0.50737 | 1.33000 | -0.32520 | True | False |
| Tottenham – Everton | 0.50654 | 1.63000 | -0.17434 | True | False |
| FC Koln – Werder Bremen | 0.50508 | 1.40000 | -0.29288 | True | False |
| Southampton – Bristol City | 0.50403 | 1.51000 | -0.23891 | True | False |
| West Brom – QPR | 0.50316 | 1.56000 | -0.21506 | True | False |
| Swansea – Burnley | 0.50315 | 1.62000 | -0.18489 | True | False |
| Hoffenheim – Stuttgart | 0.50304 | 1.25000 | -0.37120 | True | False |
| Eyupspor – Rizespor | 0.50259 | 1.70000 | -0.14560 | True | False |

### 2026-09-13 over25 top 10 — retained production

| Match | Probability | Odds | EV | Valid quote | Qualified |
|---|---:|---:|---:|---|---|
| PSV Eindhoven – Sparta Rotterdam | 0.95728 | 1.10000 | 0.05301 | True | True |
| Benfica – Gil Vicente | 0.78695 | 1.40000 | 0.10173 | True | True |
| Galatasaray – Kocaelispor | 0.77726 | 1.49000 | 0.15811 | True | True |
| Zwolle – Feyenoord | 0.73821 | 1.26000 | -0.06985 | True | False |
| RB Leipzig – Hamburg | 0.67498 | 1.30000 | -0.12253 | True | False |
| Club Brugge – Antwerp | 0.65011 | 1.33000 | -0.13535 | True | False |
| Brest – Paris Saint Germain | 0.64875 | 1.30000 | -0.15662 | True | False |
| Levante – Barcelona | 0.63298 | 1.17000 | -0.25942 | True | False |
| Manchester United – Man City | 0.57306 | 1.42000 | -0.18626 | True | False |
| Coventry – Brighton | 0.54603 | 1.62000 | -0.11544 | True | False |

### 2026-09-13 btts top 10 — retained production

| Match | Probability | Odds | EV | Valid quote | Qualified |
|---|---:|---:|---:|---|---|
| Zwolle – Feyenoord | 0.69641 | 1.41000 | -0.01806 | True | False |
| Brest – Paris Saint Germain | 0.64361 | 1.68000 | 0.08126 | True | True |
| Manchester United – Man City | 0.59595 | 1.37000 | -0.18355 | True | False |
| Coventry – Brighton | 0.57143 | 1.57000 | -0.10286 | True | False |
| RB Leipzig – Hamburg | 0.56512 | 1.55000 | -0.12406 | True | False |
| Excelsior – Utrecht | 0.56088 | 1.43000 | -0.19794 | True | False |
| Waregem – Charleroi | 0.55005 | 1.48000 | -0.18593 | True | False |
| Levante – Barcelona | 0.53821 | 1.71000 | -0.07967 | True | False |
| RAAL La Louviere – Kortrijk | 0.52702 | 1.64000 | -0.13569 | True | False |
| Sociedad – Ath Madrid | 0.52669 | 1.44000 | -0.24156 | True | False |

### 2026-09-14 over25 top 10 — retained production

| Match | Probability | Odds | EV | Valid quote | Qualified |
|---|---:|---:|---:|---|---|
| Gaziantep – Fenerbahce | 0.74629 | 1.66000 | 0.23884 | False | False |
| Inter – Udinese | 0.67078 | 1.40000 | -0.06091 | False | False |
| Villarreal – Betis | 0.56950 | 1.46000 | -0.16853 | False | False |
| Torino – Roma | 0.56730 | 1.66000 | -0.05829 | False | False |
| Leeds – Newcastle | 0.47296 | 1.66000 | -0.21488 | False | False |
| Rio Ave – Estrela | 0.41668 | 1.96000 | -0.18331 | False | False |
| Como – Parma | 0.40853 | 1.56000 | -0.36270 | False | False |
| Sp Braga – Estoril | 0.38513 | 1.59000 | -0.38764 | False | False |

### 2026-09-14 btts top 10 — retained production

| Match | Probability | Odds | EV | Valid quote | Qualified |
|---|---:|---:|---:|---|---|
| Gaziantep – Fenerbahce | 0.63385 | 1.68000 | 0.06487 | False | False |
| Villarreal – Betis | 0.56937 | 1.43000 | -0.18580 | False | False |
| Torino – Roma | 0.53241 | 1.73000 | -0.07894 | False | False |
| Leeds – Newcastle | 0.50277 | 1.53000 | -0.23076 | False | False |
| Inter – Udinese | 0.49829 | 1.91000 | -0.04827 | False | False |
| Rio Ave – Estrela | 0.47611 | 1.73000 | -0.17633 | False | False |
| Sp Braga – Estoril | 0.42890 | 1.79000 | -0.23226 | False | False |
| Como – Parma | 0.41298 | 2.21000 | -0.08732 | False | False |

### 2026-09-12 over25 top 10 — nominated challenger (not adopted)

| Match | Probability | Odds | EV | Valid quote | Qualified |
|---|---:|---:|---:|---|---|
| Augsburg – Leverkusen | 0.69640 | 1.31000 | -0.08772 | True | False |
| Hoffenheim – Stuttgart | 0.69105 | 1.26000 | -0.12928 | True | False |
| Go Ahead Eagles – Groningen | 0.66290 | 1.34000 | -0.11172 | True | False |
| For Sittard – Ajax | 0.64347 | 1.27000 | -0.18279 | True | False |
| Southampton – Bristol City | 0.63863 | 1.48000 | -0.05483 | True | False |
| Mainz – Ein Frankfurt | 0.63634 | 1.37000 | -0.12821 | True | False |
| Strasbourg – Monaco | 0.61414 | 1.45000 | -0.10950 | True | False |
| Real Madrid – Vallecano | 0.61280 | 1.18000 | -0.27689 | True | False |
| Swansea – Burnley | 0.60546 | 1.77000 | 0.07166 | True | False |
| Aston Villa – Nott'm Forest | 0.60220 | 1.80000 | 0.08396 | True | False |

### 2026-09-12 btts top 10 — nominated challenger (not adopted)

| Match | Probability | Odds | EV | Valid quote | Qualified |
|---|---:|---:|---:|---|---|
| Hoffenheim – Stuttgart | 0.69797 | 1.25000 | -0.12754 | True | False |
| Augsburg – Leverkusen | 0.69782 | 1.33000 | -0.07190 | True | False |
| Go Ahead Eagles – Groningen | 0.66897 | 1.32000 | -0.11696 | True | False |
| Mainz – Ein Frankfurt | 0.64136 | 1.36000 | -0.12775 | True | False |
| Strasbourg – Monaco | 0.63861 | 1.39000 | -0.11234 | True | False |
| Aston Villa – Nott'm Forest | 0.62277 | 1.63000 | 0.01512 | True | False |
| For Sittard – Ajax | 0.61785 | 1.44000 | -0.11030 | True | False |
| Freiburg – M'gladbach | 0.61394 | 1.47000 | -0.09751 | True | False |
| FC Koln – Werder Bremen | 0.60706 | 1.40000 | -0.15012 | True | False |
| Oud-Heverlee Leuven – Cercle Brugge | 0.60113 | 1.44000 | -0.13437 | True | False |

### 2026-09-13 over25 top 10 — nominated challenger (not adopted)

| Match | Probability | Odds | EV | Valid quote | Qualified |
|---|---:|---:|---:|---|---|
| PSV Eindhoven – Sparta Rotterdam | 0.83559 | 1.10000 | -0.08085 | True | False |
| Benfica – Gil Vicente | 0.69949 | 1.40000 | -0.02071 | True | False |
| Club Brugge – Antwerp | 0.68775 | 1.33000 | -0.08529 | True | False |
| RB Leipzig – Hamburg | 0.66902 | 1.30000 | -0.13027 | True | False |
| Zwolle – Feyenoord | 0.63967 | 1.26000 | -0.19402 | True | False |
| Galatasaray – Kocaelispor | 0.63547 | 1.49000 | -0.05315 | True | False |
| Heidenheim – Holstein Kiel | 0.62847 | 1.41000 | -0.11385 | True | False |
| Manchester United – Man City | 0.62393 | 1.42000 | -0.11403 | True | False |
| Brest – Paris Saint Germain | 0.61978 | 1.30000 | -0.19429 | True | False |
| Excelsior – Utrecht | 0.59231 | 1.49000 | -0.11746 | True | False |

### 2026-09-13 btts top 10 — nominated challenger (not adopted)

| Match | Probability | Odds | EV | Valid quote | Qualified |
|---|---:|---:|---:|---|---|
| Manchester United – Man City | 0.64499 | 1.37000 | -0.11637 | True | False |
| Heidenheim – Holstein Kiel | 0.64490 | 1.37000 | -0.11648 | True | False |
| Brest – Paris Saint Germain | 0.64085 | 1.68000 | 0.07662 | True | True |
| RB Leipzig – Hamburg | 0.61323 | 1.55000 | -0.04949 | True | False |
| Heerenveen – Telstar | 0.61291 | 1.37000 | -0.16031 | True | False |
| Sociedad – Ath Madrid | 0.61085 | 1.44000 | -0.12037 | True | False |
| Excelsior – Utrecht | 0.60399 | 1.43000 | -0.13629 | True | False |
| Levante – Barcelona | 0.59035 | 1.71000 | 0.00950 | True | False |
| Waregem – Charleroi | 0.58477 | 1.48000 | -0.13454 | True | False |
| Coventry – Brighton | 0.58239 | 1.57000 | -0.08565 | True | False |

### 2026-09-14 over25 top 10 — nominated challenger (not adopted)

| Match | Probability | Odds | EV | Valid quote | Qualified |
|---|---:|---:|---:|---|---|
| Inter – Udinese | 0.62696 | 1.40000 | -0.12225 | False | False |
| Torino – Roma | 0.61140 | 1.66000 | 0.01493 | False | False |
| Villarreal – Betis | 0.60942 | 1.46000 | -0.11024 | False | False |
| Gaziantep – Fenerbahce | 0.59903 | 1.66000 | -0.00562 | False | False |
| Como – Parma | 0.53240 | 1.56000 | -0.16945 | False | False |
| Sp Braga – Estoril | 0.51758 | 1.59000 | -0.17705 | False | False |
| Leeds – Newcastle | 0.51308 | 1.66000 | -0.14829 | False | False |
| Rio Ave – Estrela | 0.46994 | 1.96000 | -0.07893 | False | False |

### 2026-09-14 btts top 10 — nominated challenger (not adopted)

| Match | Probability | Odds | EV | Valid quote | Qualified |
|---|---:|---:|---:|---|---|
| Villarreal – Betis | 0.62208 | 1.43000 | -0.11043 | False | False |
| Inter – Udinese | 0.59171 | 1.91000 | 0.13016 | False | False |
| Torino – Roma | 0.59153 | 1.73000 | 0.02334 | False | False |
| Gaziantep – Fenerbahce | 0.58437 | 1.68000 | -0.01826 | False | False |
| Leeds – Newcastle | 0.55821 | 1.53000 | -0.14594 | False | False |
| Rio Ave – Estrela | 0.51620 | 1.73000 | -0.10697 | False | False |
| Sp Braga – Estoril | 0.51463 | 1.79000 | -0.07882 | False | False |
| Como – Parma | 0.47874 | 2.21000 | 0.05801 | False | False |

## Artifacts and limits

evaluation.json contains all metrics, reliability bins, fold dates, fit settings, hash provenance and bootstrap intervals. Per-match prediction JSON and per-Saturday selections/settlements are saved for every model/market. Saved joblib models are research-only. The final model uses historical stage selection before these September dates; target results are ignored.

The matrix uses goals 0–40 with normalized negligible-tail truncation (lambda cap 8). Calibration and ensemble preserve normalization/nonnegativity, Over 3.5 <= Over 2.5 and exact BTTS cell mass. Reported lambda_home/lambda_away are marginal expected goals after calibration/mixture; a mixture need not itself have Poisson marginals. Raw B uses Poisson intensities. Recency decay is 90 days in team histories and 365 days in fitting. Opponent attack/defense adjustment uses opponents’ own pre-match venue snapshots with five-game shrinkage.

Untimestamped prices never enter features. A market prior requires both sides from the same bookmaker and capture timestamp before the decision, with age <=24h. No historical priors pass; unseen live-only prior columns are masked to avoid applying untrained market effects. Original data publication/correction vintages are unavailable; feature causality is event-time, not ingestion-vintage.

GOAL CORE V3 REJECTED — material OOS improvement and verified usable-selection/coupon evidence are not established.
