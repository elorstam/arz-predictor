ALTER TABLE predictions ADD COLUMN raw_probability REAL;
ALTER TABLE predictions ADD COLUMN public_probability REAL;
ALTER TABLE predictions ADD COLUMN calibration_status TEXT;
ALTER TABLE predictions ADD COLUMN calibration_version TEXT;
ALTER TABLE predictions ADD COLUMN calibration_bucket TEXT;
ALTER TABLE predictions ADD COLUMN bucket_observed_rate REAL;
ALTER TABLE predictions ADD COLUMN bucket_sample_size INTEGER;
ALTER TABLE predictions ADD COLUMN bucket_calibration_gap REAL;
ALTER TABLE predictions ADD COLUMN availability TEXT;
ALTER TABLE predictions ADD COLUMN sample_quality TEXT;

UPDATE predictions SET raw_probability=model_probability, public_probability=model_probability,
    calibration_status='UNCALIBRATED_V1', availability='AVAILABLE', sample_quality='MODEL_TRAINED'
WHERE raw_probability IS NULL;

CREATE TABLE candidate_engine_runs (
    id INTEGER PRIMARY KEY,
    business_date TEXT NOT NULL,
    timezone TEXT NOT NULL,
    generated_at TEXT NOT NULL,
    model_version TEXT NOT NULL,
    calibration_version TEXT,
    candidate_policy_version TEXT NOT NULL,
    feature_engine_version TEXT NOT NULL,
    odds_cutoff_at TEXT NOT NULL,
    configuration_hash TEXT NOT NULL,
    input_fingerprint TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK(status IN ('COMPLETED','FAILED')),
    match_count_considered INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    CHECK(length(business_date)=10)
);
CREATE INDEX idx_candidate_engine_runs_date ON candidate_engine_runs(business_date,generated_at DESC);

CREATE TABLE candidate_engine_candidates (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES candidate_engine_runs(id) ON DELETE RESTRICT,
    prediction_id INTEGER NOT NULL REFERENCES predictions(id) ON DELETE RESTRICT,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE RESTRICT,
    competition_id INTEGER NOT NULL REFERENCES competitions(id) ON DELETE RESTRICT,
    category TEXT NOT NULL,
    market TEXT NOT NULL,
    selection TEXT NOT NULL,
    line_value REAL,
    raw_probability REAL NOT NULL CHECK(raw_probability BETWEEN 0 AND 1),
    public_probability REAL NOT NULL CHECK(public_probability BETWEEN 0 AND 1),
    calibration_status TEXT NOT NULL,
    calibration_version TEXT,
    calibration_bucket TEXT,
    bucket_observed_rate REAL,
    bucket_sample_size INTEGER,
    bucket_calibration_gap REAL,
    iddaa_odd REAL NOT NULL CHECK(iddaa_odd>0),
    odds_snapshot_id INTEGER,
    odds_captured_at TEXT NOT NULL,
    implied_probability REAL NOT NULL,
    probability_edge REAL NOT NULL,
    expected_value REAL NOT NULL,
    data_quality TEXT NOT NULL,
    score REAL NOT NULL,
    score_components_json TEXT NOT NULL CHECK(json_valid(score_components_json)),
    rank INTEGER NOT NULL CHECK(rank>0),
    same_match_group TEXT NOT NULL,
    correlation_type TEXT NOT NULL,
    compound_eligible INTEGER NOT NULL CHECK(compound_eligible IN (0,1)),
    qualification_state TEXT NOT NULL CHECK(qualification_state='QUALIFIED'),
    explanation_json TEXT CHECK(explanation_json IS NULL OR json_valid(explanation_json)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(run_id,category,prediction_id),
    UNIQUE(run_id,category,rank)
);
CREATE INDEX idx_candidate_engine_candidates_run_category ON candidate_engine_candidates(run_id,category,rank);
CREATE INDEX idx_candidate_engine_candidates_match ON candidate_engine_candidates(match_id);

CREATE TABLE candidate_engine_exclusions (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES candidate_engine_runs(id) ON DELETE RESTRICT,
    prediction_id INTEGER REFERENCES predictions(id) ON DELETE RESTRICT,
    match_id INTEGER REFERENCES matches(id) ON DELETE RESTRICT,
    category TEXT NOT NULL,
    reason TEXT NOT NULL,
    details_json TEXT CHECK(details_json IS NULL OR json_valid(details_json)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_candidate_engine_exclusions_run ON candidate_engine_exclusions(run_id,category,reason);

CREATE TRIGGER candidate_engine_candidates_no_update BEFORE UPDATE ON candidate_engine_candidates BEGIN SELECT RAISE(ABORT,'daily candidate snapshots are immutable'); END;
CREATE TRIGGER candidate_engine_candidates_no_delete BEFORE DELETE ON candidate_engine_candidates BEGIN SELECT RAISE(ABORT,'daily candidate snapshots are immutable'); END;
CREATE TRIGGER candidate_engine_runs_no_update BEFORE UPDATE ON candidate_engine_runs BEGIN SELECT RAISE(ABORT,'daily candidate runs are immutable'); END;
CREATE TRIGGER candidate_engine_runs_no_delete BEFORE DELETE ON candidate_engine_runs BEGIN SELECT RAISE(ABORT,'daily candidate runs are immutable'); END;
