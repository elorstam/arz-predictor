CREATE TABLE players (
    id INTEGER PRIMARY KEY,
    canonical_name TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    primary_position TEXT,
    date_of_birth TEXT,
    nationality TEXT,
    primary_team_id INTEGER REFERENCES teams(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_players_normalized_name ON players(normalized_name);

CREATE TABLE provider_player_mappings (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    provider_player_id TEXT NOT NULL,
    canonical_player_id INTEGER NOT NULL REFERENCES players(id) ON DELETE RESTRICT,
    provider_name TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    UNIQUE(provider, provider_player_id)
);

CREATE TABLE lineup_snapshots (
    id INTEGER PRIMARY KEY,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE RESTRICT,
    provider TEXT NOT NULL,
    provider_event_id TEXT NOT NULL,
    captured_at TEXT NOT NULL,
    lineup_status TEXT NOT NULL CHECK(lineup_status IN ('OFFICIAL','UNCONFIRMED','PARTIAL','UNAVAILABLE')),
    home_team_id INTEGER NOT NULL REFERENCES teams(id) ON DELETE RESTRICT,
    away_team_id INTEGER NOT NULL REFERENCES teams(id) ON DELETE RESTRICT,
    is_official INTEGER NOT NULL CHECK(is_official IN (0,1)),
    source_hash TEXT NOT NULL,
    completeness_status TEXT NOT NULL CHECK(completeness_status IN ('COMPLETE_OFFICIAL','PARTIAL','INVALID')),
    created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(provider, provider_event_id, source_hash)
);
CREATE INDEX idx_lineup_snapshots_match_latest ON lineup_snapshots(match_id, captured_at DESC, id DESC);

CREATE TABLE lineup_players (
    id INTEGER PRIMARY KEY,
    lineup_snapshot_id INTEGER NOT NULL REFERENCES lineup_snapshots(id) ON DELETE RESTRICT,
    team_id INTEGER NOT NULL REFERENCES teams(id) ON DELETE RESTRICT,
    player_id INTEGER REFERENCES players(id) ON DELETE RESTRICT,
    provider_player_id TEXT,
    provider_player_name TEXT NOT NULL,
    side TEXT NOT NULL CHECK(side IN ('HOME','AWAY')),
    role TEXT NOT NULL CHECK(role IN ('STARTER','SUBSTITUTE')),
    position TEXT,
    shirt_number INTEGER,
    formation_slot TEXT,
    captain INTEGER NOT NULL DEFAULT 0 CHECK(captain IN (0,1)),
    goalkeeper INTEGER NOT NULL DEFAULT 0 CHECK(goalkeeper IN (0,1))
);
CREATE INDEX idx_lineup_players_snapshot ON lineup_players(lineup_snapshot_id, side, role);

CREATE TABLE player_match_participation (
    id INTEGER PRIMARY KEY,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE RESTRICT,
    player_id INTEGER REFERENCES players(id) ON DELETE RESTRICT,
    team_id INTEGER NOT NULL REFERENCES teams(id) ON DELETE RESTRICT,
    started INTEGER NOT NULL CHECK(started IN (0,1)),
    substitute INTEGER NOT NULL CHECK(substitute IN (0,1)),
    minutes_played INTEGER CHECK(minutes_played IS NULL OR minutes_played BETWEEN 0 AND 130),
    position TEXT,
    lineup_snapshot_id INTEGER NOT NULL REFERENCES lineup_snapshots(id) ON DELETE RESTRICT,
    provider_player_id TEXT,
    created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(match_id, team_id, player_id, lineup_snapshot_id, started, substitute)
);
CREATE INDEX idx_participation_team_match ON player_match_participation(team_id, match_id);

CREATE TABLE lineup_feature_sets (
    id INTEGER PRIMARY KEY,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE RESTRICT,
    lineup_snapshot_id INTEGER NOT NULL REFERENCES lineup_snapshots(id) ON DELETE RESTRICT,
    feature_version TEXT NOT NULL,
    quality_status TEXT NOT NULL,
    home_resolution_count INTEGER NOT NULL,
    away_resolution_count INTEGER NOT NULL,
    home_starter_count INTEGER NOT NULL,
    away_starter_count INTEGER NOT NULL,
    historical_match_sample INTEGER NOT NULL,
    minutes_coverage REAL NOT NULL,
    position_coverage REAL NOT NULL,
    features_json TEXT NOT NULL CHECK(json_valid(features_json)),
    created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(match_id, lineup_snapshot_id, feature_version)
);
CREATE INDEX idx_lineup_features_match ON lineup_feature_sets(match_id, created_at DESC);

CREATE TABLE prediction_revisions (
    id INTEGER PRIMARY KEY,
    prediction_id INTEGER NOT NULL REFERENCES predictions(id) ON DELETE RESTRICT,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE RESTRICT,
    revision_type TEXT NOT NULL CHECK(revision_type IN ('BASE_PREMATCH','LINEUP_AWARE_PREMATCH')),
    parent_prediction_id INTEGER REFERENCES predictions(id) ON DELETE RESTRICT,
    lineup_snapshot_id INTEGER REFERENCES lineup_snapshots(id) ON DELETE RESTRICT,
    lineup_feature_set_id INTEGER REFERENCES lineup_feature_sets(id) ON DELETE RESTRICT,
    generated_at TEXT NOT NULL,
    revision_reason TEXT NOT NULL,
    model_version TEXT NOT NULL,
    calibration_version TEXT,
    revision_status TEXT NOT NULL CHECK(revision_status IN ('BASE','REVISION_PENDING_MODEL','REVISION_AVAILABLE','UNAVAILABLE')),
    revised_probability REAL CHECK(revised_probability IS NULL OR (revised_probability BETWEEN 0 AND 1)),
    UNIQUE(prediction_id, lineup_snapshot_id, revision_type)
);

CREATE TRIGGER lineup_snapshots_no_update BEFORE UPDATE ON lineup_snapshots BEGIN SELECT RAISE(ABORT,'lineup snapshots are immutable'); END;
CREATE TRIGGER lineup_snapshots_no_delete BEFORE DELETE ON lineup_snapshots BEGIN SELECT RAISE(ABORT,'lineup snapshots are immutable'); END;
CREATE TRIGGER lineup_players_no_update BEFORE UPDATE ON lineup_players BEGIN SELECT RAISE(ABORT,'lineup participants are immutable'); END;
CREATE TRIGGER lineup_players_no_delete BEFORE DELETE ON lineup_players BEGIN SELECT RAISE(ABORT,'lineup participants are immutable'); END;
CREATE TRIGGER lineup_features_no_update BEFORE UPDATE ON lineup_feature_sets BEGIN SELECT RAISE(ABORT,'lineup feature sets are immutable'); END;
CREATE TRIGGER lineup_features_no_delete BEFORE DELETE ON lineup_feature_sets BEGIN SELECT RAISE(ABORT,'lineup feature sets are immutable'); END;
