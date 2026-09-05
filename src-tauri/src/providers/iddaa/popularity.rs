use std::time::Instant;

use chrono::Utc;
use serde::Serialize;

use super::{
    acquisition::download_json,
    markets::normalize_market,
    popularity_dto::{parse_popular_football, RawPopularSelection},
    PROVIDER_ID,
};
use crate::{
    database::Database,
    repositories::{iddaa, ingestion, popularity},
};

pub const POPULAR_BETS_URL: &str =
    "https://sportsbookv2.iddaa.com/SportsBook/get-populer-bets?limit=20";

#[derive(Debug, Clone, Serialize)]
pub struct PopularityIssue {
    pub provider_event_id: Option<i64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PopularityRefreshSummary {
    pub import_run_id: i64,
    pub selections_seen: usize,
    pub selections_imported: usize,
    pub selections_failed: usize,
    pub matched_events: usize,
    pub unmatched_events: usize,
    pub snapshots_inserted: usize,
    pub unchanged_snapshots_skipped: usize,
    pub elapsed_ms: u128,
    pub issues: Vec<PopularityIssue>,
}

pub async fn refresh(database: &Database) -> Result<PopularityRefreshSummary, String> {
    let run_id = start_run(database, POPULAR_BETS_URL)?;
    let bytes = match download_json(POPULAR_BETS_URL).await {
        Ok(bytes) => bytes,
        Err(error) => {
            fail_run(database, run_id, &error.to_string());
            return Err(error.to_string());
        }
    };
    finish(database, run_id, &bytes)
}

#[cfg(test)]
pub fn import_fixture(
    database: &Database,
    bytes: &[u8],
) -> Result<PopularityRefreshSummary, String> {
    let run_id = start_run(database, "fixture://iddaa:popular-bets")?;
    finish(database, run_id, bytes)
}

fn start_run(database: &Database, source: &str) -> Result<i64, String> {
    let connection = database.connection()?;
    ingestion::start_import_run(
        &connection,
        PROVIDER_ID,
        "iddaa:popular-bets",
        "current",
        source,
    )
    .map_err(|error| error.to_string())
}

fn fail_run(database: &Database, run_id: i64, message: &str) {
    if let Ok(connection) = database.connection() {
        let _ = ingestion::fail_import_run(&connection, run_id, message);
    }
}

fn finish(
    database: &Database,
    run_id: i64,
    bytes: &[u8],
) -> Result<PopularityRefreshSummary, String> {
    let started = Instant::now();
    let selections = match parse_popular_football(bytes) {
        Ok(selections) => selections,
        Err(error) => {
            fail_run(database, run_id, &error);
            return Err(error);
        }
    };
    let captured_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut summary = PopularityRefreshSummary {
        import_run_id: run_id,
        selections_seen: selections.len(),
        selections_imported: 0,
        selections_failed: 0,
        matched_events: 0,
        unmatched_events: 0,
        snapshots_inserted: 0,
        unchanged_snapshots_skipped: 0,
        elapsed_ms: 0,
        issues: Vec::new(),
    };

    let result = (|| -> Result<(), String> {
        let mut connection = database.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        for (index, selection) in selections.iter().enumerate() {
            match import_selection(&transaction, selection, index + 1, &captured_at) {
                Ok(outcome) => {
                    summary.selections_imported += 1;
                    summary.matched_events += usize::from(outcome.matched);
                    summary.unmatched_events += usize::from(!outcome.matched);
                    summary.snapshots_inserted += outcome.inserted;
                    summary.unchanged_snapshots_skipped += outcome.unchanged;
                }
                Err(message) => {
                    summary.selections_failed += 1;
                    summary.issues.push(PopularityIssue {
                        provider_event_id: selection.event_id,
                        message,
                    });
                }
            }
        }
        transaction.commit().map_err(|error| error.to_string())?;
        iddaa::complete_run(
            &connection,
            run_id,
            summary.selections_seen,
            summary.selections_imported,
            0,
            0,
            summary.selections_failed,
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

struct ImportOutcome {
    matched: bool,
    inserted: usize,
    unchanged: usize,
}

fn import_selection(
    connection: &rusqlite::Connection,
    raw: &RawPopularSelection,
    rank: usize,
    captured_at: &str,
) -> Result<ImportOutcome, String> {
    if raw.sport_id.is_some_and(|sport| sport != 1) {
        return Err("non-football popularity entry reached football list".to_string());
    }
    let event_id = raw.event_id.ok_or_else(|| "missing eventId".to_string())?;
    let market_id = raw
        .market_id
        .ok_or_else(|| "missing marketId".to_string())?;
    let selection_code = raw
        .outcome_no
        .ok_or_else(|| "missing outcomeNo".to_string())?;
    let count = raw
        .total_played
        .filter(|count| *count >= 0)
        .ok_or_else(|| "missing or invalid verified totalPlayed count".to_string())?;
    let event_id_text = event_id.to_string();
    let market_id_text = market_id.to_string();
    let market_code = raw.market_sub_type.map(|value| value.to_string());
    let selection_code_text = selection_code.to_string();
    let line = raw
        .special_odds_value
        .as_deref()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let line_value = line.and_then(|line| line.parse::<f64>().ok());
    let market_type = normalize_market(raw.market_sub_type);
    let match_id = popularity::match_id_for_event(connection, &event_id_text)
        .map_err(|error| error.to_string())?;

    // Explicitly observed but not persisted as popularity: the public UI does not
    // document the denominator/unit of playedRatio, and odds are not popularity.
    let _deliberately_uninterpreted = (
        raw.played_ratio,
        raw.odd,
        raw.web_odd,
        raw.populer_bet_id,
        raw.event_name.as_deref(),
        raw.event_date.as_deref(),
        raw.competition_name.as_deref(),
        raw.market_name.as_deref(),
    );

    let count_inserted = popularity::insert_if_changed(
        connection,
        &popularity::PopularityInput {
            match_id,
            provider_event_id: &event_id_text,
            provider_market_id: Some(&market_id_text),
            provider_market_code: market_code.as_deref(),
            provider_selection_code: Some(&selection_code_text),
            market_type: Some(market_type.as_str()),
            provider_line: line,
            line_value,
            selection: raw.outcome_name.as_deref(),
            metric_type: "COUNT",
            metric_value: Some(count as f64),
            rank_value: None,
            raw_metric_name: "totalPlayed",
            provider_raw_value: raw.total_played_round_str.as_deref(),
            captured_at,
        },
    )
    .map_err(|error| error.to_string())?;
    let rank_inserted = popularity::insert_if_changed(
        connection,
        &popularity::PopularityInput {
            match_id,
            provider_event_id: &event_id_text,
            provider_market_id: Some(&market_id_text),
            provider_market_code: market_code.as_deref(),
            provider_selection_code: Some(&selection_code_text),
            market_type: Some(market_type.as_str()),
            provider_line: line,
            line_value,
            selection: raw.outcome_name.as_deref(),
            metric_type: "RANK",
            metric_value: None,
            rank_value: Some(rank as i64),
            raw_metric_name: "football_list_position",
            provider_raw_value: None,
            captured_at,
        },
    )
    .map_err(|error| error.to_string())?;
    let inserted = usize::from(count_inserted) + usize::from(rank_inserted);
    Ok(ImportOutcome {
        matched: match_id.is_some(),
        inserted,
        unchanged: 2 - inserted,
    })
}
