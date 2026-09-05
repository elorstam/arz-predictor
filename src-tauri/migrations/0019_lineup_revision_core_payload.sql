ALTER TABLE prediction_revisions ADD COLUMN base_model_version TEXT;
ALTER TABLE prediction_revisions ADD COLUMN base_model_hash TEXT;
ALTER TABLE prediction_revisions ADD COLUMN base_calibration_version TEXT;
ALTER TABLE prediction_revisions ADD COLUMN base_calibration_hash TEXT;
ALTER TABLE prediction_revisions ADD COLUMN lineup_model_hash TEXT;
ALTER TABLE prediction_revisions ADD COLUMN base_home_lambda REAL
    CHECK (base_home_lambda IS NULL OR base_home_lambda > 0);
ALTER TABLE prediction_revisions ADD COLUMN base_away_lambda REAL
    CHECK (base_away_lambda IS NULL OR base_away_lambda > 0);
ALTER TABLE prediction_revisions ADD COLUMN revised_home_lambda REAL
    CHECK (revised_home_lambda IS NULL OR revised_home_lambda > 0);
ALTER TABLE prediction_revisions ADD COLUMN revised_away_lambda REAL
    CHECK (revised_away_lambda IS NULL OR revised_away_lambda > 0);
ALTER TABLE prediction_revisions ADD COLUMN payload_schema TEXT;
ALTER TABLE prediction_revisions ADD COLUMN market_payload_json TEXT
    CHECK (market_payload_json IS NULL OR json_valid(market_payload_json));
ALTER TABLE prediction_revisions ADD COLUMN payload_hash TEXT;
