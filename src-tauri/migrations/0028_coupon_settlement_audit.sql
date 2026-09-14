CREATE TABLE coupon_selection_settlement_audit (
 selection_id INTEGER PRIMARY KEY REFERENCES phase8_coupon_selections(id),
 state TEXT NOT NULL,
 reason TEXT,
 result_match_id INTEGER REFERENCES matches(id),
 checked_at TEXT NOT NULL
);
CREATE INDEX idx_coupon_selection_pending ON phase8_coupon_selections(status,coupon_id,id);
CREATE INDEX idx_coupon_result_identity ON matches(competition_id,home_team_id,away_team_id,scheduled_local_date,status);
CREATE TABLE coupon_result_refresh (
 league_code TEXT NOT NULL, season_code TEXT NOT NULL,
 last_attempt_at TEXT NOT NULL, next_retry_at TEXT NOT NULL,
 state TEXT NOT NULL, detail TEXT,
 PRIMARY KEY(league_code,season_code)
);
