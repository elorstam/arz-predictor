ALTER TABLE model_versions ADD COLUMN artifact_path TEXT;
ALTER TABLE model_versions ADD COLUMN artifact_sha256 TEXT;
ALTER TABLE model_versions ADD COLUMN training_cutoff TEXT;
ALTER TABLE model_versions ADD COLUMN feature_engine_version TEXT;
ALTER TABLE model_versions ADD COLUMN feature_schema_version TEXT;
ALTER TABLE model_versions ADD COLUMN label_versions_json TEXT CHECK (label_versions_json IS NULL OR json_valid(label_versions_json));
ALTER TABLE model_versions ADD COLUMN metrics_json TEXT CHECK (metrics_json IS NULL OR json_valid(metrics_json));
ALTER TABLE model_versions ADD COLUMN registry_status TEXT NOT NULL DEFAULT 'REGISTERED'
    CHECK (registry_status IN ('REGISTERED', 'VALIDATED', 'ACTIVE', 'INVALID'));
ALTER TABLE model_versions ADD COLUMN is_active INTEGER NOT NULL DEFAULT 0 CHECK (is_active IN (0, 1));

CREATE UNIQUE INDEX idx_model_versions_one_active
ON model_versions(is_active) WHERE is_active = 1;

CREATE TABLE prediction_runs (
    id INTEGER PRIMARY KEY,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE RESTRICT,
    model_version_id INTEGER NOT NULL REFERENCES model_versions(id) ON DELETE RESTRICT,
    feature_engine_version TEXT NOT NULL,
    feature_schema_version TEXT NOT NULL,
    artifact_sha256 TEXT NOT NULL,
    generated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(match_id, model_version_id, artifact_sha256)
);

ALTER TABLE predictions ADD COLUMN prediction_run_id INTEGER REFERENCES prediction_runs(id) ON DELETE RESTRICT;
CREATE INDEX idx_predictions_run ON predictions(prediction_run_id);
CREATE UNIQUE INDEX idx_predictions_run_market_selection_line
ON predictions(prediction_run_id, market, selection, COALESCE(line_value, -999999.0))
WHERE prediction_run_id IS NOT NULL;

CREATE TRIGGER prediction_runs_no_update
BEFORE UPDATE ON prediction_runs BEGIN
    SELECT RAISE(ABORT, 'prediction runs are immutable');
END;
CREATE TRIGGER prediction_runs_no_delete
BEFORE DELETE ON prediction_runs BEGIN
    SELECT RAISE(ABORT, 'prediction runs are immutable');
END;
