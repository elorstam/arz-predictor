use std::collections::HashMap;

use csv::StringRecord;

use super::{
    catalog::DatasetDefinition,
    normalization::{normalize_kickoff, provider_match_key},
    orchestrator::ImportIssue,
    PROVIDER_ID,
};
use crate::repositories::ingestion::ImportedStatistics;

pub struct ParsedRow {
    pub row_number: usize,
    pub source_key: String,
    pub home_team: String,
    pub away_team: String,
    pub kickoff_at: String,
    pub local_date: String,
    pub kickoff_time_known: bool,
    pub final_home_goals: Option<i64>,
    pub final_away_goals: Option<i64>,
    pub halftime_home_goals: Option<i64>,
    pub halftime_away_goals: Option<i64>,
    pub statistics: ImportedStatistics,
}

pub struct ParsedDataset {
    pub rows_seen: usize,
    pub rows_skipped: usize,
    pub rows: Vec<ParsedRow>,
    pub issues: Vec<ImportIssue>,
}

struct Headers(HashMap<String, usize>);

impl Headers {
    fn new(record: &StringRecord) -> Self {
        Self(
            record
                .iter()
                .enumerate()
                .map(|(index, name)| (name.trim().to_string(), index))
                .collect(),
        )
    }

    fn required(&self, name: &str) -> Result<usize, String> {
        self.0
            .get(name)
            .copied()
            .ok_or_else(|| format!("missing required CSV column '{name}'"))
    }

    fn value<'a>(&self, record: &'a StringRecord, name: &str) -> Option<&'a str> {
        self.0
            .get(name)
            .and_then(|index| record.get(*index))
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
}

fn required_value<'a>(
    headers: &Headers,
    record: &'a StringRecord,
    name: &str,
) -> Result<&'a str, String> {
    headers
        .value(record, name)
        .ok_or_else(|| format!("missing required value '{name}'"))
}

fn optional_number(
    headers: &Headers,
    record: &StringRecord,
    name: &str,
) -> Result<Option<i64>, String> {
    match headers.value(record, name) {
        Some(value) => value
            .parse::<i64>()
            .map(Some)
            .map_err(|_| format!("invalid integer '{value}' in column '{name}'")),
        None => Ok(None),
    }
}

pub fn parse_csv(bytes: &[u8], dataset: &DatasetDefinition) -> Result<ParsedDataset, String> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(bytes);
    let headers = Headers::new(
        &reader
            .headers()
            .map_err(|error| format!("failed to read CSV headers: {error}"))?
            .clone(),
    );
    for required in ["Div", "Date", "HomeTeam", "AwayTeam"] {
        headers.required(required)?;
    }

    let mut parsed = ParsedDataset {
        rows_seen: 0,
        rows_skipped: 0,
        rows: Vec::new(),
        issues: Vec::new(),
    };
    for (index, record_result) in reader.records().enumerate() {
        let row_number = index + 2;
        parsed.rows_seen += 1;
        let record = match record_result {
            Ok(record) => record,
            Err(error) => {
                parsed.issues.push(ImportIssue {
                    row: Some(row_number),
                    message: format!("CSV record error: {error}"),
                });
                continue;
            }
        };
        match parse_row(&headers, &record, dataset, row_number) {
            Ok(Some(row)) => parsed.rows.push(row),
            Ok(None) => parsed.rows_skipped += 1,
            Err(message) => parsed.issues.push(ImportIssue {
                row: Some(row_number),
                message,
            }),
        }
    }
    Ok(parsed)
}

fn parse_row(
    headers: &Headers,
    record: &StringRecord,
    dataset: &DatasetDefinition,
    row_number: usize,
) -> Result<Option<ParsedRow>, String> {
    let div = required_value(headers, record, "Div")?;
    if !div.eq_ignore_ascii_case(dataset.league_code) {
        return Ok(None);
    }
    let date = required_value(headers, record, "Date")?;
    let home_team = required_value(headers, record, "HomeTeam")?.to_string();
    let away_team = required_value(headers, record, "AwayTeam")?.to_string();
    let kickoff = normalize_kickoff(date, headers.value(record, "Time"), dataset.timezone)?;

    let final_home_goals = optional_number(headers, record, "FTHG")?;
    let final_away_goals = optional_number(headers, record, "FTAG")?;
    if final_home_goals.is_some() != final_away_goals.is_some() {
        return Err("final score must contain both FTHG and FTAG or neither".to_string());
    }
    let halftime_home_goals = optional_number(headers, record, "HTHG")?;
    let halftime_away_goals = optional_number(headers, record, "HTAG")?;
    if halftime_home_goals.is_some() != halftime_away_goals.is_some() {
        return Err("halftime score must contain both HTHG and HTAG or neither".to_string());
    }
    let _full_time_result = headers.value(record, "FTR");
    let _halftime_result = headers.value(record, "HTR");

    let source_key = provider_match_key(
        PROVIDER_ID,
        dataset.league_code,
        dataset.season,
        &kickoff.local_date,
        &home_team,
        &away_team,
    );
    Ok(Some(ParsedRow {
        row_number,
        source_key,
        home_team,
        away_team,
        kickoff_at: kickoff.utc,
        local_date: kickoff.local_date,
        kickoff_time_known: kickoff.time_known,
        final_home_goals,
        final_away_goals,
        halftime_home_goals,
        halftime_away_goals,
        statistics: ImportedStatistics {
            home_shots: optional_number(headers, record, "HS")?,
            away_shots: optional_number(headers, record, "AS")?,
            home_shots_on_target: optional_number(headers, record, "HST")?,
            away_shots_on_target: optional_number(headers, record, "AST")?,
            home_fouls: optional_number(headers, record, "HF")?,
            away_fouls: optional_number(headers, record, "AF")?,
            home_corners: optional_number(headers, record, "HC")?,
            away_corners: optional_number(headers, record, "AC")?,
            home_yellow_cards: optional_number(headers, record, "HY")?,
            away_yellow_cards: optional_number(headers, record, "AY")?,
            home_red_cards: optional_number(headers, record, "HR")?,
            away_red_cards: optional_number(headers, record, "AR")?,
        },
    }))
}
