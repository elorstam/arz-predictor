# Goal-market selection research

Run from the project root with Python 3.10 or later (standard library only):

```powershell
python tools/goal_market_selection.py --output .tmp-dataflow/goal-selection
python -m unittest discover -s tools -p test_goal_market_selection.py -v
```

The command opens production AppData SQLite in read-only mode and reads the existing
football-data CSV cache. `--database` and `--cache` allow an explicit frozen snapshot.
It never imports odds into the app, writes candidates, changes policy settings or
publishes a coupon. It does not modify the general engine, Katlama or logos.

Outputs:

- `evaluation.json`: separate Over 2.5 / Over 3.5 policies, OOS comparison, development
  parameter frontier, source hashes, all scored live fixtures and exact exclusions.
- `report.md`: comparisons, per-date survivors, qualified ranking and diagnostic top 10.
- `report.html`: standalone readable copy of the same report.

The Over 2.5 search evaluates a bounded family of nine-component weights, probability
floors 0.45/0.50/0.55, EV floors −0.10/−0.05/0/0.02, score cutoffs 45/50/55/60 and
coupon sizes 5/6/7. Choices use only the earlier development dates. No option is
rewarded for meeting the September 12–14 coupon-count target.

Over 3.5 has separate experimental weights and a provisional probability floor
0.35; these are **not empirically tuned** when historical 3.5 prices are absent.
Its ROI and full-policy backtest must remain unavailable. A probability-only OOS
cohort cannot substitute for the old positive-EV betting policy.

The promotion decision is an evidence report, not a runtime feature flag. Even a
successful historical Bet365 proxy result cannot silently activate an Iddaa policy.
Production integration requires acceptable coupon and per-leg OOS evidence with
matching market odds, provenance, and explicit review of bookmaker transfer.

Historical research limitations: CSV odds have no per-fixture capture timestamp;
the current active raw-fallback choice was evaluated in previous recovery work;
the policy experiment is retrospective, not a never-observed prospective trial.
The fixed chronological split prevents optimizer access to later labels, but it
does not erase earlier analyst inspection of those data. Confidence intervals use
day-block resampling; they do not correct all model-selection uncertainty.
