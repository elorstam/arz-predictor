# Zero-to-ready first-run acceptance — 2026-09-14

PASS: a real Windows Tauri desktop, starting with empty isolated AppData, automatically reached READY 10/10. A graceful close and restart retained READY 10/10 without repeating bootstrap.

The interrupted implementation was already committed as `5440780fa174184f123d904957767031d32b126d`. Initial git status was clean and git diff was empty. The first-run implementation, Data Center labels, bundled resources, and existing acceptance reports were inspected before resuming. No application source changes were needed.

## Execution and evidence

Evidence is preserved in [.tmp-dataflow/zero-to-ready-20260914](.tmp-dataflow/zero-to-ready-20260914/). The desktop was built from the current commit because the existing executable predated that source. This was a debug desktop with the actual Rust backend, React frontend served by Vite, real provider requests, and existing `ARZ_DEV_APP_DATA_DIR` / `ARZ_DEV_LICENSE_BYPASS=1` hooks. The UI visibly identified DEV LICENSE. This acceptance does not claim a packaged release installer or customer-license activation test.

`fresh-before.json` records zero entries in the isolated `appdata` directory before launch. No production database, cached history, activation token, or model registry was copied into it. Executable SHA-256: `E88F0EECE41184E9CFB70873FB5283C9DD893A330CE28603B5C9396136FC6DAE`.

The only UI interaction was navigation to Data Center. Observation used the real WebView DOM, screenshots, status commands, and read-only SQLite snapshots. No refresh, preparation, import, training, activation, resolution, candidate, or coupon action was invoked by the acceptance scripts.

| Observation | Result |
| --- | --- |
| Bootstrap start | 2026-09-14 19:57:04.409 Istanbul |
| History progress | 11/20 at 19:59:07; bootstrap RUNNING, core 4/10, daily scheduler cycles 0 |
| Bootstrap completion | 20:00:56.712 Istanbul; 232.303 seconds elapsed |
| Completion state | COMPLETED, 13/13 stages, attempts 1, no bootstrap error |
| History | 20/20 datasets, 7,103 rows seen |
| Production resources | BASE `arz-base-1.0.2`, calibration `arz-base-1.0.2-cal1` installed and activated |
| Live bulletin | 334 events, 19,271 initial odds snapshots, zero failed events |
| Popularity | 20 selections imported, zero failed selections |
| Bootstrap production totals | 660 predictions, 63 candidate rows, 7 coupons across processed dates |
| UI confirmation | 20:01:19: HAZIR — 10/10, all three production capabilities ready |

Evidence: `history-progress.json/png`, `timeline.jsonl`, `first-ready.json`, and [ready-ui.png](.tmp-dataflow/zero-to-ready-20260914/ready-ui.png). The first-ready snapshot caught the backend completing just before the next UI poll; `ready-ui.json/png` confirms the UI subsequently updated automatically.

Provider primary requests timed out and the existing mirror fallback succeeded. The 20 successful bootstrap imports have 20 associated failed primary-attempt records; these are not bootstrap retries. The bootstrap attempt count remained 1. Six unresolved events remained diagnostic under the committed model-supported readiness scope; readiness does not claim every catalog event is model-ready or every persisted coupon is usable.

## Restart acceptance

Desktop PID 17136 closed gracefully and PID 26380 launched against the same AppData at 20:01:35.683 Istanbul. At 20:01:40 the backend already reported COMPLETED and READY 10/10. At 20:02:06 the Data Center visibly showed HAZIR — 10/10, with no bootstrap preparation panel. Screenshots were visually reviewed.

`before-restart.json`, `after-restart.json`, and `restart-comparison.json` prove:

- Bootstrap attempts remained 1 and completion time remained `2026-09-14T17:00:56.712840400+00:00`.
- Manifest SHA-256 remained `934deff57c696a7471ad19c28995029c13a253f72400b027578c5842c8269c64`.
- All 40 bootstrap history import records, including 20 successful imports, were unchanged.
- All 20 bootstrap CSVs and both installed production resource files retained identical sizes, modification times, and SHA-256 hashes.
- Model and calibration registry rows were unchanged.

The ordinary automatic scheduler resumed as expected and completed its startup cycle at 20:01:52. It refreshed live inputs and extended history with `E1:2324`, outside the bootstrap's 2425/2526 scope. Daily production counts therefore changed; this is distinct from rerunning bootstrap. The comparison specifically checks the complete bootstrap scope, while preserving additional daily import records for inspection.

Evidence: `restart-launch.json`, `restart-ready.json`, [restart-ready.png](.tmp-dataflow/zero-to-ready-20260914/restart-ready.png), and `restart-comparison.json` (all seven comparisons pass).

## Final review and restoration

Reviewed the committed first-run state machine, startup wiring, daily scheduler gate, resource validation/registration, readiness scope, DEV LICENSE handling, and Data Center helper/labels. The completed manifest short-circuits startup before any bootstrap stage runs. No source rewrite or further implementation was required. The committed diff passed whitespace review; desktop stderr logs were empty.

Previously completed formatting/checks, debug/release checks, 216 Rust tests, 24 frontend tests, production frontend build, and release-profile bypass rejection were retained as prior-run results supplied in the resumption request. They were not rerun or presented as new validation. The current debug desktop executable build passed with existing warnings.

The acceptance desktop and its temporary Vite server were stopped after evidence capture. Isolated AppData and evidence remain available; the existing production AppData was not used for bootstrap testing. No source, dependency, migration, or production resource changes were made in this resumption. The only tracked addition is this report.

ARZ ZERO-TO-READY FIRST-RUN COMPLETE
