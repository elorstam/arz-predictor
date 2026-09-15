-- Preserve immutable training/history snapshots while allowing a recovered
-- future fixture to obtain a current, independently auditable feature snapshot.
CREATE TABLE production_feature_revisions (
    id INTEGER PRIMARY KEY,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE RESTRICT,
    feature_engine_version TEXT NOT NULL,
    cutoff_at TEXT NOT NULL,
    feature_json TEXT NOT NULL CHECK(json_valid(feature_json)),
    data_quality_json TEXT NOT NULL CHECK(json_valid(data_quality_json)),
    created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(match_id,feature_engine_version,cutoff_at,feature_json)
);
CREATE INDEX production_feature_revisions_match ON production_feature_revisions(match_id,feature_engine_version,cutoff_at);
CREATE TRIGGER production_feature_revisions_no_update BEFORE UPDATE ON production_feature_revisions
BEGIN SELECT RAISE(ABORT,'production feature revisions are immutable'); END;
CREATE TRIGGER production_feature_revisions_no_delete BEFORE DELETE ON production_feature_revisions
BEGIN SELECT RAISE(ABORT,'production feature revisions are immutable'); END;
