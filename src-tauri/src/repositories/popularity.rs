use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::providers::iddaa::PROVIDER_ID;

pub struct PopularityInput<'a> {
    pub match_id: Option<i64>,
    pub provider_event_id: &'a str,
    pub provider_market_id: Option<&'a str>,
    pub provider_market_code: Option<&'a str>,
    pub provider_selection_code: Option<&'a str>,
    pub market_type: Option<&'a str>,
    pub provider_line: Option<&'a str>,
    pub line_value: Option<f64>,
    pub selection: Option<&'a str>,
    pub metric_type: &'a str,
    pub metric_value: Option<f64>,
    pub rank_value: Option<i64>,
    pub raw_metric_name: &'a str,
    pub provider_raw_value: Option<&'a str>,
    pub captured_at: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct PopularityStatus {
    pub last_successful_refresh: Option<String>,
    pub snapshot_count: i64,
    pub latest_snapshot_time: Option<String>,
    pub latest_selection_count: i64,
    pub matched_selection_count: i64,
    pub unmatched_selection_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LatestPopularSelection {
    pub match_id: i64,
    pub provider_event_id: String,
    pub competition: String,
    pub kickoff_at: String,
    pub home_team: String,
    pub away_team: String,
    pub provider_market_id: Option<String>,
    pub provider_market_code: Option<String>,
    pub market_type: Option<String>,
    pub line: Option<f64>,
    pub selection: Option<String>,
    pub play_count: i64,
    pub play_count_display: Option<String>,
    pub rank: i64,
    pub odd: Option<f64>,
    pub wodd: Option<f64>,
    pub captured_at: String,
}

pub fn match_id_for_event(
    connection: &Connection,
    provider_event_id: &str,
) -> rusqlite::Result<Option<i64>> {
    connection
        .query_row(
            "SELECT match_id FROM provider_match_mappings
             WHERE provider = ?1 AND external_match_id = ?2",
            params![PROVIDER_ID, provider_event_id],
            |row| row.get(0),
        )
        .optional()
}

pub fn insert_if_changed(
    connection: &Connection,
    input: &PopularityInput<'_>,
) -> rusqlite::Result<bool> {
    let latest = connection
        .query_row(
            "SELECT metric_value, rank_value, provider_raw_value, match_id
             FROM popularity_snapshots
             WHERE provider = ?1 AND provider_event_id = ?2
               AND provider_market_id IS ?3 AND provider_selection_code IS ?4
               AND metric_type = ?5
             ORDER BY captured_at DESC, id DESC LIMIT 1",
            params![
                PROVIDER_ID,
                input.provider_event_id,
                input.provider_market_id,
                input.provider_selection_code,
                input.metric_type
            ],
            |row| {
                Ok((
                    row.get::<_, Option<f64>>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .optional()?;
    if latest
        == Some((
            input.metric_value,
            input.rank_value,
            input.provider_raw_value.map(str::to_string),
            input.match_id,
        ))
    {
        return Ok(false);
    }
    connection.execute(
        "INSERT INTO popularity_snapshots (
            provider, match_id, provider_event_id, provider_market_id,
            provider_market_code, provider_selection_code, market_type,
            provider_line, line_value, selection, metric_type, metric_value,
            rank_value, raw_metric_name, provider_raw_value, captured_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            PROVIDER_ID,
            input.match_id,
            input.provider_event_id,
            input.provider_market_id,
            input.provider_market_code,
            input.provider_selection_code,
            input.market_type,
            input.provider_line,
            input.line_value,
            input.selection,
            input.metric_type,
            input.metric_value,
            input.rank_value,
            input.raw_metric_name,
            input.provider_raw_value,
            input.captured_at
        ],
    )?;
    Ok(true)
}

pub fn status(connection: &Connection) -> rusqlite::Result<PopularityStatus> {
    connection.query_row(
        "WITH latest AS (
            SELECT match_id, ROW_NUMBER() OVER (
                PARTITION BY provider_event_id, COALESCE(provider_market_id, ''),
                    COALESCE(provider_selection_code, ''), metric_type
                ORDER BY captured_at DESC, id DESC
            ) AS row_number
            FROM popularity_snapshots WHERE provider = ?1 AND metric_type = 'COUNT'
         )
         SELECT
            (SELECT MAX(completed_at) FROM data_import_runs
             WHERE provider = ?1 AND dataset_key = 'iddaa:popular-bets' AND status = 'completed'),
            (SELECT COUNT(*) FROM popularity_snapshots WHERE provider = ?1),
            (SELECT MAX(captured_at) FROM popularity_snapshots WHERE provider = ?1),
            (SELECT COUNT(*) FROM latest WHERE row_number = 1),
            (SELECT COUNT(*) FROM latest WHERE row_number = 1 AND match_id IS NOT NULL),
            (SELECT COUNT(*) FROM latest WHERE row_number = 1 AND match_id IS NULL)",
        [PROVIDER_ID],
        |row| {
            Ok(PopularityStatus {
                last_successful_refresh: row.get(0)?,
                snapshot_count: row.get(1)?,
                latest_snapshot_time: row.get(2)?,
                latest_selection_count: row.get(3)?,
                matched_selection_count: row.get(4)?,
                unmatched_selection_count: row.get(5)?,
            })
        },
    )
}

pub fn latest_popular_selections(
    connection: &Connection,
) -> rusqlite::Result<Vec<LatestPopularSelection>> {
    let mut statement = connection.prepare(
        "WITH ranked AS (
            SELECT *, ROW_NUMBER() OVER (
                PARTITION BY provider_event_id, COALESCE(provider_market_id, ''),
                    COALESCE(provider_selection_code, ''), metric_type
                ORDER BY captured_at DESC, id DESC
            ) AS row_number
            FROM popularity_snapshots WHERE provider = ?1
         ), counts AS (
            SELECT * FROM ranked WHERE row_number = 1 AND metric_type = 'COUNT'
         ), ranks AS (
            SELECT * FROM ranked WHERE row_number = 1 AND metric_type = 'RANK'
         )
         SELECT c.match_id, c.provider_event_id, competition.name, m.kickoff_at,
                home.normalized_name, away.normalized_name, c.provider_market_id,
                c.provider_market_code, c.market_type, c.line_value, c.selection,
                CAST(c.metric_value AS INTEGER), c.provider_raw_value, r.rank_value,
                odds.odd, odds.alternative_odd, c.captured_at
         FROM counts c
         JOIN ranks r ON r.provider_event_id = c.provider_event_id
            AND r.provider_market_id IS c.provider_market_id
            AND r.provider_selection_code IS c.provider_selection_code
         JOIN matches m ON m.id = c.match_id
         JOIN competitions competition ON competition.id = m.competition_id
         JOIN teams home ON home.id = m.home_team_id
         JOIN teams away ON away.id = m.away_team_id
         LEFT JOIN odds_snapshots odds ON odds.id = (
            SELECT id FROM odds_snapshots candidate
            WHERE candidate.provider = ?1 AND candidate.match_id = c.match_id
              AND candidate.provider_market_id IS c.provider_market_id
              AND candidate.provider_selection_code IS c.provider_selection_code
            ORDER BY candidate.captured_at DESC, candidate.id DESC LIMIT 1
         )
         ORDER BY r.rank_value, c.provider_event_id, c.provider_market_id",
    )?;
    let rows = statement
        .query_map([PROVIDER_ID], |row| {
            Ok(LatestPopularSelection {
                match_id: row.get(0)?,
                provider_event_id: row.get(1)?,
                competition: row.get(2)?,
                kickoff_at: row.get(3)?,
                home_team: row.get(4)?,
                away_team: row.get(5)?,
                provider_market_id: row.get(6)?,
                provider_market_code: row.get(7)?,
                market_type: row.get(8)?,
                line: row.get(9)?,
                selection: row.get(10)?,
                play_count: row.get(11)?,
                play_count_display: row.get(12)?,
                rank: row.get(13)?,
                odd: row.get(14)?,
                wodd: row.get(15)?,
                captured_at: row.get(16)?,
            })
        })?
        .collect();
    rows
}
