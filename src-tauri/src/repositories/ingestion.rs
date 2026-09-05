use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, Default)]
pub struct ImportedStatistics {
    pub home_shots: Option<i64>,
    pub away_shots: Option<i64>,
    pub home_shots_on_target: Option<i64>,
    pub away_shots_on_target: Option<i64>,
    pub home_corners: Option<i64>,
    pub away_corners: Option<i64>,
    pub home_fouls: Option<i64>,
    pub away_fouls: Option<i64>,
    pub home_yellow_cards: Option<i64>,
    pub away_yellow_cards: Option<i64>,
    pub home_red_cards: Option<i64>,
    pub away_red_cards: Option<i64>,
}

impl ImportedStatistics {
    pub fn has_any_value(&self) -> bool {
        [
            self.home_shots,
            self.away_shots,
            self.home_shots_on_target,
            self.away_shots_on_target,
            self.home_corners,
            self.away_corners,
            self.home_fouls,
            self.away_fouls,
            self.home_yellow_cards,
            self.away_yellow_cards,
            self.home_red_cards,
            self.away_red_cards,
        ]
        .iter()
        .any(Option::is_some)
    }
}

pub struct ImportedMatch<'a> {
    pub competition_id: i64,
    pub season: &'a str,
    pub home_team_id: i64,
    pub away_team_id: i64,
    pub kickoff_at: &'a str,
    pub scheduled_local_date: &'a str,
    pub kickoff_time_known: bool,
    pub status: &'a str,
    pub final_home_goals: Option<i64>,
    pub final_away_goals: Option<i64>,
    pub halftime_home_goals: Option<i64>,
    pub halftime_away_goals: Option<i64>,
}

pub struct Resolution {
    pub id: i64,
    pub created: bool,
}

pub struct MatchUpsert {
    pub match_id: i64,
    pub inserted: bool,
}

pub fn start_import_run(
    connection: &Connection,
    provider: &str,
    dataset_key: &str,
    season: &str,
    source_url: &str,
) -> rusqlite::Result<i64> {
    connection.execute(
        "INSERT INTO data_import_runs (provider, dataset_key, season, source_url)
         VALUES (?1, ?2, ?3, ?4)",
        params![provider, dataset_key, season, source_url],
    )?;
    Ok(connection.last_insert_rowid())
}

#[allow(clippy::too_many_arguments)]
pub fn complete_import_run(
    connection: &Connection,
    run_id: i64,
    competition_id: i64,
    rows_seen: usize,
    rows_inserted: usize,
    rows_updated: usize,
    rows_skipped: usize,
    rows_failed: usize,
) -> rusqlite::Result<()> {
    connection.execute(
        "UPDATE data_import_runs
         SET competition_id = ?2, completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
             status = 'completed', rows_seen = ?3, rows_inserted = ?4,
             rows_updated = ?5, rows_skipped = ?6, rows_failed = ?7
         WHERE id = ?1 AND status = 'running'",
        params![
            run_id,
            competition_id,
            rows_seen as i64,
            rows_inserted as i64,
            rows_updated as i64,
            rows_skipped as i64,
            rows_failed as i64
        ],
    )?;
    Ok(())
}

pub fn fail_import_run(
    connection: &Connection,
    run_id: i64,
    error_message: &str,
) -> rusqlite::Result<()> {
    connection.execute(
        "UPDATE data_import_runs
         SET completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), status = 'failed',
             error_message = ?2
         WHERE id = ?1 AND status = 'running'",
        params![run_id, error_message],
    )?;
    Ok(())
}

pub fn resolve_competition(
    connection: &Connection,
    provider: &str,
    external_competition_id: &str,
    name: &str,
    country: &str,
    season: &str,
) -> rusqlite::Result<Resolution> {
    if let Some(id) = connection
        .query_row(
            "SELECT competition_id FROM provider_competition_mappings
             WHERE provider = ?1 AND external_competition_id = ?2",
            params![provider, external_competition_id],
            |row| row.get(0),
        )
        .optional()?
    {
        connection.execute(
            "UPDATE competitions
             SET current_season = CASE
                     WHEN current_season IS NULL OR current_season < ?2 THEN ?2
                     ELSE current_season END,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1",
            params![id, season],
        )?;
        return Ok(Resolution { id, created: false });
    }

    let existing = connection
        .query_row(
            "SELECT id FROM competitions WHERE name = ?1 AND country = ?2",
            params![name, country],
            |row| row.get(0),
        )
        .optional()?;
    let (competition_id, created) = match existing {
        Some(id) => (id, false),
        None => {
            connection.execute(
                "INSERT INTO competitions (name, country, current_season) VALUES (?1, ?2, ?3)",
                params![name, country, season],
            )?;
            (connection.last_insert_rowid(), true)
        }
    };
    connection.execute(
        "INSERT INTO provider_competition_mappings (
            provider, external_competition_id, competition_id
         ) VALUES (?1, ?2, ?3)",
        params![provider, external_competition_id, competition_id],
    )?;
    Ok(Resolution {
        id: competition_id,
        created,
    })
}

pub fn resolve_team(
    connection: &Connection,
    provider: &str,
    source_team_name: &str,
    country: &str,
) -> rusqlite::Result<Resolution> {
    if let Some(id) = connection
        .query_row(
            "SELECT team_id FROM provider_team_mappings
             WHERE provider = ?1 AND external_team_id = ?2",
            params![provider, source_team_name],
            |row| row.get(0),
        )
        .optional()?
    {
        return Ok(Resolution { id, created: false });
    }

    let existing = connection
        .query_row(
            "SELECT id FROM teams WHERE normalized_name = ?1 AND country = ?2",
            params![source_team_name, country],
            |row| row.get(0),
        )
        .optional()?;
    let (team_id, created) = match existing {
        Some(id) => (id, false),
        None => {
            connection.execute(
                "INSERT INTO teams (normalized_name, country) VALUES (?1, ?2)",
                params![source_team_name, country],
            )?;
            (connection.last_insert_rowid(), true)
        }
    };
    connection.execute(
        "INSERT INTO provider_team_mappings (
            team_id, provider, external_team_id, external_team_name
         ) VALUES (?1, ?2, ?3, ?3)",
        params![team_id, provider, source_team_name],
    )?;
    Ok(Resolution {
        id: team_id,
        created,
    })
}

pub fn upsert_match(
    connection: &Connection,
    provider: &str,
    provider_match_key: &str,
    imported: &ImportedMatch<'_>,
) -> rusqlite::Result<MatchUpsert> {
    let existing = connection
        .query_row(
            "SELECT match_id FROM provider_match_mappings
             WHERE provider = ?1 AND external_match_id = ?2",
            params![provider, provider_match_key],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(match_id) = existing {
        connection.execute(
            "UPDATE matches
             SET kickoff_at = CASE
                     WHEN ?7 = 1 OR kickoff_time_known = 0 THEN ?6 ELSE kickoff_at END,
                 scheduled_local_date = COALESCE(?8, scheduled_local_date),
                 kickoff_time_known = MAX(kickoff_time_known, ?7),
                 status = CASE WHEN ?9 = 'finished' THEN 'finished' ELSE status END,
                 final_home_goals = COALESCE(?10, final_home_goals),
                 final_away_goals = COALESCE(?11, final_away_goals),
                 halftime_home_goals = COALESCE(?12, halftime_home_goals),
                 halftime_away_goals = COALESCE(?13, halftime_away_goals),
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1",
            params![
                match_id,
                imported.competition_id,
                imported.season,
                imported.home_team_id,
                imported.away_team_id,
                imported.kickoff_at,
                imported.kickoff_time_known,
                imported.scheduled_local_date,
                imported.status,
                imported.final_home_goals,
                imported.final_away_goals,
                imported.halftime_home_goals,
                imported.halftime_away_goals
            ],
        )?;
        return Ok(MatchUpsert {
            match_id,
            inserted: false,
        });
    }

    connection.execute(
        "INSERT INTO matches (
            competition_id, season, home_team_id, away_team_id, kickoff_at, status,
            final_home_goals, final_away_goals, halftime_home_goals, halftime_away_goals,
            scheduled_local_date, kickoff_time_known
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            imported.competition_id,
            imported.season,
            imported.home_team_id,
            imported.away_team_id,
            imported.kickoff_at,
            imported.status,
            imported.final_home_goals,
            imported.final_away_goals,
            imported.halftime_home_goals,
            imported.halftime_away_goals,
            imported.scheduled_local_date,
            imported.kickoff_time_known
        ],
    )?;
    let match_id = connection.last_insert_rowid();
    connection.execute(
        "INSERT INTO provider_match_mappings (match_id, provider, external_match_id)
         VALUES (?1, ?2, ?3)",
        params![match_id, provider, provider_match_key],
    )?;
    Ok(MatchUpsert {
        match_id,
        inserted: true,
    })
}

pub fn upsert_statistics(
    connection: &Connection,
    match_id: i64,
    statistics: &ImportedStatistics,
) -> rusqlite::Result<Option<bool>> {
    if !statistics.has_any_value() {
        return Ok(None);
    }
    let exists = connection
        .query_row(
            "SELECT 1 FROM match_statistics WHERE match_id = ?1",
            [match_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    connection.execute(
        "INSERT INTO match_statistics (
            match_id, home_shots, away_shots, home_shots_on_target, away_shots_on_target,
            home_corners, away_corners, home_fouls, away_fouls, home_yellow_cards,
            away_yellow_cards, home_red_cards, away_red_cards
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(match_id) DO UPDATE SET
            home_shots = COALESCE(excluded.home_shots, home_shots),
            away_shots = COALESCE(excluded.away_shots, away_shots),
            home_shots_on_target = COALESCE(excluded.home_shots_on_target, home_shots_on_target),
            away_shots_on_target = COALESCE(excluded.away_shots_on_target, away_shots_on_target),
            home_corners = COALESCE(excluded.home_corners, home_corners),
            away_corners = COALESCE(excluded.away_corners, away_corners),
            home_fouls = COALESCE(excluded.home_fouls, home_fouls),
            away_fouls = COALESCE(excluded.away_fouls, away_fouls),
            home_yellow_cards = COALESCE(excluded.home_yellow_cards, home_yellow_cards),
            away_yellow_cards = COALESCE(excluded.away_yellow_cards, away_yellow_cards),
            home_red_cards = COALESCE(excluded.home_red_cards, home_red_cards),
            away_red_cards = COALESCE(excluded.away_red_cards, away_red_cards),
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![
            match_id,
            statistics.home_shots,
            statistics.away_shots,
            statistics.home_shots_on_target,
            statistics.away_shots_on_target,
            statistics.home_corners,
            statistics.away_corners,
            statistics.home_fouls,
            statistics.away_fouls,
            statistics.home_yellow_cards,
            statistics.away_yellow_cards,
            statistics.home_red_cards,
            statistics.away_red_cards
        ],
    )?;
    Ok(Some(!exists))
}
