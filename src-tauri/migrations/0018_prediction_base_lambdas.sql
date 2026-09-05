ALTER TABLE prediction_runs ADD COLUMN base_home_lambda REAL
    CHECK (base_home_lambda IS NULL OR base_home_lambda > 0);
ALTER TABLE prediction_runs ADD COLUMN base_away_lambda REAL
    CHECK (base_away_lambda IS NULL OR base_away_lambda > 0);

ALTER TABLE backtest_predictions ADD COLUMN base_home_lambda REAL
    CHECK (base_home_lambda IS NULL OR base_home_lambda > 0);
ALTER TABLE backtest_predictions ADD COLUMN base_away_lambda REAL
    CHECK (base_away_lambda IS NULL OR base_away_lambda > 0);
