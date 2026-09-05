use rusqlite::Connection;

use crate::models::DatabaseStats;

fn count(connection: &Connection, table: &str) -> rusqlite::Result<i64> {
    connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
}

pub fn database_stats(connection: &Connection) -> rusqlite::Result<DatabaseStats> {
    Ok(DatabaseStats {
        competitions: count(connection, "competitions")?,
        teams: count(connection, "teams")?,
        provider_team_mappings: count(connection, "provider_team_mappings")?,
        matches: count(connection, "matches")?,
        match_statistics: count(connection, "match_statistics")?,
        provider_match_mappings: count(connection, "provider_match_mappings")?,
        odds_snapshots: count(connection, "odds_snapshots")?,
        predictions: count(connection, "predictions")?,
        model_versions: count(connection, "model_versions")?,
        coupon_candidate_runs: count(connection, "coupon_candidate_runs")?,
        coupon_candidates: count(connection, "coupon_candidates")?,
        coupons: count(connection, "coupons")?,
        coupon_selections: count(connection, "coupon_selections")?,
        coupon_system_sizes: count(connection, "coupon_system_sizes")?,
        provider_competition_mappings: count(connection, "provider_competition_mappings")?,
        data_import_runs: count(connection, "data_import_runs")?,
        provider_competition_metadata: count(connection, "provider_competition_metadata")?,
        popularity_snapshots: count(connection, "popularity_snapshots")?,
    })
}
