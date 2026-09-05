//! Read-only MODEL DESTEKLİ POPÜLERLER projection over immutable snapshots.
use chrono::{NaiveDate, Utc};
use chrono_tz::Europe::Istanbul;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::providers::iddaa::markets::{normalize_selection, NormalizedMarketType};

const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 100;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GetRequest {
    pub business_date: Option<String>,
    pub supported_only: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupportStatus {
    ModelSupported,
    ModelNotSupported,
    ModelBelowPolicy,
    ModelDataUnavailable,
    MarketNotSupported,
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResolutionStatus {
    Resolved,
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelSupportedPopularItem {
    pub popularity_rank: i64,
    pub total_played: i64,
    pub total_played_round_str: Option<String>,
    pub popularity_snapshot_id: Option<i64>,
    pub observed_at: Option<String>,
    pub source: String,
    pub match_id: Option<i64>,
    pub kickoff: Option<String>,
    pub competition: Option<String>,
    pub home_team: Option<String>,
    pub away_team: Option<String>,
    pub market: String,
    pub selection: String,
    pub line: Option<f64>,
    pub odds: Option<f64>,
    pub model_probability: Option<f64>,
    pub implied_probability: Option<f64>,
    pub edge: Option<f64>,
    pub expected_value: Option<f64>,
    pub qualified: Option<bool>,
    pub support_status: SupportStatus,
    pub prediction_context: Option<String>,
    pub prediction_source: Option<String>,
    pub candidate_run_id: Option<i64>,
    pub candidate_id: Option<i64>,
    pub model_version: Option<String>,
    pub calibration_version: Option<String>,
    pub lineup_model_version: Option<String>,
    pub lineup_revision_id: Option<i64>,
    pub resolution_status: ResolutionStatus,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelSupportedPopularResponse {
    pub business_date: String,
    pub popularity_snapshot_id: Option<i64>,
    pub popularity_snapshot_at: Option<String>,
    pub candidate_run_id: Option<i64>,
    pub prediction_context: Option<String>,
    pub total_popular_items: usize,
    pub resolved_items: usize,
    pub model_supported_items: usize,
    pub below_policy_items: usize,
    pub unsupported_market_items: usize,
    pub unresolved_items: usize,
    pub data_unavailable_items: usize,
    pub items: Vec<ModelSupportedPopularItem>,
}

#[derive(Debug, Clone)]
struct EffectiveRun {
    id: i64,
    context: String,
    model_version: String,
    calibration_version: Option<String>,
    lineup_model_version: Option<String>,
    odds_cutoff_at: String,
}

#[derive(Debug, Clone)]
struct PopularRow {
    snapshot_id: i64,
    rank: i64,
    total_played: i64,
    display: Option<String>,
    observed_at: String,
    match_id: Option<i64>,
    kickoff: Option<String>,
    competition: Option<String>,
    home: Option<String>,
    away: Option<String>,
    market: Option<String>,
    raw_selection: Option<String>,
    line: Option<f64>,
}

#[derive(Debug, Clone)]
struct CanonicalSelection {
    market: String,
    selection: String,
    line: Option<f64>,
    phase7_category: Option<&'static str>,
}

struct CandidateRow {
    id: i64,
    probability: f64,
    odds: f64,
    implied: f64,
    edge: f64,
    ev: f64,
    calibration_version: Option<String>,
    source: Option<String>,
    lineup_revision_id: Option<i64>,
    lineup_model_version: Option<String>,
}

struct PredictionRow {
    id: i64,
    probability: f64,
    availability: String,
    calibration_version: Option<String>,
}

struct RevisionValue {
    probability: f64,
    source: String,
    revision_id: i64,
    lineup_model_version: Option<String>,
}

fn current_business_date() -> String {
    Utc::now().with_timezone(&Istanbul).date_naive().to_string()
}

fn run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EffectiveRun> {
    Ok(EffectiveRun {
        id: row.get(0)?,
        context: row.get(1)?,
        model_version: row.get(2)?,
        calibration_version: row.get(3)?,
        lineup_model_version: row.get(4)?,
        odds_cutoff_at: row.get(5)?,
    })
}

fn effective_run(c: &Connection, date: &str) -> Result<Option<EffectiveRun>, String> {
    let base: Option<EffectiveRun> = c
        .query_row(
            "SELECT id,COALESCE(prediction_context,'BASE'),model_version,
                    calibration_version,lineup_model_version,odds_cutoff_at
             FROM candidate_engine_runs
             WHERE business_date=?1 AND status='COMPLETED'
               AND COALESCE(prediction_context,'BASE')='BASE'
             ORDER BY generated_at DESC,id DESC LIMIT 1",
            [date],
            run_from_row,
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some(base) = base else {
        return Ok(None);
    };
    c.query_row(
        "SELECT id,prediction_context,model_version,calibration_version,
                lineup_model_version,odds_cutoff_at
         FROM candidate_engine_runs
         WHERE parent_candidate_run_id=?1 AND business_date=?2 AND status='COMPLETED'
           AND prediction_context='LINEUP_AWARE'
         ORDER BY id DESC LIMIT 1",
        params![base.id, date],
        run_from_row,
    )
    .optional()
    .map(|lineup| lineup.or(Some(base)))
    .map_err(|e| e.to_string())
}

fn popular_rows(c: &Connection, date: &str) -> Result<Vec<PopularRow>, String> {
    let mut statement = c
        .prepare(
            "WITH latest AS (
                SELECT *,ROW_NUMBER() OVER (
                    PARTITION BY provider_event_id,COALESCE(provider_market_id,''),
                                 COALESCE(provider_selection_code,''),metric_type
                    ORDER BY captured_at DESC,id DESC
                ) row_number
                FROM popularity_snapshots WHERE provider='iddaa'
             ), counts AS (
                SELECT * FROM latest WHERE row_number=1 AND metric_type='COUNT'
             ), ranks AS (
                SELECT * FROM latest WHERE row_number=1 AND metric_type='RANK'
             )
             SELECT counts.id,ranks.rank_value,CAST(counts.metric_value AS INTEGER),
                    counts.provider_raw_value,counts.captured_at,
                    COALESCE(counts.match_id,mapping.match_id),m.kickoff_at,
                    competition.name,home.normalized_name,away.normalized_name,
                    counts.market_type,counts.selection,counts.line_value
             FROM counts
             JOIN ranks ON ranks.provider_event_id=counts.provider_event_id
                AND ranks.provider_market_id IS counts.provider_market_id
                AND ranks.provider_selection_code IS counts.provider_selection_code
             LEFT JOIN provider_match_mappings mapping
                ON mapping.provider='iddaa'
               AND mapping.external_match_id=counts.provider_event_id
             LEFT JOIN matches m ON m.id=COALESCE(counts.match_id,mapping.match_id)
             LEFT JOIN competitions competition ON competition.id=m.competition_id
             LEFT JOIN teams home ON home.id=m.home_team_id
             LEFT JOIN teams away ON away.id=m.away_team_id
             WHERE m.id IS NULL OR m.scheduled_local_date=?1
             ORDER BY ranks.rank_value ASC,counts.provider_event_id ASC,
                      COALESCE(counts.provider_market_id,''),
                      COALESCE(counts.provider_selection_code,'')",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([date], |row| {
            Ok(PopularRow {
                snapshot_id: row.get(0)?,
                rank: row.get(1)?,
                total_played: row.get(2)?,
                display: row.get(3)?,
                observed_at: row.get(4)?,
                match_id: row.get(5)?,
                kickoff: row.get(6)?,
                competition: row.get(7)?,
                home: row.get(8)?,
                away: row.get(9)?,
                market: row.get(10)?,
                raw_selection: row.get(11)?,
                line: row.get(12)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

fn canonical_selection(market: NormalizedMarketType, raw: &str) -> Option<&'static str> {
    let normalized = raw.trim().to_lowercase();
    match market {
        NormalizedMarketType::MatchResult => match normalized.as_str() {
            "home" => Some("HOME"),
            "draw" => Some("DRAW"),
            "away" => Some("AWAY"),
            _ => None,
        },
        NormalizedMarketType::TotalGoals | NormalizedMarketType::CornersTotal => {
            match normalized.as_str() {
                "over" | "üst" | "ust" => Some("OVER"),
                "under" | "alt" => Some("UNDER"),
                _ => None,
            }
        }
        NormalizedMarketType::BothTeamsToScore => match normalized.as_str() {
            "yes" => Some("YES"),
            "no" => Some("NO"),
            _ => None,
        },
        _ => None,
    }
}

fn canonical(row: &PopularRow) -> Result<CanonicalSelection, SupportStatus> {
    let market = row.market.as_deref().unwrap_or("UNKNOWN");
    let (provider_market, candidate_market) = match market {
        "MATCH_RESULT" => (Some(NormalizedMarketType::MatchResult), "MATCH_RESULT"),
        "TOTAL_GOALS" => (Some(NormalizedMarketType::TotalGoals), "TOTAL_GOALS"),
        "BOTH_TEAMS_TO_SCORE" | "BTTS" => (Some(NormalizedMarketType::BothTeamsToScore), "BTTS"),
        "CORNERS_TOTAL" | "FULL_TIME_TOTAL_CORNERS" => (
            Some(NormalizedMarketType::CornersTotal),
            "FULL_TIME_TOTAL_CORNERS",
        ),
        _ => (None, market),
    };
    let Some(provider_market) = provider_market else {
        return Err(SupportStatus::MarketNotSupported);
    };
    let raw = row.raw_selection.as_deref().unwrap_or_default();
    let selection = normalize_selection(provider_market, raw)
        .or_else(|| canonical_selection(provider_market, raw))
        .ok_or(SupportStatus::Unresolved)?;
    let phase7_category = match (candidate_market, selection, row.line) {
        ("MATCH_RESULT", "HOME" | "DRAW" | "AWAY", None) => Some("MATCH_RESULT"),
        ("TOTAL_GOALS", "OVER", Some(line)) if line.to_bits() == 2.5f64.to_bits() => {
            Some("OVER_25")
        }
        ("TOTAL_GOALS", "OVER", Some(line)) if line.to_bits() == 3.5f64.to_bits() => {
            Some("OVER_35")
        }
        ("BTTS", "YES", None) => Some("BTTS_YES"),
        ("FULL_TIME_TOTAL_CORNERS", "OVER", Some(7.5 | 8.5 | 9.5 | 10.5 | 11.5)) => Some("CORNERS"),
        _ => None,
    };
    Ok(CanonicalSelection {
        market: candidate_market.into(),
        selection: selection.into(),
        line: row.line,
        phase7_category,
    })
}

fn candidate(
    c: &Connection,
    run_id: i64,
    match_id: i64,
    selection: &CanonicalSelection,
) -> Result<Option<CandidateRow>, String> {
    c.query_row(
        "SELECT id,COALESCE(final_public_probability,public_probability),iddaa_odd,
                implied_probability,probability_edge,expected_value,calibration_version,
                prediction_source,lineup_revision_id,lineup_model_version
         FROM candidate_engine_candidates
         WHERE run_id=?1 AND match_id=?2 AND market=?3 AND selection=?4
           AND (line_value IS ?5 OR line_value=?5)
         ORDER BY id ASC LIMIT 1",
        params![
            run_id,
            match_id,
            selection.market,
            selection.selection,
            selection.line
        ],
        |row| {
            Ok(CandidateRow {
                id: row.get(0)?,
                probability: row.get(1)?,
                odds: row.get(2)?,
                implied: row.get(3)?,
                edge: row.get(4)?,
                ev: row.get(5)?,
                calibration_version: row.get(6)?,
                source: row.get(7)?,
                lineup_revision_id: row.get(8)?,
                lineup_model_version: row.get(9)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn prediction(
    c: &Connection,
    run: &EffectiveRun,
    match_id: i64,
    selection: &CanonicalSelection,
) -> Result<Option<PredictionRow>, String> {
    c.query_row(
        "SELECT p.id,COALESCE(p.public_probability,p.model_probability),
                COALESCE(p.availability,'AVAILABLE'),p.calibration_version
         FROM predictions p JOIN model_versions mv ON mv.id=p.model_version_id
         WHERE p.match_id=?1 AND p.market=?2 AND p.selection=?3
           AND (p.line_value IS ?4 OR p.line_value=?4)
           AND mv.version_identifier=?5
         ORDER BY p.id DESC LIMIT 1",
        params![
            match_id,
            selection.market,
            selection.selection,
            selection.line,
            run.model_version
        ],
        |row| {
            Ok(PredictionRow {
                id: row.get(0)?,
                probability: row.get(1)?,
                availability: row.get(2)?,
                calibration_version: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn revision_value(
    c: &Connection,
    run: &EffectiveRun,
    match_id: i64,
    selection: &CanonicalSelection,
    base_probability: f64,
) -> Result<Option<RevisionValue>, String> {
    if run.context != "LINEUP_AWARE" {
        return Ok(None);
    }
    let row: Option<(i64, Option<String>, String)> = c
        .query_row(
            "SELECT id,lineup_model_version,market_payload_json
             FROM prediction_revisions
             WHERE match_id=?1 AND revision_type='LINEUP_AWARE_PREMATCH'
               AND payload_schema=?2 AND (?3 IS NULL OR lineup_model_version=?3)
             ORDER BY id DESC LIMIT 1",
            params![
                match_id,
                crate::repositories::lineup_model::REVISION_PAYLOAD_SCHEMA,
                run.lineup_model_version
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((id, lineup_model_version, json)) = row else {
        return Ok(None);
    };
    let document: crate::repositories::lineup_model::RevisionMarketPayloadDocument =
        serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let value = document.markets.iter().find(|market| {
        market.market.eq_ignore_ascii_case(&selection.market)
            && market.selection.eq_ignore_ascii_case(&selection.selection)
            && market.line.map(f64::to_bits) == selection.line.map(f64::to_bits)
    });
    Ok(value.map(|market| RevisionValue {
        probability: if market.family_status == "ACTIVE" {
            market.final_public_probability.unwrap_or(base_probability)
        } else {
            base_probability
        },
        source: if market.family_status == "ACTIVE" {
            "LINEUP_AWARE"
        } else {
            "BASE_RETAINED"
        }
        .into(),
        revision_id: id,
        lineup_model_version,
    }))
}

fn exclusion_reason(
    c: &Connection,
    run_id: i64,
    prediction_id: i64,
    category: &str,
) -> Result<Option<String>, String> {
    let primary = if category == "MATCH_RESULT" {
        "HIGH_CONFIDENCE"
    } else {
        category
    };
    c.query_row(
        "SELECT reason FROM candidate_engine_exclusions
         WHERE run_id=?1 AND prediction_id=?2
         ORDER BY CASE WHEN category=?3 THEN 0
                       WHEN category='SURPRISE' THEN 1 ELSE 2 END,id ASC LIMIT 1",
        params![run_id, prediction_id, primary],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn current_odds(
    c: &Connection,
    run: &EffectiveRun,
    match_id: i64,
    selection: &CanonicalSelection,
) -> Result<Option<f64>, String> {
    c.query_row(
        "SELECT odd FROM odds_snapshots
         WHERE provider='iddaa' AND match_id=?1 AND captured_at<=?2
           AND UPPER(COALESCE(normalized_market_type,market_name))=UPPER(?3)
           AND UPPER(COALESCE(normalized_selection,selection))=UPPER(?4)
           AND (line_value IS ?5 OR line_value=?5)
         ORDER BY captured_at DESC,id DESC LIMIT 1",
        params![
            match_id,
            run.odds_cutoff_at,
            selection.market,
            selection.selection,
            selection.line
        ],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn analytics(probability: f64, odds: f64) -> (f64, f64, f64) {
    let implied = 1.0 / odds;
    (implied, probability - implied, probability * odds - 1.0)
}

fn base_item(row: PopularRow, run: Option<&EffectiveRun>) -> ModelSupportedPopularItem {
    ModelSupportedPopularItem {
        popularity_rank: row.rank,
        total_played: row.total_played,
        total_played_round_str: row.display,
        popularity_snapshot_id: Some(row.snapshot_id),
        observed_at: Some(row.observed_at),
        source: "iddaa".into(),
        match_id: row.match_id,
        kickoff: row.kickoff,
        competition: row.competition,
        home_team: row.home,
        away_team: row.away,
        market: row.market.unwrap_or_else(|| "UNKNOWN".into()),
        selection: row.raw_selection.unwrap_or_default(),
        line: row.line,
        odds: None,
        model_probability: None,
        implied_probability: None,
        edge: None,
        expected_value: None,
        qualified: None,
        support_status: SupportStatus::ModelDataUnavailable,
        prediction_context: run.map(|value| value.context.clone()),
        prediction_source: None,
        candidate_run_id: run.map(|value| value.id),
        candidate_id: None,
        model_version: run.map(|value| value.model_version.clone()),
        calibration_version: run.and_then(|value| value.calibration_version.clone()),
        lineup_model_version: run.and_then(|value| value.lineup_model_version.clone()),
        lineup_revision_id: None,
        resolution_status: ResolutionStatus::Resolved,
        reason: None,
    }
}

fn project_item(
    c: &Connection,
    row: PopularRow,
    run: Option<&EffectiveRun>,
) -> Result<ModelSupportedPopularItem, String> {
    let mut item = base_item(row.clone(), run);
    let Some(match_id) = row.match_id else {
        item.support_status = SupportStatus::Unresolved;
        item.resolution_status = ResolutionStatus::Unresolved;
        item.reason = Some("POPULAR_MATCH_UNRESOLVED".into());
        return Ok(item);
    };
    let selection = match canonical(&row) {
        Ok(selection) => selection,
        Err(status) => {
            item.support_status = status.clone();
            item.reason = Some(
                match status {
                    SupportStatus::MarketNotSupported => "POPULAR_MARKET_NOT_SUPPORTED",
                    _ => "POPULAR_SELECTION_UNRESOLVED",
                }
                .into(),
            );
            if status == SupportStatus::Unresolved {
                item.resolution_status = ResolutionStatus::Unresolved;
            }
            return Ok(item);
        }
    };
    item.market = selection.market.clone();
    item.selection = selection.selection.clone();
    item.line = selection.line;
    let Some(run) = run else {
        item.reason = Some("CANDIDATE_RUN_UNAVAILABLE".into());
        return Ok(item);
    };
    if let Some(candidate) = candidate(c, run.id, match_id, &selection)? {
        item.odds = Some(candidate.odds);
        item.model_probability = Some(candidate.probability);
        item.implied_probability = Some(candidate.implied);
        item.edge = Some(candidate.edge);
        item.expected_value = Some(candidate.ev);
        item.qualified = Some(true);
        item.support_status = SupportStatus::ModelSupported;
        item.prediction_source = candidate.source;
        item.candidate_id = Some(candidate.id);
        item.calibration_version = candidate
            .calibration_version
            .or_else(|| run.calibration_version.clone());
        item.lineup_revision_id = candidate.lineup_revision_id;
        item.lineup_model_version = candidate
            .lineup_model_version
            .or_else(|| run.lineup_model_version.clone());
        return Ok(item);
    }
    let Some(prediction) = prediction(c, run, match_id, &selection)? else {
        item.reason = Some("MODEL_PREDICTION_UNAVAILABLE".into());
        return Ok(item);
    };
    let revision = revision_value(c, run, match_id, &selection, prediction.probability)?;
    let probability = revision
        .as_ref()
        .map(|value| value.probability)
        .unwrap_or(prediction.probability);
    item.model_probability = Some(probability);
    item.calibration_version = prediction
        .calibration_version
        .or_else(|| run.calibration_version.clone());
    item.prediction_source = Some(
        revision
            .as_ref()
            .map(|value| value.source.as_str())
            .unwrap_or("BASE")
            .into(),
    );
    if let Some(revision) = revision {
        item.lineup_revision_id = Some(revision.revision_id);
        item.lineup_model_version = revision
            .lineup_model_version
            .or_else(|| run.lineup_model_version.clone());
    }
    if prediction.availability != "AVAILABLE" {
        item.reason = Some("MODEL_UNAVAILABLE".into());
        return Ok(item);
    }
    let Some(category) = selection.phase7_category else {
        item.qualified = Some(false);
        item.support_status = SupportStatus::ModelNotSupported;
        item.reason = Some("SELECTION_NOT_SUPPORTED_BY_PHASE_7_POLICY".into());
        return Ok(item);
    };
    let Some(reason) = exclusion_reason(c, run.id, prediction.id, category)? else {
        item.reason = Some("CANDIDATE_CONTEXT_UNAVAILABLE".into());
        return Ok(item);
    };
    if matches!(
        reason.as_str(),
        "ODDS_UNAVAILABLE" | "STALE_ODDS" | "MODEL_UNAVAILABLE"
    ) {
        item.reason = Some(reason);
        return Ok(item);
    }
    item.qualified = Some(false);
    item.support_status = SupportStatus::ModelBelowPolicy;
    item.reason = Some(reason);
    if let Some(odds) = current_odds(c, run, match_id, &selection)? {
        let (implied, edge, ev) = analytics(probability, odds);
        item.odds = Some(odds);
        item.implied_probability = Some(implied);
        item.edge = Some(edge);
        item.expected_value = Some(ev);
    }
    Ok(item)
}

pub fn get(c: &Connection, request: &GetRequest) -> Result<ModelSupportedPopularResponse, String> {
    let business_date = request
        .business_date
        .clone()
        .unwrap_or_else(current_business_date);
    NaiveDate::parse_from_str(&business_date, "%Y-%m-%d")
        .map_err(|_| "INVALID_BUSINESS_DATE".to_string())?;
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let run = effective_run(c, &business_date)?;
    let rows = popular_rows(c, &business_date)?;
    let popularity_snapshot_id = rows.iter().map(|row| row.snapshot_id).max();
    let popularity_snapshot_at = rows
        .iter()
        .map(|row| row.observed_at.as_str())
        .max()
        .map(str::to_string);
    let mut all_items = rows
        .into_iter()
        .map(|row| project_item(c, row, run.as_ref()))
        .collect::<Result<Vec<_>, _>>()?;
    all_items.sort_by(|a, b| {
        a.popularity_rank
            .cmp(&b.popularity_rank)
            .then_with(|| a.popularity_snapshot_id.cmp(&b.popularity_snapshot_id))
    });
    let total_popular_items = all_items.len();
    let resolved_items = all_items
        .iter()
        .filter(|item| item.resolution_status == ResolutionStatus::Resolved)
        .count();
    let model_supported_items = count_status(&all_items, SupportStatus::ModelSupported);
    let below_policy_items = count_status(&all_items, SupportStatus::ModelBelowPolicy);
    let unsupported_market_items = count_status(&all_items, SupportStatus::MarketNotSupported);
    let unresolved_items = count_status(&all_items, SupportStatus::Unresolved);
    let data_unavailable_items = count_status(&all_items, SupportStatus::ModelDataUnavailable);
    let items = all_items
        .into_iter()
        .filter(|item| {
            !request.supported_only.unwrap_or(false)
                || item.support_status == SupportStatus::ModelSupported
        })
        .take(limit)
        .collect();
    Ok(ModelSupportedPopularResponse {
        business_date,
        popularity_snapshot_id,
        popularity_snapshot_at,
        candidate_run_id: run.as_ref().map(|run| run.id),
        prediction_context: run.as_ref().map(|run| run.context.clone()),
        total_popular_items,
        resolved_items,
        model_supported_items,
        below_policy_items,
        unsupported_market_items,
        unresolved_items,
        data_unavailable_items,
        items,
    })
}

fn count_status(items: &[ModelSupportedPopularItem], status: SupportStatus) -> usize {
    items
        .iter()
        .filter(|item| item.support_status == status)
        .count()
}
