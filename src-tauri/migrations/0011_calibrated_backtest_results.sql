CREATE TABLE backtest_calibration_predictions (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES backtest_runs(id) ON DELETE RESTRICT,
    calibration_model_id INTEGER NOT NULL REFERENCES calibration_models(id) ON DELETE RESTRICT,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE RESTRICT,
    market TEXT NOT NULL,
    line_value REAL,
    selection TEXT NOT NULL,
    raw_probability REAL NOT NULL CHECK (raw_probability>=0 AND raw_probability<=1),
    calibrated_probability REAL CHECK (calibrated_probability IS NULL OR (calibrated_probability>=0 AND calibrated_probability<=1)),
    public_probability REAL NOT NULL CHECK (public_probability>=0 AND public_probability<=1),
    calibration_status TEXT NOT NULL,
    calibration_bucket TEXT NOT NULL,
    bucket_sample_size INTEGER NOT NULL,
    actual_result TEXT NOT NULL,
    settlement TEXT NOT NULL,
    generated_at TEXT NOT NULL
);
CREATE INDEX idx_backtest_calibration_filter ON backtest_calibration_predictions(calibration_model_id,match_id,market,line_value,selection,generated_at);
CREATE UNIQUE INDEX idx_backtest_calibration_prediction_identity
    ON backtest_calibration_predictions(run_id, calibration_model_id, match_id, market, selection, COALESCE(line_value,-999999.0));
CREATE TRIGGER backtest_calibration_predictions_no_update
BEFORE UPDATE ON backtest_calibration_predictions BEGIN
    SELECT RAISE(ABORT, 'calibrated backtest predictions are immutable');
END;
CREATE TRIGGER backtest_calibration_predictions_no_delete
BEFORE DELETE ON backtest_calibration_predictions BEGIN
    SELECT RAISE(ABORT, 'calibrated backtest predictions are immutable');
END;
