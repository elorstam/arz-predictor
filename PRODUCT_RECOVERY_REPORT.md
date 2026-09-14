# ARZ Predictor recovery — 11 September 2026

The imported ten-competition BASE universe now reaches real desktop candidates and selection-bearing coupons. Full coverage expansion remains blocked by the official football-data CSV host being unreachable from this environment. No model probabilities, fixtures, candidates, or lineups were fabricated, and qualification thresholds were not relaxed.

## Measured production state

Audit time: 23:40 Europe/Istanbul. The comparison backup and final audit use the same future-match cutoff universe; the user's earlier UI observation had six more upcoming events and thirteen matches remaining that day.

| Metric | Before | After |
|---|---:|---:|
| Live Iddaa competitions | 211 (reported) | 211 |
| Upcoming bulletin matches | 1,157 (reported); 1,151 at comparison cutoff | 1,151 |
| Stored Iddaa events / event competitions | 2,372 / 266 | 2,372 / 266 |
| Imported model-supported competitions | 10 | 10 |
| Catalogued European divisions | 10 original | 22 |
| Supported upcoming matches | 88 | 88 |
| Unsupported upcoming matches | 1,063 | 1,063 |
| Supported resolved / unresolved | 85 / 3 | 86 / 2 |
| Supported resolution | 96.59% | 97.73% |
| Upcoming feature-ready / BASE-predicted / calibrated | 0 / 0 / 0 | 70 / 70 / 70 |
| Resolved upcoming matches lacking sufficient history | — | 16 |
| Logos ready / failed | 0 / 2,269 | 3 / 3 |
| Logos missing, including unavailable discovery | 701 | 2,963 |
| Known teams | 2,970 | 2,969 |
| Valid cached logo bytes | 0 | 215,835 |

Missing logo states are 2,955 MISSING plus 8 UNAVAILABLE. Rate-limited and unattempted discoveries are no longer misreported as thousands of corrupt downloads. QPR's audited canonical merge explains the one-team reduction.

| Istanbul business date | BASE run | Qualified candidates | Actual usable coupons |
|---|---:|---:|---:|
| 11 September | 26 | 0 | 0 |
| 12 September | 27 | 15 | 3 |
| 13 September | 28 | 13 | 3 |
| 14 September | 29 | 4 | 2 |

At acceptance time no matches remained in today's current bulletin. Its zero result is truthful. The daily diagnostic includes 35 stored day records: 33 outside coverage, and two started matches, also lacking applicable odds. Exclusion groups may overlap.

September 12 populars contain 13 distinct selections: 11 with model numbers and below-policy classification, one missing model history, one unresolved. No duplicate selection rows. September 11 also visibly classifies an unsupported competition and two unsupported markets as outside model coverage. September 13 includes one positively model-supported popular selection. Popularity is not treated as model endorsement.

## Repairs and evidence

- Coverage is shared across matches, candidate exclusions, popularity and readiness. Unsupported competitions are separated from genuine supported-team resolution failures and missing inputs.
- The live pipeline accepts Iddaa fixture identity and odds with canonical football-data history, without requiring an identical provider fixture row. It processes every upcoming supported, resolved match with sufficient history and prepares daily candidates/coupons for upcoming business dates.
- Corrected binary raw probabilities: the opposing outcome previously reused the first outcome's probability. Calibration consumed that incorrect field. Corrected prediction revisions are appended without deleting historical predictions. All 1,820 latest binary pairs have zero raw/public complement error. Calibration training used the correct original model-probability field; a costly recalibration was unnecessary.
- Candidate loading now respects actual inference timestamps, active model/revision selection, generation cutoff and match start. Unchanged odds receive fresh observations after the deduplication interval. Category suppression ranks eligible selections before choosing the representative market. Explicit exclusions replace silent drops.
- Empty drafts are neither generated as successful results nor returned/countable as usable coupons. Valid BASE drafts survive unrelated LINEUP_AWARE comparison runs. Reading coupon impact with no authoritative lineup revision no longer creates a candidate run.
- September 12 has an Over 2.5 coupon with Strasbourg–Monaco at 1.45 and Swansea–Burnley at 1.77: exact product 2.5665, UI 2.57. Over 3.5 has a real selection at 2.19. Surprise has seven real selections and 29 system columns. All returned accumulator and system-column odds were independently multiplied and verified. Unqualified corner, BTTS, high-confidence and compound types remain explicitly unavailable.
- Popular snapshots are deduplicated by genuine provider selection identity. Deterministic stored Iddaa market mappings repair applicable UNKNOWN markets. Supported rows expose model probability, odds, edge and EV; disagreement remains visible.
- Lineup implementation exists, but production has neither a trained active lineup artifact nor a configured authoritative live provider. Data Center reports these separately. BASE is operational; no lineups were invented.
- Logo discovery URL/thumbnail parsing, response validation, retry/backoff and known-source prioritization were repaired. HTTP 429 stops discovery instead of marking the whole queue failed. Tauri's cache scope now correctly uses `$APPDATA/assets/**`; `$APPDATA` already contains the app identifier. Chelsea, Liverpool and Sevilla PNG files passed decoding; Chelsea/Liverpool visibly render in the real desktop. Other teams use clean initials.
- Data Center and the footer use the same readiness label. Both show PARTIAL / “Kısmen hazır”, with BASE prediction/candidate capability ready. Optional asset and lineup gaps do not report a broken prediction system.

## Desktop acceptance

The actual Tauri desktop was launched against production AppData and inspected through its WebView runtime and screenshots, after rebuilding. Public bulletin refresh and current pipeline generation were executed. Screenshots and runtime/SQLite audits are in `.tmp-dataflow/acceptance/`, `.tmp-dataflow/final-runtime.json`, `.tmp-dataflow/runtime-summary.json`, `.tmp-dataflow/before-audit.json` and `.tmp-dataflow/after-audit.json`.

| Screen | Result |
|---|---|
| Today | Truthful 0 current matches, 0 candidates, 0 ready coupons; no false preparation banner |
| Candidates | Real next-day selections and model numbers; today's aggregate exclusions visible |
| Coupons | Real selection-bearing cards, exact odds; empty types explicitly unavailable; revision inspection preserves cards |
| Populars | Model numbers, disagreement, coverage and input/resolution failures separated; duplicate check passed |
| Matches | All 1,151 bulletin matches visible with coverage labels; actual supported-team crests rendered |
| Data Center | Supported-universe resolution denominator, current prediction count, optional limitations; footer agrees |

## External limitations

The [official football-data catalog](https://www.football-data.co.uk/data.php) supports adding E2, E3, EC, SC0–SC3, D2, I2, SP2, F2 and G1 to the original ten divisions. These were added structurally with three recent seasons. The recent-season catalog now has 65 datasets: 29 imported and 36 missing. Official CSV downloads redirect to a host that timed out even outside the sandbox. Available repository mirrors did not supply the required new current-season statistical CSVs. Older reduced-schema results were not presented as complete current coverage. Consequently these additional twelve divisions are not counted as active model coverage.

Two supported fixture failures remain for Paderborn and Elversberg because their canonical history is not imported. Sixteen resolved upcoming matches lack the minimum historical inputs. Another suitable data source or restored official CSV access is required to expand usable coverage safely. QPR was resolved through a documented alias verified against the [official club history](https://www.qpr.co.uk/club/history), without lowering confidence.

General Wikipedia metadata discovery is rate-limited. Three verified direct thumbnail sources work; comprehensive automatic logo discovery remains limited and is reported honestly. No reliable configured live lineup source exists in the repository.

## Validation and safety

- `cargo fmt --check`, `cargo check`, `cargo build`: passed.
- Relevant Rust suites passed: candidate acceptance 3; coupon 14 (including the new desktop-discovered comparison regression); populars 8; Data Center 5; assets 1; binary complement regression 1; Iddaa 13 (3 live tests ignored); resolver 7; migration 0009 check 1.
- `npm test`: 6 passed. `npm run build`: passed.
- Runtime assertions: no empty returned coupons, accumulator/system-column products agree, no duplicate popular selection identities, no new candidate runs from no-lineup coupon inspection.
- Timestamped SQLite backups precede production reprocessing, including `.tmp-dataflow/production-2026-09-11T19-51-28-309Z.sqlite3`. Historical predictions/performance and audit history were retained. Licensing, updater/signing/release behavior and ARZ License Admin were not changed. No expensive calibration or licensing/updater acceptance was repeated.

## Changed files

Backend: `src-tauri/src/repositories/{model_coverage,current_flow,prediction_engine,calibration,candidate_engine,coupon_engine,model_supported_populars,data_center,iddaa,resolution,mod}.rs`; corresponding candidate/coupon/popular/Data Center test files; `src-tauri/src/providers/football_data/catalog.rs`; `src-tauri/src/providers/iddaa/orchestrator.rs`; `src-tauri/src/{assets,commands,lib}.rs`; `src-tauri/{Cargo.toml,Cargo.lock,tauri.conf.json}`.

Frontend: `src/App.tsx`, `src/components/{MatchRow,StatusBadge}.tsx`, `src/lib/format.ts`, `src/pages/{TodayPage,CandidatesPage,CouponsPage,PopularsPage,DataCenterPage}.tsx`, `src/services/tauri.ts`, `src/types.ts`. The working tree already contained recovery edits at task start; these were retained and extended. No commit or release was published.

Full requested recovery remains blocked by unavailable expanded historical/current-season CSVs and rate-limited general logo discovery. The imported supported BASE universe is functioning end-to-end within the explicit history limitations above.
