# Daily Coupons UI and Katlama recovery acceptance

Date: 2026-09-14, Istanbul business date. Scope: Günün Kuponları presentation and automatic Katlama settlement/publication. Existing model, threshold, category, staking and VOID rules are unchanged.

## Root cause and fix

There were separate ingestion and publication problems:

1. Actual coupon **122**, series **1**, had two selections still marked PENDING. The bulletin match rows (8685 and 9080) were marked cancelled after disappearing from the bulletin, with no scores. Neither had a finished equivalent in the database. Result refresh reached a stale fallback CSV after the official host timed out: I1 ended September 7 and B1 September 6. The real results existed externally but had not reached local settlement; this was not a known-score row being ignored.
2. The five-minute cycle generated/published before settlement. Its reuse fingerprint, and the candidate fingerprint, omitted series progression. A newly settled step could therefore be skipped when odds were unchanged.
3. Normal daily generation created a compound draft but did not attach it to the active series. The separate manual step-generation action was needed to make it READY.
4. Bounded settlement queues could delay the active series behind historical selections/coupons.

The cycle now fetches a due result dataset, settles selections/coupons and advances the series **before** deciding whether to reuse or regenerate publication. Both fingerprints include settled series state. Current-day live publication attaches a valid compound draft to an existing active series in the publication transaction, using the existing step-generation function. A pending step remains immutable; no qualifying combination is a normal unavailable outcome. Active-series settlement is prioritized within existing bounds. A shared result-refresh mutex prevents the minute worker and five-minute cycle claiming the same due download.

## Actual AppData recovery

A fresh backup was made at `.tmp-dataflow/production-2026-09-14T17-55-55-570Z.sqlite3` before recovery.

Official club reports confirm September 12 results:

- [Cagliari: Atalanta 1–2 Cagliari](https://cagliaricalcio.com/news/match-report-atalanta-cagliari-1-2/).
- [Westerlo: Westerlo 4–2 Standard](https://kvcwesterlo.be/wedstrijd/westerlo-standard-liege-2026-09-12/).

These two scores were first verified in an isolated production-data clone, then imported into actual AppData using the existing local CSV importer. This was a **one-time sourced recovery**, not a successful automatic download from the unavailable official CSV host. The adapter files are explicitly documented as club-report data, not downloaded football-data CSVs. No selection outcome, coupon outcome, stake or series state was edited directly. The normal importer and settlement engine produced the following:

| Evidence | Before | After |
|---|---|---|
| Selection 468, under 3.5 | PENDING_DATA | WON; result match 12810, 1–2 |
| Selection 469, under 3.5 | PENDING_DATA | LOST; result match 12811, 4–2 |
| Coupon 122 | Pending | SETTLED / LOST |
| Series 1 | Step 1, reset count 0 | Step 1, reset count 1, stake 100000 cents |
| Transition audit | None | One LOSS_RESET, coupon 122 |
| Next coupon | None attached for today | Coupon 329, READY, step 1, two selections, odds 1.5029 |

Import runs 428/429 persisted local file provenance. Their final result rows are linked through `coupon_selection_settlement_audit.result_match_id`; original bulletin rows are retained. Settlement completed at **17:56:14 UTC**, and normal startup automatic publication completed at **17:56:22 UTC**, candidate run 186. No manual settlement, generate-coupon or refresh button was invoked. Restart retained coupon 329, the same stake, two history rows and exactly one reset/transition.

The recurring result feed still depends on upstream availability/freshness. This change does not scrape arbitrary club reports automatically or claim that a stale mirror contains fresh results.

## Controlled real desktop acceptance

The actual Tauri executable and WebView2 were used with isolated AppData databases. Synthetic fixtures supply controlled predictions/odds and final match scores; the real result settlement, candidate engine, publication, scheduler and frontend execute normally. Fixture setup does not write settlement/progression outcomes. These fixtures explicitly bypass first-run preparation with a completed fixture manifest and are **not** new Zero-to-Ready evidence.

| Case | Observed result | Restart / protection |
|---|---|---|
| WIN, step 1 | Results inserted 17:40:22 UTC; minute worker settled WON at 17:41:06; uninterrupted normal five-minute cycle at 17:45:06 published step-2 coupon 7, two selections, 1.69 odds | Restart retained step 2, coupon 7 and one transition; production reported UNCHANGED_INPUTS |
| LOSS | Confirmed loss settled; normal startup pipeline reset to step 1 and published coupon 7 automatically | Restart retained reset count 1 and coupon 7; UNCHANGED_INPUTS |
| PENDING | No final scores: step 1 and old coupon remain pending | No transition or next step published |
| Known result, no next combination | WON settled and state advanced to step 2, but next coupon unavailable | Desktop says **“Bugün uygun Katlama kombinasyonu bulunamadı”**, not previous-result pending |

The WIN observation used the same PID 8936 from startup through scheduled publication; no manual action or restart occurred before the next coupon appeared. Earlier interrupted launches are not counted as this evidence. Some acceptance windows exited normally (exit code 0) during the session; completed acceptance observations and their timestamps are preserved.

## UI and final visible coupons

The page has a centered 1480px maximum container, three columns at wide desktop sizes, two at narrower sizes and one at small widths. Cards show type, selection count, READY badge and prominent combined odds. League metadata is muted; match/market and right-aligned probability/odds use compact rows and separators. Katlama has a dedicated compact state component and appears as a normal READY grid card. Maintenance operations remain under details, including BTTS refresh when its category is unavailable.

Real WebView2 captures verified 1600px / 1360px / 850px layouts with three / two / one columns and no horizontal overflow. Typical five-selection cards are about 526px high. The actual recovered desktop shows a 3 + 1 arrangement:

| Coupon | Selections | Combined odds at recovery |
|---|---:|---:|
| 2.5 Üst | 5 | 9.51 |
| KG Var | 5 | 12.30 |
| Yüksek Güven | 5 | 11.18 |
| Katlama, step 1 | 2 | 1.5029 |

READY availability changes with live odds and kickoff times. Corner, O3.5 and Surprise rules were not relaxed to fill the grid. Historical Coupon Performance and settlement history are preserved. The only unrelated file touch removes an already-unused `readinessLabel` import from DataCenterPage to satisfy TypeScript; no Data Center layout changed.

## Validation and diff review

- Rust library suite: **217 passed, 0 failed, 10 ignored**.
- Additional focused fingerprint regression added afterward: **1 passed**. It proves a step-1 loss invalidates otherwise unchanged publication input and repeated reads remain stable.
- Integrated settlement/publication/reopen test passed for WIN, LOSS, PENDING, no-combination and step-7 WIN reset.
- Frontend suite: **32 passed** including six Katlama label cases.
- Native debug executable build passed; real desktop acceptance used that executable.
- Production frontend TypeScript/Vite build passed, including the final missing-BTTS technical-action preservation change.
- Final `cargo fmt --check` and `git diff --check` passed.
- Final diff reviewed: no model resources, prediction thresholds, coupon-category policies, migrations, first-run implementation or five-minute interval changed. Existing release/first-run validations were not repeated.

## Local evidence index

Evidence is retained under `.tmp-dataflow/katlama-recovery/` (ignored local acceptance artifacts):

- `win-live-before-result.json`, `win-live-automatic.json/png`, `win-live-db-automatic.json`, `win-live-restart.json/png`, `win-live-db-restart.json`.
- `loss-db-accepted.json`, `loss-restart.json/png`, `loss-db-restart.json`.
- `pending-navigate.json/png`, `pending-db-accepted.json`.
- `no-combo-navigate.json/png`, `no-combo-db-accepted.json`.
- `production-two-columns.json/png`, `production-one-column.json/png`.
- `club-results-sources.json`, `club-I1.csv`, `club-B1.csv`, `club-import-9244.json` (clone), `club-import-9245.json` (actual AppData).
- `actual-db-recovered.json`, `actual-db-restart.json`, `actual-navigate.json/png`, `actual-restart.json/png`.
- `launch.ps1`, `desktop.mjs`, `scenario-db.mjs`, `import-club-results.mjs`, `monitored-launches.jsonl`, `monitored-exits.jsonl`.

The earlier `ZERO_TO_READY_FIRST_RUN_ACCEPTANCE.md` is preserved separately; this work does not replace its evidence.
