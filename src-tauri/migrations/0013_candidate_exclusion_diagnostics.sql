ALTER TABLE candidate_engine_exclusions ADD COLUMN business_date TEXT;
ALTER TABLE candidate_engine_exclusions ADD COLUMN competition_id INTEGER REFERENCES competitions(id) ON DELETE RESTRICT;
ALTER TABLE candidate_engine_exclusions ADD COLUMN market TEXT;
ALTER TABLE candidate_engine_exclusions ADD COLUMN selection TEXT;
ALTER TABLE candidate_engine_exclusions ADD COLUMN line_value REAL;
ALTER TABLE candidate_engine_exclusions ADD COLUMN generated_at TEXT;
UPDATE candidate_engine_exclusions SET business_date=(SELECT business_date FROM candidate_engine_runs WHERE id=run_id), competition_id=(SELECT competition_id FROM matches WHERE id=match_id), generated_at=(SELECT generated_at FROM candidate_engine_runs WHERE id=run_id) WHERE business_date IS NULL;
