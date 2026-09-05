ALTER TABLE odds_snapshots ADD COLUMN provider_market_id TEXT;
ALTER TABLE odds_snapshots ADD COLUMN normalized_market_type TEXT NOT NULL DEFAULT 'UNKNOWN';
ALTER TABLE odds_snapshots ADD COLUMN provider_selection_code TEXT;
ALTER TABLE odds_snapshots ADD COLUMN provider_line TEXT;
ALTER TABLE odds_snapshots ADD COLUMN normalized_selection TEXT;

CREATE TABLE provider_competition_metadata (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    external_competition_id TEXT NOT NULL,
    external_name TEXT,
    country TEXT,
    season TEXT,
    resolution_status TEXT NOT NULL DEFAULT 'unresolved'
        CHECK (resolution_status IN ('resolved', 'unresolved')),
    first_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (provider, external_competition_id)
);

CREATE INDEX idx_odds_snapshots_latest_provider_selection
    ON odds_snapshots (
        provider,
        match_id,
        market_code,
        provider_market_id,
        provider_line,
        line_value,
        provider_selection_code,
        selection,
        captured_at DESC,
        id DESC
    );

CREATE INDEX idx_odds_snapshots_normalized_market
    ON odds_snapshots (match_id, normalized_market_type, line_value, captured_at DESC);

CREATE INDEX idx_provider_competition_metadata_status
    ON provider_competition_metadata (provider, resolution_status, last_seen_at);
