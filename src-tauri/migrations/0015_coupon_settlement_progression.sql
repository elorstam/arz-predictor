ALTER TABLE phase8_coupons ADD COLUMN series_id INTEGER REFERENCES phase8_compound_series(id);
ALTER TABLE phase8_coupons ADD COLUMN step_number INTEGER;
ALTER TABLE phase8_coupons ADD COLUMN settled_at TEXT;
ALTER TABLE phase8_coupons ADD COLUMN settlement_result TEXT;
CREATE TABLE phase8_series_steps (
    id INTEGER PRIMARY KEY,
    series_id INTEGER NOT NULL REFERENCES phase8_compound_series(id) ON DELETE RESTRICT,
    step_number INTEGER NOT NULL CHECK(step_number BETWEEN 1 AND 7),
    coupon_id INTEGER NOT NULL REFERENCES phase8_coupons(id) ON DELETE RESTRICT,
    business_date TEXT NOT NULL,
    stake_cents INTEGER NOT NULL CHECK(stake_cents>0),
    combined_odd REAL NOT NULL,
    potential_return_cents INTEGER NOT NULL,
    result TEXT NOT NULL DEFAULT 'UNSETTLED' CHECK(result IN ('WON','LOST','VOID','UNSETTLED')),
    settled_at TEXT,
    UNIQUE(series_id,step_number,coupon_id)
);
CREATE INDEX phase8_series_steps_series ON phase8_series_steps(series_id,step_number);
