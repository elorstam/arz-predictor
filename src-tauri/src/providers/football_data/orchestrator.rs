use std::{collections::HashSet, time::Instant};

use serde::Serialize;

use super::{
    acquisition::{download_csv, network_diagnostic, read_local_csv, NetworkDiagnostic},
    cache,
    catalog::DatasetDefinition,
    parser::parse_csv,
    PROVIDER_ID,
};
use crate::{
    database::Database,
    repositories::ingestion::{self, ImportedMatch},
};

#[derive(Debug, Clone, Serialize)]
pub struct ImportIssue {
    pub row: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportSummary {
    pub import_run_id: i64,
    pub dataset: String,
    pub source_url: String,
    pub rows_seen: usize,
    pub matches_inserted: usize,
    pub matches_updated: usize,
    pub competitions_created: usize,
    pub competitions_found: usize,
    pub teams_created: usize,
    pub teams_found: usize,
    pub statistics_inserted: usize,
    pub statistics_updated: usize,
    pub rows_skipped: usize,
    pub rows_failed: usize,
    pub duration_ms: u128,
    pub issues: Vec<ImportIssue>,
}

pub(crate) async fn import_dataset(
    database: &Database,
    dataset: &DatasetDefinition,
) -> Result<ImportSummary, String> {
    let source_url = dataset.source_url();
    let run_id = start_run(database, dataset, &source_url)?;
    let acquired = match download_csv(&source_url).await {
        Ok(acquired) => acquired,
        Err(error) => {
            mark_failed(database, run_id, &error.to_string());
            return Err(error.to_string());
        }
    };
    let app_data_dir = database
        .path()
        .parent()
        .ok_or_else(|| "database path has no application-data parent".to_string())?;
    if let Err(error) = cache::store_validated_bytes(app_data_dir, dataset, &acquired.bytes) {
        mark_failed(database, run_id, &error);
        return Err(error);
    }
    finish_acquired_import(database, dataset, &source_url, run_id, &acquired.bytes)
}

pub(crate) fn import_local_dataset(
    database: &Database,
    dataset: &DatasetDefinition,
    path: &std::path::Path,
) -> Result<ImportSummary, String> {
    let source = format!("file://{}", path.display());
    let run_id = start_run(database, dataset, &source)?;
    let bytes = match read_local_csv(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            mark_failed(database, run_id, &error.to_string());
            return Err(error.to_string());
        }
    };
    finish_acquired_import(database, dataset, &source, run_id, &bytes)
}

pub(crate) fn import_cached_bytes(
    database: &Database,
    dataset: &DatasetDefinition,
    path: &std::path::Path,
    bytes: &[u8],
) -> Result<ImportSummary, String> {
    let source = format!("cache://{} ({})", dataset.key(), path.display());
    let run_id = start_run(database, dataset, &source)?;
    finish_acquired_import(database, dataset, &source, run_id, bytes)
}

pub(crate) async fn diagnose_dataset(dataset: &DatasetDefinition) -> NetworkDiagnostic {
    network_diagnostic(&dataset.source_url()).await
}

#[cfg(test)]
pub fn import_fixture(
    database: &Database,
    dataset: &DatasetDefinition,
    bytes: &[u8],
) -> Result<ImportSummary, String> {
    let source_url = format!("fixture://{}", dataset.key());
    let run_id = start_run(database, dataset, &source_url)?;
    finish_acquired_import(database, dataset, &source_url, run_id, bytes)
}

fn start_run(
    database: &Database,
    dataset: &DatasetDefinition,
    source_url: &str,
) -> Result<i64, String> {
    let connection = database.connection()?;
    ingestion::start_import_run(
        &connection,
        PROVIDER_ID,
        &dataset.key(),
        dataset.season,
        source_url,
    )
    .map_err(|error| error.to_string())
}

fn mark_failed(database: &Database, run_id: i64, message: &str) {
    if let Ok(connection) = database.connection() {
        let _ = ingestion::fail_import_run(&connection, run_id, message);
    }
}

fn finish_acquired_import(
    database: &Database,
    dataset: &DatasetDefinition,
    source_url: &str,
    run_id: i64,
    bytes: &[u8],
) -> Result<ImportSummary, String> {
    let started = Instant::now();
    let parsed = match parse_csv(bytes, dataset) {
        Ok(parsed) => parsed,
        Err(error) => {
            let categorized = format!("malformed_csv: {error}");
            mark_failed(database, run_id, &categorized);
            return Err(categorized);
        }
    };

    let result = (|| -> Result<(ImportSummary, i64), String> {
        let mut connection = database.connection()?;
        let mut transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let competition = ingestion::resolve_competition(
            &transaction,
            PROVIDER_ID,
            dataset.league_code,
            dataset.competition_name,
            dataset.country,
            dataset.season,
        )
        .map_err(|error| error.to_string())?;
        let mut summary = ImportSummary {
            import_run_id: run_id,
            dataset: dataset.key(),
            source_url: source_url.to_string(),
            rows_seen: parsed.rows_seen,
            matches_inserted: 0,
            matches_updated: 0,
            competitions_created: usize::from(competition.created),
            competitions_found: usize::from(!competition.created),
            teams_created: 0,
            teams_found: 0,
            statistics_inserted: 0,
            statistics_updated: 0,
            rows_skipped: parsed.rows_skipped,
            rows_failed: parsed.issues.len(),
            duration_ms: 0,
            issues: parsed.issues,
        };
        let mut resolved_team_names = HashSet::new();

        for row in parsed.rows {
            let savepoint = transaction.savepoint().map_err(|error| error.to_string())?;
            let row_result = (|| -> rusqlite::Result<(bool, bool, bool, Option<bool>)> {
                let home = ingestion::resolve_team(
                    &savepoint,
                    PROVIDER_ID,
                    &row.home_team,
                    dataset.country,
                )?;
                let away = ingestion::resolve_team(
                    &savepoint,
                    PROVIDER_ID,
                    &row.away_team,
                    dataset.country,
                )?;
                let status = if row.final_home_goals.is_some() {
                    "finished"
                } else {
                    "scheduled"
                };
                let saved_match = ingestion::upsert_match(
                    &savepoint,
                    PROVIDER_ID,
                    &row.source_key,
                    &ImportedMatch {
                        competition_id: competition.id,
                        season: dataset.season,
                        home_team_id: home.id,
                        away_team_id: away.id,
                        kickoff_at: &row.kickoff_at,
                        scheduled_local_date: &row.local_date,
                        kickoff_time_known: row.kickoff_time_known,
                        status,
                        final_home_goals: row.final_home_goals,
                        final_away_goals: row.final_away_goals,
                        halftime_home_goals: row.halftime_home_goals,
                        halftime_away_goals: row.halftime_away_goals,
                    },
                )?;
                let statistics = ingestion::upsert_statistics(
                    &savepoint,
                    saved_match.match_id,
                    &row.statistics,
                )?;
                Ok((home.created, away.created, saved_match.inserted, statistics))
            })();

            match row_result {
                Ok((home_created, away_created, match_inserted, statistics)) => {
                    savepoint.commit().map_err(|error| error.to_string())?;
                    for (name, created) in [
                        (row.home_team.as_str(), home_created),
                        (row.away_team.as_str(), away_created),
                    ] {
                        if resolved_team_names.insert(name.to_string()) {
                            if created {
                                summary.teams_created += 1;
                            } else {
                                summary.teams_found += 1;
                            }
                        }
                    }
                    if match_inserted {
                        summary.matches_inserted += 1;
                    } else {
                        summary.matches_updated += 1;
                    }
                    match statistics {
                        Some(true) => summary.statistics_inserted += 1,
                        Some(false) => summary.statistics_updated += 1,
                        None => {}
                    }
                }
                Err(error) => {
                    summary.rows_failed += 1;
                    summary.issues.push(ImportIssue {
                        row: Some(row.row_number),
                        message: format!("database import error: {error}"),
                    });
                }
            }
        }
        transaction.commit().map_err(|error| error.to_string())?;
        summary.duration_ms = started.elapsed().as_millis();
        ingestion::complete_import_run(
            &connection,
            run_id,
            competition.id,
            summary.rows_seen,
            summary.matches_inserted,
            summary.matches_updated,
            summary.rows_skipped,
            summary.rows_failed,
        )
        .map_err(|error| error.to_string())?;
        Ok((summary, competition.id))
    })();

    match result {
        Ok((summary, _)) => Ok(summary),
        Err(error) => {
            mark_failed(database, run_id, &error);
            Err(error)
        }
    }
}
