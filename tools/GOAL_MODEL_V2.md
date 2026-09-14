# Goal Model V2 research pipeline

This is an isolated research runner. It cannot activate models or publish coupons.

## Reproduce

From the repository root (PowerShell):

```powershell
python -m venv .tmp-dataflow/goal-v2-venv
.\.tmp-dataflow\goal-v2-venv\Scripts\python.exe -m pip install -r tools/requirements-goal-v2.txt
cd src-tauri
cargo run --example goal_v2_export -- "$env:APPDATA\com.footballpredictor.app\football-predictor.sqlite3" ..\.tmp-dataflow\goal-v2-features.jsonl
cd ..
.\.tmp-dataflow\goal-v2-venv\Scripts\python.exe tools/goal_model_v2.py
.\.tmp-dataflow\goal-v2-venv\Scripts\python.exe -m unittest discover -s tools -p test_goal_model_v2.py
```

The exporter opens SQLite read-only in a consistent read transaction. A connection-local TEMP view moves each target's information cutoff to 24 hours before midnight Istanbul. It calls the existing feature engine without modifying stored fixtures or snapshots. The test fixture proves that target/same-day/future outcome changes cannot affect the snapshot.

The Python runner trains market-specific logistic and gradient-boosted models, separate chronological Platt calibrators, and ensembles with the production Poisson goal architecture. Four expanding chronological OOS folds keep fit, tuning, calibration, variant selection and test periods separate, with 48-hour label embargoes between stages. Seeds and hyperparameter grids are fixed. Goal features include venue/overall last-five/ten attack and concession, opponent-Elo adjustment, league rates, smoothed Poisson probabilities, BTTS, shots and trends. Untimestamped odds are excluded from features.

Output: `.tmp-dataflow/goal-v2/report.md`, `evaluation.json`, eight fold/market research artifacts, and ten pairs of per-match predictions and every-Saturday settlement files. Each Saturday records exactly five distinct picks or a concrete unavailable status. All ROI uses recorded Bet365 non-closing odds as a clearly labeled price proxy; original per-quote timing is absent. Over 3.5 returns remain N/A.

Artifacts are for offline reproduction, not deployment. A future adoption requires coherent goal-line probabilities, improved probability metrics and positive improved coupon results on fresh untouched temporal data, plus adequate timestamped prices. This run rejects V2. Do not tune repeatedly against these OOS blocks and describe them as untouched.
