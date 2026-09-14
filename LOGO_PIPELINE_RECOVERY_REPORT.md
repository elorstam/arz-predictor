# ARZ logo pipeline recovery — production desktop

Snapshot: **2026-09-13T20:03:18.310Z**. Real Tauri desktop, production `C:\Users\husey\AppData\Roaming\com.footballpredictor.app\football-predictor.sqlite3`. The background worker remains running; later desktop counts can exceed this frozen audit.

## Before / after

| Counter | Before | After |
|---|---:|---:|
| READY | 635 | 806 |
| QUEUED | 0 | 2343 |
| DISCOVERING | 0 | 0 |
| DOWNLOADING | 0 | 0 |
| RETRY_LATER | 0 | 0 |
| RATE_LIMITED | 0 | 0 |
| SOURCE_NOT_FOUND | 0 | 40 |
| UNAVAILABLE | 2556 | 3 |
| INVALID | 1 | 0 |

Before columns reproduce the old Data Center counters: UNAVAILABLE combined 1,936 unavailable rows and 620 generic FAILED rows; INVALID was the remaining FAILED row. SOURCE_NOT_FOUND was not tracked separately. The new counters are disjoint: QUEUED includes undiscovered work, while SOURCE_NOT_FOUND requires exhaustion of the configured discovery plan. No queued team is presented as permanently unavailable.
There are 2313 unsearched non-ready jobs still queued for gradual discovery; 244 teams have a persisted new discovery plan or completed local identity check. This is pipeline recovery, not a claim that every known team now has a logo.

## Failure audit

| Classification of non-ready teams | Before | After (latest recorded cause) |
|---|---:|---:|
| SOURCE_NOT_DISCOVERED | 1935 | 1773 |
| SOURCE_404 | 1 | 9 |
| RATE_LIMITED | 0 | 0 |
| TIMEOUT | 0 | 0 |
| INVALID_IMAGE | 1 | 1 |
| DOWNLOAD_FAILED | 620 | 600 |
| PROVIDER_MAPPING_MISSING | 0 | 3 |
| UNSUPPORTED_SOURCE | 0 | 0 |
| PERMANENTLY_UNAVAILABLE | 0 | 0 |

The 620 legacy DOWNLOAD_FAILED records had no recorded error text. Their original HTTP/transport cause is unknowable; it was not fabricated as TIMEOUT or 404. Their audit records retain `LEGACY_FAILURE_DETAIL_NOT_RECORDED`. Eight old UNAVAILABLE rows also lacked a recorded reason. The 1,927 recorded `no exact safe logo match` results only exhausted the old narrow Wikipedia query, not all possible sources.
Of the original **621 FAILED** rows, **15 are now READY**. The remainder have concrete discovery jobs or explicit exhausted-source outcomes. Generic terminal FAILED rows: **0**. A legacy failure reason can remain attached to a QUEUED job until that job produces new evidence; it is not a terminal retry decision.

## Root causes and changes

- The scheduler only dispatched QUEUED/MISSING jobs. Old UNAVAILABLE and three-strike FAILED rows were stranded, leaving an empty queue.
- Wikipedia exact-title lookup rejected abbreviations and dotted `F.C.` titles; most teams lacked country metadata on the team row even when their competition identified the country.
- A 404 stopped the entire team instead of advancing to another source. The small football-data.org catalog matched unscoped names and could reassign a dead URL.
- Migration 26 preserves the legacy evidence, then creates durable `logo_sources`, append-only `logo_attempts`, and provider cooldown records. Successful provider/URL mappings remain attached to the team.
- Discovery first uses existing provider IDs in the correct namespace, then persisted canonical/provider names, normalized name plus a unique country derived from canonical competitions or explicit Iddaa country prefixes, then saved aliases and a small country-scoped abbreviation list. No cross-country fuzzy matching is used. Conflicting same-provider catalog matches and ambiguous SportsDB responses are rejected.
- One request runs at a time, separated by 2.1 seconds. Each batch is bounded to 24 jobs. Local catalog discovery is also bounded to 24, on the background database connection; there is no network scan in a UI command.
- Visible team requests receive a five-minute priority lease, model-supported teams have second priority, remaining teams third. The worker reselects the next team between requests. Expired visibility does not permanently outrank unvisited teams.
- Startup restores interrupted jobs and per-source leases. The timer resumes persisted jobs automatically. A 429 respects Retry-After and provider cooldown; 5xx/timeout/transport failures back off; 404, invalid image and unsupported endpoints advance to a different source. Terminal URLs are not retried by routine scans.
- Every request records provider, URL, stage, lookup key/context, HTTP status, attempt time, next retry and terminal/error state. PNG/MIME validation and streaming byte limits precede publication; cached images are validated before use.
- Match rows request logos on entering the viewport and poll missing visible logos. Data Center polls every 15 seconds; logo recovery remains optional and does not alter the core readiness denominator.

## Configured sources

| Source | Identity / URL | Coverage and provider IDs |
|---|---|---|
| football-data.org crests | Existing football-data.org numeric ID → `https://crests.football-data.org/{id}.png`; saved validated URLs; country-verified Brighton catalog record | Only catalog/provider-covered clubs; Iddaa or football-data.co.uk IDs are never substituted. Existing catalog has 100 records and 48 were READY before recovery. SVG-derived PNGs still require a real successful image response. |
| football-badges public PNG catalog | Exact normalized filename/team name **and country**; pinned GitHub raw URL at `ba8ec269d88498a3842567887677198af43e3dd8` | 398 PNG entries, 25 European country directories; no external numeric ID required. |
| TheSportsDB | `lookupteam.php?id=...` for an existing SportsDB ID, otherwise `searchteams.php?t=...`; soccer + exact canonical/alternate name + country validation, then returned `strBadge` HTTPS URL | Global football database, limited by provider coverage and free API response limits; successful source/lookup evidence is persisted. Country aliases include “The Netherlands”. No guessed badge ID. |
| Wikimedia page summaries | URL-encoded exact canonical/alias title; football description + country/demonym + exact normalized title, then allowed Wikimedia thumbnail host | Teams with a suitable public page thumbnail; no numeric team ID required. Dotted F.C. normalization is handled; an unsafe or ambiguous result is rejected. |

All sources share bounded concurrency, per-source terminal records and persisted provider cooldown. TheSportsDB free API can return a limited search result (for example Brighton Women for “Brighton”); the pipeline rejected that mismatched identity and recovered the men’s crest from the country-verified football-data.org mapping.

Source documentation: [football-data.org](https://www.football-data.org/documentation/quickstart), [football-badges public CDN usage and supported countries](https://github.com/leoratzlaff/football-badges), [TheSportsDB API](https://www.thesportsdb.com/documentation). The researched football-logos.cc catalog is **not enabled** because its [usage terms](https://football-logos.cc/license/) exclude commercial products. Club trademarks remain owned by their clubs; public accessibility is not represented as unrestricted copyright ownership.

## Observed source success rates

Legacy records contain the last state, not a complete request history. A historical HTTP success/404 rate cannot be reconstructed honestly. Before recovery: football-data.org had 48 READY and one recorded 404 (48/49 final assets); Wikimedia variants had 587 READY, 621 FAILED and 1,935 UNAVAILABLE (587/3,143 final assets). These are asset-state ratios, not request success rates.

The new ledger provides actual request denominators for this recovery run:

| Provider | Stage | Attempts | Success | Success rate | HTTP 404 | HTTP 429 | Timeout |
|---|---|---:|---:|---:|---:|---:|---:|
| football-badges | IMAGE | 49 | 49 | 100.0% | 0 | 0 | 0 |
| football-data.org | IMAGE | 1 | 1 | 100.0% | 0 | 0 | 0 |
| thesportsdb | IMAGE | 120 | 120 | 100.0% | 0 | 0 | 0 |
| thesportsdb | SPORTSDB | 207 | 120 | 58.0% | 0 | 0 | 0 |
| wikimedia-pageimages-v3 | IMAGE | 1 | 1 | 100.0% | 0 | 0 | 0 |
| wikimedia-pageimages-v3 | WIKI | 54 | 1 | 1.9% | 11 | 0 | 0 |

**171 previously unsuccessful assets were recovered.** In **15 cases**, the new run itself recorded an unsuccessful source request before successful recovery. These are observed fallback counts, not extrapolated coverage. Maximum observed traffic: **25 requests in one UTC minute**. Duplicate successful downloads of the same source URL: **1**.

## Desktop and restart acceptance

The actual rebuilt desktop was restarted during recovery. READY was 738 immediately before the final restart; subsequent automatic processing increased it further without pressing Start. Screenshots show the Data Center’s BACKGROUND state and populated queue, with General Status still HAZIR / 10 of 10.

Actual Candidates and Data Center screens were inspected. A separate temporary QA contact sheet inside the desktop rendered the production asset files for 20 current teams, 20 model-supported teams and 20 previously FAILED teams; it was removed after inspection. All available sample files decoded in the desktop. Missing samples retain their actual pending/exhausted status instead of substituted artwork.

| Sample | Inspected | Logos verified |
|---|---:|---:|
| Current teams | 20 | 20 |
| Supported-league teams | 20 | 17 |
| Previously LOGO_FAILED teams | 20 | 15 |

Current sample: Galatasaray, Kocaelispor, PSV, Sparta Rotterdam, Gil Vicente, Benfica, Brighton, Coventry, Excelsior, Utrecht, Santa Clara, Arouca, Lecce, Monza, Famalicao, Sporting CP, Atletico Madrid, Real Sociedad, Charleroi and Waregem.

The non-ready supported sample identities were Charlton, Vallecano and Santander. The non-ready legacy sample identities were Rad. Bijelji, Kanagawa Sagamihara (K), INAC Kobe Leonessa (K), Neftçi and Safa. They were inspected and left with truthful source evidence; no unrelated club badge was accepted just to increase counts.

The desktop requestAnimationFrame sample had a maximum gap of 7.2 ms over 120 frames and no gap over 100 ms while the worker was running. This is a measured sample, not a universal performance guarantee.

Validation: frontend production build and Tauri custom-protocol build passed; 6 asset tests, 2 discovery tests, 19 database/migration tests and 7 readiness tests passed. Tests cover provider-ID priority, country separation, real 404 fallback/image decoding, no repeat download after READY, provider cooldown persistence, cache corruption, queue bounds and terminal jobs not being reopened by scans. Existing unrelated compiler warnings remain.

## Evidence files

- `.tmp-dataflow/logo-recovery/before.json`: all 3,192 original team asset records and classification.
- `.tmp-dataflow/logo-recovery/final-evidence.json`: frozen transactional counters, source statistics, samples and SQLite quick_check.
- `.tmp-dataflow/logo-recovery/evidence-before-final-restart.json`: 738 READY before restart.
- `.tmp-dataflow/logo-recovery/desktop-final.json`: real desktop identity, backend readiness and frame timing.
- `.tmp-dataflow/logo-recovery/desktop-final-data-center.png`: inspected actual Data Center (767 READY at screenshot time).
- `.tmp-dataflow/logo-recovery/desktop-visible-teams.png`: inspected actual Candidates screen; off-viewport logos are loaded when scrolled into view.
- `.tmp-dataflow/logo-recovery/qa-visible.png`, `qa-supported.png`, `qa-failed.png`, `inspected-60.json`: explicit sample inspection, separate from product screenshots.
- Backup before production recovery: `.tmp-dataflow/production-2026-09-13T19-21-57-943Z.sqlite3`.

Final cache-loss regression also passed: deleting a local file makes its proven successful IMAGE source runnable again, while a terminal dead source remains terminal. New SportsDB image mappings retain the discovered provider team ID in their lookup evidence. The resulting final desktop build was relaunched and inspected; `.tmp-dataflow/logo-recovery/desktop-release.json` recorded 800 READY and 2,349 queued, with no frame gap over 100 ms (maximum 13.8 ms). The worker continued beyond that count without a manual start.

Licensing, updater, prediction models, candidate policy and coupon policy were not changed by this work. Remaining queued discovery continues in the real application.
