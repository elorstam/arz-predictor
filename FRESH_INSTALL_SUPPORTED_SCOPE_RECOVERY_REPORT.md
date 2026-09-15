# Fresh-install supported-scope recovery — 2026-09-15

Implemented and verified in a real Windows release desktop, using normal production AppData paths and bundled production artifacts. The old extracted v1.0.4 executable reproduced the false READY state on the controlled affected database. The corrected executable repaired that state automatically and retained the repair across restart.

## 1. Root cause

Production scope was an unchecked side effect of historical ingestion, rather than a required startup invariant:

- The BASE artifact carries inference features/markets and its integrity hash; installing it does not register the deployment competition universe.
- `first_run.rs` contained the ten deployment league codes, but used them only to select history downloads. Completed imports plus cached files were accepted without checking historical competition ownership.
- Scope consumers infer support through `provider_competition_mappings(provider='football-data.co.uk')`. History rows and a valid model can therefore exist while their required scope links are absent.
- An existing Iddaa mapping returns early in `iddaa::resolve_competition`. A provider competition created before canonical history can remain attached to its old competition. Ordinary incremental resolution processes queued supported events; it does not reconstruct the missing universe or discover events excluded before enqueueing.
- A completed first-run manifest returned immediately on startup. It did not revalidate these dependencies.
- Readiness counted missing inputs inside the already filtered production set. An empty set yielded zero missing inputs. It had no independent requirement that the production universe exist and own history.

Consequently the path was: valid bundled model + history → absent/stale competition links → all Iddaa events outside scope → no resolver queue work → no features/predictions/candidates/coupons → false READY.

**Direct release reproduction:** the previous v1.0.4 executable reported READY 10/10 with 517 registered Iddaa events, zero supported events, zero football-data competition mappings, and zero features/predictions/candidates/coupons. Restart preserved that failure. See `baseline-start-ready.json`, `baseline-restart-ready.json`, and their database snapshots under the evidence directory below.

The original office database was not provided. This establishes the code defects and reproduces the reported failure state; it does not establish which operation originally removed or mislinked that particular office installation's metadata. In particular, the old UI's `production_events` metric counts eligible upcoming events, not historical scope ownership. Its zero value alone cannot diagnose which mapping is missing.

## 2. Why previous Zero-to-Ready acceptance missed it

The previous acceptance exercised an empty **debug** desktop with DEV LICENSE, where history imported successfully before live data. It did not test loss of scope beneath an already completed manifest, nor assert the ten competition links and their historical ownership independently of readiness. Its report explicitly excluded a packaged production installer acceptance.

The preserved v1.0.4 release fresh-start script waited only 15 seconds. Its stored manifest stopped during history preparation, with one dataset imported. That proved launch/database initialization, not usable scope after completion and restart.

## 3. Production-scope fix

`repositories/production_scope.rs` now centralizes the existing deployment universe as `arz-base-1.0.2-scope-v1`:

`E0, E1, SP1, D1, I1, F1, N1, P1, B1, T1`.

Names, countries and provider codes come from the shipped, versioned football-data catalog. Existing versioned conservative competition aliases provide Iddaa linkage. No developer database IDs or new permissive matching rules are used.

The repair:

1. Reuses existing canonical competitions and reconstructs missing football-data mappings.
2. Repairs recognized stale Iddaa competition links and their event ownership.
3. Enqueues current/future supported events, including previously unsupported queue entries.
4. Runs atomically with an immediate SQLite transaction, avoiding a read-to-write lock upgrade race with background workers.

First-run startup checks the live scope before honoring a completed manifest. Incomplete scope invalidates the dependent stages while retaining completed model/calibration registration. Dataset completion now checks real historical match ownership; available validated CSVs are reused before attempting downloads. Normal automatic refresh also repairs scope before incremental resolution, and mapping repairs invalidate the unchanged-input production shortcut.

The existing pipeline then performs incremental resolution → features → BASE/calibrated predictions → daily candidates → rule-constrained coupons. No global historical resolver merge is introduced.

## 4. Readiness fix

Readiness independently requires all ten deployment mappings, owned historical matches for each, and no recognized stale Iddaa competition links. Missing scope produces `PRODUCTION_SCOPE_BOOTSTRAP_INCOMPLETE` and overall `ACTION_REQUIRED`; history, resolution and features cannot collectively false-pass.

New metrics expose supported/required competition counts and actual production-history match count. The existing ten-check score remains intact.

A healthy registry/history with genuinely no supported fixtures still passes the resolution/features checks. Dedicated tests distinguish this case from an empty or history-less registry. Safe team-resolution rules and existing treatment of unresolved inputs remain unchanged.

## 5. Office-equivalent before/after

The controlled affected fixture retained the real release bootstrap's model, calibration, 20 history datasets and live provider data. Required competition mappings and derived production outputs were absent; the completed first-run manifest remained present.

It contained 320 real registered provider events plus 197 explicitly synthetic, cancelled archival events, giving exactly 517 registered events. The synthetic events were outside the supported universe and could not produce predictions. This is an equivalent failure-state test, **not a copy of the office's 342-live-match feed**.

| Measurement | Previous release | Corrected release | Corrected restart |
| --- | ---: | ---: | ---: |
| Registered Iddaa events | 517 | 517 | 517 |
| Required competition mappings | 0 | 10 | 10 |
| Production-history matches | 0 linked | 7,103 | 7,103 |
| Registered supported events | 0 | 11 | 11 |
| Resolved/model-ready events | 0 | 7 | 7 |
| Today supported / resolved | 0 / 0 | 5 / 4 | 5 / 4 |
| Out-of-scope events | 517 | 506 | 506 |
| Feature rows | 0 | 11 | 11 |
| Prediction rows | 0 | 385 | 385 |
| Persisted candidate rows | 0 | 76 | 76 |
| Persisted coupon rows | 0 | 6 | 6 |
| Readiness | **False 10/10** | 10/10 with usable scope | 10/10 |

Candidate/coupon table totals include multiple publication runs/dates; they are not counts of distinct usable offers. The recovery flow reported 20 candidates for today, 12 for September 16, six for September 17, and **one usable coupon today**. Four supported events remain safely unresolved and do not receive predictions. No thresholds or scope rules were loosened to admit them.

## 6. Empty-AppData acceptance

- Actual release desktop, `http://tauri.localhost/`, normal production AppData location; no dev AppData override, test clock or license bypass.
- Initial observation: ten competition registrations but zero history, readiness **1/10**, bootstrap running.
- Completed automatically from provider history and bundled artifacts: 20 datasets, 7,103 history matches, 11 supported events, seven model-ready events and READY 10/10.
- Early test windows closed before completion with clean exit code 0; the source of those closes was not established. The same fresh state resumed automatically with its completed imports retained. Running the acceptance window hidden allowed observation through completion.
- Final-binary restart verification retained the original completed manifest and attempt count. Both restart snapshots contained 100 competitions, 837 teams, ten scope mappings, seven features and 385 predictions. Ordinary automatic publication produced cumulative totals of 114 candidate rows and nine coupon rows; these totals were stable across the two final snapshots.

The license screen remained in force. Readiness was observed through read-only Tauri commands, and output through read-only SQLite snapshots. No activation bypass or data-preparation button was used. This proves real release desktop behavior; a new NSIS installer/customer activation was not tested or published in this task.

## 7. Existing-install self-heal and restart

The final release detected the affected completed manifest and repaired it without user action. Recovery processed eleven supported queue entries: seven resolved, four retained for review. The history stage recorded **20/20 reused, zero rows imported this attempt**.

`comparison.json` verifies:

- Recovery adds no canonical entities: 100 competitions and 837 teams before/after.
- Recovery/restart preserve model and calibration registry records and artifact contents.
- All bootstrap CSV and production artifact hashes remain unchanged.
- All twenty bootstrap dataset import histories remain unchanged.
- Completed manifests and attempt counts remain unchanged across subsequent restarts: affected state 3 → 3; fresh state 2 → 2.
- Scope and production counts remain stable across affected-state restart.

The normal bounded maintenance worker starts additional historical-source jobs outside the twenty bootstrap datasets. These are recorded separately and are not a repeated full bootstrap.

Original AppData was temporarily preserved and restored after every desktop test. No production AppData deletion, reinstall, model training, threshold adjustment, license change or updater change was performed.

## 8. Tests, build and artifacts

- Rust full suite: **224 passed, 0 failed, 11 intentionally ignored**.
- Frontend: **33 passed** across twelve files.
- `cargo fmt --check` and `git diff --check`: passed.
- `npm run tauri build -- --no-bundle`: passed, including TypeScript and Vite production build. Existing unused-variable/dead-code warnings remain.
- Regression coverage includes midnight coupon persistence, Katlama progression, settlement, automatic refresh timing, bootstrap gating, licensing and updater tests.
- Final release executable: `src-tauri/target/release/ARZ Predictor.exe`.
- SHA-256: `A29D4D04B98500C8916F888898D5EA4ADF055391039678D43BB22C1F79D3EFE7`.
- BASE/calibration artifacts remain byte-for-byte unchanged. No release was uploaded or installed over the user's installation.

Evidence and reproducible local acceptance helpers: [.tmp-dataflow/scope-recovery](.tmp-dataflow/scope-recovery/), especially `comparison.json`, `affected-fixture.json`, `baseline-*-ready.json`, `affected-*-ready.json`, `fresh-verify-*-ready.json`, manifests, database snapshots and timelines. Desktop screenshots show the preserved license gate; backend readiness evidence is in the corresponding read-only command snapshots.

ARZ FRESH-INSTALL SUPPORTED-SCOPE RECOVERY COMPLETE
