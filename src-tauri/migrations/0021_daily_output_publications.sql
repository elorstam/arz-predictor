-- Publishing a daily result is distinct from inserting a replay/comparison run.
CREATE TABLE candidate_run_execution (
    run_id INTEGER PRIMARY KEY REFERENCES candidate_engine_runs(id),
    purpose TEXT NOT NULL CHECK(purpose IN ('LIVE','REPLAY'))
);
CREATE TABLE daily_output_publications (
    business_date TEXT PRIMARY KEY,
    candidate_run_id INTEGER NOT NULL REFERENCES candidate_engine_runs(id),
    published_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
INSERT INTO daily_output_publications(business_date,candidate_run_id)
SELECT r.business_date,r.id FROM candidate_engine_runs r
WHERE r.status='COMPLETED' AND COALESCE(r.prediction_context,'BASE')='BASE'
AND r.id=(SELECT n.id FROM candidate_engine_runs n WHERE n.business_date=r.business_date
  AND n.status='COMPLETED' AND COALESCE(n.prediction_context,'BASE')='BASE'
  ORDER BY julianday(n.generated_at) DESC,n.id DESC LIMIT 1);
CREATE INDEX idx_daily_runs_asof ON candidate_engine_runs(business_date,prediction_context,status,generated_at,id);
CREATE INDEX idx_history_home ON matches(home_team_id,status,kickoff_at);
CREATE INDEX idx_history_away ON matches(away_team_id,status,kickoff_at);
CREATE INDEX idx_candidate_odds_asof ON odds_snapshots(match_id,normalized_market_type,normalized_selection,line_value,captured_at DESC,id DESC);
