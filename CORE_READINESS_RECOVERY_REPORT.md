# ARZ Core Readiness — 13 September 2026

Verified in the rebuilt real Tauri desktop application using production AppData, not a mock database. General status changed from **PARTIAL to READY**. Core score is **10 / 10**; prediction, candidate and coupon capabilities are all true.

Production database: `C:\Users\husey\AppData\Roaming\com.footballpredictor.app\football-predictor.sqlite3`. SQLite quick_check passed. Before-write backup: `.tmp-dataflow/production-2026-09-13T18-57-36-985Z.sqlite3`.

## Ten required checks

| Core check | Final state | Evidence |
|---|---|---|
| Live Iddaa and odds | READY | Current bulletin and odds within the 24-hour freshness gate |
| Historical/model input | READY | 7,982 finished matches; zero future production events missing minimum team history |
| BASE model | READY | Active `arz-live-2026-09-06`, validated artifact `55661f138feb…` |
| Calibration | READY | Active calibration compatible with the BASE artifact |
| Features | READY | `fe_v1`; zero future production events missing valid feature data |
| Local database | READY | Production database accessible; foreign keys enabled; quick_check OK |
| Current/future resolution | READY | Today 29/29; remaining future 10/10; zero blocking unresolved |
| Candidate generation | READY | Published daily view readable; 64 unique selections displayed |
| Coupon generation | READY | Published sources readable; Corner, BTTS, High Confidence and Surprise coupons displayed |
| Popularity | READY | Persisted popularity snapshot: 116 selections; current-date desktop view: 18 rows |

Readiness is calculated in `src-tauri/src/repositories/data_center.rs`. The frontend renders the backend's ten checks and score. Optional modules are excluded from this denominator. Every required check must be READY for General Status to be READY; a real required blocker lowers the score.

## Resolution recovery

| Metric | Before | After |
|---|---:|---:|
| Today supported | 29 | 29 |
| Today matched | 28 | 29 |
| Today unmatched | 1 | 0 |
| Registered supported | 176 | 176 |
| Registered matched | 171 | 176 |
| Registered unresolved | 5 | 0 |
| Out of scope | 2,345 | 2,345 |
| Future supported / matched | 11 / 11 | 10 / 10 |
| Future unresolved | 0 | 0 |

The future count decreased as the clock passed a kickoff during verification; no event was removed to improve the score. All five formerly unresolved events remain in the database with corrected team mappings.

| Iddaa event | League / fixture | Kickoff UTC | Before classification | Final mapping repair |
|---|---|---|---|---|
| 3104262 | Bundesliga: M'gladbach–Elversberg | Sep 5 13:30 | Historical, cancelled; non-blocking | Elversberg 1516 → 3399 |
| 3104536 | Bundesliga: Paderborn–Freiburg | Sep 5 13:30 | Historical, cancelled; non-blocking | Paderborn 280 → 3395 |
| 3103243 | Bundesliga: Schalke 04–Bayern Munich | Sep 5 16:30 | Historical, cancelled; non-blocking | Schalke 04 264 → 3400 |
| 3121196 | Bundesliga: Dortmund–Paderborn | Sep 12 13:30 | Stale prior-date scheduled row; non-blocking | Paderborn 280 → 3395 |
| 3125363 | Bundesliga: Elversberg–Bayern Munich | Sep 13 15:30 | Today's genuine missing canonical mapping; kickoff already passed at audit | Elversberg 1516 → 3399 |

The exact current event is local match **9626**, kickoff **13 September 18:30 Istanbul**. Before: home team 1516 had only `iddaa:Elversberg`; away 2210 already had `iddaa:Bayern Münih` and `football-data.co.uk:Bayern Munich`. Elversberg's football-data canonical team/history was absent. No persisted alias could supply a nonexistent canonical history target. After: home 3399 carries both `iddaa:Elversberg` and `football-data.co.uk:Elversberg`; away mapping is unchanged.

The same absent German D2 history affected Paderborn and Schalke. Imported the real 306-match D2 2024/25 season, then used the existing transactional `merge_normalized_teams` desktop command for three exact-name canonical merges. Existing provider mappings were preserved and merge audit records written. Resolver thresholds were not changed. No synthetic results, odds or probabilities were inserted.

Source: [Club Football Match Data](https://github.com/xgabora/Club-Football-Match-Data), whose documentation attributes match results/statistics to football-data.co.uk. Downloaded data blob SHA: `2c9305453f82586b0ab7d1f6bfa4c40c4aae8c90`. Extraction kept actual results/statistics, normalized date/time and integer serialization, and excluded Elo/odds. Provenance and converted CSV are in `.tmp-dataflow/readiness/D2-provenance.json` and `D2-2425-converted.csv`. An initial import rejected all rows for HH:MM:SS formatting; after correcting the format, run 66 imported all 306 with zero failures.

Also repaired a real future input gap: Maritimo had only four finished matches and match 9328 lacked features. Imported 306 genuine P1 2022/23 matches from the existing [football-data mirror](https://raw.githubusercontent.com/Char2mant/futbol-veri-aynasi/main/data/fd/2223/P1.csv), then generated a real BASE prediction for Moreirense–Maritimo through `prediction_generate_for_match`. The artifact, calibrated market output and features are recorded in `.tmp-dataflow/readiness/prediction-Maritimo.json`. No model redesign or retraining occurred.

Resolution readiness now gates scheduled, not-yet-started, model-supported production events. Historical/cancelled rows, provider diagnostics and unsupported competitions remain visible without blocking this capability. Today's full business-date counts remain separately visible. Required history and features use the production universe, not the complete historical catalog.

## Optional modules

| Module | Final state | Actual condition |
|---|---|---|
| Lineup model | OPTIONAL / İSTEĞE BAĞLI | Engine exists; trained production artifact absent |
| Live lineup provider | OPTIONAL / İSTEĞE BAĞLI | Authoritative provider unavailable |
| Team logos | OPTIONAL / İSTEĞE BAĞLI | Worker idle; cosmetic gaps remain |

Logo counts at verification: **635 ready, 0 queued, 0 downloading, 0 retry later, 0 rate limited, 2,556 unavailable, 1 invalid**. Because no queue was processing at acceptance, the UI correctly used OPTIONAL. When the worker runs, its state is BACKGROUND / ARKA PLANDA TAMAMLANIYOR. Neither condition changes the core score. A configured but invalid optional lineup artifact still reports its error without making the core pipeline unready.

## Real desktop acceptance

Inspected the real rendered Data Center, Candidates, Coupons and Populars screenshots. Data Center shows **HAZIR**, “Temel tahmin, aday ve kupon akışı kullanıma hazır.”, **10 / 10 HAZIR**, and all three capabilities **Evet**.

Today, Candidates and Coupons share publication **`2026-09-13:base:63:btts:80`**, BASE candidate run 63 with the explicit BTTS run 80 overlay. The candidates remain BASE-labelled; unavailable lineup functionality is not represented as a lineup revision.

Candidate counts were preserved by this readiness cleanup:

| Tab | Before cleanup | After cleanup |
|---|---:|---:|
| Tümü | 64 | 64 |
| Korner | 16 | 16 |
| 2.5 Üst | 3 | 3 |
| 3.5 Üst | 3 | 3 |
| KG Var | 17 | 17 |
| Yüksek Güven | 11 | 11 |
| Sürpriz | 8 | 8 |
| Katlama | 17 | 17 |

Displayed coupons: Corner 7, BTTS 7, High Confidence 10, Surprise 7. Over 2.5 and Over 3.5 correctly report insufficient selections (3 available versus a minimum of 5). The existing two-selection Katlama step is explicitly dated September 12 and awaits settlement; today's eligible pool is 17. These policy/settlement conditions are not fabricated into filled daily coupons. Populars displays 18 rows and retains unsupported/missing-odds labels where applicable.

Six displayed candidate records were rechecked against persisted candidate, prediction and odds rows, including probability, odds, EV and confidence. Coupon source membership, shared publication identity and rapid tab/date navigation passed.

Validation: `npm.cmd run build` passed; production-protocol Rust desktop build passed; all **7 data_center_tests** passed, including each core blocker lowering readiness and historical unresolved rows not blocking future resolution. Existing unrelated compiler warnings remain.

Evidence:

- `.tmp-dataflow/readiness/before.json` and `after.json`: backend snapshots and production database identity.
- `.tmp-dataflow/readiness/resolution-audit.json`: all five before/after mappings and SQLite integrity result.
- `.tmp-dataflow/readiness/desktop-acceptance.json`: real desktop assertions and final backend checks.
- `.tmp-dataflow/readiness/desktop-data-center.png`: inspected complete Data Center screen.
- `.tmp-dataflow/readiness/desktop-candidates.png`, `desktop-coupons.png`, `desktop-populars.png`: inspected production screens.
- `.tmp-dataflow/candidates-recovery/acceptance.json`: six-record cross-check and shared publication acceptance.

Licensing and updater were not modified by this cleanup.
