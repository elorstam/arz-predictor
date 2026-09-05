use rusqlite::Connection;

use crate::models::FootballDataQualitySummary;

#[derive(Debug, Clone)]
pub struct DatasetBootstrapDatabaseStatus {
    pub last_successful_import_at: Option<String>,
    pub last_import_status: Option<String>,
    pub match_count: i64,
    pub finished_match_count: i64,
    pub matches_with_statistics: i64,
}

#[derive(Debug, Clone)]
pub struct NormalizedTotals {
    pub matches: i64,
    pub teams: i64,
    pub earliest_match: Option<String>,
    pub latest_match: Option<String>,
}

pub fn dataset_bootstrap_status(
    connection: &Connection,
    dataset_key: &str,
) -> rusqlite::Result<DatasetBootstrapDatabaseStatus> {
    connection.query_row(
        "SELECT
            (SELECT MAX(completed_at) FROM data_import_runs
             WHERE provider = 'football-data.co.uk' AND dataset_key = ?1 AND status = 'completed'),
            (SELECT status FROM data_import_runs
             WHERE provider = 'football-data.co.uk' AND dataset_key = ?1 ORDER BY id DESC LIMIT 1),
            COUNT(DISTINCT m.id),
            COUNT(DISTINCT CASE WHEN m.status = 'finished' THEN m.id END),
            COUNT(DISTINCT CASE WHEN ms.match_id IS NOT NULL THEN m.id END)
         FROM matches m
         JOIN provider_match_mappings pm ON pm.match_id = m.id
         LEFT JOIN match_statistics ms ON ms.match_id = m.id
         WHERE pm.provider = 'football-data.co.uk'
           AND m.competition_id = (
               SELECT competition_id FROM data_import_runs
               WHERE provider = 'football-data.co.uk' AND dataset_key = ?1
                 AND competition_id IS NOT NULL ORDER BY id DESC LIMIT 1
           )
           AND m.season = (
               SELECT season FROM data_import_runs
               WHERE provider = 'football-data.co.uk' AND dataset_key = ?1
               ORDER BY id DESC LIMIT 1
           )",
        [dataset_key],
        |row| {
            Ok(DatasetBootstrapDatabaseStatus {
                last_successful_import_at: row.get(0)?,
                last_import_status: row.get(1)?,
                match_count: row.get(2)?,
                finished_match_count: row.get(3)?,
                matches_with_statistics: row.get(4)?,
            })
        },
    )
}

pub fn normalized_totals(connection: &Connection) -> rusqlite::Result<NormalizedTotals> {
    connection.query_row(
        "SELECT
            (SELECT COUNT(*) FROM matches),
            (SELECT COUNT(*) FROM teams),
            (SELECT MIN(COALESCE(scheduled_local_date, substr(kickoff_at, 1, 10))) FROM matches),
            (SELECT MAX(COALESCE(scheduled_local_date, substr(kickoff_at, 1, 10))) FROM matches)",
        [],
        |row| {
            Ok(NormalizedTotals {
                matches: row.get(0)?,
                teams: row.get(1)?,
                earliest_match: row.get(2)?,
                latest_match: row.get(3)?,
            })
        },
    )
}

pub fn football_data_summary(
    connection: &Connection,
    provider: &str,
) -> rusqlite::Result<Vec<FootballDataQualitySummary>> {
    let mut statement = connection.prepare(
        "SELECT
            c.id,
            c.name,
            c.country,
            m.season,
            COUNT(DISTINCT m.id),
            COUNT(DISTINCT CASE WHEN m.status = 'finished' THEN m.id END),
            COUNT(DISTINCT CASE WHEN m.status = 'scheduled' THEN m.id END),
            COUNT(DISTINCT CASE WHEN ms.home_shots IS NOT NULL AND ms.away_shots IS NOT NULL THEN m.id END),
            COUNT(DISTINCT CASE WHEN ms.home_shots_on_target IS NOT NULL AND ms.away_shots_on_target IS NOT NULL THEN m.id END),
            COUNT(DISTINCT CASE WHEN ms.home_corners IS NOT NULL AND ms.away_corners IS NOT NULL THEN m.id END),
            COUNT(DISTINCT CASE WHEN ms.home_yellow_cards IS NOT NULL AND ms.away_yellow_cards IS NOT NULL THEN m.id END),
            COUNT(DISTINCT CASE WHEN m.halftime_home_goals IS NOT NULL AND m.halftime_away_goals IS NOT NULL THEN m.id END),
            MIN(COALESCE(m.scheduled_local_date, substr(m.kickoff_at, 1, 10))),
            MAX(COALESCE(m.scheduled_local_date, substr(m.kickoff_at, 1, 10))),
            (
                SELECT MAX(r.completed_at)
                FROM data_import_runs r
                WHERE r.provider = ?1
                  AND r.competition_id = c.id
                  AND r.season = m.season
                  AND r.status = 'completed'
            )
         FROM matches m
         JOIN competitions c ON c.id = m.competition_id
         JOIN provider_match_mappings pm
           ON pm.match_id = m.id AND pm.provider = ?1
         LEFT JOIN match_statistics ms ON ms.match_id = m.id
         GROUP BY c.id, c.name, c.country, m.season
         ORDER BY c.country, c.name, m.season",
    )?;
    let summaries = statement
        .query_map([provider], |row| {
            Ok(FootballDataQualitySummary {
                competition_id: row.get(0)?,
                competition_name: row.get(1)?,
                country: row.get(2)?,
                season: row.get(3)?,
                match_count: row.get(4)?,
                finished_match_count: row.get(5)?,
                scheduled_match_count: row.get(6)?,
                matches_with_shots: row.get(7)?,
                matches_with_shots_on_target: row.get(8)?,
                matches_with_corners: row.get(9)?,
                matches_with_cards: row.get(10)?,
                matches_with_halftime: row.get(11)?,
                earliest_match_date: row.get(12)?,
                latest_match_date: row.get(13)?,
                last_successful_import_at: row.get(14)?,
            })
        })?
        .collect();
    summaries
}
