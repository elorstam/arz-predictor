CREATE TABLE feature_sets (
    id INTEGER PRIMARY KEY,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    feature_engine_version TEXT NOT NULL,
    cutoff_at TEXT NOT NULL,
    calculated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    feature_json TEXT NOT NULL CHECK (json_valid(feature_json)),
    data_quality_json TEXT NOT NULL CHECK (json_valid(data_quality_json)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(match_id,feature_engine_version,cutoff_at)
);

CREATE TABLE training_labels (
    id INTEGER PRIMARY KEY,
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    label_version TEXT NOT NULL,
    label_json TEXT NOT NULL CHECK (json_valid(label_json)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(match_id,label_version)
);

CREATE INDEX idx_feature_sets_version_cutoff ON feature_sets(feature_engine_version,cutoff_at);
CREATE INDEX idx_feature_sets_match ON feature_sets(match_id);
CREATE INDEX idx_training_labels_match ON training_labels(match_id);

CREATE TRIGGER feature_sets_immutable_update BEFORE UPDATE ON feature_sets
BEGIN SELECT RAISE(ABORT,'feature snapshots are immutable'); END;
CREATE TRIGGER feature_sets_immutable_delete BEFORE DELETE ON feature_sets
BEGIN SELECT RAISE(ABORT,'feature snapshots are immutable'); END;
CREATE TRIGGER training_labels_immutable_update BEFORE UPDATE ON training_labels
BEGIN SELECT RAISE(ABORT,'training labels are immutable'); END;
CREATE TRIGGER training_labels_immutable_delete BEFORE DELETE ON training_labels
BEGIN SELECT RAISE(ABORT,'training labels are immutable'); END;
