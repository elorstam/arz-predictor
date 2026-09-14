CREATE TABLE popularity_selection_quotes (
 provider_event_id TEXT NOT NULL,
 provider_market_id TEXT NOT NULL,
 provider_selection_code TEXT NOT NULL,
 captured_at TEXT NOT NULL,
 odd REAL,
 market_name TEXT,
 PRIMARY KEY(provider_event_id,provider_market_id,provider_selection_code,captured_at)
);
