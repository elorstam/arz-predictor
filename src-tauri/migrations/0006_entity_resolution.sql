CREATE TABLE team_aliases (
    id INTEGER PRIMARY KEY,
    team_id INTEGER NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    normalized_alias TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('MANUAL', 'PROVIDER', 'AUTO_CONFIRMED')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (team_id, normalized_alias)
);

CREATE INDEX idx_team_aliases_normalized ON team_aliases(normalized_alias);

CREATE TABLE team_resolution_candidates (
    id INTEGER PRIMARY KEY,
    source_provider TEXT NOT NULL,
    source_team_mapping_id INTEGER NOT NULL REFERENCES provider_team_mappings(id) ON DELETE CASCADE,
    candidate_team_id INTEGER NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    confidence_score REAL NOT NULL CHECK (confidence_score >= 0 AND confidence_score <= 1),
    confidence_level TEXT NOT NULL CHECK (confidence_level IN ('EXACT', 'VERY_HIGH', 'REVIEW', 'UNRESOLVED')),
    reason_json TEXT NOT NULL CHECK (json_valid(reason_json)),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'rejected')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    reviewed_at TEXT,
    UNIQUE (source_team_mapping_id, candidate_team_id)
);

CREATE INDEX idx_resolution_candidates_status ON team_resolution_candidates(status, confidence_level);
CREATE INDEX idx_resolution_candidates_source ON team_resolution_candidates(source_team_mapping_id);

CREATE TABLE resolution_audit_log (
    id INTEGER PRIMARY KEY,
    entity_type TEXT NOT NULL,
    source_entity TEXT NOT NULL,
    target_entity TEXT,
    action TEXT NOT NULL,
    method TEXT NOT NULL,
    confidence REAL CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 1)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    details_json TEXT CHECK (details_json IS NULL OR json_valid(details_json))
);

CREATE INDEX idx_resolution_audit_entity ON resolution_audit_log(entity_type, created_at);

DROP TRIGGER odds_snapshots_no_update;
CREATE TRIGGER odds_snapshots_no_update
BEFORE UPDATE ON odds_snapshots
WHEN OLD.provider IS NOT NEW.provider OR OLD.market_code IS NOT NEW.market_code
 OR OLD.market_name IS NOT NEW.market_name OR OLD.line_value IS NOT NEW.line_value
 OR OLD.selection IS NOT NEW.selection OR OLD.odd IS NOT NEW.odd
 OR OLD.alternative_odd IS NOT NEW.alternative_odd OR OLD.raw_odd IS NOT NEW.raw_odd
 OR OLD.captured_at IS NOT NEW.captured_at OR OLD.provider_market_id IS NOT NEW.provider_market_id
 OR OLD.normalized_market_type IS NOT NEW.normalized_market_type
 OR OLD.provider_selection_code IS NOT NEW.provider_selection_code
 OR OLD.provider_line IS NOT NEW.provider_line OR OLD.normalized_selection IS NOT NEW.normalized_selection
BEGIN SELECT RAISE(ABORT, 'odds snapshot fields are immutable'); END;

DROP TRIGGER popularity_snapshots_no_update;
CREATE TRIGGER popularity_snapshots_no_update
BEFORE UPDATE ON popularity_snapshots
WHEN OLD.provider IS NOT NEW.provider OR OLD.provider_event_id IS NOT NEW.provider_event_id
 OR OLD.provider_market_id IS NOT NEW.provider_market_id OR OLD.provider_market_code IS NOT NEW.provider_market_code
 OR OLD.provider_selection_code IS NOT NEW.provider_selection_code OR OLD.market_type IS NOT NEW.market_type
 OR OLD.provider_line IS NOT NEW.provider_line OR OLD.line_value IS NOT NEW.line_value
 OR OLD.selection IS NOT NEW.selection OR OLD.metric_type IS NOT NEW.metric_type
 OR OLD.metric_value IS NOT NEW.metric_value OR OLD.rank_value IS NOT NEW.rank_value
 OR OLD.raw_metric_name IS NOT NEW.raw_metric_name OR OLD.provider_raw_value IS NOT NEW.provider_raw_value
 OR OLD.captured_at IS NOT NEW.captured_at
BEGIN SELECT RAISE(ABORT, 'popularity snapshot fields are immutable'); END;

DROP TRIGGER predictions_forecast_fields_immutable;
CREATE TRIGGER predictions_forecast_fields_immutable
BEFORE UPDATE ON predictions
WHEN OLD.market IS NOT NEW.market OR OLD.selection IS NOT NEW.selection
 OR OLD.line_value IS NOT NEW.line_value OR OLD.model_probability IS NOT NEW.model_probability
 OR OLD.confidence_bucket IS NOT NEW.confidence_bucket OR OLD.iddaa_odd_at_prediction IS NOT NEW.iddaa_odd_at_prediction
 OR OLD.created_at IS NOT NEW.created_at OR OLD.kickoff_at IS NOT NEW.kickoff_at
 OR OLD.model_version_id IS NOT NEW.model_version_id
BEGIN SELECT RAISE(ABORT, 'prediction snapshot fields are immutable'); END;

DROP TRIGGER coupon_selections_creation_fields_immutable;
CREATE TRIGGER coupon_selections_creation_fields_immutable
BEFORE UPDATE ON coupon_selections
WHEN OLD.coupon_id IS NOT NEW.coupon_id OR OLD.prediction_id IS NOT NEW.prediction_id
 OR OLD.source_candidate_id IS NOT NEW.source_candidate_id OR OLD.selection_order IS NOT NEW.selection_order
 OR OLD.market_snapshot IS NOT NEW.market_snapshot OR OLD.selection_snapshot IS NOT NEW.selection_snapshot
 OR OLD.line_value_snapshot IS NOT NEW.line_value_snapshot
 OR OLD.model_probability_snapshot IS NOT NEW.model_probability_snapshot OR OLD.odd_snapshot IS NOT NEW.odd_snapshot
 OR OLD.created_at IS NOT NEW.created_at
BEGIN SELECT RAISE(ABORT, 'coupon selection snapshot fields are immutable'); END;
