CREATE TABLE backtest_runs (
    id INTEGER PRIMARY KEY,
    model_family_version TEXT NOT NULL,
    feature_engine_version TEXT NOT NULL,
    feature_schema_version TEXT NOT NULL,
    label_version TEXT NOT NULL,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    training_start TEXT,
    training_end TEXT,
    evaluation_start TEXT,
    evaluation_end TEXT,
    configuration_json TEXT NOT NULL CHECK (json_valid(configuration_json)),
    status TEXT NOT NULL CHECK (status IN ('RUNNING','COMPLETED','FAILED')),
    result_hash TEXT
);
CREATE TABLE backtest_predictions (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES backtest_runs(id) ON DELETE RESTRICT,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE RESTRICT,
    fold INTEGER NOT NULL,
    market TEXT NOT NULL,
    line_value REAL,
    selection TEXT NOT NULL,
    raw_probability REAL NOT NULL CHECK (raw_probability>=0 AND raw_probability<=1),
    calibrated_probability REAL CHECK (calibrated_probability IS NULL OR (calibrated_probability>=0 AND calibrated_probability<=1)),
    actual_result TEXT NOT NULL,
    settlement TEXT NOT NULL CHECK (settlement IN ('WON','LOST','VOID','UNSETTLED','EXCLUDED')),
    out_of_sample INTEGER NOT NULL DEFAULT 1 CHECK (out_of_sample=1),
    generated_at TEXT NOT NULL
);
CREATE INDEX idx_backtest_predictions_run_market ON backtest_predictions(run_id,market,line_value,selection);
CREATE TABLE calibration_models (
    id INTEGER PRIMARY KEY,
    parent_model_version TEXT NOT NULL,
    parent_artifact_sha256 TEXT NOT NULL,
    calibration_version TEXT NOT NULL UNIQUE,
    market_family TEXT NOT NULL,
    calibration_method TEXT NOT NULL,
    parameters_json TEXT NOT NULL CHECK (json_valid(parameters_json)),
    fitted_from TEXT,
    fitted_to TEXT,
    sample_count INTEGER NOT NULL,
    validation_metrics_json TEXT NOT NULL CHECK (json_valid(validation_metrics_json)),
    artifact_sha256 TEXT NOT NULL,
    artifact_path TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 0 CHECK (is_active IN (0,1)),
    created_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_calibration_one_active ON calibration_models(is_active) WHERE is_active=1;
CREATE UNIQUE INDEX idx_backtest_prediction_identity ON backtest_predictions(run_id,match_id,market,selection,COALESCE(line_value,-999999.0));
CREATE TRIGGER backtest_predictions_no_update
BEFORE UPDATE ON backtest_predictions BEGIN
    SELECT RAISE(ABORT, 'backtest predictions are immutable');
END;
CREATE TRIGGER backtest_predictions_no_delete
BEFORE DELETE ON backtest_predictions BEGIN
    SELECT RAISE(ABORT, 'backtest predictions are immutable');
END;
