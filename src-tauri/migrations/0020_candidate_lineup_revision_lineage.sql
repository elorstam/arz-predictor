ALTER TABLE candidate_engine_runs ADD COLUMN prediction_context TEXT
    CHECK (prediction_context IS NULL OR prediction_context IN ('BASE','LINEUP_AWARE'));
ALTER TABLE candidate_engine_runs ADD COLUMN parent_candidate_run_id INTEGER
    REFERENCES candidate_engine_runs(id) ON DELETE RESTRICT;
ALTER TABLE candidate_engine_runs ADD COLUMN lineup_model_version TEXT;
ALTER TABLE candidate_engine_runs ADD COLUMN lineup_model_hash TEXT;
ALTER TABLE candidate_engine_runs ADD COLUMN revision_context_hash TEXT;
ALTER TABLE candidate_engine_runs ADD COLUMN base_model_hash TEXT;
ALTER TABLE candidate_engine_runs ADD COLUMN base_calibration_hash TEXT;

ALTER TABLE candidate_engine_candidates ADD COLUMN prediction_source TEXT
    CHECK (prediction_source IS NULL OR prediction_source IN ('BASE','LINEUP_AWARE','BASE_RETAINED'));
ALTER TABLE candidate_engine_candidates ADD COLUMN lineup_revision_id INTEGER;
ALTER TABLE candidate_engine_candidates ADD COLUMN lineup_snapshot_id INTEGER;
ALTER TABLE candidate_engine_candidates ADD COLUMN base_public_probability REAL
    CHECK (base_public_probability IS NULL OR (base_public_probability >= 0 AND base_public_probability <= 1));
ALTER TABLE candidate_engine_candidates ADD COLUMN final_public_probability REAL
    CHECK (final_public_probability IS NULL OR (final_public_probability >= 0 AND final_public_probability <= 1));
ALTER TABLE candidate_engine_candidates ADD COLUMN delta_percentage_points REAL;
ALTER TABLE candidate_engine_candidates ADD COLUMN lineup_family_status TEXT;
ALTER TABLE candidate_engine_candidates ADD COLUMN lineup_fallback_reason TEXT;
ALTER TABLE candidate_engine_candidates ADD COLUMN lineup_model_version TEXT;
ALTER TABLE candidate_engine_candidates ADD COLUMN lineup_model_hash TEXT;
