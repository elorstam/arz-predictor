use rusqlite::{params, Connection, OptionalExtension};

use crate::models::Match;

pub struct NewMatch<'a> {
    pub competition_id: i64,
    pub season: &'a str,
    pub home_team_id: i64,
    pub away_team_id: i64,
    pub kickoff_at: &'a str,
}

pub fn insert(connection: &Connection, new_match: &NewMatch<'_>) -> rusqlite::Result<i64> {
    connection.execute(
        "INSERT INTO matches (
            competition_id, season, home_team_id, away_team_id, kickoff_at
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            new_match.competition_id,
            new_match.season,
            new_match.home_team_id,
            new_match.away_team_id,
            new_match.kickoff_at
        ],
    )?;
    Ok(connection.last_insert_rowid())
}

pub fn find_by_id(connection: &Connection, id: i64) -> rusqlite::Result<Option<Match>> {
    connection
        .query_row(
            "SELECT id, competition_id, season, home_team_id, away_team_id, kickoff_at,
                    status, final_home_goals, final_away_goals, halftime_home_goals,
                    halftime_away_goals, created_at, updated_at, scheduled_local_date,
                    kickoff_time_known
             FROM matches WHERE id = ?1",
            [id],
            |row| {
                Ok(Match {
                    id: row.get(0)?,
                    competition_id: row.get(1)?,
                    season: row.get(2)?,
                    home_team_id: row.get(3)?,
                    away_team_id: row.get(4)?,
                    kickoff_at: row.get(5)?,
                    status: row.get(6)?,
                    final_home_goals: row.get(7)?,
                    final_away_goals: row.get(8)?,
                    halftime_home_goals: row.get(9)?,
                    halftime_away_goals: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                    scheduled_local_date: row.get(13)?,
                    kickoff_time_known: row.get(14)?,
                })
            },
        )
        .optional()
}
