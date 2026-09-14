ALTER TABLE entity_assets ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE entity_assets ADD COLUMN last_error TEXT;
ALTER TABLE entity_assets ADD COLUMN retry_state TEXT;
UPDATE entity_assets SET status='MISSING' WHERE status IN ('QUEUED','DOWNLOADING');
CREATE INDEX idx_assets_priority_retry ON entity_assets(priority DESC,status,next_retry_at);
CREATE TABLE compound_transition_audit (
 id INTEGER PRIMARY KEY,
 series_id INTEGER NOT NULL REFERENCES phase8_compound_series(id),
 coupon_id INTEGER NOT NULL REFERENCES phase8_coupons(id),
 from_step INTEGER NOT NULL, to_step INTEGER NOT NULL,
 result TEXT NOT NULL, reason TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 UNIQUE(coupon_id)
);
