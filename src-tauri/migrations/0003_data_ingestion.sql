CREATE TABLE provider_competition_mappings (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    external_competition_id TEXT NOT NULL,
    competition_id INTEGER NOT NULL REFERENCES competitions(id) ON DELETE RESTRICT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (provider, external_competition_id)
);

CREATE TABLE data_import_runs (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    dataset_key TEXT NOT NULL,
    competition_id INTEGER REFERENCES competitions(id) ON DELETE RESTRICT,
    season TEXT NOT NULL,
    source_url TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    completed_at TEXT,
    status TEXT NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'completed', 'failed')),
    rows_seen INTEGER NOT NULL DEFAULT 0 CHECK (rows_seen >= 0),
    rows_inserted INTEGER NOT NULL DEFAULT 0 CHECK (rows_inserted >= 0),
    rows_updated INTEGER NOT NULL DEFAULT 0 CHECK (rows_updated >= 0),
    rows_skipped INTEGER NOT NULL DEFAULT 0 CHECK (rows_skipped >= 0),
    rows_failed INTEGER NOT NULL DEFAULT 0 CHECK (rows_failed >= 0),
    error_message TEXT,
    CHECK (
        (status = 'running' AND completed_at IS NULL)
        OR (status <> 'running' AND completed_at IS NOT NULL)
    )
);

ALTER TABLE matches ADD COLUMN scheduled_local_date TEXT;
ALTER TABLE matches ADD COLUMN kickoff_time_known INTEGER NOT NULL DEFAULT 1
    CHECK (kickoff_time_known IN (0, 1));

CREATE INDEX idx_provider_competition_mappings_competition
    ON provider_competition_mappings(competition_id);
CREATE INDEX idx_data_import_runs_provider_dataset_started
    ON data_import_runs(provider, dataset_key, started_at);
CREATE INDEX idx_data_import_runs_status_started
    ON data_import_runs(status, started_at);
CREATE INDEX idx_matches_local_date
    ON matches(competition_id, season, scheduled_local_date);
