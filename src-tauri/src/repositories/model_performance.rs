//! MODEL PERFORMANSI: read-only probability-quality analytics.
use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Europe::Istanbul;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{calibration::MIN_PERFORMANCE_GROUP_SAMPLE, performance::binary_quality_metrics};

const SUPPORTED_MARKETS: [&str; 8] = [
    "MATCH_RESULT",
    "BTTS",
    "TOTAL_GOALS",
    "TEAM_TOTAL_GOALS",
    "HOME_TEAM_TOTAL_GOALS",
    "AWAY_TEAM_TOTAL_GOALS",
    "FULL_TIME_TOTAL_CORNERS",
    "FULL_TIME_TOTAL_CARDS",
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PerformanceWindow {
    #[serde(rename = "LAST_7_DAYS")]
    Last7Days,
    #[serde(rename = "LAST_30_DAYS")]
    Last30Days,
    #[serde(rename = "SEASON")]
    Season,
    #[serde(rename = "ALL_TIME")]
    AllTime,
}
impl Default for PerformanceWindow {
    fn default() -> Self {
        Self::Last30Days
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PredictionContext {
    Base,
    LineupAware,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReliabilityStatus {
    Sufficient,
    LowSample,
    InsufficientData,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MetricType {
    Binary,
    Multiclass,
    BinarySelection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelPerformanceRequest {
    #[serde(default)]
    pub window: PerformanceWindow,
    pub market: Option<String>,
    pub competition_id: Option<i64>,
    pub prediction_context: Option<PredictionContext>,
    pub model_version: Option<String>,
    pub calibration_version: Option<String>,
    #[serde(default)]
    pub candidate_only: bool,
    /// Deterministic clock override for tests/exported reports.
    pub as_of: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelPerformanceFilters {
    pub market: Option<String>,
    pub competition_id: Option<i64>,
    pub prediction_context: Option<PredictionContext>,
    pub model_version: Option<String>,
    pub calibration_version: Option<String>,
    pub candidate_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricSummary {
    pub prediction_context: PredictionContext,
    pub metric_type: MetricType,
    pub market: Option<String>,
    pub competition_id: Option<i64>,
    pub competition_name: Option<String>,
    pub category: Option<String>,
    pub model_version: String,
    pub calibration_version: Option<String>,
    pub lineup_model_version: Option<String>,
    pub sample_count: usize,
    pub status: ReliabilityStatus,
    pub average_predicted_probability: Option<f64>,
    pub actual_hit_rate: Option<f64>,
    pub accuracy: Option<f64>,
    pub brier_score: Option<f64>,
    pub log_loss: Option<f64>,
    pub calibration_error_ece: Option<f64>,
    pub calibration_gap: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfidenceBucket {
    pub prediction_context: PredictionContext,
    pub market: String,
    pub model_version: String,
    pub calibration_version: Option<String>,
    pub lineup_model_version: Option<String>,
    pub bucket: String,
    pub sample_count: usize,
    pub status: ReliabilityStatus,
    pub average_predicted_probability: f64,
    pub actual_hit_rate: f64,
    pub calibration_gap: f64,
    pub brier_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OddsRangeSummary {
    pub prediction_context: PredictionContext,
    pub market: String,
    pub model_version: String,
    pub calibration_version: Option<String>,
    pub lineup_model_version: Option<String>,
    pub odds_range: String,
    pub sample_count: usize,
    pub status: ReliabilityStatus,
    pub average_predicted_probability: f64,
    pub actual_hit_rate: f64,
    pub brier_score: f64,
    pub log_loss: f64,
    pub calibration_error_ece: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextComparison {
    pub market: String,
    pub metric_type: MetricType,
    pub paired_sample_count: usize,
    pub status: ReliabilityStatus,
    pub base: MetricSummary,
    pub lineup_aware: MetricSummary,
    pub brier_delta_lineup_minus_base: Option<f64>,
    pub log_loss_delta_lineup_minus_base: Option<f64>,
    pub hit_rate_or_accuracy_delta_lineup_minus_base: Option<f64>,
    pub ece_delta_lineup_minus_base: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelPerformanceResponse {
    pub window: PerformanceWindow,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub prediction_context: Option<PredictionContext>,
    pub filters: ModelPerformanceFilters,
    pub sample_count: usize,
    pub status: ReliabilityStatus,
    pub overall: Vec<MetricSummary>,
    pub by_market: Vec<MetricSummary>,
    pub confidence_buckets: Vec<ConfidenceBucket>,
    pub by_competition: Vec<MetricSummary>,
    pub by_odds_range: Vec<OddsRangeSummary>,
    pub by_model_version: Vec<MetricSummary>,
    pub candidate_policy_summary: Vec<MetricSummary>,
    pub base_vs_lineup: Vec<ContextComparison>,
}

#[derive(Debug, Clone)]
struct Observation {
    match_id: i64,
    date: NaiveDate,
    season_current: bool,
    competition_id: i64,
    competition_name: String,
    market: String,
    selection: String,
    probability: f64,
    actual: bool,
    context: PredictionContext,
    model_version: String,
    calibration_version: Option<String>,
    lineup_model_version: Option<String>,
    odds: Option<f64>,
    category: Option<String>,
}

#[derive(Debug, Clone)]
struct Evaluation {
    match_id: i64,
    context: PredictionContext,
    metric_type: MetricType,
    market: String,
    competition_id: i64,
    competition_name: String,
    model_version: String,
    calibration_version: Option<String>,
    lineup_model_version: Option<String>,
    category: Option<String>,
    probability: f64,
    actual: bool,
    brier: f64,
    log_loss: f64,
    odds: Option<f64>,
}

fn normalized_market(market: &str) -> &str {
    match market {
        "HOME_TEAM_TOTAL_GOALS" | "AWAY_TEAM_TOTAL_GOALS" => "TEAM_TOTAL_GOALS",
        value => value,
    }
}

fn valid_probability(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn local_date(timestamp: &str) -> Result<NaiveDate, String> {
    DateTime::parse_from_rfc3339(timestamp)
        .map(|value| value.with_timezone(&Istanbul).date_naive())
        .map_err(|_| format!("INVALID_TIMESTAMP:{timestamp}"))
}

fn label(
    market: &str,
    selection: &str,
    line: Option<f64>,
    home_goals: Option<i64>,
    away_goals: Option<i64>,
    home_corners: Option<i64>,
    away_corners: Option<i64>,
    home_cards: Option<i64>,
    away_cards: Option<i64>,
) -> Option<bool> {
    let over = |count: i64| match (selection, line) {
        ("OVER", Some(line)) => Some(count as f64 > line),
        ("UNDER", Some(line)) => Some((count as f64) < line),
        _ => None,
    };
    match market {
        "MATCH_RESULT" => home_goals.zip(away_goals).and_then(|(home, away)| {
            let result = if home > away {
                "HOME"
            } else if home == away {
                "DRAW"
            } else {
                "AWAY"
            };
            matches!(selection, "HOME" | "DRAW" | "AWAY").then_some(selection == result)
        }),
        "BTTS" => home_goals
            .zip(away_goals)
            .and_then(|(home, away)| match selection {
                "YES" => Some(home > 0 && away > 0),
                "NO" => Some(!(home > 0 && away > 0)),
                _ => None,
            }),
        "TOTAL_GOALS" => home_goals
            .zip(away_goals)
            .and_then(|(home, away)| over(home + away)),
        "HOME_TEAM_TOTAL_GOALS" => home_goals.and_then(over),
        "AWAY_TEAM_TOTAL_GOALS" => away_goals.and_then(over),
        "FULL_TIME_TOTAL_CORNERS" => home_corners
            .zip(away_corners)
            .and_then(|(home, away)| over(home + away)),
        "FULL_TIME_TOTAL_CARDS" => home_cards
            .zip(away_cards)
            .and_then(|(home, away)| over(home + away)),
        _ => None,
    }
}

fn historical_odd(
    c: &Connection,
    match_id: i64,
    cutoff: &str,
    market: &str,
    selection: &str,
    line: Option<f64>,
) -> Result<Option<f64>, String> {
    c.query_row(
        "SELECT odd FROM odds_snapshots WHERE match_id=?1 AND captured_at<=?2
         AND UPPER(COALESCE(normalized_market_type,market_name))=UPPER(?3)
         AND UPPER(COALESCE(normalized_selection,selection))=UPPER(?4)
         AND (line_value IS ?5 OR line_value=?5)
         ORDER BY captured_at DESC,id DESC LIMIT 1",
        params![match_id, cutoff, market, selection, line],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn load_base(c: &Connection) -> Result<Vec<Observation>, String> {
    let mut statement = c
        .prepare(
            "SELECT b.match_id,m.kickoff_at,m.season=competition.current_season,
                    m.competition_id,competition.name,b.market,b.selection,b.line_value,
                    b.public_probability,b.actual_result,b.generated_at,
                    runs.model_family_version,calibration.calibration_version,
                    m.final_home_goals,m.final_away_goals,stats.home_corners,
                    stats.away_corners,stats.home_yellow_cards,stats.away_yellow_cards
             FROM backtest_calibration_predictions b
             JOIN backtest_runs runs ON runs.id=b.run_id
             JOIN calibration_models calibration ON calibration.id=b.calibration_model_id
             JOIN matches m ON m.id=b.match_id
             JOIN competitions competition ON competition.id=m.competition_id
             LEFT JOIN match_statistics stats ON stats.match_id=m.id
             WHERE runs.status='COMPLETED' AND b.settlement IN ('WON','LOST')
               AND m.status='finished' AND m.final_home_goals IS NOT NULL
               AND m.final_away_goals IS NOT NULL
             ORDER BY m.kickoff_at,b.match_id,b.market,b.selection,b.id",
        )
        .map_err(|e| e.to_string())?;
    let raw = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<f64>>(7)?,
                row.get::<_, f64>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, Option<i64>>(13)?,
                row.get::<_, Option<i64>>(14)?,
                row.get::<_, Option<i64>>(15)?,
                row.get::<_, Option<i64>>(16)?,
                row.get::<_, Option<i64>>(17)?,
                row.get::<_, Option<i64>>(18)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (
        match_id,
        kickoff,
        season_current,
        competition_id,
        competition_name,
        source_market,
        selection,
        line,
        probability,
        persisted_actual,
        generated_at,
        model_version,
        calibration_version,
        hg,
        ag,
        hc,
        ac,
        hy,
        ay,
    ) in raw
    {
        if !SUPPORTED_MARKETS.contains(&source_market.as_str()) || !valid_probability(probability) {
            continue;
        }
        let Some(actual) = label(&source_market, &selection, line, hg, ag, hc, ac, hy, ay) else {
            continue;
        };
        if (persisted_actual == "true") != actual {
            continue;
        }
        let odds = historical_odd(c, match_id, &generated_at, &source_market, &selection, line)?;
        out.push(Observation {
            match_id,
            date: local_date(&kickoff)?,
            season_current,
            competition_id,
            competition_name,
            market: normalized_market(&source_market).into(),
            selection,
            probability,
            actual,
            context: PredictionContext::Base,
            model_version,
            calibration_version: Some(calibration_version),
            lineup_model_version: None,
            odds,
            category: None,
        });
    }
    Ok(out)
}

#[allow(clippy::type_complexity)]
fn revision_rows(
    c: &Connection,
) -> Result<
    Vec<(
        i64,
        String,
        bool,
        i64,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    )>,
    String,
> {
    let mut statement = c
        .prepare(
            "WITH ranked AS (
           SELECT revisions.*,ROW_NUMBER() OVER (
             PARTITION BY revisions.match_id ORDER BY revisions.generated_at DESC,revisions.id DESC
           ) row_number
           FROM prediction_revisions revisions
           JOIN matches candidate_match ON candidate_match.id=revisions.match_id
           WHERE revisions.revision_type='LINEUP_AWARE_PREMATCH'
             AND revisions.revision_status='REVISION_AVAILABLE'
             AND revisions.payload_schema=?1 AND revisions.market_payload_json IS NOT NULL
             AND datetime(revisions.generated_at)<=datetime(candidate_match.kickoff_at)
         )
         SELECT ranked.match_id,m.kickoff_at,m.season=competition.current_season,
                m.competition_id,competition.name,ranked.generated_at,
                COALESCE(ranked.base_model_version,ranked.model_version),
                COALESCE(ranked.base_calibration_version,ranked.calibration_version),
                ranked.lineup_model_version,ranked.market_payload_json,
                m.final_home_goals,m.final_away_goals,stats.home_corners,stats.away_corners,
                stats.home_yellow_cards,stats.away_yellow_cards
         FROM ranked JOIN matches m ON m.id=ranked.match_id
         JOIN competitions competition ON competition.id=m.competition_id
         LEFT JOIN match_statistics stats ON stats.match_id=m.id
         WHERE ranked.row_number=1 AND m.status='finished'
           AND m.final_home_goals IS NOT NULL AND m.final_away_goals IS NOT NULL
         ORDER BY m.kickoff_at,m.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([super::lineup_model::REVISION_PAYLOAD_SCHEMA], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get(10)?,
                row.get(11)?,
                row.get(12)?,
                row.get(13)?,
                row.get(14)?,
                row.get(15)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

fn load_lineup(c: &Connection, include_base_pair: bool) -> Result<Vec<Observation>, String> {
    let mut out = Vec::new();
    for (
        match_id,
        kickoff,
        season_current,
        competition_id,
        competition_name,
        generated_at,
        model_version,
        calibration_version,
        lineup_model_version,
        json,
        hg,
        ag,
        hc,
        ac,
        hy,
        ay,
    ) in revision_rows(c)?
    {
        let date = local_date(&kickoff)?;
        let document: super::lineup_model::RevisionMarketPayloadDocument =
            serde_json::from_str(&json).map_err(|e| e.to_string())?;
        for payload in document.markets {
            if payload.family_status != "ACTIVE"
                || !SUPPORTED_MARKETS.contains(&payload.market.as_str())
            {
                continue;
            }
            let Some(actual) = label(
                &payload.market,
                &payload.selection,
                payload.line,
                hg,
                ag,
                hc,
                ac,
                hy,
                ay,
            ) else {
                continue;
            };
            let odds = historical_odd(
                c,
                match_id,
                &generated_at,
                &payload.market,
                &payload.selection,
                payload.line,
            )?;
            let mut push = |probability: Option<f64>, context: PredictionContext| {
                if let Some(probability) = probability.filter(|value| valid_probability(*value)) {
                    out.push(Observation {
                        match_id,
                        date,
                        season_current,
                        competition_id,
                        competition_name: competition_name.clone(),
                        market: normalized_market(&payload.market).into(),
                        selection: payload.selection.clone(),
                        probability,
                        actual,
                        context,
                        model_version: model_version.clone(),
                        calibration_version: calibration_version.clone(),
                        lineup_model_version: (include_base_pair
                            || context == PredictionContext::LineupAware)
                            .then_some(lineup_model_version.clone())
                            .flatten(),
                        odds,
                        category: None,
                    });
                }
            };
            if include_base_pair {
                push(payload.base_public_probability, PredictionContext::Base);
            }
            push(
                payload.final_public_probability,
                PredictionContext::LineupAware,
            );
        }
    }
    Ok(out)
}

fn load_candidates(c: &Connection) -> Result<Vec<Observation>, String> {
    let mut statement = c
        .prepare(
            "SELECT candidates.match_id,m.kickoff_at,m.season=competition.current_season,
                m.competition_id,competition.name,candidates.market,candidates.selection,
                candidates.line_value,COALESCE(candidates.final_public_probability,
                candidates.public_probability),runs.model_version,
                COALESCE(candidates.calibration_version,runs.calibration_version),
                candidates.lineup_model_version,candidates.prediction_source,candidates.iddaa_odd,
                candidates.category,m.final_home_goals,m.final_away_goals,stats.home_corners,
                stats.away_corners,stats.home_yellow_cards,stats.away_yellow_cards
         FROM candidate_engine_candidates candidates
         JOIN candidate_engine_runs runs ON runs.id=candidates.run_id
         JOIN matches m ON m.id=candidates.match_id
         JOIN competitions competition ON competition.id=m.competition_id
         LEFT JOIN match_statistics stats ON stats.match_id=m.id
         WHERE runs.status='COMPLETED' AND candidates.qualification_state='QUALIFIED'
           AND m.status='finished' AND m.final_home_goals IS NOT NULL
           AND m.final_away_goals IS NOT NULL
         ORDER BY m.kickoff_at,candidates.match_id,candidates.id",
        )
        .map_err(|e| e.to_string())?;
    let raw = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<f64>>(7)?,
                row.get::<_, f64>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, f64>(13)?,
                row.get::<_, String>(14)?,
                row.get::<_, Option<i64>>(15)?,
                row.get::<_, Option<i64>>(16)?,
                row.get::<_, Option<i64>>(17)?,
                row.get::<_, Option<i64>>(18)?,
                row.get::<_, Option<i64>>(19)?,
                row.get::<_, Option<i64>>(20)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (
        match_id,
        kickoff,
        season_current,
        competition_id,
        competition_name,
        source_market,
        selection,
        line,
        probability,
        model_version,
        calibration_version,
        lineup_model_version,
        source,
        odds,
        category,
        hg,
        ag,
        hc,
        ac,
        hy,
        ay,
    ) in raw
    {
        if !SUPPORTED_MARKETS.contains(&source_market.as_str()) || !valid_probability(probability) {
            continue;
        }
        let Some(actual) = label(&source_market, &selection, line, hg, ag, hc, ac, hy, ay) else {
            continue;
        };
        let context = if source.as_deref() == Some("LINEUP_AWARE") {
            PredictionContext::LineupAware
        } else {
            PredictionContext::Base
        };
        out.push(Observation {
            match_id,
            date: local_date(&kickoff)?,
            season_current,
            competition_id,
            competition_name,
            market: normalized_market(&source_market).into(),
            selection,
            probability,
            actual,
            context,
            model_version,
            calibration_version,
            lineup_model_version: (context == PredictionContext::LineupAware)
                .then_some(lineup_model_version)
                .flatten(),
            odds: Some(odds),
            category: Some(category),
        });
    }
    Ok(out)
}

fn as_of_date(request: &ModelPerformanceRequest) -> Result<NaiveDate, String> {
    match request.as_of.as_deref() {
        Some(value) if value.len() == 10 => {
            NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| "INVALID_AS_OF".into())
        }
        Some(value) => local_date(value).map_err(|_| "INVALID_AS_OF".into()),
        None => Ok(Utc::now().with_timezone(&Istanbul).date_naive()),
    }
}

fn filtered(
    rows: Vec<Observation>,
    request: &ModelPerformanceRequest,
) -> Result<(Vec<Observation>, Option<NaiveDate>, Option<NaiveDate>), String> {
    let as_of = as_of_date(request)?;
    let from = match request.window {
        PerformanceWindow::Last7Days => Some(as_of - Duration::days(6)),
        PerformanceWindow::Last30Days => Some(as_of - Duration::days(29)),
        PerformanceWindow::Season | PerformanceWindow::AllTime => None,
    };
    let requested_market = request.market.as_deref().map(normalized_market);
    let mut rows = rows
        .into_iter()
        .filter(|row| {
            row.date <= as_of
                && from.is_none_or(|from| row.date >= from)
                && (request.window != PerformanceWindow::Season || row.season_current)
                && requested_market.is_none_or(|market| row.market == market)
                && request
                    .competition_id
                    .is_none_or(|competition| row.competition_id == competition)
                && request
                    .prediction_context
                    .is_none_or(|context| row.context == context)
                && request
                    .model_version
                    .as_deref()
                    .is_none_or(|version| row.model_version == version)
                && request
                    .calibration_version
                    .as_deref()
                    .is_none_or(|version| row.calibration_version.as_deref() == Some(version))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then_with(|| a.match_id.cmp(&b.match_id))
            .then_with(|| a.market.cmp(&b.market))
            .then_with(|| a.selection.cmp(&b.selection))
    });
    let selected_from = from.or_else(|| rows.first().map(|row| row.date));
    let selected_to = match request.window {
        PerformanceWindow::Last7Days | PerformanceWindow::Last30Days => Some(as_of),
        _ => rows.last().map(|row| row.date),
    };
    Ok((rows, selected_from, selected_to))
}

fn evaluations(rows: &[Observation], candidate_only: bool) -> Vec<Evaluation> {
    let mut output = Vec::new();
    let mut result_groups: BTreeMap<
        (
            i64,
            PredictionContext,
            String,
            Option<String>,
            Option<String>,
            i64,
            Option<String>,
        ),
        Vec<&Observation>,
    > = BTreeMap::new();
    for row in rows {
        if row.market == "MATCH_RESULT" && !candidate_only {
            result_groups
                .entry((
                    row.match_id,
                    row.context,
                    row.model_version.clone(),
                    row.calibration_version.clone(),
                    row.lineup_model_version.clone(),
                    row.competition_id,
                    row.category.clone(),
                ))
                .or_default()
                .push(row);
            continue;
        }
        let y = if row.actual { 1.0 } else { 0.0 };
        output.push(Evaluation {
            match_id: row.match_id,
            context: row.context,
            metric_type: if candidate_only {
                MetricType::BinarySelection
            } else {
                MetricType::Binary
            },
            market: row.market.clone(),
            competition_id: row.competition_id,
            competition_name: row.competition_name.clone(),
            model_version: row.model_version.clone(),
            calibration_version: row.calibration_version.clone(),
            lineup_model_version: row.lineup_model_version.clone(),
            category: row.category.clone(),
            probability: row.probability,
            actual: row.actual,
            brier: (row.probability - y).powi(2),
            log_loss: -y * row.probability.max(1e-12).ln()
                - (1.0 - y) * (1.0 - row.probability).max(1e-12).ln(),
            odds: row.odds,
        });
    }
    for (
        (
            match_id,
            context,
            model_version,
            calibration_version,
            lineup_model_version,
            competition_id,
            category,
        ),
        group,
    ) in result_groups
    {
        let by_selection = group
            .iter()
            .filter(|row| matches!(row.selection.as_str(), "HOME" | "DRAW" | "AWAY"))
            .map(|row| (row.selection.as_str(), *row))
            .collect::<BTreeMap<_, _>>();
        if by_selection.len() != 3 || by_selection.values().filter(|row| row.actual).count() != 1 {
            continue;
        }
        let actual = by_selection.values().find(|row| row.actual).unwrap();
        let pick = by_selection
            .values()
            .max_by(|a, b| a.probability.total_cmp(&b.probability))
            .unwrap();
        let brier = by_selection
            .values()
            .map(|row| (row.probability - if row.actual { 1.0 } else { 0.0 }).powi(2))
            .sum();
        output.push(Evaluation {
            match_id,
            context,
            metric_type: MetricType::Multiclass,
            market: "MATCH_RESULT".into(),
            competition_id,
            competition_name: actual.competition_name.clone(),
            model_version,
            calibration_version,
            lineup_model_version,
            category,
            probability: pick.probability,
            actual: pick.selection == actual.selection,
            brier,
            log_loss: -actual.probability.max(1e-12).ln(),
            odds: pick.odds,
        });
    }
    output.sort_by(|a, b| {
        a.context
            .cmp(&b.context)
            .then_with(|| a.market.cmp(&b.market))
            .then_with(|| a.match_id.cmp(&b.match_id))
    });
    output
}

fn reliability(sample_count: usize) -> ReliabilityStatus {
    if sample_count == 0 {
        ReliabilityStatus::InsufficientData
    } else if sample_count < MIN_PERFORMANCE_GROUP_SAMPLE {
        ReliabilityStatus::LowSample
    } else {
        ReliabilityStatus::Sufficient
    }
}

#[derive(Clone)]
struct SummaryIdentity {
    context: PredictionContext,
    metric_type: MetricType,
    market: Option<String>,
    competition_id: Option<i64>,
    competition_name: Option<String>,
    category: Option<String>,
    model_version: String,
    calibration_version: Option<String>,
    lineup_model_version: Option<String>,
}

fn summarize(rows: &[&Evaluation], identity: SummaryIdentity) -> MetricSummary {
    let sample_count = rows.len();
    let status = reliability(sample_count);
    let quality = binary_quality_metrics(
        &rows
            .iter()
            .map(|row| (row.probability, row.actual))
            .collect::<Vec<_>>(),
    );
    let brier = (sample_count > 0)
        .then(|| rows.iter().map(|row| row.brier).sum::<f64>() / sample_count as f64);
    let log_loss = (sample_count > 0)
        .then(|| rows.iter().map(|row| row.log_loss).sum::<f64>() / sample_count as f64);
    MetricSummary {
        prediction_context: identity.context,
        metric_type: identity.metric_type,
        market: identity.market,
        competition_id: identity.competition_id,
        competition_name: identity.competition_name,
        category: identity.category,
        model_version: identity.model_version,
        calibration_version: identity.calibration_version,
        lineup_model_version: identity.lineup_model_version,
        sample_count,
        status,
        average_predicted_probability: quality.map(|value| value.average_probability),
        actual_hit_rate: (identity.metric_type != MetricType::Multiclass)
            .then(|| quality.map(|value| value.actual_rate))
            .flatten(),
        accuracy: (identity.metric_type == MetricType::Multiclass)
            .then(|| quality.map(|value| value.actual_rate))
            .flatten(),
        brier_score: brier,
        log_loss,
        calibration_error_ece: (status == ReliabilityStatus::Sufficient)
            .then(|| quality.map(|value| value.ece))
            .flatten(),
        calibration_gap: quality.map(|value| value.actual_rate - value.average_probability),
    }
}

type GroupKey = (
    PredictionContext,
    MetricType,
    String,
    Option<String>,
    Option<String>,
);

fn identity(key: &GroupKey) -> SummaryIdentity {
    SummaryIdentity {
        context: key.0,
        metric_type: key.1,
        market: None,
        competition_id: None,
        competition_name: None,
        category: None,
        model_version: key.2.clone(),
        calibration_version: key.3.clone(),
        lineup_model_version: key.4.clone(),
    }
}

fn overall(evaluations: &[Evaluation]) -> Vec<MetricSummary> {
    let mut groups: BTreeMap<GroupKey, Vec<&Evaluation>> = BTreeMap::new();
    for row in evaluations {
        groups
            .entry((
                row.context,
                row.metric_type,
                row.model_version.clone(),
                row.calibration_version.clone(),
                row.lineup_model_version.clone(),
            ))
            .or_default()
            .push(row);
    }
    groups
        .into_iter()
        .map(|(key, rows)| summarize(&rows, identity(&key)))
        .collect()
}

fn by_market(evaluations: &[Evaluation]) -> Vec<MetricSummary> {
    let mut groups: BTreeMap<(GroupKey, String), Vec<&Evaluation>> = BTreeMap::new();
    for row in evaluations {
        groups
            .entry((
                (
                    row.context,
                    row.metric_type,
                    row.model_version.clone(),
                    row.calibration_version.clone(),
                    row.lineup_model_version.clone(),
                ),
                row.market.clone(),
            ))
            .or_default()
            .push(row);
    }
    groups
        .into_iter()
        .map(|((key, market), rows)| {
            let mut id = identity(&key);
            id.market = Some(market);
            summarize(&rows, id)
        })
        .collect()
}

fn by_competition(evaluations: &[Evaluation]) -> Vec<MetricSummary> {
    let mut groups: BTreeMap<(GroupKey, String, i64, String), Vec<&Evaluation>> = BTreeMap::new();
    for row in evaluations {
        groups
            .entry((
                (
                    row.context,
                    row.metric_type,
                    row.model_version.clone(),
                    row.calibration_version.clone(),
                    row.lineup_model_version.clone(),
                ),
                row.market.clone(),
                row.competition_id,
                row.competition_name.clone(),
            ))
            .or_default()
            .push(row);
    }
    groups
        .into_iter()
        .map(|((key, market, competition_id, name), rows)| {
            let mut id = identity(&key);
            id.market = Some(market);
            id.competition_id = Some(competition_id);
            id.competition_name = Some(name);
            summarize(&rows, id)
        })
        .collect()
}

fn by_category(evaluations: &[Evaluation]) -> Vec<MetricSummary> {
    let mut groups: BTreeMap<(GroupKey, String, String), Vec<&Evaluation>> = BTreeMap::new();
    for row in evaluations.iter().filter(|row| row.category.is_some()) {
        groups
            .entry((
                (
                    row.context,
                    row.metric_type,
                    row.model_version.clone(),
                    row.calibration_version.clone(),
                    row.lineup_model_version.clone(),
                ),
                row.market.clone(),
                row.category.clone().unwrap(),
            ))
            .or_default()
            .push(row);
    }
    groups
        .into_iter()
        .map(|((key, market, category), rows)| {
            let mut id = identity(&key);
            id.market = Some(market);
            id.category = Some(category);
            summarize(&rows, id)
        })
        .collect()
}

fn confidence_bucket(probability: f64) -> &'static str {
    if probability < 0.50 {
        "BELOW_50"
    } else if probability < 0.60 {
        "50_59"
    } else if probability < 0.70 {
        "60_69"
    } else if probability < 0.80 {
        "70_79"
    } else if probability < 0.90 {
        "80_89"
    } else {
        "90_100"
    }
}

fn confidence_buckets(evaluations: &[Evaluation]) -> Vec<ConfidenceBucket> {
    let mut groups: BTreeMap<
        (
            PredictionContext,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
        ),
        Vec<&Evaluation>,
    > = BTreeMap::new();
    for row in evaluations {
        groups
            .entry((
                row.context,
                row.market.clone(),
                row.model_version.clone(),
                row.calibration_version.clone(),
                row.lineup_model_version.clone(),
                confidence_bucket(row.probability).into(),
            ))
            .or_default()
            .push(row);
    }
    groups
        .into_iter()
        .map(|((context, market, model, cal, lineup, bucket), rows)| {
            let quality = binary_quality_metrics(
                &rows
                    .iter()
                    .map(|row| (row.probability, row.actual))
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            ConfidenceBucket {
                prediction_context: context,
                market,
                model_version: model,
                calibration_version: cal,
                lineup_model_version: lineup,
                bucket,
                sample_count: rows.len(),
                status: reliability(rows.len()),
                average_predicted_probability: quality.average_probability,
                actual_hit_rate: quality.actual_rate,
                calibration_gap: quality.actual_rate - quality.average_probability,
                brier_score: rows.iter().map(|row| row.brier).sum::<f64>() / rows.len() as f64,
            }
        })
        .collect()
}

fn odds_range(odds: f64) -> Option<&'static str> {
    match odds {
        value if (1.01..1.30).contains(&value) => Some("1.01_1.29"),
        value if (1.30..1.50).contains(&value) => Some("1.30_1.49"),
        value if (1.50..1.80).contains(&value) => Some("1.50_1.79"),
        value if (1.80..2.00).contains(&value) => Some("1.80_1.99"),
        value if (2.00..2.50).contains(&value) => Some("2.00_2.49"),
        value if (2.50..3.00).contains(&value) => Some("2.50_2.99"),
        value if value >= 3.00 => Some("3.00_PLUS"),
        _ => None,
    }
}

fn by_odds_range(evaluations: &[Evaluation]) -> Vec<OddsRangeSummary> {
    let mut groups: BTreeMap<
        (
            PredictionContext,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
        ),
        Vec<&Evaluation>,
    > = BTreeMap::new();
    for row in evaluations {
        let Some(band) = row.odds.and_then(odds_range) else {
            continue;
        };
        groups
            .entry((
                row.context,
                row.market.clone(),
                row.model_version.clone(),
                row.calibration_version.clone(),
                row.lineup_model_version.clone(),
                band.into(),
            ))
            .or_default()
            .push(row);
    }
    groups
        .into_iter()
        .map(
            |((context, market, model, cal, lineup, odds_range), rows)| {
                let quality = binary_quality_metrics(
                    &rows
                        .iter()
                        .map(|row| (row.probability, row.actual))
                        .collect::<Vec<_>>(),
                )
                .unwrap();
                let status = reliability(rows.len());
                OddsRangeSummary {
                    prediction_context: context,
                    market,
                    model_version: model,
                    calibration_version: cal,
                    lineup_model_version: lineup,
                    odds_range,
                    sample_count: rows.len(),
                    status,
                    average_predicted_probability: quality.average_probability,
                    actual_hit_rate: quality.actual_rate,
                    brier_score: rows.iter().map(|row| row.brier).sum::<f64>() / rows.len() as f64,
                    log_loss: rows.iter().map(|row| row.log_loss).sum::<f64>() / rows.len() as f64,
                    calibration_error_ece: (status == ReliabilityStatus::Sufficient)
                        .then_some(quality.ece),
                }
            },
        )
        .collect()
}

fn metric_rate(summary: &MetricSummary) -> Option<f64> {
    summary.accuracy.or(summary.actual_hit_rate)
}

fn context_comparisons(rows: &[Observation]) -> Vec<ContextComparison> {
    let evaluations = evaluations(rows, false);
    let mut groups: BTreeMap<
        (String, MetricType, String, Option<String>, Option<String>),
        (Vec<&Evaluation>, Vec<&Evaluation>),
    > = BTreeMap::new();
    for row in &evaluations {
        let key = (
            row.market.clone(),
            row.metric_type,
            row.model_version.clone(),
            row.calibration_version.clone(),
            row.lineup_model_version.clone(),
        );
        let pair = groups.entry(key).or_default();
        if row.context == PredictionContext::Base {
            pair.0.push(row)
        } else {
            pair.1.push(row)
        }
    }
    let mut output = Vec::new();
    for ((market, metric_type, model, cal, lineup), (base, lineup_rows)) in groups {
        if base.is_empty() || lineup_rows.is_empty() {
            continue;
        }
        let paired = base.len().min(lineup_rows.len());
        let base_summary = summarize(
            &base,
            SummaryIdentity {
                context: PredictionContext::Base,
                metric_type,
                market: Some(market.clone()),
                competition_id: None,
                competition_name: None,
                category: None,
                model_version: model.clone(),
                calibration_version: cal.clone(),
                lineup_model_version: None,
            },
        );
        let lineup_summary = summarize(
            &lineup_rows,
            SummaryIdentity {
                context: PredictionContext::LineupAware,
                metric_type,
                market: Some(market.clone()),
                competition_id: None,
                competition_name: None,
                category: None,
                model_version: model,
                calibration_version: cal,
                lineup_model_version: lineup,
            },
        );
        output.push(ContextComparison {
            market,
            metric_type,
            paired_sample_count: paired,
            status: if paired < MIN_PERFORMANCE_GROUP_SAMPLE {
                ReliabilityStatus::InsufficientData
            } else {
                ReliabilityStatus::Sufficient
            },
            brier_delta_lineup_minus_base: lineup_summary
                .brier_score
                .zip(base_summary.brier_score)
                .map(|(a, b)| a - b),
            log_loss_delta_lineup_minus_base: lineup_summary
                .log_loss
                .zip(base_summary.log_loss)
                .map(|(a, b)| a - b),
            hit_rate_or_accuracy_delta_lineup_minus_base: metric_rate(&lineup_summary)
                .zip(metric_rate(&base_summary))
                .map(|(a, b)| a - b),
            ece_delta_lineup_minus_base: lineup_summary
                .calibration_error_ece
                .zip(base_summary.calibration_error_ece)
                .map(|(a, b)| a - b),
            base: base_summary,
            lineup_aware: lineup_summary,
        });
    }
    output
}

pub fn get(
    c: &Connection,
    request: &ModelPerformanceRequest,
) -> Result<ModelPerformanceResponse, String> {
    let source = if request.candidate_only {
        load_candidates(c)?
    } else {
        let mut rows = load_base(c)?;
        rows.extend(load_lineup(c, false)?);
        rows
    };
    let (rows, from, to) = filtered(source, request)?;
    let evaluated = evaluations(&rows, request.candidate_only);
    let comparisons = if request.candidate_only {
        Vec::new()
    } else {
        let mut comparison_request = request.clone();
        comparison_request.prediction_context = None;
        let (paired, _, _) = filtered(load_lineup(c, true)?, &comparison_request)?;
        context_comparisons(&paired)
    };
    let sample_count = evaluated.len();
    let status = reliability(sample_count);
    let overall = overall(&evaluated);
    let by_market = by_market(&evaluated);
    let by_competition = by_competition(&evaluated);
    let confidence_buckets = confidence_buckets(&evaluated);
    let by_odds_range = by_odds_range(&evaluated);
    let candidate_policy_summary = by_category(&evaluated);
    let mut by_model_version = by_market.clone();
    by_model_version.sort_by(|a, b| {
        a.model_version
            .cmp(&b.model_version)
            .then_with(|| a.calibration_version.cmp(&b.calibration_version))
            .then_with(|| a.prediction_context.cmp(&b.prediction_context))
            .then_with(|| a.market.cmp(&b.market))
    });
    Ok(ModelPerformanceResponse {
        window: request.window,
        from_date: from.map(|date| date.to_string()),
        to_date: to.map(|date| date.to_string()),
        prediction_context: request.prediction_context,
        filters: ModelPerformanceFilters {
            market: request.market.clone(),
            competition_id: request.competition_id,
            prediction_context: request.prediction_context,
            model_version: request.model_version.clone(),
            calibration_version: request.calibration_version.clone(),
            candidate_only: request.candidate_only,
        },
        sample_count,
        status,
        overall,
        by_market,
        confidence_buckets,
        by_competition,
        by_odds_range,
        by_model_version,
        candidate_policy_summary,
        base_vs_lineup: comparisons,
    })
}
