CREATE TABLE phase8_coupons (
    id INTEGER PRIMARY KEY,
    coupon_type TEXT NOT NULL,
    business_date TEXT NOT NULL,
    source_candidate_run_id INTEGER NOT NULL REFERENCES candidate_engine_runs(id) ON DELETE RESTRICT,
    policy_version TEXT NOT NULL,
    model_version TEXT NOT NULL,
    calibration_version TEXT,
    generation_cutoff TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'DRAFT' CHECK(status IN ('DRAFT','FINALIZED','SETTLED','CANCELLED')),
    combined_decimal_odd REAL,
    target_odds_reached INTEGER NOT NULL DEFAULT 0 CHECK(target_odds_reached IN (0,1)),
    system_sizes_json TEXT CHECK(system_sizes_json IS NULL OR json_valid(system_sizes_json)),
    unit_stake_cents INTEGER CHECK(unit_stake_cents IS NULL OR unit_stake_cents > 0),
    total_stake_cents INTEGER CHECK(total_stake_cents IS NULL OR total_stake_cents >= 0),
    metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json)),
    created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(business_date,source_candidate_run_id,coupon_type,policy_version)
);
CREATE TABLE phase8_coupon_selections (
    id INTEGER PRIMARY KEY,
    coupon_id INTEGER NOT NULL REFERENCES phase8_coupons(id) ON DELETE RESTRICT,
    candidate_id INTEGER NOT NULL REFERENCES candidate_engine_candidates(id) ON DELETE RESTRICT,
    selection_order INTEGER NOT NULL CHECK(selection_order>0),
    match_id INTEGER NOT NULL,
    competition_id INTEGER NOT NULL,
    market TEXT NOT NULL,
    selection TEXT NOT NULL,
    line_value REAL,
    raw_probability REAL NOT NULL,
    public_probability REAL NOT NULL,
    calibration_status TEXT NOT NULL,
    calibration_version TEXT,
    iddaa_odd REAL NOT NULL,
    odds_snapshot_id INTEGER,
    odds_captured_at TEXT NOT NULL,
    probability_edge REAL NOT NULL,
    expected_value REAL NOT NULL,
    score REAL NOT NULL,
    rank INTEGER NOT NULL,
    correlation_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'PENDING' CHECK(status IN ('PENDING','WON','LOST','VOID')),
    UNIQUE(coupon_id,candidate_id), UNIQUE(coupon_id,selection_order)
);
CREATE TABLE phase8_system_columns (
    id INTEGER PRIMARY KEY,
    coupon_id INTEGER NOT NULL REFERENCES phase8_coupons(id) ON DELETE RESTRICT,
    column_number INTEGER NOT NULL,
    system_size INTEGER NOT NULL,
    candidate_ids_json TEXT NOT NULL CHECK(json_valid(candidate_ids_json)),
    column_decimal_odd REAL NOT NULL,
    status TEXT NOT NULL DEFAULT 'PENDING' CHECK(status IN ('PENDING','WON','LOST','VOID')),
    UNIQUE(coupon_id,column_number), UNIQUE(coupon_id,candidate_ids_json)
);
CREATE TABLE phase8_compound_series (
    id INTEGER PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    business_date TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('ACTIVE','COMPLETED','FAILED_RESET','CANCELLED')),
    current_step INTEGER NOT NULL CHECK(current_step BETWEEN 1 AND 7),
    starting_stake_cents INTEGER NOT NULL CHECK(starting_stake_cents>0),
    current_stake_cents INTEGER NOT NULL CHECK(current_stake_cents>0),
    completed_steps INTEGER NOT NULL DEFAULT 0,
    latest_coupon_id INTEGER REFERENCES phase8_coupons(id),
    reset_count INTEGER NOT NULL DEFAULT 0,
    metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json))
);
CREATE UNIQUE INDEX phase8_one_active_series ON phase8_compound_series(status) WHERE status='ACTIVE';
CREATE TRIGGER phase8_finalized_coupon_immutable BEFORE UPDATE ON phase8_coupons WHEN OLD.status IN ('FINALIZED','SETTLED') AND (NEW.coupon_type IS NOT OLD.coupon_type OR NEW.business_date IS NOT OLD.business_date OR NEW.source_candidate_run_id IS NOT OLD.source_candidate_run_id OR NEW.combined_decimal_odd IS NOT OLD.combined_decimal_odd OR NEW.system_sizes_json IS NOT OLD.system_sizes_json OR NEW.unit_stake_cents IS NOT OLD.unit_stake_cents OR NEW.total_stake_cents IS NOT OLD.total_stake_cents) BEGIN SELECT RAISE(ABORT,'finalized coupon is immutable'); END;
CREATE TRIGGER phase8_finalized_selection_immutable BEFORE UPDATE ON phase8_coupon_selections WHEN EXISTS(SELECT 1 FROM phase8_coupons WHERE id=OLD.coupon_id AND status IN ('FINALIZED','SETTLED')) AND (NEW.candidate_id IS NOT OLD.candidate_id OR NEW.iddaa_odd IS NOT OLD.iddaa_odd OR NEW.raw_probability IS NOT OLD.raw_probability) BEGIN SELECT RAISE(ABORT,'finalized selection is immutable'); END;
CREATE TRIGGER phase8_finalized_selection_no_delete BEFORE DELETE ON phase8_coupon_selections WHEN EXISTS(SELECT 1 FROM phase8_coupons WHERE id=OLD.coupon_id AND status IN ('FINALIZED','SETTLED')) BEGIN SELECT RAISE(ABORT,'finalized selection is immutable'); END;
