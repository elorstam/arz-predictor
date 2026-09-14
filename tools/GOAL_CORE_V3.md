# Goal Core V3 research

The shared model and its temporal validation runner are isolated from the application. They cannot activate models, change coupon policies, or publish selections. Current evidence rejects adoption; see `GOAL_CORE_V3_REPORT.md`.

## Reproduce (PowerShell, repository root)

Use the V2 Python environment and pinned `tools/requirements-goal-v2.txt`. The historical input is the existing causal V2 export. To recreate it, run `goal_v2_export` as documented in `tools/GOAL_MODEL_V2.md`.

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --example goal_v3_export -- "$env:APPDATA\com.footballpredictor.app\football-predictor.sqlite3" .tmp-dataflow/goal-v3-live-features.jsonl
.\.tmp-dataflow\goal-v2-venv\Scripts\python.exe tools/evaluate_goal_core_v3.py
.\.tmp-dataflow\goal-v2-venv\Scripts\python.exe -m unittest discover -s tools -p test_goal_core_v3.py
cargo test --manifest-path src-tauri/Cargo.toml --example goal_v3_export
```

Both exporters open SQLite read-only. The September exporter includes all 1,145 stored fixtures across September 12–14, including missing outcomes; the evaluator ignores their labels. Its TEMP view is connection-local. The report separately counts fixtures with known kickoffs and at least five prior matches per team, and actual retained production predictions available at each day's midnight decision.

## Design

`goal_core_v3.py` provides:

- Separate regularized, recency-weighted home/away Poisson intensities using rolling 5/10 overall and venue scoring/conceding/clean-sheet/failed-to-score statistics, elapsed-day decay, home advantage, league baseline and opponents' pre-match attack/defense strengths.
- A normalized score probability matrix, with Over 2.5, Over 3.5, BTTS and marginal/total expected goals derived directly from it.
- Joint event-cell calibration using a two-parameter exponential tilt; it cannot independently contradict another market.
- A coherent production projection preserving A's Over 2.5 and BTTS marginal probabilities, and matrix-level convex ensembling.
- A causal production architecture replay including its fixed-gradient Poisson/logistic estimators, validation-selected BTTS blend, and family Platt calibration with production acceptance/fallback rules. Archived live artifacts are not applied backward.

`evaluate_goal_core_v3.py` uses four expanding chronological outer folds. Ridge is selected from the fixed grid `[0.1, 1, 10]` on the tuning stage. Calibration and variant/ensemble selection use later, separate historical stages. All stage boundaries embargo 48 hours of outcomes. A's calibration additionally separates fit/validation halves. The chosen B/C/D variant is nominated before each test block. No thresholds are optimized: both markets retain probability >=0.64 and positive EV, plus verified price freshness <=24h when applicable.

Saturday coupons contain exactly five distinct qualifying fixtures ranked by probability/id; if fewer than five pass, no coupon is settled. Separate Over 2.5 and BTTS coupons prevent counting correlated markets on the same fixture as different legs. This is offline evaluation and changes no production minimum rules.

Verified historical price coverage is zero because results end September 3 and timestamped quotes start September 4. Non-closing Bet365 Over 2.5 CSV prices enter only labeled retrospective proxy results. They never enter training, priors or verified adoption evidence. BTTS has no matched historical quotes. A prior requires contemporaneous two-sided bookmaker prices, recorded before the decision and no older than 24h; training-all-missing prior columns are also masked live.

## Outputs

`.tmp-dataflow/goal-v3/` contains `evaluation.json`, four fold joblib artifacts, a final research artifact, ten per-market/model prediction files, twenty Saturday audit files (verified/proxy), and September prediction summaries with top probabilities, odds, EV and freshness. `GOAL_CORE_V3_REPORT.md` contains readable tables for A/B/C/D/N and both retained production and rejected challenger September forecasts.

The research corpus was previously examined in V2. Temporal OOS is preserved, but this is not a fresh research holdout or a data-publication-vintage replay. The gate cannot approve production on this evidence. Never relabel missing betting metrics as zero, treat three winning/losing coupon observations as strong evidence, or lower thresholds to manufacture five selections.
