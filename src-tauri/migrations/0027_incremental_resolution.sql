CREATE TABLE daily_resolution_queue (
 match_id INTEGER PRIMARY KEY REFERENCES matches(id),
 provider_event_id TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('NEW','AUTO_RESOLVING','RESOLVED','RETRY_LATER','AMBIGUOUS','UNSUPPORTED','MANUAL_REVIEW')),
 attempts INTEGER NOT NULL DEFAULT 0,
 last_attempt_at TEXT, next_retry_at TEXT, reason TEXT, evidence_json TEXT NOT NULL DEFAULT '[]',
 updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_daily_resolution_due ON daily_resolution_queue(status,next_retry_at,match_id);
CREATE INDEX IF NOT EXISTS idx_matches_current_resolution ON matches(status,kickoff_at,competition_id);
CREATE TABLE provider_scoped_team_aliases (
 provider TEXT NOT NULL, competition_id INTEGER NOT NULL REFERENCES competitions(id),
 normalized_alias TEXT NOT NULL, source_name TEXT NOT NULL,
 team_id INTEGER NOT NULL REFERENCES teams(id), method TEXT NOT NULL, confidence REAL NOT NULL,
 learned_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 PRIMARY KEY(provider,competition_id,normalized_alias)
);
CREATE TABLE provider_selection_mappings (
 provider TEXT NOT NULL, match_id INTEGER NOT NULL REFERENCES matches(id),
 market_id TEXT NOT NULL, selection_id TEXT NOT NULL, line_key TEXT NOT NULL,
 market TEXT NOT NULL, outcome TEXT NOT NULL,
 PRIMARY KEY(provider,match_id,market_id,selection_id,line_key)
);
CREATE TABLE daily_refresh_audit (
 id INTEGER PRIMARY KEY, import_run_id INTEGER, business_date TEXT NOT NULL,
 started_at TEXT NOT NULL, finished_at TEXT, status TEXT NOT NULL,
 ingestion_ms INTEGER, resolution_ms INTEGER, production_ms INTEGER, readiness_ms INTEGER,
 report_json TEXT NOT NULL DEFAULT '{}', error TEXT
);
-- One-time bounded-universe seed. Historical and unsupported rows never enter work.
INSERT INTO daily_resolution_queue(match_id,provider_event_id,status)
 SELECT m.id,p.external_match_id,'NEW' FROM matches m
 JOIN provider_match_mappings p ON p.match_id=m.id AND p.provider='iddaa'
 WHERE m.status='scheduled' AND m.kickoff_at>strftime('%Y-%m-%dT%H:%M:%SZ','now')
 AND EXISTS(SELECT 1 FROM provider_competition_mappings cp WHERE cp.competition_id=m.competition_id AND cp.provider='football-data.co.uk');
