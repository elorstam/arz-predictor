CREATE TABLE coupon_candidate_runs (
    id INTEGER PRIMARY KEY,
    target_date TEXT NOT NULL,
    coupon_type TEXT NOT NULL CHECK (coupon_type IN (
        'HIGH_CONFIDENCE', 'OVER_25', 'BTTS', 'VALUE', 'SURPRISE',
        'SYSTEM_5_6', 'SYSTEM_4_5_6', 'SINGLE'
    )),
    generated_at TEXT NOT NULL,
    model_version_id INTEGER NOT NULL REFERENCES model_versions(id) ON DELETE RESTRICT,
    status TEXT NOT NULL CHECK (status IN ('pending', 'completed', 'failed')),
    target_candidate_count INTEGER NOT NULL CHECK (target_candidate_count > 0),
    qualified_candidate_count INTEGER NOT NULL DEFAULT 0 CHECK (qualified_candidate_count >= 0),
    generation_config_json TEXT CHECK (
        generation_config_json IS NULL OR json_valid(generation_config_json)
    ),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (length(target_date) = 10)
);

CREATE TABLE coupon_candidates (
    id INTEGER PRIMARY KEY,
    candidate_run_id INTEGER NOT NULL REFERENCES coupon_candidate_runs(id) ON DELETE RESTRICT,
    prediction_id INTEGER NOT NULL REFERENCES predictions(id) ON DELETE RESTRICT,
    rank INTEGER NOT NULL CHECK (rank > 0),
    ranking_score REAL NOT NULL,
    model_probability_snapshot REAL NOT NULL CHECK (
        model_probability_snapshot >= 0 AND model_probability_snapshot <= 1
    ),
    iddaa_odd_snapshot REAL CHECK (iddaa_odd_snapshot IS NULL OR iddaa_odd_snapshot > 0),
    market_snapshot TEXT NOT NULL,
    selection_snapshot TEXT NOT NULL,
    line_value_snapshot REAL,
    explanation_json TEXT CHECK (explanation_json IS NULL OR json_valid(explanation_json)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (candidate_run_id, prediction_id),
    UNIQUE (candidate_run_id, rank)
);

CREATE TABLE coupons (
    id INTEGER PRIMARY KEY,
    coupon_type TEXT NOT NULL CHECK (coupon_type IN (
        'HIGH_CONFIDENCE', 'OVER_25', 'BTTS', 'VALUE', 'SURPRISE',
        'SYSTEM_5_6', 'SYSTEM_4_5_6', 'SINGLE'
    )),
    target_date TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    model_version_id INTEGER REFERENCES model_versions(id) ON DELETE RESTRICT,
    source_candidate_run_id INTEGER REFERENCES coupon_candidate_runs(id) ON DELETE RESTRICT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'won', 'lost', 'partial', 'void')),
    total_decimal_odd REAL CHECK (total_decimal_odd IS NULL OR total_decimal_odd > 0),
    reference_stake REAL CHECK (reference_stake IS NULL OR reference_stake > 0),
    settled_return REAL CHECK (settled_return IS NULL OR settled_return >= 0),
    settled_profit REAL,
    settled_at TEXT,
    notes TEXT,
    CHECK (length(target_date) = 10),
    CHECK (
        (status = 'pending' AND settled_return IS NULL AND settled_profit IS NULL AND settled_at IS NULL)
        OR (status <> 'pending' AND settled_at IS NOT NULL)
    )
);

CREATE TABLE coupon_selections (
    id INTEGER PRIMARY KEY,
    coupon_id INTEGER NOT NULL REFERENCES coupons(id) ON DELETE RESTRICT,
    prediction_id INTEGER REFERENCES predictions(id) ON DELETE RESTRICT,
    source_candidate_id INTEGER REFERENCES coupon_candidates(id) ON DELETE RESTRICT,
    selection_order INTEGER NOT NULL CHECK (selection_order > 0),
    match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE RESTRICT,
    market_snapshot TEXT NOT NULL,
    selection_snapshot TEXT NOT NULL,
    line_value_snapshot REAL,
    model_probability_snapshot REAL NOT NULL CHECK (
        model_probability_snapshot >= 0 AND model_probability_snapshot <= 1
    ),
    odd_snapshot REAL CHECK (odd_snapshot IS NULL OR odd_snapshot > 0),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'won', 'lost', 'void')),
    settled_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (coupon_id, source_candidate_id),
    UNIQUE (coupon_id, selection_order),
    CHECK ((status = 'pending' AND settled_at IS NULL) OR (status <> 'pending' AND settled_at IS NOT NULL))
);

CREATE TABLE coupon_system_sizes (
    coupon_id INTEGER NOT NULL REFERENCES coupons(id) ON DELETE RESTRICT,
    system_size INTEGER NOT NULL CHECK (system_size BETWEEN 1 AND 6),
    PRIMARY KEY (coupon_id, system_size)
);

CREATE INDEX idx_candidate_runs_type_date ON coupon_candidate_runs(coupon_type, target_date);
CREATE INDEX idx_candidate_runs_model_date ON coupon_candidate_runs(model_version_id, target_date);
CREATE INDEX idx_candidate_runs_status_date ON coupon_candidate_runs(status, target_date);
CREATE INDEX idx_coupon_candidates_run_rank ON coupon_candidates(candidate_run_id, rank);
CREATE INDEX idx_coupon_candidates_prediction ON coupon_candidates(prediction_id);
CREATE INDEX idx_coupon_candidates_probability ON coupon_candidates(model_probability_snapshot);
CREATE INDEX idx_coupon_candidates_odds ON coupon_candidates(iddaa_odd_snapshot);
CREATE INDEX idx_coupon_candidates_market ON coupon_candidates(market_snapshot);
CREATE INDEX idx_coupons_type_date ON coupons(coupon_type, target_date);
CREATE INDEX idx_coupons_status_date ON coupons(status, target_date);
CREATE INDEX idx_coupons_model_date ON coupons(model_version_id, target_date);
CREATE INDEX idx_coupons_candidate_run ON coupons(source_candidate_run_id);
CREATE INDEX idx_coupon_selections_coupon_order ON coupon_selections(coupon_id, selection_order);
CREATE INDEX idx_coupon_selections_prediction ON coupon_selections(prediction_id);
CREATE INDEX idx_coupon_selections_candidate ON coupon_selections(source_candidate_id);
CREATE INDEX idx_coupon_selections_match ON coupon_selections(match_id);
CREATE INDEX idx_coupon_selections_status ON coupon_selections(status);
CREATE INDEX idx_coupon_selections_probability ON coupon_selections(model_probability_snapshot);
CREATE INDEX idx_coupon_selections_odds ON coupon_selections(odd_snapshot);

CREATE TRIGGER coupon_candidates_no_update
BEFORE UPDATE ON coupon_candidates
BEGIN
    SELECT RAISE(ABORT, 'coupon candidate snapshots are immutable');
END;

CREATE TRIGGER coupon_candidates_no_delete
BEFORE DELETE ON coupon_candidates
BEGIN
    SELECT RAISE(ABORT, 'coupon candidate snapshots are immutable');
END;

CREATE TRIGGER coupons_creation_fields_immutable
BEFORE UPDATE ON coupons
WHEN OLD.coupon_type IS NOT NEW.coupon_type
  OR OLD.target_date IS NOT NEW.target_date
  OR OLD.created_at IS NOT NEW.created_at
  OR OLD.model_version_id IS NOT NEW.model_version_id
  OR OLD.source_candidate_run_id IS NOT NEW.source_candidate_run_id
  OR OLD.total_decimal_odd IS NOT NEW.total_decimal_odd
  OR OLD.reference_stake IS NOT NEW.reference_stake
  OR OLD.notes IS NOT NEW.notes
BEGIN
    SELECT RAISE(ABORT, 'coupon creation snapshot fields are immutable');
END;

CREATE TRIGGER coupons_no_delete
BEFORE DELETE ON coupons
BEGIN
    SELECT RAISE(ABORT, 'coupon snapshots are immutable');
END;

CREATE TRIGGER coupon_selections_creation_fields_immutable
BEFORE UPDATE ON coupon_selections
WHEN OLD.coupon_id IS NOT NEW.coupon_id
  OR OLD.prediction_id IS NOT NEW.prediction_id
  OR OLD.source_candidate_id IS NOT NEW.source_candidate_id
  OR OLD.selection_order IS NOT NEW.selection_order
  OR OLD.match_id IS NOT NEW.match_id
  OR OLD.market_snapshot IS NOT NEW.market_snapshot
  OR OLD.selection_snapshot IS NOT NEW.selection_snapshot
  OR OLD.line_value_snapshot IS NOT NEW.line_value_snapshot
  OR OLD.model_probability_snapshot IS NOT NEW.model_probability_snapshot
  OR OLD.odd_snapshot IS NOT NEW.odd_snapshot
  OR OLD.created_at IS NOT NEW.created_at
BEGIN
    SELECT RAISE(ABORT, 'coupon selection snapshot fields are immutable');
END;

CREATE TRIGGER coupon_selections_no_delete
BEFORE DELETE ON coupon_selections
BEGIN
    SELECT RAISE(ABORT, 'coupon selection snapshots are immutable');
END;

CREATE TRIGGER coupon_system_sizes_validate_insert
BEFORE INSERT ON coupon_system_sizes
WHEN NOT EXISTS (
    SELECT 1
    FROM coupons
    WHERE coupons.id = NEW.coupon_id
      AND (
          (coupons.coupon_type = 'SYSTEM_5_6' AND NEW.system_size IN (5, 6))
          OR (coupons.coupon_type = 'SYSTEM_4_5_6' AND NEW.system_size IN (4, 5, 6))
      )
)
BEGIN
    SELECT RAISE(ABORT, 'system size is not valid for coupon type');
END;

CREATE TRIGGER coupon_system_sizes_no_update
BEFORE UPDATE ON coupon_system_sizes
BEGIN
    SELECT RAISE(ABORT, 'coupon system sizes are immutable');
END;

CREATE TRIGGER coupon_system_sizes_no_delete
BEFORE DELETE ON coupon_system_sizes
BEGIN
    SELECT RAISE(ABORT, 'coupon system sizes are immutable');
END;
