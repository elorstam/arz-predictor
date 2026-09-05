use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::providers::iddaa::PROVIDER_ID;

#[derive(Debug, Clone)]
pub struct CompetitionInput<'a> {
    pub external_id: &'a str,
    pub name: Option<&'a str>,
    pub country: Option<&'a str>,
    pub season: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct Resolution {
    pub id: i64,
    pub created: bool,
}

#[derive(Debug, Clone)]
pub struct MatchInput<'a> {
    pub external_event_id: &'a str,
    pub competition_id: i64,
    pub season: &'a str,
    pub home_team_id: i64,
    pub away_team_id: i64,
    pub kickoff_at: &'a str,
    pub local_date: &'a str,
}

#[derive(Debug, Clone)]
pub struct OddsInput<'a> {
    pub match_id: i64,
    pub market_code: &'a str,
    pub provider_market_id: Option<&'a str>,
    pub normalized_market_type: &'a str,
    pub provider_line: Option<&'a str>,
    pub line_value: Option<f64>,
    pub provider_selection_code: Option<&'a str>,
    pub selection: &'a str,
    pub normalized_selection: Option<&'a str>,
    pub odd: f64,
    pub alternative_odd: Option<f64>,
    pub captured_at: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct LatestOdd {
    pub match_id: i64,
    pub provider_market_code: String,
    pub provider_market_id: Option<String>,
    pub normalized_market_type: String,
    pub provider_line: Option<String>,
    pub line: Option<f64>,
    pub provider_selection_code: Option<String>,
    pub selection: String,
    pub normalized_selection: Option<String>,
    pub odd: f64,
    pub wodd: Option<f64>,
    pub captured_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpcomingMatch {
    pub match_id: i64,
    pub provider_event_id: String,
    pub competition_id: i64,
    pub competition: String,
    pub kickoff_at: String,
    pub home_team_id: i64,
    pub home_team: String,
    pub away_team_id: i64,
    pub away_team: String,
    pub available_markets: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BulletinStatus {
    pub last_successful_refresh: Option<String>,
    pub upcoming_match_count: i64,
    pub competition_count: i64,
    pub team_count: i64,
    pub latest_odds_snapshot_time: Option<String>,
    pub odds_snapshot_count: i64,
    pub unresolved_competition_count: i64,
}

pub fn record_competition_metadata(
    connection: &Connection,
    input: &CompetitionInput<'_>,
    resolved: bool,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO provider_competition_metadata (
            provider, external_competition_id, external_name, country, season, resolution_status
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(provider, external_competition_id) DO UPDATE SET
            external_name = COALESCE(excluded.external_name, external_name),
            country = COALESCE(excluded.country, country),
            season = COALESCE(excluded.season, season),
            resolution_status = CASE
                WHEN excluded.resolution_status = 'resolved' THEN 'resolved'
                ELSE resolution_status END,
            last_seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![
            PROVIDER_ID,
            input.external_id,
            input.name,
            input.country,
            input.season,
            if resolved { "resolved" } else { "unresolved" }
        ],
    )?;
    Ok(())
}

pub fn resolve_competition(
    connection: &Connection,
    input: &CompetitionInput<'_>,
) -> rusqlite::Result<Option<Resolution>> {
    if let Some(id) = connection
        .query_row(
            "SELECT competition_id FROM provider_competition_mappings
             WHERE provider = ?1 AND external_competition_id = ?2",
            params![PROVIDER_ID, input.external_id],
            |row| row.get(0),
        )
        .optional()?
    {
        record_competition_metadata(connection, input, true)?;
        return Ok(Some(Resolution { id, created: false }));
    }
    let Some(name) = input.name.map(str::trim).filter(|name| !name.is_empty()) else {
        record_competition_metadata(connection, input, false)?;
        return Ok(None);
    };
    let existing = connection
        .query_row(
            "SELECT id FROM competitions WHERE name = ?1 AND country IS ?2",
            params![name, input.country],
            |row| row.get(0),
        )
        .optional()?
        .or_else(|| {
            let mut stmt = connection
                .prepare("SELECT id,name,country FROM competitions WHERE country IS ?1")
                .ok()?;
            let rows: Vec<(i64, String, Option<String>)> = stmt
                .query_map([input.country], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .ok()?
                .filter_map(Result::ok)
                .collect();
            rows.into_iter()
                .find(|(_, n, c)| {
                    crate::repositories::resolution::competition_alias_compatible(
                        name,
                        n,
                        input.country,
                        c.as_deref(),
                    )
                })
                .map(|(id, _, _)| id)
        });
    let (id, created) = match existing {
        Some(id) => (id, false),
        None => {
            connection.execute(
                "INSERT INTO competitions (name, country, current_season) VALUES (?1, ?2, ?3)",
                params![name, input.country, input.season],
            )?;
            (connection.last_insert_rowid(), true)
        }
    };
    connection.execute(
        "INSERT INTO provider_competition_mappings (
            provider, external_competition_id, competition_id
         ) VALUES (?1, ?2, ?3)",
        params![PROVIDER_ID, input.external_id, id],
    )?;
    record_competition_metadata(connection, input, true)?;
    Ok(Some(Resolution { id, created }))
}

pub fn resolve_team(
    connection: &Connection,
    source_name: &str,
    country: Option<&str>,
) -> rusqlite::Result<Resolution> {
    if let Some(id) = connection
        .query_row(
            "SELECT team_id FROM provider_team_mappings
             WHERE provider = ?1 AND external_team_id = ?2",
            params![PROVIDER_ID, source_name],
            |row| row.get(0),
        )
        .optional()?
    {
        return Ok(Resolution { id, created: false });
    }
    let existing = connection
        .query_row(
            "SELECT id FROM teams WHERE normalized_name = ?1 AND country IS ?2",
            params![source_name, country],
            |row| row.get(0),
        )
        .optional()?;
    let (id, created) = match existing {
        Some(id) => (id, false),
        None => {
            connection.execute(
                "INSERT INTO teams (normalized_name, country) VALUES (?1, ?2)",
                params![source_name, country],
            )?;
            (connection.last_insert_rowid(), true)
        }
    };
    connection.execute(
        "INSERT INTO provider_team_mappings (
            team_id, provider, external_team_id, external_team_name
         ) VALUES (?1, ?2, ?3, ?3)",
        params![id, PROVIDER_ID, source_name],
    )?;
    Ok(Resolution { id, created })
}

pub fn upsert_match(
    connection: &Connection,
    input: &MatchInput<'_>,
) -> rusqlite::Result<(i64, bool)> {
    if let Some(match_id) = connection
        .query_row(
            "SELECT match_id FROM provider_match_mappings
             WHERE provider = ?1 AND external_match_id = ?2",
            params![PROVIDER_ID, input.external_event_id],
            |row| row.get(0),
        )
        .optional()?
    {
        connection.execute(
            "UPDATE matches SET competition_id = ?2, season = ?3, home_team_id = ?4,
                away_team_id = ?5, kickoff_at = ?6, scheduled_local_date = ?7,
                kickoff_time_known = 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1",
            params![
                match_id,
                input.competition_id,
                input.season,
                input.home_team_id,
                input.away_team_id,
                input.kickoff_at,
                input.local_date
            ],
        )?;
        return Ok((match_id, false));
    }

    let exact = connection
        .query_row(
            "SELECT id FROM matches WHERE competition_id = ?1 AND season = ?2
             AND home_team_id = ?3 AND away_team_id = ?4 AND kickoff_at = ?5",
            params![
                input.competition_id,
                input.season,
                input.home_team_id,
                input.away_team_id,
                input.kickoff_at
            ],
            |row| row.get(0),
        )
        .optional()?;
    let (match_id, inserted) = match exact {
        Some(id) => (id, false),
        None => {
            connection.execute(
                "INSERT INTO matches (
                    competition_id, season, home_team_id, away_team_id, kickoff_at,
                    status, scheduled_local_date, kickoff_time_known
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'scheduled', ?6, 1)",
                params![
                    input.competition_id,
                    input.season,
                    input.home_team_id,
                    input.away_team_id,
                    input.kickoff_at,
                    input.local_date
                ],
            )?;
            (connection.last_insert_rowid(), true)
        }
    };
    connection.execute(
        "INSERT INTO provider_match_mappings (match_id, provider, external_match_id)
         VALUES (?1, ?2, ?3)",
        params![match_id, PROVIDER_ID, input.external_event_id],
    )?;
    Ok((match_id, inserted))
}

pub fn insert_odds_if_changed(
    connection: &Connection,
    input: &OddsInput<'_>,
) -> rusqlite::Result<bool> {
    let latest = connection
        .query_row(
            "SELECT odd, alternative_odd FROM odds_snapshots
             WHERE provider = ?1 AND match_id = ?2 AND market_code = ?3
               AND provider_market_id IS ?4 AND provider_line IS ?5
               AND provider_selection_code IS ?6 AND selection = ?7
             ORDER BY captured_at DESC, id DESC LIMIT 1",
            params![
                PROVIDER_ID,
                input.match_id,
                input.market_code,
                input.provider_market_id,
                input.provider_line,
                input.provider_selection_code,
                input.selection
            ],
            |row| Ok((row.get::<_, f64>(0)?, row.get::<_, Option<f64>>(1)?)),
        )
        .optional()?;
    if latest == Some((input.odd, input.alternative_odd)) {
        return Ok(false);
    }
    connection.execute(
        "INSERT INTO odds_snapshots (
            match_id, provider, market_code, market_name, line_value, selection,
            odd, alternative_odd, captured_at, provider_market_id,
            normalized_market_type, provider_selection_code, provider_line,
            normalized_selection
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?4, ?11, ?12, ?13)",
        params![
            input.match_id,
            PROVIDER_ID,
            input.market_code,
            input.normalized_market_type,
            input.line_value,
            input.selection,
            input.odd,
            input.alternative_odd,
            input.captured_at,
            input.provider_market_id,
            input.provider_selection_code,
            input.provider_line,
            input.normalized_selection
        ],
    )?;
    Ok(true)
}

pub fn complete_run(
    connection: &Connection,
    run_id: i64,
    seen: usize,
    inserted: usize,
    updated: usize,
    skipped: usize,
    failed: usize,
) -> rusqlite::Result<()> {
    connection.execute(
        "UPDATE data_import_runs SET completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
            status = 'completed', rows_seen = ?2, rows_inserted = ?3, rows_updated = ?4,
            rows_skipped = ?5, rows_failed = ?6 WHERE id = ?1 AND status = 'running'",
        params![
            run_id,
            seen as i64,
            inserted as i64,
            updated as i64,
            skipped as i64,
            failed as i64
        ],
    )?;
    Ok(())
}

pub fn latest_odds(connection: &Connection, match_id: i64) -> rusqlite::Result<Vec<LatestOdd>> {
    let mut statement = connection.prepare(
        "SELECT match_id, market_code, provider_market_id, normalized_market_type,
                provider_line, line_value, provider_selection_code, selection,
                normalized_selection, odd, alternative_odd, captured_at
         FROM (
             SELECT *, ROW_NUMBER() OVER (
                 PARTITION BY match_id, market_code, COALESCE(provider_market_id, ''),
                    COALESCE(provider_line, ''), COALESCE(provider_selection_code, ''), selection
                 ORDER BY captured_at DESC, id DESC
             ) AS rank
             FROM odds_snapshots WHERE provider = ?1 AND match_id = ?2
         ) WHERE rank = 1
         ORDER BY normalized_market_type, line_value, provider_market_id, provider_selection_code, selection",
    )?;
    let rows = statement
        .query_map(params![PROVIDER_ID, match_id], |row| {
            Ok(LatestOdd {
                match_id: row.get(0)?,
                provider_market_code: row.get(1)?,
                provider_market_id: row.get(2)?,
                normalized_market_type: row.get(3)?,
                provider_line: row.get(4)?,
                line: row.get(5)?,
                provider_selection_code: row.get(6)?,
                selection: row.get(7)?,
                normalized_selection: row.get(8)?,
                odd: row.get(9)?,
                wodd: row.get(10)?,
                captured_at: row.get(11)?,
            })
        })?
        .collect();
    rows
}

pub fn upcoming_matches(connection: &Connection) -> rusqlite::Result<Vec<UpcomingMatch>> {
    let mut statement = connection.prepare(
        "SELECT m.id, pm.external_match_id, c.id, c.name, m.kickoff_at,
                h.id, h.normalized_name, a.id, a.normalized_name,
                COUNT(DISTINCT o.market_code || ':' || COALESCE(o.provider_market_id, '') || ':' || COALESCE(o.provider_line, ''))
         FROM matches m
         JOIN provider_match_mappings pm ON pm.match_id = m.id AND pm.provider = ?1
         JOIN competitions c ON c.id = m.competition_id
         JOIN teams h ON h.id = m.home_team_id
         JOIN teams a ON a.id = m.away_team_id
         LEFT JOIN odds_snapshots o ON o.match_id = m.id AND o.provider = ?1
         WHERE m.status = 'scheduled'
         GROUP BY m.id, pm.external_match_id, c.id, c.name, m.kickoff_at, h.id, h.normalized_name, a.id, a.normalized_name
         ORDER BY m.kickoff_at, m.id",
    )?;
    let rows = statement
        .query_map([PROVIDER_ID], |row| {
            Ok(UpcomingMatch {
                match_id: row.get(0)?,
                provider_event_id: row.get(1)?,
                competition_id: row.get(2)?,
                competition: row.get(3)?,
                kickoff_at: row.get(4)?,
                home_team_id: row.get(5)?,
                home_team: row.get(6)?,
                away_team_id: row.get(7)?,
                away_team: row.get(8)?,
                available_markets: row.get(9)?,
            })
        })?
        .collect();
    rows
}

pub fn bulletin_status(connection: &Connection) -> rusqlite::Result<BulletinStatus> {
    connection.query_row(
        "SELECT
            (SELECT MAX(completed_at) FROM data_import_runs WHERE provider = ?1
                AND dataset_key = 'iddaa:football-bulletin' AND status = 'completed'),
            (SELECT COUNT(DISTINCT m.id) FROM matches m JOIN provider_match_mappings pm
                ON pm.match_id = m.id WHERE pm.provider = ?1 AND m.status = 'scheduled'),
            (SELECT COUNT(DISTINCT m.competition_id) FROM matches m JOIN provider_match_mappings pm
                ON pm.match_id = m.id WHERE pm.provider = ?1 AND m.status = 'scheduled'),
            (SELECT COUNT(DISTINCT team_id) FROM provider_team_mappings WHERE provider = ?1),
            (SELECT MAX(captured_at) FROM odds_snapshots WHERE provider = ?1),
            (SELECT COUNT(*) FROM odds_snapshots WHERE provider = ?1),
            (SELECT COUNT(*) FROM provider_competition_metadata
                WHERE provider = ?1 AND resolution_status = 'unresolved')",
        [PROVIDER_ID],
        |row| {
            Ok(BulletinStatus {
                last_successful_refresh: row.get(0)?,
                upcoming_match_count: row.get(1)?,
                competition_count: row.get(2)?,
                team_count: row.get(3)?,
                latest_odds_snapshot_time: row.get(4)?,
                odds_snapshot_count: row.get(5)?,
                unresolved_competition_count: row.get(6)?,
            })
        },
    )
}
