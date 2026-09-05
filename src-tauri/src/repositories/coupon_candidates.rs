use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{CouponCandidate, CouponCandidateRun};

pub struct NewCandidateRun<'a> {
    pub target_date: &'a str,
    pub coupon_type: &'a str,
    pub generated_at: &'a str,
    pub model_version_id: i64,
    pub status: &'a str,
    pub target_candidate_count: i64,
    pub qualified_candidate_count: i64,
    pub generation_config_json: Option<&'a str>,
}

pub struct NewCandidate<'a> {
    pub candidate_run_id: i64,
    pub prediction_id: i64,
    pub rank: i64,
    pub ranking_score: f64,
    pub model_probability_snapshot: f64,
    pub iddaa_odd_snapshot: Option<f64>,
    pub market_snapshot: &'a str,
    pub selection_snapshot: &'a str,
    pub line_value_snapshot: Option<f64>,
    pub explanation_json: Option<&'a str>,
}

pub fn insert_run(connection: &Connection, run: &NewCandidateRun<'_>) -> rusqlite::Result<i64> {
    connection.execute(
        "INSERT INTO coupon_candidate_runs (
            target_date, coupon_type, generated_at, model_version_id, status,
            target_candidate_count, qualified_candidate_count, generation_config_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            run.target_date,
            run.coupon_type,
            run.generated_at,
            run.model_version_id,
            run.status,
            run.target_candidate_count,
            run.qualified_candidate_count,
            run.generation_config_json
        ],
    )?;
    Ok(connection.last_insert_rowid())
}

pub fn find_run_by_id(
    connection: &Connection,
    id: i64,
) -> rusqlite::Result<Option<CouponCandidateRun>> {
    connection
        .query_row(
            "SELECT id, target_date, coupon_type, generated_at, model_version_id, status,
                    target_candidate_count, qualified_candidate_count, generation_config_json,
                    created_at
             FROM coupon_candidate_runs WHERE id = ?1",
            [id],
            |row| {
                Ok(CouponCandidateRun {
                    id: row.get(0)?,
                    target_date: row.get(1)?,
                    coupon_type: row.get(2)?,
                    generated_at: row.get(3)?,
                    model_version_id: row.get(4)?,
                    status: row.get(5)?,
                    target_candidate_count: row.get(6)?,
                    qualified_candidate_count: row.get(7)?,
                    generation_config_json: row.get(8)?,
                    created_at: row.get(9)?,
                })
            },
        )
        .optional()
}

pub fn insert_candidate(
    connection: &Connection,
    candidate: &NewCandidate<'_>,
) -> rusqlite::Result<i64> {
    connection.execute(
        "INSERT INTO coupon_candidates (
            candidate_run_id, prediction_id, rank, ranking_score,
            model_probability_snapshot, iddaa_odd_snapshot, market_snapshot,
            selection_snapshot, line_value_snapshot, explanation_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            candidate.candidate_run_id,
            candidate.prediction_id,
            candidate.rank,
            candidate.ranking_score,
            candidate.model_probability_snapshot,
            candidate.iddaa_odd_snapshot,
            candidate.market_snapshot,
            candidate.selection_snapshot,
            candidate.line_value_snapshot,
            candidate.explanation_json
        ],
    )?;
    Ok(connection.last_insert_rowid())
}

pub fn list_for_run(
    connection: &Connection,
    candidate_run_id: i64,
) -> rusqlite::Result<Vec<CouponCandidate>> {
    let mut statement = connection.prepare(
        "SELECT id, candidate_run_id, prediction_id, rank, ranking_score,
                model_probability_snapshot, iddaa_odd_snapshot, market_snapshot,
                selection_snapshot, line_value_snapshot, explanation_json, created_at
         FROM coupon_candidates
         WHERE candidate_run_id = ?1
         ORDER BY rank",
    )?;
    let candidates = statement
        .query_map([candidate_run_id], |row| {
            Ok(CouponCandidate {
                id: row.get(0)?,
                candidate_run_id: row.get(1)?,
                prediction_id: row.get(2)?,
                rank: row.get(3)?,
                ranking_score: row.get(4)?,
                model_probability_snapshot: row.get(5)?,
                iddaa_odd_snapshot: row.get(6)?,
                market_snapshot: row.get(7)?,
                selection_snapshot: row.get(8)?,
                line_value_snapshot: row.get(9)?,
                explanation_json: row.get(10)?,
                created_at: row.get(11)?,
            })
        })?
        .collect();
    candidates
}
