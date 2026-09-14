# Remaining desktop acceptance — 2026-09-14

Follow-up: the previously blocked midnight scenario was subsequently completed with the explicitly authorized debug-only shared clock hook. See [MIDNIGHT_ROLLOVER_ACCEPTANCE.md](MIDNIGHT_ROLLOVER_ACCEPTANCE.md) for the real desktop evidence, race correction, and restored normal-clock verification. The findings below retain the original acceptance history.

No application implementation changes were made. Testing uses the existing executable and production AppData. Test scripts and evidence are under `.tmp-dataflow/acceptance-*`.

## Provider connectivity failure

A Windows firewall rule attempt was denied with Windows error 5; no rule was created. Instead, a temporary localhost CONNECT proxy was used only by the test application's inherited environment. It tunnels TLS without decrypting it and rejects Iddaa destinations while OFFLINE. Other providers remain reachable. This tests real Rust provider requests, not mocked frontend responses.

Baseline automatic startup: 02:10:37 Istanbul; last success 02:10:39. The proxy was then switched OFFLINE while the desktop remained running. At its unmodified five-minute deadline, 02:15:37, the scheduler automatically attempted bulletin and popularity downloads. Both failed with a connection/tunnel error. No manual refresh was requested.

The failed cycle completed in 551 ms, retained the prior success timestamp and skipped publication with `UPSTREAM_FAILED`. Candidate run #121 and all persisted coupon DTOs were unchanged. The real UI retained 38 daily selections and four READY coupons. The raw candidate command returned 44 strict candidate rows before and after; that count is distinct from the deduplicated daily UI pool.

Data Center visibly showed `Yenileme hatası`, last success 02:10:39 and next check 02:20:37. Core capability remained 10/10 because cached inputs were still within their valid freshness windows; the failed refresh was not represented as a successful refresh. Screenshots and full DOM/command snapshots are `acceptance-before.json`, `acceptance-offline.json`, `acceptance-offline.png`; transport evidence is `acceptance-proxy.jsonl`.

## Sleep/resume simulation

The provider connection was restored to ONLINE. At 02:16:15.416, the running application's native process was suspended using NtSuspendProcess for 310 seconds, with automatic NtResumeProcess in a finally block. This simulates process execution being paused across the refresh deadline without changing Windows time or database timestamps. It is not a physical machine suspend. Native timestamps are recorded in `acceptance-suspend.jsonl`.

Resume succeeded at 02:21:25.454. The scheduler started exactly one automatic cycle in that same second (epoch 1789341685), completed it in 2,872 ms, and returned IDLE with cycle count advancing from 2 to 3. It did not replay missed intervals. Subsequent focus and visibility events left the count at 3 and did not enqueue another refresh. The next check was scheduled for 02:26:25, 300 seconds after the recovery start.

This same automatic recovery cycle verified restored provider connectivity: bulletin refresh timestamp advanced from 02:10:39 to 02:21:27, latest odds advanced to 02:21:26, and popularity imported 20 selections with 24 new snapshots. Error cleared; no manual button was pressed. Production was skipped with UNCHANGED_INPUTS because the changes did not affect its relevant fingerprint. No duplicate daily publication was created. Historical coupon counts by date were identical before/after: September 10: 5, September 11: 10, September 12: 49, September 13: 49, September 14: 94.

Actual UI navigation through Today, Candidates, Coupons and Data Center remained functional after resume. The final Data Center screenshot shows 10/10 READY, last success 02:21:28 and next check 02:26:25. Evidence: `acceptance-resumed.json`, `acceptance-resumed.png`, `acceptance-resume-focus.json`, `acceptance-history-before.json`, `acceptance-history-after.json`.

## Midnight scenario limitation

The frontend has a shared business-date abstraction, but the running Rust scheduler reads `chrono::Utc::now()` directly for both scheduling and its business date. No existing desktop/backend clock override was found. Frontend-only Date mocking would not advance the real scheduler and would therefore not prove the required coordinated midnight transition. System clock changes were not used because they would affect production timestamps. No implementation or test override was added under this acceptance-only instruction.

Consequently, the complete real-desktop midnight scenario remains blocked: there is no existing safe shared frontend/backend clock override. Pure scheduler clock-input tests from the prior task are not claimed as desktop acceptance.

## Restoration

The test desktop and local proxy were stopped, then the same executable was relaunched without the proxy environment (PID 16276). The normal-network startup refresh began at 02:22:32 and succeeded at 02:22:35, with no error. The temporary proxy process is no longer running, its mode file is ONLINE, and no firewall rule exists. Windows time was never changed and no process remains suspended. Source and migration files were not modified by this acceptance task.

No unrelated model calculations, recovery jobs or regression suites were rerun. The application's ordinary automatic jobs continued as designed.

Final normal-network scheduled cycle started at 02:27:32 (epoch 1789342052), exactly 300 seconds after startup, and completed successfully in 5,777 ms at 02:27:38. It imported 130 events with zero failures, observed changed odds, and appropriately published run #122. This is a changed-input publication, not a replay of the outage or sleep intervals. Data Center remains 10/10 READY; Today, Candidates and Coupons show the normal Istanbul date 2026-09-14 and four READY coupon cards remain visible. Scheduler is enabled, interval 300, IDLE without error; next check 02:32:32. Evidence: `acceptance-final.json`, `acceptance-final.png` and the production `automatic-refresh.jsonl`.

## Result

- Provider outage and automatic recovery: PASS.
- Real desktop process sleep/resume simulation: PASS; one immediate stale cycle, no overlap/replay.
- Safe coordinated Istanbul midnight simulation: BLOCKED by absent existing backend/shared clock override; not run, no production clock changes.
- Restoration and subsequent normal 300-second cadence: PASS.
