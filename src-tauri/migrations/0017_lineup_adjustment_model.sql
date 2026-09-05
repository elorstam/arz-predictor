CREATE TABLE lineup_adjustment_models (
    id INTEGER PRIMARY KEY,
    version_identifier TEXT NOT NULL UNIQUE,
    artifact_path TEXT NOT NULL,
    artifact_sha256 TEXT NOT NULL,
    parent_model_version TEXT,
    parent_model_hash TEXT,
    feature_version TEXT NOT NULL,
    registry_hash TEXT NOT NULL,
    training_samples INTEGER NOT NULL,
    validation_samples INTEGER NOT NULL,
    test_samples INTEGER NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('VALIDATED','ACTIVE','INVALID','NOT_BENEFICIAL','INSUFFICIENT_DATA')),
    created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE UNIQUE INDEX lineup_adjustment_one_active ON lineup_adjustment_models(status) WHERE status='ACTIVE';
CREATE TRIGGER lineup_adjustment_models_no_update BEFORE UPDATE ON lineup_adjustment_models BEGIN SELECT RAISE(ABORT,'lineup adjustment artifacts are immutable'); END;
CREATE TRIGGER lineup_adjustment_models_no_delete BEFORE DELETE ON lineup_adjustment_models BEGIN SELECT RAISE(ABORT,'lineup adjustment artifacts are immutable'); END;
ALTER TABLE prediction_revisions ADD COLUMN base_probability REAL;
ALTER TABLE prediction_revisions ADD COLUMN revised_raw_probability REAL;
ALTER TABLE prediction_revisions ADD COLUMN revised_public_probability REAL;
ALTER TABLE prediction_revisions ADD COLUMN delta_percentage_points REAL;
ALTER TABLE prediction_revisions ADD COLUMN lineup_model_version TEXT;
ALTER TABLE prediction_revisions ADD COLUMN data_quality TEXT;
