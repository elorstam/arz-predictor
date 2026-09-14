ALTER TABLE entity_assets ADD COLUMN discovery_version INTEGER NOT NULL DEFAULT 0;
ALTER TABLE entity_assets ADD COLUMN discovery_state TEXT NOT NULL DEFAULT 'NOT_SEARCHED';
ALTER TABLE entity_assets ADD COLUMN failure_kind TEXT;
ALTER TABLE entity_assets ADD COLUMN last_http_status INTEGER;
ALTER TABLE entity_assets ADD COLUMN discovery_completed_at TEXT;
ALTER TABLE entity_assets ADD COLUMN visible_until TEXT;
CREATE TABLE logo_sources (
 id INTEGER PRIMARY KEY,
 entity_type TEXT NOT NULL, entity_id INTEGER NOT NULL,
 provider TEXT NOT NULL, url TEXT NOT NULL, kind TEXT NOT NULL,
 lookup_key TEXT NOT NULL, identity_context TEXT NOT NULL,
 priority INTEGER NOT NULL, state TEXT NOT NULL DEFAULT 'PENDING',
 attempts INTEGER NOT NULL DEFAULT 0, http_status INTEGER, error_kind TEXT,
 last_attempt_at TEXT, next_retry_at TEXT, resolved_url TEXT,
 UNIQUE(entity_type,entity_id,url)
);
CREATE INDEX logo_sources_work ON logo_sources(entity_type,entity_id,state,priority);
CREATE TABLE logo_attempts (
 id INTEGER PRIMARY KEY, source_id INTEGER, entity_type TEXT NOT NULL,entity_id INTEGER NOT NULL,
 provider TEXT, url TEXT, stage TEXT NOT NULL, http_status INTEGER,
 outcome TEXT NOT NULL, detail TEXT, attempted_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TABLE logo_provider_backoff(provider TEXT PRIMARY KEY,next_retry_at TEXT NOT NULL);
INSERT INTO logo_attempts(entity_type,entity_id,provider,url,stage,outcome,detail,attempted_at)
 SELECT entity_type,entity_id,source_provider,source_url,'LEGACY',
 CASE WHEN retry_state='INVALID' THEN 'INVALID_IMAGE' WHEN last_error LIKE 'HTTP_404:%' THEN 'SOURCE_404'
 WHEN retry_state='RATE_LIMITED' THEN 'RATE_LIMITED' WHEN status='FAILED' THEN 'DOWNLOAD_FAILED'
 ELSE 'SOURCE_NOT_DISCOVERED' END,
 COALESCE(last_error,'LEGACY_FAILURE_DETAIL_NOT_RECORDED'),COALESCE(last_attempt_at,updated_at)
 FROM entity_assets WHERE status<>'READY';
UPDATE entity_assets SET status='QUEUED',discovery_state='NOT_SEARCHED',next_retry_at=NULL,
 failure_kind=CASE WHEN retry_state='INVALID' THEN 'INVALID_IMAGE' WHEN last_error LIKE 'HTTP_404:%' THEN 'SOURCE_404'
 WHEN status='FAILED' THEN 'DOWNLOAD_FAILED' ELSE 'SOURCE_NOT_DISCOVERED' END,
 retry_state=NULL WHERE status<>'READY';
