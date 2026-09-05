CREATE TABLE popularity_snapshots (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    match_id INTEGER REFERENCES matches(id) ON DELETE SET NULL,
    provider_event_id TEXT NOT NULL,
    provider_market_id TEXT,
    provider_market_code TEXT,
    provider_selection_code TEXT,
    market_type TEXT,
    provider_line TEXT,
    line_value REAL,
    selection TEXT,
    metric_type TEXT NOT NULL CHECK (metric_type IN ('COUNT', 'RANK', 'SHARE', 'PERCENTAGE')),
    metric_value REAL,
    rank_value INTEGER,
    raw_metric_name TEXT NOT NULL,
    provider_raw_value TEXT,
    captured_at TEXT NOT NULL,
    CHECK (
        (metric_type = 'RANK' AND rank_value IS NOT NULL AND rank_value > 0 AND metric_value IS NULL)
        OR
        (metric_type <> 'RANK' AND metric_value IS NOT NULL AND metric_value >= 0 AND rank_value IS NULL)
    )
);

CREATE INDEX idx_popularity_latest_selection_metric
    ON popularity_snapshots (
        provider,
        provider_event_id,
        provider_market_id,
        provider_selection_code,
        metric_type,
        captured_at DESC,
        id DESC
    );

CREATE INDEX idx_popularity_match_market
    ON popularity_snapshots (match_id, market_type, line_value, captured_at DESC);

CREATE INDEX idx_popularity_captured
    ON popularity_snapshots (provider, captured_at DESC);

CREATE TRIGGER popularity_snapshots_no_update
BEFORE UPDATE ON popularity_snapshots
BEGIN
    SELECT RAISE(ABORT, 'popularity snapshots are immutable');
END;

CREATE TRIGGER popularity_snapshots_no_delete
BEFORE DELETE ON popularity_snapshots
BEGIN
    SELECT RAISE(ABORT, 'popularity snapshots are immutable');
END;
