CREATE TABLE btts_daily_runs (
    id INTEGER PRIMARY KEY,
    business_date TEXT NOT NULL,
    as_of TEXT NOT NULL,
    candidate_run_id INTEGER NOT NULL REFERENCES candidate_engine_runs(id),
    report_json TEXT NOT NULL CHECK(json_valid(report_json)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX btts_daily_runs_date ON btts_daily_runs(business_date,id DESC);
CREATE TRIGGER btts_daily_runs_no_update BEFORE UPDATE ON btts_daily_runs
BEGIN SELECT RAISE(ABORT,'immutable BTTS audit'); END;
CREATE TRIGGER btts_daily_runs_no_delete BEFORE DELETE ON btts_daily_runs
BEGIN SELECT RAISE(ABORT,'immutable BTTS audit'); END;
