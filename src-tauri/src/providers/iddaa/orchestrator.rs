use std::{collections::HashMap, path::Path, time::Instant};

use chrono::{DateTime, Datelike, Utc};
use serde::Serialize;

use super::{
    acquisition::{download_json, read_local_json, COMPETITIONS_URL, EVENTS_URL},
    dto::{parse_bulletin, parse_competitions, RawCompetition, RawEvent},
    markets::{normalize_market, normalize_selection},
    PROVIDER_ID,
};
use crate::{
    database::Database,
    repositories::{iddaa, ingestion},
};

#[derive(Debug, Clone, Serialize)]
pub struct IngestionIssue {
    pub event_id: Option<i64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefreshSummary {
    pub import_run_id: i64,
    pub source: String,
    pub events_seen: usize,
    pub competitions_resolved: usize,
    pub competitions_unresolved: usize,
    pub teams_created: usize,
    pub matches_inserted: usize,
    pub matches_updated: usize,
    pub markets_seen: usize,
    pub selections_seen: usize,
    pub odds_snapshots_inserted: usize,
    pub unchanged_odds_skipped: usize,
    pub failed_events: usize,
    pub elapsed_ms: u128,
    pub competition_metadata_error: Option<String>,
    pub issues: Vec<IngestionIssue>,
}

pub async fn refresh_bulletin(database: &Database) -> Result<RefreshSummary, String> {
    let competition_run_id = start_dataset_run(database, "iddaa:competitions", COMPETITIONS_URL)?;
    let competition_bytes = download_json(COMPETITIONS_URL).await;
    let (competitions, competition_error) = match competition_bytes {
        Ok(bytes) => match parse_competitions(&bytes) {
            Ok(competitions) => {
                complete_metadata_run(database, competition_run_id, competitions.len());
                (competitions, None)
            }
            Err(error) => {
                fail_run(database, competition_run_id, &error);
                (Vec::new(), Some(error))
            }
        },
        Err(error) => {
            fail_run(database, competition_run_id, &error.to_string());
            (Vec::new(), Some(error.to_string()))
        }
    };
    let run_id = start_run(database, EVENTS_URL)?;
    let bytes = match download_json(EVENTS_URL).await {
        Ok(bytes) => bytes,
        Err(error) => {
            fail_run(database, run_id, &error.to_string());
            return Err(error.to_string());
        }
    };
    finish_import(
        database,
        run_id,
        EVENTS_URL,
        &bytes,
        competitions,
        competition_error,
    )
}

pub fn import_local_bulletin(database: &Database, path: &Path) -> Result<RefreshSummary, String> {
    let source = format!("file://{}", path.display());
    let run_id = start_run(database, &source)?;
    let bytes = match read_local_json(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            fail_run(database, run_id, &error.to_string());
            return Err(error.to_string());
        }
    };
    finish_import(database, run_id, &source, &bytes, Vec::new(), None)
}

#[cfg(test)]
pub fn import_fixture(database: &Database, bytes: &[u8]) -> Result<RefreshSummary, String> {
    let source = "fixture://iddaa:football-bulletin";
    let run_id = start_run(database, source)?;
    finish_import(database, run_id, source, bytes, Vec::new(), None)
}

fn start_run(database: &Database, source: &str) -> Result<i64, String> {
    start_dataset_run(database, "iddaa:football-bulletin", source)
}

fn start_dataset_run(database: &Database, dataset_key: &str, source: &str) -> Result<i64, String> {
    let connection = database.connection()?;
    ingestion::start_import_run(&connection, PROVIDER_ID, dataset_key, "current", source)
        .map_err(|error| error.to_string())
}

fn complete_metadata_run(database: &Database, run_id: i64, rows: usize) {
    if let Ok(connection) = database.connection() {
        let _ = iddaa::complete_run(&connection, run_id, rows, rows, 0, 0, 0);
    }
}

fn fail_run(database: &Database, run_id: i64, error: &str) {
    if let Ok(connection) = database.connection() {
        let _ = ingestion::fail_import_run(&connection, run_id, error);
    }
}

fn finish_import(
    database: &Database,
    run_id: i64,
    source: &str,
    bytes: &[u8],
    separate_competitions: Vec<RawCompetition>,
    competition_metadata_error: Option<String>,
) -> Result<RefreshSummary, String> {
    let started = Instant::now();
    let parsed = match parse_bulletin(bytes) {
        Ok(parsed) => parsed,
        Err(error) => {
            fail_run(database, run_id, &error);
            return Err(error);
        }
    };
    let competition_map: HashMap<i64, RawCompetition> = separate_competitions
        .into_iter()
        .chain(parsed.competitions)
        .filter_map(|competition| competition.i.map(|id| (id, competition)))
        .collect();
    let captured_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut summary = RefreshSummary {
        import_run_id: run_id,
        source: source.to_string(),
        events_seen: parsed.events.len() + parsed.malformed_events.len(),
        competitions_resolved: 0,
        competitions_unresolved: 0,
        teams_created: 0,
        matches_inserted: 0,
        matches_updated: 0,
        markets_seen: 0,
        selections_seen: 0,
        odds_snapshots_inserted: 0,
        unchanged_odds_skipped: 0,
        failed_events: parsed.malformed_events.len(),
        elapsed_ms: 0,
        competition_metadata_error,
        issues: parsed
            .malformed_events
            .into_iter()
            .map(|message| IngestionIssue {
                event_id: None,
                message,
            })
            .collect(),
    };

    let result = (|| -> Result<(), String> {
        let mut connection = database.connection()?;
        let mut transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let mut competition_outcomes: HashMap<i64, bool> = HashMap::new();
        for event in parsed.events {
            let event_id = event.i;
            let savepoint = transaction.savepoint().map_err(|error| error.to_string())?;
            match import_event(
                &savepoint,
                &event,
                competition_map.get(&event.ci.unwrap_or_default()),
                &captured_at,
            ) {
                Ok(EventOutcome::Imported(outcome)) => {
                    savepoint.commit().map_err(|error| error.to_string())?;
                    if let Some(competition_id) = event.ci {
                        competition_outcomes.insert(competition_id, true);
                    }
                    summary.teams_created += outcome.teams_created;
                    summary.matches_inserted += usize::from(outcome.match_inserted);
                    summary.matches_updated += usize::from(!outcome.match_inserted);
                    summary.markets_seen += outcome.markets_seen;
                    summary.selections_seen += outcome.selections_seen;
                    summary.odds_snapshots_inserted += outcome.snapshots_inserted;
                    summary.unchanged_odds_skipped += outcome.unchanged_skipped;
                }
                Ok(EventOutcome::UnresolvedCompetition(external_id)) => {
                    savepoint.commit().map_err(|error| error.to_string())?;
                    competition_outcomes.entry(external_id).or_insert(false);
                    summary.failed_events += 1;
                    summary.issues.push(IngestionIssue {
                        event_id,
                        message: format!(
                            "competition metadata unresolved for external ID {external_id}"
                        ),
                    });
                }
                Err(error) => {
                    summary.failed_events += 1;
                    summary.issues.push(IngestionIssue {
                        event_id,
                        message: error,
                    });
                }
            }
        }
        summary.competitions_resolved = competition_outcomes
            .values()
            .filter(|resolved| **resolved)
            .count();
        summary.competitions_unresolved = competition_outcomes
            .values()
            .filter(|resolved| !**resolved)
            .count();
        transaction.commit().map_err(|error| error.to_string())?;
        iddaa::complete_run(
            &connection,
            run_id,
            summary.events_seen,
            summary.matches_inserted,
            summary.matches_updated,
            summary.events_seen.saturating_sub(
                summary.matches_inserted + summary.matches_updated + summary.failed_events,
            ),
            summary.failed_events,
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        fail_run(database, run_id, &error);
        return Err(error);
    }
    summary.elapsed_ms = started.elapsed().as_millis();
    Ok(summary)
}

struct EventImportOutcome {
    teams_created: usize,
    match_inserted: bool,
    markets_seen: usize,
    selections_seen: usize,
    snapshots_inserted: usize,
    unchanged_skipped: usize,
}

enum EventOutcome {
    Imported(EventImportOutcome),
    UnresolvedCompetition(i64),
}

fn import_event(
    connection: &rusqlite::Connection,
    event: &RawEvent,
    metadata: Option<&RawCompetition>,
    captured_at: &str,
) -> Result<EventOutcome, String> {
    let event_id = event.i.ok_or_else(|| "missing event ID 'i'".to_string())?;
    if event.sid.is_some_and(|sport| sport != 1) {
        return Err(format!("event {event_id} is not football (sid != 1)"));
    }
    let competition_id = event
        .ci
        .ok_or_else(|| format!("event {event_id} missing competition ID 'ci'"))?;
    let home = required_text(event.hn.as_deref(), "home team 'hn'", event_id)?;
    let away = required_text(event.an.as_deref(), "away team 'an'", event_id)?;
    if home == away {
        return Err(format!(
            "event {event_id} has identical home and away teams"
        ));
    }
    let kickoff = normalize_timestamp(
        event
            .d
            .ok_or_else(|| format!("event {event_id} missing kickoff 'd'"))?,
    )?;
    let external_competition_id = competition_id.to_string();
    let competition_input = iddaa::CompetitionInput {
        external_id: &external_competition_id,
        name: metadata.and_then(|value| value.n.as_deref()),
        country: metadata.and_then(|value| value.country.as_deref()),
        season: metadata.and_then(|value| value.season.as_deref()),
    };
    let Some(competition) = iddaa::resolve_competition(connection, &competition_input)
        .map_err(|error| error.to_string())?
    else {
        return Ok(EventOutcome::UnresolvedCompetition(competition_id));
    };
    let country = metadata.and_then(|value| value.country.as_deref());
    let home_team =
        iddaa::resolve_team(connection, home, country).map_err(|error| error.to_string())?;
    let away_team =
        iddaa::resolve_team(connection, away, country).map_err(|error| error.to_string())?;
    let season = metadata
        .and_then(|value| value.season.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| kickoff.year().to_string());
    let kickoff_at = kickoff.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let local_date = kickoff.date_naive().to_string();
    let external_event_id = event_id.to_string();
    let (match_id, match_inserted) = iddaa::upsert_match(
        connection,
        &iddaa::MatchInput {
            external_event_id: &external_event_id,
            competition_id: competition.id,
            season: &season,
            home_team_id: home_team.id,
            away_team_id: away_team.id,
            kickoff_at: &kickoff_at,
            local_date: &local_date,
        },
    )
    .map_err(|error| error.to_string())?;

    let mut outcome = EventImportOutcome {
        teams_created: usize::from(home_team.created) + usize::from(away_team.created),
        match_inserted,
        markets_seen: event.m.len(),
        selections_seen: 0,
        snapshots_inserted: 0,
        unchanged_skipped: 0,
    };
    let _observed_undocumented_fields = (event.mbc, event.k_odd);
    for market in &event.m {
        let market_type = normalize_market(market.st);
        let market_code = market
            .st
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let market_id = market.i.map(|value| value.to_string());
        let line = market
            .sov
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok());
        let _observed_undocumented_market_fields = (market.t, market.mbc);
        for selection in &market.o {
            outcome.selections_seen += 1;
            let Some(name) = selection
                .n
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
            else {
                continue;
            };
            let Some(odd) = selection.odd.filter(|odd| odd.is_finite() && *odd > 0.0) else {
                continue;
            };
            let wodd = selection.wodd.filter(|odd| odd.is_finite() && *odd > 0.0);
            let selection_code = selection.no.map(|value| value.to_string());
            let inserted = iddaa::insert_odds_if_changed(
                connection,
                &iddaa::OddsInput {
                    match_id,
                    market_code: &market_code,
                    provider_market_id: market_id.as_deref(),
                    normalized_market_type: market_type.as_str(),
                    provider_line: market.sov.as_deref(),
                    line_value: line,
                    provider_selection_code: selection_code.as_deref(),
                    selection: name,
                    normalized_selection: normalize_selection(market_type, name),
                    odd,
                    alternative_odd: wodd,
                    captured_at,
                },
            )
            .map_err(|error| error.to_string())?;
            if inserted {
                outcome.snapshots_inserted += 1;
            } else {
                outcome.unchanged_skipped += 1;
            }
        }
    }
    Ok(EventOutcome::Imported(outcome))
}

fn required_text<'a>(
    value: Option<&'a str>,
    field: &str,
    event_id: i64,
) -> Result<&'a str, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("event {event_id} missing {field}"))
}

pub(crate) fn normalize_timestamp(value: i64) -> Result<DateTime<Utc>, String> {
    let seconds = if value.abs() >= 100_000_000_000 {
        value / 1000
    } else {
        value
    };
    let timestamp = DateTime::from_timestamp(seconds, 0)
        .ok_or_else(|| format!("invalid Unix kickoff timestamp: {value}"))?;
    if !(2000..=2100).contains(&timestamp.year()) {
        return Err(format!(
            "kickoff timestamp outside supported range: {value}"
        ));
    }
    Ok(timestamp)
}
