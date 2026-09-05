CREATE TABLE competitions (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    country TEXT,
    current_season TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (name, country)
);

CREATE TABLE teams (
    id INTEGER PRIMARY KEY,
    normalized_name TEXT NOT NULL,
    country TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (normalized_name, country)
);

CREATE TABLE provider_team_mappings (
    id INTEGER PRIMARY KEY,
    team_id INTEGER NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    external_team_id TEXT NOT NULL,
    external_team_name TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (provider, external_team_id)
);

CREATE TABLE matches (
    id INTEGER PRIMARY KEY,
    competition_id INTEGER NOT NULL REFERENCES competitions(id) ON DELETE RESTRICT,
    season TEXT NOT NULL,
    home_team_id INTEGER NOT NULL REFERENCES teams(id) ON DELETE RESTRICT,
    away_team_id INTEGER NOT NULL REFERENCES teams(id) ON DELETE RESTRICT,
    kickoff_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'scheduled'
        CHECK (status IN ('scheduled', 'postponed', 'cancelled', 'in_progress', 'finished', 'abandoned')),
    final_home_goals INTEGER CHECK (final_home_goals IS NULL OR final_home_goals >= 0),
    final_away_goals INTEGER CHECK (final_away_goals IS NULL OR final_away_goals >= 0),
    halftime_home_goals INTEGER CHECK (halftime_home_goals IS NULL OR halftime_home_goals >= 0),
    halftime_away_goals INTEGER CHECK (halftime_away_goals IS NULL OR halftime_away_goals >= 0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (home_team_id <> away_team_id),
    UNIQUE (competition_id, season, home_team_id, away_team_id, kickoff_at)
);

CREATE TABLE match_statistics (
    match_id INTEGER PRIMARY KEY REFERENCES matches(id) ON DELETE CASCADE,
    home_shots INTEGER CHECK (home_shots IS NULL OR home_shots >= 0),
    away_shots INTEGER CHECK (away_shots IS NULL OR away_shots >= 0),
    home_shots_on_target INTEGER CHECK (home_shots_on_target IS NULL OR home_shots_on_target >= 0),
    away_shots_on_target INTEGER CHECK (away_shots_on_target IS NULL OR away_shots_on_target >= 0),
    home_corners INTEGER CHECK (home_corners IS NULL OR home_corners >= 0),
    away_corners INTEGER CHECK (away_corners IS NULL OR away_corners >= 0),
    home_fouls INTEGER CHECK (home_fouls IS NULL OR home_fouls >= 0),
    away_fouls INTEGER CHECK (away_fouls IS NULL OR away_fouls >= 0),
    home_yellow_cards INTEGER CHECK (home_yellow_cards IS NULL OR home_yellow_cards >= 0),
    away_yellow_cards INTEGER CHECK (away_yellow_cards IS NULL OR away_yellow_cards >= 0),
    home_red_cards INTEGER CHECK (home_red_cards IS NULL OR home_red_cards >= 0),
    away_red_cards INTEGER CHECK (away_red_cards IS NULL OR away_red_cards >= 0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE provider_match_mappings (
    id INTEGER PRIMARY KEY,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    external_match_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (provider, external_match_id)
);

CREATE TABLE odds_snapshots (
    id INTEGER PRIMARY KEY,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    market_code TEXT NOT NULL,
    market_name TEXT NOT NULL,
    line_value REAL,
    selection TEXT NOT NULL,
    odd REAL NOT NULL CHECK (odd > 0),
    alternative_odd REAL CHECK (alternative_odd IS NULL OR alternative_odd > 0),
    raw_odd TEXT,
    captured_at TEXT NOT NULL
);

CREATE TABLE model_versions (
    id INTEGER PRIMARY KEY,
    version_identifier TEXT NOT NULL UNIQUE,
    model_name TEXT NOT NULL,
    model_type TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    notes TEXT,
    config_json TEXT CHECK (config_json IS NULL OR json_valid(config_json))
);

CREATE TABLE predictions (
    id INTEGER PRIMARY KEY,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE RESTRICT,
    market TEXT NOT NULL,
    selection TEXT NOT NULL,
    line_value REAL,
    model_probability REAL NOT NULL CHECK (model_probability >= 0 AND model_probability <= 1),
    confidence_bucket TEXT NOT NULL,
    iddaa_odd_at_prediction REAL CHECK (iddaa_odd_at_prediction IS NULL OR iddaa_odd_at_prediction > 0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    kickoff_at TEXT NOT NULL,
    model_version_id INTEGER NOT NULL REFERENCES model_versions(id) ON DELETE RESTRICT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'won', 'lost', 'void')),
    settled_at TEXT,
    CHECK ((status = 'pending' AND settled_at IS NULL) OR (status <> 'pending' AND settled_at IS NOT NULL))
);

CREATE INDEX idx_provider_team_mappings_team ON provider_team_mappings(team_id);
CREATE INDEX idx_matches_competition_season_kickoff ON matches(competition_id, season, kickoff_at);
CREATE INDEX idx_matches_home_team_kickoff ON matches(home_team_id, kickoff_at);
CREATE INDEX idx_matches_away_team_kickoff ON matches(away_team_id, kickoff_at);
CREATE INDEX idx_matches_status_kickoff ON matches(status, kickoff_at);
CREATE INDEX idx_provider_match_mappings_match ON provider_match_mappings(match_id);
CREATE INDEX idx_odds_snapshots_match_market_captured ON odds_snapshots(match_id, market_code, captured_at);
CREATE INDEX idx_odds_snapshots_provider_captured ON odds_snapshots(provider, captured_at);
CREATE INDEX idx_predictions_created_at ON predictions(created_at);
CREATE INDEX idx_predictions_kickoff_at ON predictions(kickoff_at);
CREATE INDEX idx_predictions_status ON predictions(status);
CREATE INDEX idx_predictions_confidence ON predictions(confidence_bucket);
CREATE INDEX idx_predictions_market ON predictions(market, selection);
CREATE INDEX idx_predictions_model_version ON predictions(model_version_id);
CREATE INDEX idx_predictions_match ON predictions(match_id);
CREATE INDEX idx_predictions_odds ON predictions(iddaa_odd_at_prediction);

CREATE TRIGGER odds_snapshots_no_update
BEFORE UPDATE ON odds_snapshots
BEGIN
    SELECT RAISE(ABORT, 'odds snapshots are immutable');
END;

CREATE TRIGGER odds_snapshots_no_delete
BEFORE DELETE ON odds_snapshots
BEGIN
    SELECT RAISE(ABORT, 'odds snapshots are immutable');
END;

CREATE TRIGGER predictions_forecast_fields_immutable
BEFORE UPDATE ON predictions
WHEN OLD.match_id IS NOT NEW.match_id
  OR OLD.market IS NOT NEW.market
  OR OLD.selection IS NOT NEW.selection
  OR OLD.line_value IS NOT NEW.line_value
  OR OLD.model_probability IS NOT NEW.model_probability
  OR OLD.confidence_bucket IS NOT NEW.confidence_bucket
  OR OLD.iddaa_odd_at_prediction IS NOT NEW.iddaa_odd_at_prediction
  OR OLD.created_at IS NOT NEW.created_at
  OR OLD.kickoff_at IS NOT NEW.kickoff_at
  OR OLD.model_version_id IS NOT NEW.model_version_id
BEGIN
    SELECT RAISE(ABORT, 'prediction snapshot fields are immutable');
END;

CREATE TRIGGER predictions_no_delete
BEFORE DELETE ON predictions
BEGIN
    SELECT RAISE(ABORT, 'prediction snapshots are immutable');
END;
