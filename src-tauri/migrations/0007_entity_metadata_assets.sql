CREATE TABLE team_metadata (
    team_id INTEGER PRIMARY KEY REFERENCES teams(id) ON DELETE CASCADE,
    display_name TEXT,
    short_name TEXT,
    city TEXT,
    venue_name TEXT,
    website TEXT,
    founded_year INTEGER CHECK (founded_year IS NULL OR founded_year BETWEEN 1800 AND 2200),
    source_provider TEXT,
    source_metadata_json TEXT CHECK (source_metadata_json IS NULL OR json_valid(source_metadata_json)),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE competition_metadata (
    competition_id INTEGER PRIMARY KEY REFERENCES competitions(id) ON DELETE CASCADE,
    display_name TEXT,
    category TEXT,
    source_provider TEXT,
    source_metadata_json TEXT CHECK (source_metadata_json IS NULL OR json_valid(source_metadata_json)),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE entity_assets (
    id INTEGER PRIMARY KEY,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('TEAM','COMPETITION')),
    entity_id INTEGER NOT NULL,
    asset_type TEXT NOT NULL DEFAULT 'LOGO' CHECK (asset_type='LOGO'),
    source_provider TEXT,
    source_url TEXT,
    local_relative_path TEXT,
    content_sha256 TEXT,
    mime_type TEXT,
    byte_size INTEGER CHECK (byte_size IS NULL OR byte_size >= 0),
    width INTEGER CHECK (width IS NULL OR width > 0),
    height INTEGER CHECK (height IS NULL OR height > 0),
    status TEXT NOT NULL DEFAULT 'MISSING' CHECK (status IN ('MISSING','QUEUED','DOWNLOADING','READY','FAILED','UNAVAILABLE')),
    failure_count INTEGER NOT NULL DEFAULT 0 CHECK (failure_count >= 0),
    last_attempt_at TEXT,
    last_success_at TEXT,
    next_retry_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(entity_type,entity_id,asset_type)
);

CREATE INDEX idx_entity_assets_status_retry ON entity_assets(status,next_retry_at);

CREATE TRIGGER entity_assets_team_fk_insert BEFORE INSERT ON entity_assets
WHEN NEW.entity_type='TEAM' AND NOT EXISTS(SELECT 1 FROM teams WHERE id=NEW.entity_id)
BEGIN SELECT RAISE(ABORT,'unknown team asset entity'); END;
CREATE TRIGGER entity_assets_competition_fk_insert BEFORE INSERT ON entity_assets
WHEN NEW.entity_type='COMPETITION' AND NOT EXISTS(SELECT 1 FROM competitions WHERE id=NEW.entity_id)
BEGIN SELECT RAISE(ABORT,'unknown competition asset entity'); END;
CREATE TRIGGER entity_assets_team_fk_update BEFORE UPDATE OF entity_type,entity_id ON entity_assets
WHEN NEW.entity_type='TEAM' AND NOT EXISTS(SELECT 1 FROM teams WHERE id=NEW.entity_id)
BEGIN SELECT RAISE(ABORT,'unknown team asset entity'); END;
CREATE TRIGGER entity_assets_competition_fk_update BEFORE UPDATE OF entity_type,entity_id ON entity_assets
WHEN NEW.entity_type='COMPETITION' AND NOT EXISTS(SELECT 1 FROM competitions WHERE id=NEW.entity_id)
BEGIN SELECT RAISE(ABORT,'unknown competition asset entity'); END;
