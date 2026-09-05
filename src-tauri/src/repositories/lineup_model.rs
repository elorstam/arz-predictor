//! Learned second-stage lineup adjustment model.  This is deliberately a small
//! deterministic CPU artifact: no odds, popularity, IDs, network, or runtime
//! Python are involved.
use super::lineup::LINEUP_FEATURE_VERSION;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

pub const ARTIFACT_SCHEMA_VERSION: &str = "lineup_adjustment_artifact_v1";
pub const FEATURE_REGISTRY_VERSION: &str = "lineup_pred_features_v1";
pub const MIN_LINEUP_TRAINING_MATCHES: usize = 20;
pub const MIN_LINEUP_VALIDATION_MATCHES: usize = 5;
pub const MIN_LINEUP_TEST_MATCHES: usize = 5;
pub const MIN_PLAYER_RESOLUTION_COVERAGE: f64 = 0.90;

pub const FEATURE_REGISTRY: &[&str] = &[
    "base_logit",
    "home_starter_continuity",
    "away_starter_continuity",
    "home_changed_starters",
    "away_changed_starters",
    "home_regular_starter_presence",
    "away_regular_starter_presence",
    "home_goalkeeper_continuity",
    "away_goalkeeper_continuity",
    "home_prior_match_sample",
    "away_prior_match_sample",
    "home_away_continuity_difference",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainingRow {
    pub match_id: i64,
    pub competition_id: i64,
    pub kickoff_utc: String,
    pub base_prediction_id: i64,
    pub base_prediction_is_oos: bool,
    pub base_training_cutoff: String,
    pub base_fold: usize,
    pub base_model_version: String,
    pub base_model_hash: String,
    pub base_calibration_version: String,
    pub base_calibration_hash: String,
    pub base_home_lambda: f64,
    pub base_away_lambda: f64,
    pub base_home_probability: f64,
    pub base_draw_probability: f64,
    pub base_away_probability: f64,
    pub base_btts_yes_probability: f64,
    pub lineup_snapshot_id: i64,
    pub lineup_feature_set_id: i64,
    pub lineup_feature_vector: BTreeMap<String, f64>,
    pub lineup_feature_quality: String,
    pub actual_home_goals: i64,
    pub actual_away_goals: i64,
    pub match_result_label: String,
    pub btts_label: bool,
    pub total_over_15_label: bool,
    pub total_over_25_label: bool,
    pub total_over_35_label: bool,
    pub home_over_05_label: bool,
    pub home_over_15_label: bool,
    pub away_over_05_label: bool,
    pub away_over_15_label: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct LegacyMarketTrainingRow {
    as_of: String,
    market: String,
    base_probability: f64,
    label: bool,
    features: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CoreTrainingDatasetDiagnostics {
    pub historical_matches_considered: usize,
    pub accepted: usize,
    pub missing_oos_base_prediction: usize,
    pub base_prediction_not_oos: usize,
    pub missing_base_lambda: usize,
    pub missing_base_probability: usize,
    pub missing_model_lineage: usize,
    pub missing_calibration_lineage: usize,
    pub missing_official_lineup: usize,
    pub incomplete_lineup: usize,
    pub poor_lineup_feature_quality: usize,
    pub missing_feature_set: usize,
    pub missing_result: usize,
    pub invalid_non_finite_value: usize,
    pub lineage_mismatch: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoreTrainingDataset {
    pub rows: Vec<TrainingRow>,
    pub diagnostics: CoreTrainingDatasetDiagnostics,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Preprocessing {
    pub means: BTreeMap<String, f64>,
    pub stddevs: BTreeMap<String, f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketModel {
    pub market: String,
    pub coefficients: BTreeMap<String, f64>,
    pub intercept: f64,
    pub validation_log_loss: f64,
    pub test_log_loss: f64,
    pub base_test_log_loss: f64,
    pub status: String,
    pub sample_count: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineupAdjustmentArtifact {
    pub artifact_schema_version: String,
    pub model_version: String,
    pub parent_model_version: Option<String>,
    pub parent_model_hash: Option<String>,
    #[serde(default)]
    pub parent_calibration_version: Option<String>,
    #[serde(default)]
    pub parent_calibration_hash: Option<String>,
    pub lineup_feature_version: String,
    pub feature_registry_version: String,
    pub feature_registry: Vec<String>,
    pub preprocessing: Preprocessing,
    pub training_period: [String; 2],
    pub validation_period: [String; 2],
    pub test_period: [String; 2],
    pub training_samples: usize,
    pub validation_samples: usize,
    pub test_samples: usize,
    pub markets: BTreeMap<String, MarketModel>,
    #[serde(default)]
    pub core_model: Option<CoreModel>,
    #[serde(default)]
    pub core_calibration: Option<CoreCalibration>,
    #[serde(default)]
    pub core_metrics: BTreeMap<String, CoreMetrics>,
    pub created_at: String,
    pub artifact_sha256: String,
    #[serde(default)]
    pub family_statuses: BTreeMap<String, String>,
    #[serde(default)]
    pub benefit_gate_config: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CoreCalibration {
    pub match_result_temperature: Option<f64>,
    pub binary: BTreeMap<String, PlattCalibration>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlattCalibration {
    pub a: f64,
    pub b: f64,
    pub status: String,
    pub sample_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CoreMetrics {
    pub sample_count: usize,
    pub base_log_loss: f64,
    pub lineup_log_loss: f64,
    pub base_brier: f64,
    pub lineup_brier: f64,
    pub base_ece: f64,
    pub lineup_ece: f64,
    pub status: String,
    #[serde(default)]
    pub base_accuracy: Option<f64>,
    #[serde(default)]
    pub lineup_accuracy: Option<f64>,
    #[serde(default)]
    pub base_mae: Option<f64>,
    #[serde(default)]
    pub lineup_mae: Option<f64>,
    #[serde(default)]
    pub base_rmse: Option<f64>,
    #[serde(default)]
    pub lineup_rmse: Option<f64>,
}

/// Fits multiclass temperature on the validation partition only.  The grid is
/// deliberately fixed so an artifact is portable and reproducible.
pub fn fit_temperature(rows: &[CoreTrainingRow], model: &CoreModel) -> Option<f64> {
    if rows.len() < MIN_LINEUP_VALIDATION_MATCHES {
        return None;
    }
    let mut best = (f64::INFINITY, 1.0);
    for step in 1..=40 {
        let temperature = 0.5 + step as f64 * 0.05;
        let loss = rows
            .iter()
            .map(|row| {
                let raw = revised_multiclass(row.base_result, &model.match_result, &row.features);
                -calibrate_match_result(raw, temperature)[row.result_class]
                    .max(1e-12)
                    .ln()
            })
            .sum::<f64>()
            / rows.len() as f64;
        if loss < best.0 {
            best = (loss, temperature);
        }
    }
    Some(best.1)
}

/// Fits a binary Platt transform on validation predictions.  This is not a
/// probability heuristic: a and b are learned by deterministic gradient descent.
pub fn fit_platt(
    rows: &[CoreTrainingRow],
    base: impl Fn(&CoreTrainingRow) -> f64,
) -> Option<PlattCalibration> {
    fit_platt_with_label(rows, base, |row| row.btts)
}

fn fit_platt_with_label(
    rows: &[CoreTrainingRow],
    base: impl Fn(&CoreTrainingRow) -> f64,
    label: impl Fn(&CoreTrainingRow) -> bool,
) -> Option<PlattCalibration> {
    if rows.len() < MIN_LINEUP_VALIDATION_MATCHES {
        return None;
    }
    let mut a = 1.0;
    let mut b = 0.0;
    for _ in 0..500 {
        let mut ga = 0.0;
        let mut gb = 0.0;
        for row in rows {
            let z = a * logit(base(row)) + b;
            let error = sigmoid(z) - if label(row) { 1.0 } else { 0.0 };
            ga += error * logit(base(row));
            gb += error;
        }
        let n = rows.len() as f64;
        a = (a - 0.01 * ga / n).clamp(-20.0, 20.0);
        b = (b - 0.01 * gb / n).clamp(-20.0, 20.0);
    }
    Some(PlattCalibration {
        a,
        b,
        status: "CALIBRATED".into(),
        sample_count: rows.len(),
    })
}

fn ece_binary(predictions: &[(f64, bool)]) -> f64 {
    if predictions.is_empty() {
        return 0.0;
    }
    let mut bins = vec![Vec::new(); 10];
    for &(p, y) in predictions {
        let p = clip(p);
        bins[(p * 10.0).floor().min(9.0) as usize].push((p, if y { 1.0 } else { 0.0 }));
    }
    bins.into_iter()
        .filter(|b| !b.is_empty())
        .map(|b| {
            let mean_p = b.iter().map(|x| x.0).sum::<f64>() / b.len() as f64;
            let mean_y = b.iter().map(|x| x.1).sum::<f64>() / b.len() as f64;
            b.len() as f64 / predictions.len() as f64 * (mean_p - mean_y).abs()
        })
        .sum()
}

fn binary_pair(base: &[(f64, bool)], lineup: &[(f64, bool)]) -> CoreMetrics {
    let metrics = |values: &[(f64, bool)]| {
        let n = values.len().max(1) as f64;
        let mut ll = 0.0;
        let mut br = 0.0;
        for &(p, y) in values {
            let p = clip(p);
            let t = if y { 1.0 } else { 0.0 };
            ll += -(t * p.ln() + (1.0 - t) * (1.0 - p).ln());
            br += (p - t).powi(2);
        }
        (ll / n, br / n, ece_binary(values))
    };
    let (bll, bbr, be) = metrics(base);
    let (lll, lbr, le) = metrics(lineup);
    CoreMetrics {
        sample_count: base.len().min(lineup.len()),
        base_log_loss: bll,
        lineup_log_loss: lll,
        base_brier: bbr,
        lineup_brier: lbr,
        base_ece: be,
        lineup_ece: le,
        status: "EVALUATED".into(),
        ..CoreMetrics::default()
    }
}

fn multiclass_pair(
    rows: &[CoreTrainingRow],
    model: &CoreModel,
    calibration: Option<&CoreCalibration>,
) -> CoreMetrics {
    let mut base_ll = 0.0;
    let mut line_ll = 0.0;
    let mut base_br = 0.0;
    let mut line_br = 0.0;
    let mut base_acc = 0usize;
    let mut line_acc = 0usize;
    let mut base_bins = Vec::new();
    let mut line_bins = Vec::new();
    for row in rows {
        let base = row.base_result;
        let line = infer_core(
            model,
            calibration,
            row.base_home_lambda,
            row.base_away_lambda,
            row.base_result,
            row.base_btts,
            &row.features,
        )
        .match_result;
        let target = row.result_class;
        base_ll += -clip(base[target]).ln();
        line_ll += -clip(line[target]).ln();
        base_br += base
            .iter()
            .enumerate()
            .map(|(i, p)| (p - if i == target { 1.0 } else { 0.0 }).powi(2))
            .sum::<f64>();
        line_br += line
            .iter()
            .enumerate()
            .map(|(i, p)| (p - if i == target { 1.0 } else { 0.0 }).powi(2))
            .sum::<f64>();
        base_acc += usize::from(
            base.iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|x| x.0)
                == Some(target),
        );
        line_acc += usize::from(
            line.iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|x| x.0)
                == Some(target),
        );
        base_bins.push((base[target], true));
        line_bins.push((line[target], true));
    }
    let n = rows.len().max(1) as f64;
    CoreMetrics {
        sample_count: rows.len(),
        base_log_loss: base_ll / n,
        lineup_log_loss: line_ll / n,
        base_brier: base_br / n,
        lineup_brier: line_br / n,
        base_ece: ece_binary(&base_bins),
        lineup_ece: ece_binary(&line_bins),
        status: "EVALUATED".into(),
        base_accuracy: Some(base_acc as f64 / n),
        lineup_accuracy: Some(line_acc as f64 / n),
        ..CoreMetrics::default()
    }
}

fn goal_count_pair(rows: &[CoreTrainingRow], home: bool, model: &CoreModel) -> CoreMetrics {
    let mut bmae = 0.0;
    let mut lmae = 0.0;
    let mut br = 0.0;
    let mut lr = 0.0;
    for r in rows {
        let actual = if home { r.home_goals } else { r.away_goals };
        let base = if home {
            r.base_home_lambda
        } else {
            r.base_away_lambda
        };
        let revised = if home {
            apply_offset(base, &model.home_goal_offset, &r.features)
        } else {
            apply_offset(base, &model.away_goal_offset, &r.features)
        };
        bmae += (base - actual).abs();
        lmae += (revised - actual).abs();
        br += (base - actual).powi(2);
        lr += (revised - actual).powi(2);
    }
    let n = rows.len().max(1) as f64;
    CoreMetrics {
        sample_count: rows.len(),
        base_mae: Some(bmae / n),
        lineup_mae: Some(lmae / n),
        base_rmse: Some((br / n).sqrt()),
        lineup_rmse: Some((lr / n).sqrt()),
        status: "EVALUATED".into(),
        ..CoreMetrics::default()
    }
}

fn family_gate(metrics: &CoreMetrics, min_n: usize) -> String {
    if metrics.sample_count < min_n {
        return "INSUFFICIENT_DATA".into();
    }
    if ![
        metrics.base_log_loss,
        metrics.lineup_log_loss,
        metrics.base_brier,
        metrics.lineup_brier,
        metrics.base_ece,
        metrics.lineup_ece,
    ]
    .iter()
    .all(|v| v.is_finite())
    {
        return "INSUFFICIENT_DATA".into();
    }
    if metrics.base_log_loss - metrics.lineup_log_loss > 0.0001
        && metrics.lineup_brier <= metrics.base_brier + 0.02
        && metrics.lineup_ece <= metrics.base_ece + 0.02
    {
        "ACTIVE".into()
    } else {
        "NOT_BENEFICIAL".into()
    }
}

pub fn binary_metrics(predictions: &[(f64, bool)]) -> CoreMetrics {
    let n = predictions.len();
    if n == 0 {
        return CoreMetrics::default();
    }
    let mut log_loss = 0.0;
    let mut brier = 0.0;
    let mut ece = 0.0;
    let mut bins = vec![Vec::new(); 10];
    for &(p, y) in predictions {
        let p = clip(p);
        let target = if y { 1.0 } else { 0.0 };
        log_loss += -(target * p.ln() + (1.0 - target) * (1.0 - p).ln());
        brier += (p - target).powi(2);
        bins[(p * 10.0).floor().min(9.0) as usize].push((p, target));
    }
    for bin in bins.into_iter().filter(|b| !b.is_empty()) {
        let weight = bin.len() as f64 / n as f64;
        let mean_p = bin.iter().map(|x| x.0).sum::<f64>() / bin.len() as f64;
        let mean_y = bin.iter().map(|x| x.1).sum::<f64>() / bin.len() as f64;
        ece += weight * (mean_p - mean_y).abs();
    }
    CoreMetrics {
        sample_count: n,
        base_log_loss: log_loss / n as f64,
        lineup_log_loss: log_loss / n as f64,
        base_brier: brier / n as f64,
        lineup_brier: brier / n as f64,
        base_ece: ece,
        lineup_ece: ece,
        status: "EVALUATED".into(),
        ..CoreMetrics::default()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainReport {
    pub artifact_path: String,
    pub artifact_sha256: String,
    pub training_samples: usize,
    pub validation_samples: usize,
    pub test_samples: usize,
    pub active_markets: Vec<String>,
    pub insufficient_markets: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RevisionResult {
    pub prediction_id: i64,
    pub revision_id: i64,
    pub market: String,
    pub base_probability: f64,
    pub revised_probability: Option<f64>,
    pub delta_percentage_points: Option<f64>,
    pub revision_status: String,
    pub lineup_model_version: Option<String>,
}

pub const REVISION_PAYLOAD_SCHEMA: &str = "lineup_revision_payload_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RevisionMarketPayload {
    pub market: String,
    pub selection: String,
    pub line: Option<f64>,
    pub base_raw_probability: Option<f64>,
    pub base_public_probability: Option<f64>,
    pub revised_raw_probability: Option<f64>,
    pub revised_calibrated_probability: Option<f64>,
    pub final_public_probability: Option<f64>,
    pub delta_percentage_points: Option<f64>,
    pub family_status: String,
    pub calibration_status: String,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RevisionMarketPayloadDocument {
    pub payload_schema: String,
    pub markets: Vec<RevisionMarketPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RevisionCorePayload {
    pub prediction_id: i64,
    pub match_id: i64,
    pub lineup_snapshot_id: i64,
    pub lineup_feature_set_id: i64,
    pub generated_at: String,
    pub revision_reason: String,
    pub base_model_version: String,
    pub base_model_hash: String,
    pub base_calibration_version: String,
    pub base_calibration_hash: String,
    pub lineup_model_version: String,
    pub lineup_model_hash: String,
    pub base_home_lambda: Option<f64>,
    pub base_away_lambda: Option<f64>,
    pub revised_home_lambda: Option<f64>,
    pub revised_away_lambda: Option<f64>,
    pub payload: RevisionMarketPayloadDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedRevisionCorePayload {
    pub id: i64,
    pub prediction_id: i64,
    pub match_id: i64,
    pub lineup_snapshot_id: Option<i64>,
    pub lineup_feature_set_id: Option<i64>,
    pub base_model_version: Option<String>,
    pub base_model_hash: Option<String>,
    pub base_calibration_version: Option<String>,
    pub base_calibration_hash: Option<String>,
    pub lineup_model_version: Option<String>,
    pub lineup_model_hash: Option<String>,
    pub base_home_lambda: Option<f64>,
    pub base_away_lambda: Option<f64>,
    pub revised_home_lambda: Option<f64>,
    pub revised_away_lambda: Option<f64>,
    pub payload_schema: Option<String>,
    pub market_payload_json: Option<String>,
    pub payload_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoreTrainingRow {
    pub as_of: String,
    pub base_home_lambda: f64,
    pub base_away_lambda: f64,
    pub base_result: [f64; 3],
    pub base_btts: f64,
    pub home_goals: f64,
    pub away_goals: f64,
    pub result_class: usize,
    pub btts: bool,
    pub features: BTreeMap<String, f64>,
}

impl TrainingRow {
    pub fn to_core_training_row(&self) -> Result<CoreTrainingRow, String> {
        if !self.base_prediction_is_oos || self.base_prediction_id <= 0 {
            return Err("BASE_PREDICTION_NOT_OOS".into());
        }
        if self.base_model_version.is_empty()
            || self.base_model_hash.is_empty()
            || self.base_calibration_version.is_empty()
            || self.base_calibration_hash.is_empty()
            || self.base_training_cutoff.is_empty()
        {
            return Err("BASE_LINEAGE_INCOMPLETE".into());
        }
        for value in [
            self.base_home_lambda,
            self.base_away_lambda,
            self.base_home_probability,
            self.base_draw_probability,
            self.base_away_probability,
            self.base_btts_yes_probability,
        ] {
            if !value.is_finite() {
                return Err("INVALID_BASE_VALUE".into());
            }
        }
        if self.base_home_lambda <= 0.0 || self.base_away_lambda <= 0.0 {
            return Err("MISSING_BASE_LAMBDA".into());
        }
        if [
            self.base_home_probability,
            self.base_draw_probability,
            self.base_away_probability,
            self.base_btts_yes_probability,
        ]
        .iter()
        .any(|p| !(0.0..=1.0).contains(p))
        {
            return Err("INVALID_BASE_PROBABILITY".into());
        }
        if self.lineup_snapshot_id <= 0
            || self.lineup_feature_set_id <= 0
            || self.lineup_feature_quality != "COMPLETE"
        {
            return Err("LINEUP_FEATURES_INCOMPLETE".into());
        }
        if self.actual_home_goals < 0 || self.actual_away_goals < 0 {
            return Err("MISSING_FINAL_RESULT".into());
        }
        Ok(CoreTrainingRow {
            as_of: self.kickoff_utc.clone(),
            base_home_lambda: self.base_home_lambda,
            base_away_lambda: self.base_away_lambda,
            base_result: [
                self.base_home_probability,
                self.base_draw_probability,
                self.base_away_probability,
            ],
            base_btts: self.base_btts_yes_probability,
            home_goals: self.actual_home_goals as f64,
            away_goals: self.actual_away_goals as f64,
            result_class: match self.match_result_label.as_str() {
                "HOME" => 0,
                "DRAW" => 1,
                "AWAY" => 2,
                _ => return Err("INVALID_RESULT_LABEL".into()),
            },
            btts: self.btts_label,
            features: self.lineup_feature_vector.clone(),
        })
    }
}

fn lineup_feature_vector(value: &str) -> Result<BTreeMap<String, f64>, String> {
    let json: serde_json::Value = serde_json::from_str(value).map_err(|_| "INVALID_FEATURE_SET")?;
    let mut out = BTreeMap::new();
    let number = |v: Option<&serde_json::Value>| v.and_then(|x| x.as_f64()).unwrap_or(0.0);
    let home = json.get("home").ok_or("INVALID_FEATURE_SET")?;
    let away = json.get("away").ok_or("INVALID_FEATURE_SET")?;
    for (prefix, side) in [("home", home), ("away", away)] {
        out.insert(
            format!("{prefix}_starter_continuity"),
            number(side.get("starter_continuity_count")),
        );
        out.insert(
            format!("{prefix}_changed_starters"),
            number(side.get("changed_starters")),
        );
        out.insert(
            format!("{prefix}_regular_starter_presence"),
            number(side.get("regular_starter_presence")),
        );
        out.insert(
            format!("{prefix}_goalkeeper_continuity"),
            number(side.get("goalkeeper_continuity")),
        );
        out.insert(
            format!("{prefix}_prior_match_sample"),
            number(side.get("prior_match_sample")),
        );
    }
    out.insert(
        "home_away_continuity_difference".into(),
        number(json.get("home_minus_away_continuity")),
    );
    Ok(out)
}

fn legacy_market_rows(row: &TrainingRow) -> Vec<LegacyMarketTrainingRow> {
    let features = row.lineup_feature_vector.clone();
    [
        (
            "MATCH_RESULT_HOME",
            row.base_home_probability,
            row.match_result_label == "HOME",
        ),
        ("BTTS_YES", row.base_btts_yes_probability, row.btts_label),
        ("TOTAL_GOALS_OVER_1.5", 0.0, row.total_over_15_label),
        ("TOTAL_GOALS_OVER_2.5", 0.0, row.total_over_25_label),
        ("TOTAL_GOALS_OVER_3.5", 0.0, row.total_over_35_label),
        ("HOME_TEAM_TOTAL_OVER_0.5", 0.0, row.home_over_05_label),
        ("HOME_TEAM_TOTAL_OVER_1.5", 0.0, row.home_over_15_label),
        ("AWAY_TEAM_TOTAL_OVER_0.5", 0.0, row.away_over_05_label),
        ("AWAY_TEAM_TOTAL_OVER_1.5", 0.0, row.away_over_15_label),
    ]
    .into_iter()
    .map(
        |(market, base_probability, label)| LegacyMarketTrainingRow {
            as_of: row.kickoff_utc.clone(),
            market: market.into(),
            base_probability,
            label,
            features: features.clone(),
        },
    )
    .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LinearCorrection {
    pub intercept: f64,
    pub coefficients: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoreModel {
    pub home_goal_offset: LinearCorrection,
    pub away_goal_offset: LinearCorrection,
    pub match_result: Vec<LinearCorrection>,
    pub btts: LinearCorrection,
    pub match_result_blend: f64,
    pub btts_blend: f64,
}

impl Default for CoreModel {
    fn default() -> Self {
        Self {
            home_goal_offset: LinearCorrection::default(),
            away_goal_offset: LinearCorrection::default(),
            match_result: vec![LinearCorrection::default(); 3],
            btts: LinearCorrection::default(),
            match_result_blend: 0.0,
            btts_blend: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalMarketProbabilities {
    pub home_lambda: f64,
    pub away_lambda: f64,
    pub match_result: [f64; 3],
    pub total_over: [f64; 3],
    pub home_over: [f64; 2],
    pub away_over: [f64; 2],
    pub btts_yes: f64,
}

pub fn calibrate_binary(p: f64, calibration: &PlattCalibration) -> f64 {
    sigmoid(calibration.a * logit(p) + calibration.b)
}

pub fn calibrate_match_result(raw: [f64; 3], temperature: f64) -> [f64; 3] {
    let t = temperature.max(1e-6);
    softmax(raw.map(|p| clip(p).ln() / t))
}

fn project_monotone(values: &mut [f64]) {
    for i in 1..values.len() {
        if values[i] > values[i - 1] {
            values[i] = values[i - 1];
        }
    }
}

pub fn apply_core_calibration(
    markets: &mut GoalMarketProbabilities,
    calibration: Option<&CoreCalibration>,
) {
    let Some(cal) = calibration else { return };
    if let Some(t) = cal.match_result_temperature {
        markets.match_result = calibrate_match_result(markets.match_result, t);
    }
    for (key, value) in [
        ("TOTAL_GOALS", &mut markets.total_over[..]),
        ("HOME_TEAM_GOALS", &mut markets.home_over[..]),
        ("AWAY_TEAM_GOALS", &mut markets.away_over[..]),
    ] {
        for (i, item) in value.iter_mut().enumerate() {
            let line_key = match key {
                "TOTAL_GOALS" => format!("TOTAL_GOALS_O{}", ["1.5", "2.5", "3.5"][i]),
                "HOME_TEAM_GOALS" => format!("HOME_TEAM_GOALS_O{}", ["0.5", "1.5"][i]),
                _ => format!("AWAY_TEAM_GOALS_O{}", ["0.5", "1.5"][i]),
            };
            if let Some(p) = cal
                .binary
                .get(&line_key)
                .or_else(|| cal.binary.get(key))
                .filter(|p| p.status == "CALIBRATED")
            {
                *item = calibrate_binary(*item, p);
            }
        }
        project_monotone(value);
    }
    if let Some(p) = cal.binary.get("BTTS").filter(|p| p.status == "CALIBRATED") {
        markets.btts_yes = calibrate_binary(markets.btts_yes, p);
    }
}

pub fn infer_core(
    model: &CoreModel,
    calibration: Option<&CoreCalibration>,
    base_home_lambda: f64,
    base_away_lambda: f64,
    base_result: [f64; 3],
    base_btts: f64,
    features: &BTreeMap<String, f64>,
) -> GoalMarketProbabilities {
    let home = apply_offset(base_home_lambda, &model.home_goal_offset, features);
    let away = apply_offset(base_away_lambda, &model.away_goal_offset, features);
    let mut output = revised_goal_markets(home, away);
    let learned_result = revised_multiclass(base_result, &model.match_result, features);
    output.match_result = [0usize, 1, 2].map(|i| {
        model.match_result_blend * learned_result[i]
            + (1.0 - model.match_result_blend) * output.match_result[i]
    });
    let x = core_x(features);
    let learned_btts = sigmoid(
        logit(base_btts)
            + model.btts.intercept
            + x.iter()
                .enumerate()
                .map(|(i, value)| {
                    value
                        * model
                            .btts
                            .coefficients
                            .get(FEATURE_REGISTRY[i])
                            .copied()
                            .unwrap_or(0.0)
                })
                .sum::<f64>(),
    );
    output.btts_yes = model.btts_blend * learned_btts + (1.0 - model.btts_blend) * output.btts_yes;
    apply_core_calibration(&mut output, calibration);
    output
}

fn safe_lambda(v: f64) -> f64 {
    if v.is_finite() {
        v.max(1e-6).min(20.0)
    } else {
        1e-6
    }
}
fn core_x(features: &BTreeMap<String, f64>) -> Vec<f64> {
    FEATURE_REGISTRY
        .iter()
        .map(|n| {
            if *n == "base_logit" {
                0.0
            } else {
                features.get(*n).copied().unwrap_or(0.0)
            }
        })
        .collect()
}
fn poisson_offset(rows: &[CoreTrainingRow], home: bool) -> LinearCorrection {
    let mut w = vec![0.0; FEATURE_REGISTRY.len()];
    let mut b = 0.0;
    for _ in 0..300 {
        for r in rows {
            let x = core_x(&r.features);
            let base = if home {
                safe_lambda(r.base_home_lambda)
            } else {
                safe_lambda(r.base_away_lambda)
            };
            let y = if home { r.home_goals } else { r.away_goals };
            let eta = (base.ln() + b + w.iter().zip(&x).map(|(a, z)| a * z).sum::<f64>())
                .clamp(-10.0, 10.0);
            let e = eta.exp() - y;
            b -= 0.005 * e;
            for (i, z) in x.iter().enumerate() {
                w[i] -= 0.005 * (e * z + 0.001 * w[i]);
            }
        }
    }
    LinearCorrection {
        intercept: b,
        coefficients: FEATURE_REGISTRY
            .iter()
            .zip(w)
            .map(|(n, v)| (n.to_string(), v))
            .collect(),
    }
}
fn softmax_fit(rows: &[CoreTrainingRow]) -> Vec<LinearCorrection> {
    let mut weights = vec![vec![0.0; FEATURE_REGISTRY.len()]; 3];
    let mut intercepts = [0.0; 3];
    for _ in 0..400 {
        for row in rows {
            let x = core_x(&row.features);
            let logits = [0usize, 1, 2].map(|k| {
                intercepts[k]
                    + logit(row.base_result[k])
                    + weights[k].iter().zip(&x).map(|(w, v)| w * v).sum::<f64>()
            });
            let p = softmax(logits);
            for k in 0..3 {
                let error = p[k] - if row.result_class == k { 1.0 } else { 0.0 };
                intercepts[k] -= 0.01 * error;
                for (j, value) in x.iter().enumerate() {
                    weights[k][j] -= 0.01 * (error * value + 0.001 * weights[k][j]);
                }
            }
        }
    }
    (0..3)
        .map(|k| LinearCorrection {
            intercept: intercepts[k],
            coefficients: FEATURE_REGISTRY
                .iter()
                .zip(weights[k].iter().copied())
                .map(|(name, value)| (name.to_string(), value))
                .collect(),
        })
        .collect()
}
fn binary_fit_core(rows: &[CoreTrainingRow]) -> LinearCorrection {
    let binary = rows
        .iter()
        .map(|r| LegacyMarketTrainingRow {
            as_of: r.as_of.clone(),
            market: "CORE".into(),
            base_probability: r.base_btts,
            label: r.btts,
            features: r.features.clone(),
        })
        .collect::<Vec<_>>();
    let fitted = fit(
        &binary,
        "CORE",
        &Preprocessing {
            means: BTreeMap::new(),
            stddevs: BTreeMap::new(),
        },
    );
    LinearCorrection {
        intercept: fitted.intercept,
        coefficients: fitted.coefficients,
    }
}
fn choose_blend(rows: &[CoreTrainingRow], model: &CoreModel, match_result: bool) -> f64 {
    if rows.is_empty() {
        return 0.0;
    }
    let mut best = (f64::INFINITY, 0.0);
    for step in 0..=10 {
        let weight = step as f64 / 10.0;
        let loss = rows
            .iter()
            .map(|row| {
                let poisson = revised_goal_markets(
                    apply_offset(row.base_home_lambda, &model.home_goal_offset, &row.features),
                    apply_offset(row.base_away_lambda, &model.away_goal_offset, &row.features),
                );
                if match_result {
                    let learned =
                        revised_multiclass(row.base_result, &model.match_result, &row.features);
                    let p = [0usize, 1, 2]
                        .map(|k| weight * learned[k] + (1.0 - weight) * poisson.match_result[k]);
                    -p[row.result_class].max(1e-12).ln()
                } else {
                    let learned = sigmoid(
                        model.btts.intercept
                            + core_x(&row.features)
                                .iter()
                                .enumerate()
                                .map(|(i, x)| {
                                    x * model
                                        .btts
                                        .coefficients
                                        .get(FEATURE_REGISTRY[i])
                                        .copied()
                                        .unwrap_or(0.0)
                                })
                                .sum::<f64>()
                            + logit(row.base_btts),
                    );
                    let p = weight * learned + (1.0 - weight) * poisson.btts_yes;
                    if row.btts {
                        -p.max(1e-12).ln()
                    } else {
                        -(1.0 - p).max(1e-12).ln()
                    }
                }
            })
            .sum::<f64>()
            / rows.len() as f64;
        if loss < best.0 {
            best = (loss, weight);
        }
    }
    best.1
}

fn fit_core_model(rows: &[CoreTrainingRow]) -> CoreModel {
    CoreModel {
        home_goal_offset: poisson_offset(rows, true),
        away_goal_offset: poisson_offset(rows, false),
        match_result: softmax_fit(rows),
        btts: binary_fit_core(rows),
        match_result_blend: 0.0,
        btts_blend: 0.0,
    }
}
fn softmax(logits: [f64; 3]) -> [f64; 3] {
    let m = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let e = logits.map(|x| (x - m).exp());
    let s = e.iter().sum::<f64>();
    [e[0] / s, e[1] / s, e[2] / s]
}
fn pmf(lambda: f64, n: usize) -> Vec<f64> {
    let mut p = vec![0.0; n + 1];
    p[0] = (-lambda).exp();
    for i in 1..=n {
        p[i] = p[i - 1] * lambda / i as f64;
    }
    p
}
fn over(lambda: f64, line: f64) -> f64 {
    let k = line.floor() as usize;
    1.0 - pmf(safe_lambda(lambda), k).iter().sum::<f64>()
}
pub fn revised_goal_markets(home: f64, away: f64) -> GoalMarketProbabilities {
    let h = safe_lambda(home);
    let a = safe_lambda(away);
    let hp = pmf(h, 40);
    let ap = pmf(a, 40);
    let mut result = [0.0; 3];
    for i in 0..=40 {
        for j in 0..=40 {
            let p = hp[i] * ap[j];
            if i > j {
                result[0] += p
            } else if i == j {
                result[1] += p
            } else {
                result[2] += p
            }
        }
    }
    let total_over = [over(h + a, 1.5), over(h + a, 2.5), over(h + a, 3.5)];
    let home_over = [over(h, 0.5), over(h, 1.5)];
    let away_over = [over(a, 0.5), over(a, 1.5)];
    let btts = 1.0 - (-h).exp() - (-a).exp() + (-(h + a)).exp();
    GoalMarketProbabilities {
        home_lambda: h,
        away_lambda: a,
        match_result: {
            let s = result.iter().sum::<f64>();
            [result[0] / s, result[1] / s, result[2] / s]
        },
        total_over,
        home_over,
        away_over,
        btts_yes: btts.clamp(0.0, 1.0),
    }
}
pub fn apply_offset(base: f64, model: &LinearCorrection, features: &BTreeMap<String, f64>) -> f64 {
    let x = core_x(features);
    safe_lambda(
        base * ((model.intercept
            + x.iter()
                .enumerate()
                .map(|(i, z)| {
                    z * model
                        .coefficients
                        .get(FEATURE_REGISTRY[i])
                        .copied()
                        .unwrap_or(0.0)
                })
                .sum::<f64>())
        .clamp(-5.0, 5.0))
        .exp(),
    )
}
pub fn revised_multiclass(
    base: [f64; 3],
    models: &[LinearCorrection],
    features: &BTreeMap<String, f64>,
) -> [f64; 3] {
    let x = core_x(features);
    let logits = [0usize, 1, 2].map(|i| {
        let l = logit(base[i]);
        models
            .get(i)
            .map(|m| {
                m.intercept
                    + x.iter()
                        .enumerate()
                        .map(|(j, z)| {
                            z * m
                                .coefficients
                                .get(FEATURE_REGISTRY[j])
                                .copied()
                                .unwrap_or(0.0)
                        })
                        .sum::<f64>()
                    + l
            })
            .unwrap_or(l)
    });
    softmax(logits)
}
pub fn train_core(rows: &mut [CoreTrainingRow]) -> Result<CoreModel, String> {
    rows.sort_by(|a, b| a.as_of.cmp(&b.as_of));
    if rows.len() < 30 {
        return Err("LINEUP_MODEL_INSUFFICIENT_DATA".into());
    }
    let train_end = rows.len() * 70 / 100;
    let train = &rows[..train_end];
    let home = poisson_offset(train, true);
    let away = poisson_offset(train, false);
    let mut model = CoreModel {
        home_goal_offset: home,
        away_goal_offset: away,
        match_result: softmax_fit(train),
        btts: binary_fit_core(train),
        match_result_blend: 0.0,
        btts_blend: 0.0,
    };
    model.match_result_blend = choose_blend(&rows[train_end..], &model, true);
    model.btts_blend = choose_blend(&rows[train_end..], &model, false);
    Ok(model)
}

#[derive(Debug)]
struct PersistedBaseRow {
    id: i64,
    market: String,
    line: Option<f64>,
    selection: String,
    probability: f64,
    is_oos: bool,
    home_lambda: Option<f64>,
    away_lambda: Option<f64>,
    fold: usize,
    cutoff: String,
    model_version: String,
    model_hash: String,
    calibration_version: String,
    calibration_hash: String,
}

fn relevant_base_row<'a>(
    rows: &'a [PersistedBaseRow],
    market: &str,
    selection: &str,
) -> Option<&'a PersistedBaseRow> {
    rows.iter()
        .find(|r| r.market == market && r.selection == selection && r.line.is_none())
}

pub fn build_core_training_dataset(c: &Connection) -> Result<CoreTrainingDataset, String> {
    let mut diagnostics = CoreTrainingDatasetDiagnostics::default();
    let mut matches = c
        .prepare("SELECT id,competition_id,kickoff_at,final_home_goals,final_away_goals FROM matches WHERE status='finished' OR final_home_goals IS NOT NULL OR final_away_goals IS NOT NULL ORDER BY kickoff_at,id")
        .map_err(|e| e.to_string())?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    diagnostics.historical_matches_considered = matches.len();
    let mut accepted = Vec::new();

    for (match_id, competition_id, kickoff, home_goals, away_goals) in matches.drain(..) {
        let Some(home_goals) = home_goals else {
            diagnostics.missing_result += 1;
            continue;
        };
        let Some(away_goals) = away_goals else {
            diagnostics.missing_result += 1;
            continue;
        };
        let mut base_stmt = c
            .prepare("SELECT bp.id,bp.market,bp.line_value,bp.selection,bp.raw_probability,bp.out_of_sample,bp.base_home_lambda,bp.base_away_lambda,bp.fold,bp.generated_at,br.model_family_version,COALESCE(mv.artifact_sha256,''),COALESCE(cm.calibration_version,''),COALESCE(cm.artifact_sha256,'') FROM backtest_predictions bp JOIN backtest_runs br ON br.id=bp.run_id LEFT JOIN model_versions mv ON mv.version_identifier=br.model_family_version LEFT JOIN calibration_models cm ON cm.id=(SELECT id FROM calibration_models WHERE parent_model_version=br.model_family_version ORDER BY id DESC LIMIT 1) WHERE bp.match_id=?1 ORDER BY bp.generated_at DESC,bp.id DESC")
            .map_err(|e| e.to_string())?;
        let base_rows = base_stmt
            .query_map([match_id], |r| {
                Ok(PersistedBaseRow {
                    id: r.get(0)?,
                    market: r.get(1)?,
                    line: r.get(2)?,
                    selection: r.get(3)?,
                    probability: r.get(4)?,
                    is_oos: r.get::<_, i64>(5)? == 1,
                    home_lambda: r.get(6)?,
                    away_lambda: r.get(7)?,
                    fold: r.get::<_, i64>(8)? as usize,
                    cutoff: r.get(9)?,
                    model_version: r.get(10)?,
                    model_hash: r.get(11)?,
                    calibration_version: r.get(12)?,
                    calibration_hash: r.get(13)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let Some(home) = relevant_base_row(&base_rows, "MATCH_RESULT", "HOME") else {
            diagnostics.missing_oos_base_prediction += 1;
            continue;
        };
        let Some(draw) = relevant_base_row(&base_rows, "MATCH_RESULT", "DRAW") else {
            diagnostics.missing_base_probability += 1;
            continue;
        };
        let Some(away) = relevant_base_row(&base_rows, "MATCH_RESULT", "AWAY") else {
            diagnostics.missing_base_probability += 1;
            continue;
        };
        let Some(btts) = relevant_base_row(&base_rows, "BTTS", "YES") else {
            diagnostics.missing_base_probability += 1;
            continue;
        };
        let relevant = [home, draw, away, btts];
        if relevant.iter().any(|r| !r.is_oos) {
            diagnostics.base_prediction_not_oos += 1;
            continue;
        }
        if relevant.iter().any(|r| r.cutoff >= kickoff || r.fold == 0) {
            diagnostics.base_prediction_not_oos += 1;
            continue;
        }
        if relevant
            .iter()
            .any(|r| r.home_lambda.is_none() || r.away_lambda.is_none())
        {
            diagnostics.missing_base_lambda += 1;
            continue;
        }
        if relevant
            .iter()
            .any(|r| !r.probability.is_finite() || !(0.0..=1.0).contains(&r.probability))
        {
            diagnostics.invalid_non_finite_value += 1;
            continue;
        }
        if relevant.iter().any(|r| {
            r.home_lambda.is_none_or(|v| !v.is_finite() || v <= 0.0)
                || r.away_lambda.is_none_or(|v| !v.is_finite() || v <= 0.0)
        }) {
            diagnostics.invalid_non_finite_value += 1;
            continue;
        }
        if relevant
            .iter()
            .any(|r| r.model_version.is_empty() || r.model_hash.is_empty())
        {
            diagnostics.missing_model_lineage += 1;
            continue;
        }
        if relevant
            .iter()
            .any(|r| r.calibration_version.is_empty() || r.calibration_hash.is_empty())
        {
            diagnostics.missing_calibration_lineage += 1;
            continue;
        }
        if relevant.iter().any(|r| {
            r.model_version != home.model_version
                || r.model_hash != home.model_hash
                || r.calibration_version != home.calibration_version
                || r.calibration_hash != home.calibration_hash
        }) {
            diagnostics.lineage_mismatch += 1;
            continue;
        }

        let feature = c
            .query_row("SELECT fs.id,fs.lineup_snapshot_id,fs.quality_status,fs.feature_version,fs.features_json,s.is_official,s.completeness_status,(SELECT COUNT(*) FROM lineup_players lp WHERE lp.lineup_snapshot_id=s.id AND lp.side='HOME' AND lp.role='STARTER'),(SELECT COUNT(*) FROM lineup_players lp WHERE lp.lineup_snapshot_id=s.id AND lp.side='AWAY' AND lp.role='STARTER') FROM lineup_feature_sets fs JOIN lineup_snapshots s ON s.id=fs.lineup_snapshot_id WHERE fs.match_id=?1 AND fs.feature_version=?2 ORDER BY fs.id DESC LIMIT 1", params![match_id, LINEUP_FEATURE_VERSION], |r| {
                Ok((r.get::<_, i64>(0)?,r.get::<_, i64>(1)?,r.get::<_, String>(2)?,r.get::<_, String>(3)?,r.get::<_, String>(4)?,r.get::<_, i64>(5)? == 1,r.get::<_, String>(6)?,r.get::<_, i64>(7)?,r.get::<_, i64>(8)?))
            })
            .optional()
            .map_err(|e| e.to_string())?;
        let Some((
            feature_id,
            snapshot_id,
            quality,
            version,
            feature_json,
            official,
            completeness,
            home_starters,
            away_starters,
        )) = feature
        else {
            diagnostics.missing_feature_set += 1;
            continue;
        };
        if !official {
            diagnostics.missing_official_lineup += 1;
            continue;
        }
        if completeness != "COMPLETE_OFFICIAL" || home_starters != 11 || away_starters != 11 {
            diagnostics.incomplete_lineup += 1;
            continue;
        }
        if version != LINEUP_FEATURE_VERSION || quality != "COMPLETE" {
            diagnostics.poor_lineup_feature_quality += 1;
            continue;
        }
        let features = match lineup_feature_vector(&feature_json) {
            Ok(value) if value.values().all(|v| v.is_finite()) => value,
            _ => {
                diagnostics.poor_lineup_feature_quality += 1;
                continue;
            }
        };
        let match_result_label = if home_goals > away_goals {
            "HOME"
        } else if home_goals == away_goals {
            "DRAW"
        } else {
            "AWAY"
        };
        let row = TrainingRow {
            match_id,
            competition_id,
            kickoff_utc: kickoff.clone(),
            base_prediction_id: home.id,
            base_prediction_is_oos: true,
            base_training_cutoff: home.cutoff.clone(),
            base_fold: home.fold,
            base_model_version: home.model_version.clone(),
            base_model_hash: home.model_hash.clone(),
            base_calibration_version: home.calibration_version.clone(),
            base_calibration_hash: home.calibration_hash.clone(),
            base_home_lambda: home.home_lambda.unwrap(),
            base_away_lambda: home.away_lambda.unwrap(),
            base_home_probability: home.probability,
            base_draw_probability: draw.probability,
            base_away_probability: away.probability,
            base_btts_yes_probability: btts.probability,
            lineup_snapshot_id: snapshot_id,
            lineup_feature_set_id: feature_id,
            lineup_feature_vector: features,
            lineup_feature_quality: quality,
            actual_home_goals: home_goals,
            actual_away_goals: away_goals,
            match_result_label: match_result_label.into(),
            btts_label: home_goals > 0 && away_goals > 0,
            total_over_15_label: home_goals + away_goals > 1,
            total_over_25_label: home_goals + away_goals > 2,
            total_over_35_label: home_goals + away_goals > 3,
            home_over_05_label: home_goals > 0,
            home_over_15_label: home_goals > 1,
            away_over_05_label: away_goals > 0,
            away_over_15_label: away_goals > 1,
        };
        row.to_core_training_row()?;
        accepted.push(row);
    }
    accepted.sort_by(|a, b| {
        a.kickoff_utc
            .cmp(&b.kickoff_utc)
            .then(a.match_id.cmp(&b.match_id))
    });
    diagnostics.accepted = accepted.len();
    Ok(CoreTrainingDataset {
        rows: accepted,
        diagnostics,
    })
}

fn hash_artifact(a: &LineupAdjustmentArtifact) -> Result<String, String> {
    let mut v = serde_json::to_value(a).map_err(|e| e.to_string())?;
    let object = v.as_object_mut().unwrap();
    object.remove("artifact_sha256");
    object.remove("created_at");
    fn canonicalize(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Number(number) => {
                if let Some(float) = number.as_f64() {
                    *value = serde_json::Value::String(format!("{float:.12e}"));
                }
            }
            serde_json::Value::Array(values) => values.iter_mut().for_each(canonicalize),
            serde_json::Value::Object(values) => {
                let sorted = values
                    .iter_mut()
                    .map(|(key, value)| {
                        canonicalize(value);
                        (key.clone(), value.clone())
                    })
                    .collect::<BTreeMap<_, _>>();
                values.clear();
                values.extend(sorted);
            }
            _ => {}
        }
    }
    canonicalize(&mut v);
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&v).map_err(|e| e.to_string())?)
    ))
}
fn clip(p: f64) -> f64 {
    p.max(1e-9).min(1.0 - 1e-9)
}
fn logit(p: f64) -> f64 {
    let p = clip(p);
    (p / (1.0 - p)).ln()
}
fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}
fn vector(row: &LegacyMarketTrainingRow, prep: &Preprocessing) -> Vec<f64> {
    FEATURE_REGISTRY
        .iter()
        .map(|name| {
            let raw = if *name == "base_logit" {
                logit(row.base_probability)
            } else {
                *row.features.get(*name).unwrap_or(&0.0)
            };
            (raw - *prep.means.get(*name).unwrap_or(&0.0))
                / prep.stddevs.get(*name).copied().unwrap_or(1.0)
        })
        .collect()
}
fn fit(rows: &[LegacyMarketTrainingRow], market: &str, prep: &Preprocessing) -> MarketModel {
    let mut w = vec![0.0; FEATURE_REGISTRY.len()];
    let mut b = 0.0;
    for _ in 0..500 {
        for row in rows.iter().filter(|r| r.market == market) {
            let x = vector(row, prep);
            let p = sigmoid(b + w.iter().zip(&x).map(|(a, c)| a * c).sum::<f64>());
            let e = p - if row.label { 1.0 } else { 0.0 };
            b -= 0.02 * e;
            for (j, xj) in x.iter().enumerate() {
                w[j] -= 0.02 * (e * xj + 0.001 * w[j]);
            }
        }
    }
    let coefficients = FEATURE_REGISTRY
        .iter()
        .zip(w)
        .map(|(n, v)| (n.to_string(), v))
        .collect();
    MarketModel {
        market: market.to_string(),
        coefficients,
        intercept: b,
        validation_log_loss: 0.0,
        test_log_loss: 0.0,
        base_test_log_loss: 0.0,
        status: "ACTIVE".into(),
        sample_count: rows.iter().filter(|r| r.market == market).count(),
    }
}
fn loss(rows: &[LegacyMarketTrainingRow], m: &MarketModel, prep: &Preprocessing) -> f64 {
    if rows.is_empty() {
        return 0.0;
    }
    rows.iter()
        .map(|r| {
            let z = m.intercept
                + FEATURE_REGISTRY
                    .iter()
                    .map(|n| {
                        m.coefficients[*n]
                            * ((if *n == "base_logit" {
                                logit(r.base_probability)
                            } else {
                                *r.features.get(*n).unwrap_or(&0.0)
                            }) - *prep.means.get(*n).unwrap_or(&0.0))
                            / prep.stddevs.get(*n).unwrap_or(&1.0)
                    })
                    .sum::<f64>();
            let p = clip(sigmoid(z));
            if r.label {
                -p.ln()
            } else {
                -(1.0 - p).ln()
            }
        })
        .sum::<f64>()
        / rows.len() as f64
}
pub fn train_rows(
    rows: &mut [TrainingRow],
    model_version: &str,
    output: &Path,
    parent_model_version: Option<String>,
    parent_model_hash: Option<String>,
) -> Result<TrainReport, String> {
    rows.sort_by(|a, b| {
        a.kickoff_utc
            .cmp(&b.kickoff_utc)
            .then(a.match_id.cmp(&b.match_id))
    });
    if rows.len()
        < MIN_LINEUP_TRAINING_MATCHES + MIN_LINEUP_VALIDATION_MATCHES + MIN_LINEUP_TEST_MATCHES
    {
        return Err("LINEUP_MODEL_INSUFFICIENT_DATA".into());
    }
    let core_rows = rows
        .iter()
        .map(TrainingRow::to_core_training_row)
        .collect::<Result<Vec<_>, _>>()?;
    let core_train_end = core_rows.len() * 70 / 100;
    let core_validation_end = core_rows.len() * 85 / 100;
    let mut core_model = fit_core_model(&core_rows[..core_train_end]);
    core_model.match_result_blend = choose_blend(
        &core_rows[core_train_end..core_validation_end],
        &core_model,
        true,
    );
    core_model.btts_blend = choose_blend(
        &core_rows[core_train_end..core_validation_end],
        &core_model,
        false,
    );
    let validation_core = &core_rows[core_train_end..core_validation_end];
    let test_core = &core_rows[core_validation_end..];
    let mut core_calibration = CoreCalibration::default();
    core_calibration.match_result_temperature = fit_temperature(validation_core, &core_model);
    if let Some(p) = fit_platt(validation_core, |r| r.base_btts) {
        core_calibration.binary.insert("BTTS".into(), p);
    }
    for (key, idx, threshold) in [
        ("TOTAL_GOALS_O1.5", 0usize, 1.0),
        ("TOTAL_GOALS_O2.5", 1, 2.0),
        ("TOTAL_GOALS_O3.5", 2, 3.0),
    ] {
        if let Some(p) = fit_platt_with_label(
            validation_core,
            move |r| revised_goal_markets(r.base_home_lambda, r.base_away_lambda).total_over[idx],
            move |r| r.home_goals + r.away_goals > threshold,
        ) {
            core_calibration.binary.insert(key.into(), p);
        }
    }
    for (key, idx, threshold, home) in [
        ("HOME_TEAM_GOALS_O0.5", 0usize, 0.0, true),
        ("HOME_TEAM_GOALS_O1.5", 1, 1.0, true),
        ("AWAY_TEAM_GOALS_O0.5", 0, 0.0, false),
        ("AWAY_TEAM_GOALS_O1.5", 1, 1.0, false),
    ] {
        if let Some(p) = fit_platt_with_label(
            validation_core,
            move |r| {
                let g = revised_goal_markets(r.base_home_lambda, r.base_away_lambda);
                if home {
                    g.home_over[idx]
                } else {
                    g.away_over[idx]
                }
            },
            move |r| {
                if home {
                    r.home_goals > threshold
                } else {
                    r.away_goals > threshold
                }
            },
        ) {
            core_calibration.binary.insert(key.into(), p);
        }
    }
    let legacy_rows = rows.iter().flat_map(legacy_market_rows).collect::<Vec<_>>();
    let n = legacy_rows.len();
    let tr = n * 70 / 100;
    let va = n * 85 / 100;
    let train_rows = &legacy_rows[..tr];
    let validation = &legacy_rows[tr..va];
    let test = &legacy_rows[va..];
    let mut means = BTreeMap::new();
    let mut stds = BTreeMap::new();
    for name in FEATURE_REGISTRY {
        let vals: Vec<f64> = train_rows
            .iter()
            .map(|r| {
                if *name == "base_logit" {
                    logit(r.base_probability)
                } else {
                    *r.features.get(*name).unwrap_or(&0.0)
                }
            })
            .collect();
        let mean = vals.iter().sum::<f64>() / vals.len() as f64;
        let sd = (vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / vals.len() as f64).sqrt();
        means.insert((*name).into(), mean);
        stds.insert((*name).into(), if sd < 1e-12 { 1.0 } else { sd });
    }
    let prep = Preprocessing {
        means,
        stddevs: stds,
    };
    let mut markets = BTreeMap::new();
    for market in [
        "MATCH_RESULT_HOME",
        "BTTS_YES",
        "TOTAL_GOALS_OVER_1.5",
        "TOTAL_GOALS_OVER_2.5",
        "TOTAL_GOALS_OVER_3.5",
        "HOME_TEAM_TOTAL_OVER_0.5",
        "HOME_TEAM_TOTAL_OVER_1.5",
        "AWAY_TEAM_TOTAL_OVER_0.5",
        "AWAY_TEAM_TOTAL_OVER_1.5",
    ] {
        let mut m = fit(train_rows, market, &prep);
        m.validation_log_loss = loss(validation, &m, &prep);
        m.test_log_loss = loss(test, &m, &prep);
        m.base_test_log_loss = test
            .iter()
            .filter(|r| r.market == market)
            .map(|r| {
                let p = clip(r.base_probability);
                if r.label {
                    -p.ln()
                } else {
                    -(1.0 - p).ln()
                }
            })
            .sum::<f64>()
            / test.iter().filter(|r| r.market == market).count().max(1) as f64;
        if m.sample_count < MIN_LINEUP_TRAINING_MATCHES {
            m.status = "INSUFFICIENT_DATA".into();
        } else if m.test_log_loss >= m.base_test_log_loss {
            m.status = "NOT_BENEFICIAL".into();
        }
        markets.insert(market.into(), m);
    }
    let core_metrics = {
        let mut m = BTreeMap::new();
        m.insert(
            "MATCH_RESULT".into(),
            multiclass_pair(test_core, &core_model, Some(&core_calibration)),
        );
        let base_btts = test_core
            .iter()
            .map(|r| (r.base_btts, r.btts))
            .collect::<Vec<_>>();
        let lineup_btts = test_core
            .iter()
            .map(|r| {
                (
                    infer_core(
                        &core_model,
                        Some(&core_calibration),
                        r.base_home_lambda,
                        r.base_away_lambda,
                        r.base_result,
                        r.base_btts,
                        &r.features,
                    )
                    .btts_yes,
                    r.btts,
                )
            })
            .collect::<Vec<_>>();
        m.insert("BTTS".into(), binary_pair(&base_btts, &lineup_btts));
        for (name, idx, threshold) in [
            ("O1.5", 0usize, 1.0),
            ("O2.5", 1usize, 2.0),
            ("O3.5", 2usize, 3.0),
        ] {
            let b = test_core
                .iter()
                .map(|r| {
                    (
                        revised_goal_markets(r.base_home_lambda, r.base_away_lambda).total_over
                            [idx],
                        r.home_goals + r.away_goals > threshold,
                    )
                })
                .collect::<Vec<_>>();
            let l = test_core
                .iter()
                .map(|r| {
                    (
                        infer_core(
                            &core_model,
                            Some(&core_calibration),
                            r.base_home_lambda,
                            r.base_away_lambda,
                            r.base_result,
                            r.base_btts,
                            &r.features,
                        )
                        .total_over[idx],
                        r.home_goals + r.away_goals > threshold,
                    )
                })
                .collect::<Vec<_>>();
            m.insert(name.into(), binary_pair(&b, &l));
        }
        for (name, idx, threshold, home) in [
            ("HOME_O0.5", 0usize, 0.0, true),
            ("HOME_O1.5", 1usize, 1.0, true),
            ("AWAY_O0.5", 0usize, 0.0, false),
            ("AWAY_O1.5", 1usize, 1.0, false),
        ] {
            let b = test_core
                .iter()
                .map(|r| {
                    let g = revised_goal_markets(r.base_home_lambda, r.base_away_lambda);
                    (
                        if home {
                            g.home_over[idx]
                        } else {
                            g.away_over[idx]
                        },
                        (if home { r.home_goals } else { r.away_goals }) > threshold,
                    )
                })
                .collect::<Vec<_>>();
            let l = test_core
                .iter()
                .map(|r| {
                    let g = infer_core(
                        &core_model,
                        Some(&core_calibration),
                        r.base_home_lambda,
                        r.base_away_lambda,
                        r.base_result,
                        r.base_btts,
                        &r.features,
                    );
                    (
                        if home {
                            g.home_over[idx]
                        } else {
                            g.away_over[idx]
                        },
                        (if home { r.home_goals } else { r.away_goals }) > threshold,
                    )
                })
                .collect::<Vec<_>>();
            m.insert(name.into(), binary_pair(&b, &l));
        }
        let home_count = goal_count_pair(test_core, true, &core_model);
        let away_count = goal_count_pair(test_core, false, &core_model);
        m.insert("HOME_GOALS".into(), home_count);
        m.insert("AWAY_GOALS".into(), away_count);
        m
    };
    let persisted_parent_model_version =
        parent_model_version.or_else(|| rows.first().map(|r| r.base_model_version.clone()));
    let persisted_parent_model_hash =
        parent_model_hash.or_else(|| rows.first().map(|r| r.base_model_hash.clone()));
    let mut a = LineupAdjustmentArtifact {
        artifact_schema_version: ARTIFACT_SCHEMA_VERSION.into(),
        model_version: model_version.into(),
        parent_model_version: persisted_parent_model_version,
        parent_model_hash: persisted_parent_model_hash,
        parent_calibration_version: rows.first().map(|r| r.base_calibration_version.clone()),
        parent_calibration_hash: rows.first().map(|r| r.base_calibration_hash.clone()),
        lineup_feature_version: LINEUP_FEATURE_VERSION.into(),
        feature_registry_version: FEATURE_REGISTRY_VERSION.into(),
        feature_registry: FEATURE_REGISTRY.iter().map(|s| s.to_string()).collect(),
        preprocessing: prep,
        training_period: [
            rows[0].kickoff_utc.clone(),
            rows[(rows.len() * 70 / 100) - 1].kickoff_utc.clone(),
        ],
        validation_period: [
            rows[rows.len() * 70 / 100].kickoff_utc.clone(),
            rows[(rows.len() * 85 / 100) - 1].kickoff_utc.clone(),
        ],
        test_period: [
            rows[rows.len() * 85 / 100].kickoff_utc.clone(),
            rows[rows.len() - 1].kickoff_utc.clone(),
        ],
        training_samples: rows.len() * 70 / 100,
        validation_samples: rows.len() * 15 / 100,
        test_samples: rows.len() - (rows.len() * 85 / 100),
        markets,
        core_model: Some(core_model),
        core_calibration: Some(core_calibration.clone()),
        core_metrics,
        created_at: Utc::now().to_rfc3339(),
        artifact_sha256: String::new(),
        family_statuses: BTreeMap::new(),
        benefit_gate_config: BTreeMap::from([
            (
                String::from("min_test_samples"),
                MIN_LINEUP_TEST_MATCHES.to_string(),
            ),
            (String::from("ece_tolerance"), "0.02".into()),
            (String::from("brier_tolerance"), "0.02".into()),
            (
                String::from("minimum_log_loss_improvement"),
                "0.0001".into(),
            ),
        ]),
    };
    a.family_statuses.insert(
        "MATCH_RESULT".into(),
        family_gate(
            a.core_metrics.get("MATCH_RESULT").unwrap(),
            MIN_LINEUP_TEST_MATCHES,
        ),
    );
    a.family_statuses.insert(
        "BTTS".into(),
        family_gate(a.core_metrics.get("BTTS").unwrap(), MIN_LINEUP_TEST_MATCHES),
    );
    let total_status = ["O1.5", "O2.5", "O3.5"]
        .iter()
        .map(|k| family_gate(a.core_metrics.get(*k).unwrap(), MIN_LINEUP_TEST_MATCHES))
        .collect::<Vec<_>>();
    a.family_statuses.insert(
        "TOTAL_GOALS".into(),
        if total_status.iter().all(|s| s == "ACTIVE") {
            "ACTIVE".into()
        } else if total_status.iter().any(|s| s == "INSUFFICIENT_DATA") {
            "INSUFFICIENT_DATA".into()
        } else {
            "NOT_BENEFICIAL".into()
        },
    );
    let team_status = ["HOME_O0.5", "HOME_O1.5", "AWAY_O0.5", "AWAY_O1.5"]
        .iter()
        .map(|k| family_gate(a.core_metrics.get(*k).unwrap(), MIN_LINEUP_TEST_MATCHES))
        .collect::<Vec<_>>();
    let count_support = ["HOME_GOALS", "AWAY_GOALS"].iter().all(|key| {
        a.core_metrics
            .get(*key)
            .map(|m| {
                m.sample_count >= MIN_LINEUP_TEST_MATCHES
                    && m.lineup_mae.unwrap_or(f64::INFINITY)
                        <= m.base_mae.unwrap_or(f64::NEG_INFINITY)
                    && m.lineup_rmse.unwrap_or(f64::INFINITY)
                        <= m.base_rmse.unwrap_or(f64::NEG_INFINITY)
            })
            .unwrap_or(false)
    });
    a.family_statuses.insert(
        "TEAM_TOTAL_GOALS".into(),
        if count_support && team_status.iter().all(|s| s == "ACTIVE") {
            "ACTIVE".into()
        } else if team_status.iter().any(|s| s == "INSUFFICIENT_DATA") {
            "INSUFFICIENT_DATA".into()
        } else {
            "NOT_BENEFICIAL".into()
        },
    );
    a.family_statuses
        .insert("CORNERS".into(), "NOT_SUPPORTED_V1".into());
    a.family_statuses
        .insert("CARDS".into(), "NOT_SUPPORTED_V1".into());
    fs::write(
        output,
        serde_json::to_vec_pretty(&a).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    a = serde_json::from_slice(&fs::read(output).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    a.artifact_sha256 = hash_artifact(&a)?;
    fs::write(
        output,
        serde_json::to_vec_pretty(&a).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    a = serde_json::from_slice(&fs::read(output).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    a.artifact_sha256 = hash_artifact(&a)?;
    fs::write(
        output,
        serde_json::to_vec_pretty(&a).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let active = a
        .markets
        .values()
        .filter(|m| m.status == "ACTIVE")
        .map(|m| m.market.clone())
        .collect();
    let insufficient = a
        .markets
        .values()
        .filter(|m| m.status != "ACTIVE")
        .map(|m| m.market.clone())
        .collect();
    Ok(TrainReport {
        artifact_path: output.to_string_lossy().into(),
        artifact_sha256: a.artifact_sha256,
        training_samples: rows.len() * 70 / 100,
        validation_samples: rows.len() * 15 / 100,
        test_samples: rows.len() - (rows.len() * 85 / 100),
        active_markets: active,
        insufficient_markets: insufficient,
    })
}

pub fn train(
    c: &Connection,
    model_version: &str,
    output: &Path,
    parent_model_version: Option<String>,
    parent_model_hash: Option<String>,
) -> Result<TrainReport, String> {
    let dataset = build_core_training_dataset(c)?;
    if dataset.rows.len()
        < MIN_LINEUP_TRAINING_MATCHES + MIN_LINEUP_VALIDATION_MATCHES + MIN_LINEUP_TEST_MATCHES
    {
        return Err(format!(
            "LINEUP_MODEL_INSUFFICIENT_DATA: accepted={} diagnostics={:?}",
            dataset.rows.len(),
            dataset.diagnostics
        ));
    }
    let mut rows = dataset.rows;
    train_rows(
        &mut rows,
        model_version,
        output,
        parent_model_version,
        parent_model_hash,
    )
}
pub fn load(path: &Path) -> Result<LineupAdjustmentArtifact, String> {
    let a: LineupAdjustmentArtifact =
        serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if hash_artifact(&a)? != a.artifact_sha256 {
        return Err("LINEUP_ADJUSTMENT_ARTIFACT_INVALID".into());
    }
    if a.feature_registry_version != FEATURE_REGISTRY_VERSION
        || a.lineup_feature_version != LINEUP_FEATURE_VERSION
    {
        return Err("LINEUP_FEATURE_REGISTRY_MISMATCH".into());
    }
    Ok(a)
}
pub fn register(
    c: &Connection,
    report: &TrainReport,
    a: &LineupAdjustmentArtifact,
) -> Result<i64, String> {
    let hash = hash_artifact(a)?;
    if hash != report.artifact_sha256 {
        return Err("LINEUP_ADJUSTMENT_ARTIFACT_INVALID".into());
    }
    c.execute("INSERT INTO lineup_adjustment_models(version_identifier,artifact_path,artifact_sha256,parent_model_version,parent_model_hash,feature_version,registry_hash,training_samples,validation_samples,test_samples,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'VALIDATED')",params![a.model_version,report.artifact_path,a.artifact_sha256,a.parent_model_version,a.parent_model_hash,a.lineup_feature_version,hash,a.training_samples,a.validation_samples,a.test_samples]).map_err(|e|e.to_string())?;
    Ok(c.last_insert_rowid())
}
pub fn activate(c: &Connection, version: &str) -> Result<(), String> {
    let exists:i64=c.query_row("SELECT COUNT(*) FROM lineup_adjustment_models WHERE version_identifier=?1 AND status='VALIDATED'",[version],|r|r.get(0)).map_err(|e|e.to_string())?;
    if exists == 0 {
        return Err("LINEUP_ADJUSTMENT_ARTIFACT_INVALID".into());
    }
    c.execute(
        "UPDATE lineup_adjustment_models SET status='VALIDATED' WHERE status='ACTIVE'",
        [],
    )
    .map_err(|e| e.to_string())?;
    c.execute(
        "UPDATE lineup_adjustment_models SET status='ACTIVE' WHERE version_identifier=?1",
        [version],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn status(c: &Connection) -> Result<serde_json::Value, String> {
    let row:Option<(String,String,String)>=c.query_row("SELECT version_identifier,artifact_path,status FROM lineup_adjustment_models WHERE status='ACTIVE'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;
    Ok(
        serde_json::json!({"artifact":row,"feature_version":LINEUP_FEATURE_VERSION,"adjustment_model_status":if row.is_some(){"ACTIVE"}else{"LINEUP_MODEL_NOT_TRAINED_ON_PRODUCTION_DATA"},"live_provider":"LIVE_LINEUP_PROVIDER_NOT_CONFIGURED"}),
    )
}
fn revision_payload_hash(payload: &RevisionCorePayload) -> Result<String, String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(payload).map_err(|e| e.to_string())?)
    ))
}

fn validate_revision_probability(value: Option<f64>) -> Result<(), String> {
    if let Some(value) = value {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err("INVALID_REVISION_PROBABILITY".into());
        }
    }
    Ok(())
}

pub fn persist_revision_core_payload(
    c: &Connection,
    payload: &RevisionCorePayload,
) -> Result<i64, String> {
    if payload.payload.payload_schema != REVISION_PAYLOAD_SCHEMA {
        return Err("REVISION_PAYLOAD_SCHEMA_UNSUPPORTED".into());
    }
    for value in [
        payload.base_home_lambda,
        payload.base_away_lambda,
        payload.revised_home_lambda,
        payload.revised_away_lambda,
    ] {
        if let Some(value) = value {
            if !value.is_finite() || value <= 0.0 {
                return Err("INVALID_REVISION_LAMBDA".into());
            }
        }
    }
    for market in &payload.payload.markets {
        for value in [
            market.base_raw_probability,
            market.base_public_probability,
            market.revised_raw_probability,
            market.revised_calibrated_probability,
            market.final_public_probability,
        ] {
            validate_revision_probability(value)?;
        }
        if let Some(delta) = market.delta_percentage_points {
            if !delta.is_finite() {
                return Err("INVALID_REVISION_DELTA".into());
            }
        }
        if !matches!(
            market.family_status.as_str(),
            "ACTIVE" | "NOT_BENEFICIAL" | "INSUFFICIENT_DATA" | "NOT_SUPPORTED_V1"
        ) {
            return Err("INVALID_REVISION_FAMILY_STATUS".into());
        }
        if !matches!(
            market.calibration_status.as_str(),
            "CALIBRATED" | "UNCALIBRATED_INSUFFICIENT_DATA" | "NOT_SUPPORTED_V1"
        ) {
            return Err("INVALID_REVISION_CALIBRATION_STATUS".into());
        }
    }
    let json = serde_json::to_string(&payload.payload).map_err(|e| e.to_string())?;
    let hash = revision_payload_hash(payload)?;
    c.execute("INSERT OR IGNORE INTO prediction_revisions(prediction_id,match_id,revision_type,parent_prediction_id,lineup_snapshot_id,lineup_feature_set_id,generated_at,revision_reason,model_version,calibration_version,lineup_model_version,revision_status,revised_probability,base_model_version,base_model_hash,base_calibration_version,base_calibration_hash,lineup_model_hash,base_home_lambda,base_away_lambda,revised_home_lambda,revised_away_lambda,payload_schema,market_payload_json,payload_hash) VALUES(?1,?2,'LINEUP_AWARE_PREMATCH',?1,?3,?4,?5,?6,?7,?9,?7,'REVISION_AVAILABLE',NULL,?8,?10,?9,?11,?12,?13,?14,?15,?16,?17,?18,?19)", params![payload.prediction_id,payload.match_id,payload.lineup_snapshot_id,payload.lineup_feature_set_id,payload.generated_at,payload.revision_reason,payload.lineup_model_version,payload.base_model_version,payload.base_calibration_version,payload.base_model_hash,payload.base_calibration_hash,payload.lineup_model_hash,payload.base_home_lambda,payload.base_away_lambda,payload.revised_home_lambda,payload.revised_away_lambda,REVISION_PAYLOAD_SCHEMA,json,hash]).map_err(|e| e.to_string())?;
    c.query_row("SELECT id FROM prediction_revisions WHERE prediction_id=?1 AND lineup_snapshot_id=?2 AND lineup_feature_set_id=?3 AND revision_type='LINEUP_AWARE_PREMATCH' AND payload_hash=?4", params![payload.prediction_id,payload.lineup_snapshot_id,payload.lineup_feature_set_id,hash], |r| r.get(0)).map_err(|e| e.to_string())
}

pub fn load_revision_core_payload(
    c: &Connection,
    revision_id: i64,
) -> Result<PersistedRevisionCorePayload, String> {
    c.query_row("SELECT id,prediction_id,match_id,lineup_snapshot_id,lineup_feature_set_id,base_model_version,base_model_hash,base_calibration_version,base_calibration_hash,lineup_model_version,lineup_model_hash,base_home_lambda,base_away_lambda,revised_home_lambda,revised_away_lambda,payload_schema,market_payload_json,payload_hash FROM prediction_revisions WHERE id=?1", [revision_id], |r| Ok(PersistedRevisionCorePayload { id:r.get(0)?, prediction_id:r.get(1)?, match_id:r.get(2)?, lineup_snapshot_id:r.get(3)?, lineup_feature_set_id:r.get(4)?, base_model_version:r.get(5)?, base_model_hash:r.get(6)?, base_calibration_version:r.get(7)?, base_calibration_hash:r.get(8)?, lineup_model_version:r.get(9)?, lineup_model_hash:r.get(10)?, base_home_lambda:r.get(11)?, base_away_lambda:r.get(12)?, revised_home_lambda:r.get(13)?, revised_away_lambda:r.get(14)?, payload_schema:r.get(15)?, market_payload_json:r.get(16)?, payload_hash:r.get(17)? })).map_err(|e| e.to_string())
}

pub fn generate_revision(
    c: &Connection,
    prediction_id: i64,
    feature_set_id: i64,
) -> Result<RevisionResult, String> {
    let (match_id, market, selection, base_probability, run_id, model_id, kickoff): (i64,String,String,f64,Option<i64>,i64,String) = c.query_row("SELECT match_id,market,selection,COALESCE(public_probability,model_probability),prediction_run_id,model_version_id,kickoff_at FROM predictions WHERE id=?1", [prediction_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).map_err(|_| "PREDICTION_NOT_FOUND".to_string())?;
    let run_id = run_id.ok_or_else(|| "BASE_PREDICTION_NOT_OOS".to_string())?;
    let (base_home_lambda, base_away_lambda, base_model_hash): (Option<f64>,Option<f64>,String) = c.query_row("SELECT pr.base_home_lambda,pr.base_away_lambda,mv.artifact_sha256 FROM prediction_runs pr JOIN model_versions mv ON mv.id=pr.model_version_id WHERE pr.id=?1", [run_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|e| e.to_string())?;
    let base_home_lambda = base_home_lambda.ok_or_else(|| "MISSING_BASE_LAMBDA".to_string())?;
    let base_away_lambda = base_away_lambda.ok_or_else(|| "MISSING_BASE_LAMBDA".to_string())?;
    let (base_model_version, base_calibration_version, base_calibration_hash): (String,Option<String>,Option<String>) = c.query_row("SELECT mv.version_identifier,p.calibration_version,cm.artifact_sha256 FROM predictions p JOIN model_versions mv ON mv.id=p.model_version_id LEFT JOIN calibration_models cm ON cm.calibration_version=p.calibration_version WHERE p.id=?1", [prediction_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|e| e.to_string())?;
    let base_calibration_version =
        base_calibration_version.ok_or_else(|| "BASE_CALIBRATION_INCOMPATIBLE".to_string())?;
    let base_calibration_hash =
        base_calibration_hash.ok_or_else(|| "BASE_CALIBRATION_INCOMPATIBLE".to_string())?;
    let (snapshot_id, features_json): (i64,String) = c.query_row("SELECT lineup_snapshot_id,features_json FROM lineup_feature_sets WHERE id=?1 AND quality_status='COMPLETE'", [feature_set_id], |r| Ok((r.get(0)?,r.get(1)?))).map_err(|_| "LINEUP_ADJUSTMENT_INSUFFICIENT_DATA".to_string())?;
    let features = lineup_feature_vector(&features_json)?;
    let active: Option<(String,String)> = c.query_row("SELECT version_identifier,artifact_path FROM lineup_adjustment_models WHERE status='ACTIVE'", [], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e| e.to_string())?;
    let Some((version, path)) = active else {
        return Err("LINEUP_ADJUSTMENT_INSUFFICIENT_DATA".into());
    };
    let artifact = load(Path::new(&path))?;
    if artifact.parent_model_version.as_deref() != Some(base_model_version.as_str())
        || artifact.parent_model_hash.as_deref() != Some(base_model_hash.as_str())
    {
        return Err("BASE_MODEL_INCOMPATIBLE".into());
    }
    if artifact.parent_calibration_version.as_deref() != Some(base_calibration_version.as_str())
        || artifact.parent_calibration_hash.as_deref() != Some(base_calibration_hash.as_str())
    {
        return Err("BASE_CALIBRATION_INCOMPATIBLE".into());
    }
    let mut base_result = [0.0; 3];
    for (i, sel) in ["HOME", "DRAW", "AWAY"].iter().enumerate() {
        base_result[i] = c.query_row("SELECT COALESCE(public_probability,model_probability) FROM predictions WHERE prediction_run_id=?1 AND market='MATCH_RESULT' AND selection=?2 AND line_value IS NULL", params![run_id,sel], |r| r.get(0)).map_err(|_| "BASE_PREDICTION_NOT_OOS".to_string())?;
    }
    let base_btts: f64 = c.query_row("SELECT COALESCE(public_probability,model_probability) FROM predictions WHERE prediction_run_id=?1 AND market='BTTS' AND selection='YES' AND line_value IS NULL", [run_id], |r| r.get(0)).map_err(|_| "BASE_PREDICTION_NOT_OOS".to_string())?;
    let revised = infer_core(
        artifact
            .core_model
            .as_ref()
            .ok_or_else(|| "LINEUP_MODEL_INSUFFICIENT_DATA".to_string())?,
        artifact.core_calibration.as_ref(),
        base_home_lambda,
        base_away_lambda,
        base_result,
        base_btts,
        &features,
    );
    let base_goals = revised_goal_markets(base_home_lambda, base_away_lambda);
    let family = |name: &str| {
        artifact
            .family_statuses
            .get(name)
            .cloned()
            .unwrap_or_else(|| "NOT_BENEFICIAL".into())
    };
    let market_entry = |m: &str, s: &str, line: Option<f64>, bp: f64, rp: f64, status: String| {
        RevisionMarketPayload {
            market: m.into(),
            selection: s.into(),
            line,
            base_raw_probability: Some(bp),
            base_public_probability: Some(bp),
            revised_raw_probability: Some(rp),
            revised_calibrated_probability: Some(rp),
            final_public_probability: Some(if status == "ACTIVE" { rp } else { bp }),
            delta_percentage_points: Some(if status == "ACTIVE" {
                (rp - bp) * 100.0
            } else {
                0.0
            }),
            family_status: status.clone(),
            calibration_status: if status == "ACTIVE" {
                "CALIBRATED".into()
            } else {
                "UNCALIBRATED_INSUFFICIENT_DATA".into()
            },
            fallback_reason: if status == "ACTIVE" {
                None
            } else {
                Some(format!("BASE_RETAINED:{status}"))
            },
        }
    };
    let mut markets = Vec::new();
    for (s, i) in [("HOME", 0usize), ("DRAW", 1), ("AWAY", 2)] {
        markets.push(market_entry(
            "MATCH_RESULT",
            s,
            None,
            base_result[i],
            revised.match_result[i],
            family("MATCH_RESULT"),
        ));
    }
    let btts_status = family("BTTS");
    markets.push(market_entry(
        "BTTS",
        "YES",
        None,
        base_btts,
        revised.btts_yes,
        btts_status.clone(),
    ));
    markets.push(market_entry(
        "BTTS",
        "NO",
        None,
        1.0 - base_btts,
        1.0 - revised.btts_yes,
        btts_status,
    ));
    let total_status = family("TOTAL_GOALS");
    for (line, i) in [(1.5, 0usize), (2.5, 1), (3.5, 2)] {
        markets.push(market_entry(
            "TOTAL_GOALS",
            "OVER",
            Some(line),
            base_goals.total_over[i],
            revised.total_over[i],
            total_status.clone(),
        ));
        markets.push(market_entry(
            "TOTAL_GOALS",
            "UNDER",
            Some(line),
            1.0 - base_goals.total_over[i],
            1.0 - revised.total_over[i],
            total_status.clone(),
        ));
    }
    let team_status = family("TEAM_TOTAL_GOALS");
    for (side, vals_base, vals_rev) in [
        ("HOME", base_goals.home_over, revised.home_over),
        ("AWAY", base_goals.away_over, revised.away_over),
    ] {
        for (line, i) in [(0.5, 0usize), (1.5, 1)] {
            markets.push(market_entry(
                &format!("{side}_TEAM_TOTAL_GOALS"),
                "OVER",
                Some(line),
                vals_base[i],
                vals_rev[i],
                team_status.clone(),
            ));
            markets.push(market_entry(
                &format!("{side}_TEAM_TOTAL_GOALS"),
                "UNDER",
                Some(line),
                1.0 - vals_base[i],
                1.0 - vals_rev[i],
                team_status.clone(),
            ));
        }
    }
    for unsupported in ["CORNERS", "CARDS"] {
        markets.push(RevisionMarketPayload {
            market: unsupported.into(),
            selection: "BASE_RETAINED".into(),
            line: None,
            base_raw_probability: None,
            base_public_probability: None,
            revised_raw_probability: None,
            revised_calibrated_probability: None,
            final_public_probability: None,
            delta_percentage_points: Some(0.0),
            family_status: "NOT_SUPPORTED_V1".into(),
            calibration_status: "NOT_SUPPORTED_V1".into(),
            fallback_reason: Some("BASE_RETAINED:NOT_SUPPORTED_V1".into()),
        });
    }
    let payload = RevisionCorePayload {
        prediction_id,
        match_id,
        lineup_snapshot_id: snapshot_id,
        lineup_feature_set_id: feature_set_id,
        // The runtime payload is a deterministic snapshot of its immutable inputs;
        // wall-clock time must not defeat same-input deduplication.
        generated_at: kickoff.clone(),
        revision_reason: "INFER_CORE_LINEUP_ADJUSTMENT".into(),
        base_model_version,
        base_model_hash,
        base_calibration_version,
        base_calibration_hash,
        lineup_model_version: version.clone(),
        lineup_model_hash: artifact.artifact_sha256.clone(),
        base_home_lambda: Some(base_home_lambda),
        base_away_lambda: Some(base_away_lambda),
        revised_home_lambda: Some(revised.home_lambda),
        revised_away_lambda: Some(revised.away_lambda),
        payload: RevisionMarketPayloadDocument {
            payload_schema: REVISION_PAYLOAD_SCHEMA.into(),
            markets,
        },
    };
    let id = persist_revision_core_payload(c, &payload)?;
    let requested_key = market_entry(
        &market,
        &selection,
        None,
        base_probability,
        base_probability,
        family(&if market == "MATCH_RESULT" {
            "MATCH_RESULT"
        } else {
            "BTTS"
        }),
    );
    let selected = payload
        .payload
        .markets
        .iter()
        .find(|x| x.market == market && x.selection == selection)
        .unwrap_or(&requested_key);
    Ok(RevisionResult {
        prediction_id,
        revision_id: id,
        market,
        base_probability,
        revised_probability: selected.final_public_probability,
        delta_percentage_points: selected.delta_percentage_points,
        revision_status: if selected.family_status == "ACTIVE" {
            "LINEUP_ADJUSTMENT_APPLIED"
        } else {
            "BASE_RETAINED"
        }
        .into(),
        lineup_model_version: Some(version),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use rusqlite::params;

    fn complete_test_row(i: usize) -> TrainingRow {
        let mut features = BTreeMap::new();
        features.insert("home_starter_continuity".into(), (i % 5) as f64);
        TrainingRow {
            match_id: i as i64 + 1,
            competition_id: 1,
            kickoff_utc: format!("2025-{:02}-01T00:00:00Z", 1 + i / 20),
            base_prediction_id: i as i64 + 1,
            base_prediction_is_oos: true,
            base_training_cutoff: format!("2024-{:02}-01T00:00:00Z", 1 + i / 20),
            base_fold: 1,
            base_model_version: "base-v1".into(),
            base_model_hash: "base-hash".into(),
            base_calibration_version: "cal-v1".into(),
            base_calibration_hash: "cal-hash".into(),
            base_home_lambda: 1.2,
            base_away_lambda: 0.9,
            base_home_probability: 0.5,
            base_draw_probability: 0.25,
            base_away_probability: 0.25,
            base_btts_yes_probability: 0.5,
            lineup_snapshot_id: i as i64 + 1,
            lineup_feature_set_id: i as i64 + 1,
            lineup_feature_vector: features,
            lineup_feature_quality: "COMPLETE".into(),
            actual_home_goals: if i % 3 == 0 { 2 } else { 0 },
            actual_away_goals: if i % 2 == 0 { 1 } else { 0 },
            match_result_label: if i % 3 == 0 { "HOME" } else { "DRAW" }.into(),
            btts_label: i % 2 == 0,
            total_over_15_label: i % 4 == 0,
            total_over_25_label: i % 5 == 0,
            total_over_35_label: i % 6 == 0,
            home_over_05_label: i % 3 == 0,
            home_over_15_label: i % 3 == 0,
            away_over_05_label: i % 2 == 0,
            away_over_15_label: false,
        }
    }

    fn populate_persisted_acceptance_fixture(c: &Connection, mode: &str) {
        c.execute(
            "INSERT INTO competitions(id,name,country) VALUES(1,'Acceptance League','TR')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO teams(id,normalized_name,country) VALUES(1,'home','TR'),(2,'away','TR')",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO model_versions(id,version_identifier,model_name,artifact_sha256) VALUES(1,'base-v1','fixture','base-hash')", []).unwrap();
        c.execute("INSERT INTO calibration_models(id,parent_model_version,parent_artifact_sha256,calibration_version,market_family,calibration_method,parameters_json,sample_count,validation_metrics_json,artifact_sha256,artifact_path,created_at) VALUES(1,'base-v1','base-hash','cal-v1','MULTI_FAMILY','FIXTURE','{}',60,'{}','cal-hash','fixture.json','2025-01-01')", []).unwrap();
        c.execute("INSERT INTO backtest_runs(id,model_family_version,feature_engine_version,feature_schema_version,label_version,started_at,completed_at,evaluation_start,evaluation_end,configuration_json,status,result_hash) VALUES(1,'base-v1','fe_v1','pred_features_v1','labels_v1','2025-01-01','2025-01-01','2025-01-01','2025-12-31','{}','COMPLETED','run-hash')", []).unwrap();
        for i in 1..=60i64 {
            let kickoff = format!("2025-{i:02}-01T12:00:00Z");
            let signal = i % 2 == 0;
            let overfit_train_signal = i <= 42 && signal;
            let home_goals = match mode {
                "NO_SIGNAL" => {
                    if i % 3 == 0 {
                        3
                    } else {
                        0
                    }
                }
                "OVERFIT" => {
                    if i > 42 {
                        if i % 3 == 0 {
                            3
                        } else {
                            0
                        }
                    } else if overfit_train_signal {
                        3
                    } else {
                        0
                    }
                }
                _ => {
                    if signal {
                        3
                    } else {
                        0
                    }
                }
            };
            let base_exact = mode == "NO_SIGNAL" || (mode == "OVERFIT" && i > 42);
            let base_high = base_exact && home_goals > 0;
            let home_lambda = if base_high {
                3.0
            } else if mode == "NO_SIGNAL" {
                0.1
            } else {
                1.0
            };
            let feature_high = if mode == "NO_SIGNAL" {
                false
            } else if mode == "OVERFIT" {
                overfit_train_signal
            } else {
                signal
            };
            c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,final_home_goals,final_away_goals) VALUES(?1,1,'2025',1,2,?2,'finished',?3,0)", params![i,kickoff,home_goals]).unwrap();
            c.execute("INSERT INTO lineup_snapshots(id,match_id,provider,provider_event_id,captured_at,lineup_status,home_team_id,away_team_id,is_official,source_hash,completeness_status) VALUES(?1,?1,'fixture',?2,?3,'OFFICIAL',1,2,1,?2,'COMPLETE_OFFICIAL')", params![i,format!("event-{i}"),format!("2025-{i:02}-01T10:00:00Z")]).unwrap();
            for n in 0..11i64 {
                c.execute("INSERT INTO lineup_players(lineup_snapshot_id,team_id,provider_player_name,side,role,position,goalkeeper) VALUES(?1,1,?2,'HOME','STARTER','DF',?3)", params![i,format!("H-{i}-{n}"),n==0]).unwrap();
                c.execute("INSERT INTO lineup_players(lineup_snapshot_id,team_id,provider_player_name,side,role,position,goalkeeper) VALUES(?1,2,?2,'AWAY','STARTER','DF',?3)", params![i,format!("A-{i}-{n}"),n==0]).unwrap();
            }
            let feature_json = format!(
                r#"{{"home":{{"starter_continuity_count":{},"changed_starters":{},"regular_starter_presence":0.5,"goalkeeper_continuity":1,"prior_match_sample":8}},"away":{{"starter_continuity_count":5,"changed_starters":5,"regular_starter_presence":0.5,"goalkeeper_continuity":1,"prior_match_sample":8}},"home_minus_away_continuity":{}}}"#,
                if feature_high { 11 } else { 1 },
                if feature_high { 0 } else { 10 },
                if feature_high { 6 } else { -4 }
            );
            c.execute("INSERT INTO lineup_feature_sets(id,match_id,lineup_snapshot_id,feature_version,quality_status,home_resolution_count,away_resolution_count,home_starter_count,away_starter_count,historical_match_sample,minutes_coverage,position_coverage,features_json) VALUES(?1,?1,?1,'lineup_features_v1','COMPLETE',11,11,11,11,8,1,1,?2)", params![i,feature_json]).unwrap();
            let home_p = if base_exact {
                if home_goals > 0 {
                    0.999999
                } else {
                    0.0000005
                }
            } else if base_high {
                0.999999
            } else {
                0.33
            };
            let draw_p = if base_exact {
                if home_goals > 0 {
                    0.0000005
                } else {
                    0.999999
                }
            } else {
                (1.0 - home_p) / 2.0
            };
            let away_p = if base_exact {
                0.0000005
            } else {
                (1.0 - home_p) / 2.0
            };
            for (id, market, selection, probability) in [
                (i * 10 + 1, "MATCH_RESULT", "HOME", home_p),
                (i * 10 + 2, "MATCH_RESULT", "DRAW", draw_p),
                (i * 10 + 3, "MATCH_RESULT", "AWAY", away_p),
                (i * 10 + 4, "BTTS", "YES", if base_high { 0.2 } else { 0.1 }),
            ] {
                c.execute("INSERT INTO backtest_predictions(id,run_id,match_id,fold,market,line_value,selection,raw_probability,actual_result,settlement,out_of_sample,generated_at,base_home_lambda,base_away_lambda) VALUES(?1,1,?2,1,?3,NULL,?4,?5,'true','WON',1,?6,?7,0.7)", params![id,i,market,selection,probability,format!("2025-{i:02}-01T09:00:00Z"),home_lambda]).unwrap();
            }
        }
    }

    #[test]
    fn revision_core_payload_roundtrips_and_is_validated() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        c.execute(
            "INSERT INTO competitions(id,name,country) VALUES(1,'Revision League','TR')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO teams(id,normalized_name,country) VALUES(1,'home','TR'),(2,'away','TR')",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO model_versions(id,version_identifier,model_name) VALUES(1,'base-v1','fixture')", []).unwrap();
        for id in 1..=2 {
            c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status) VALUES(?1,1,'2025',1,2,?2,'finished')", params![id,format!("2025-01-0{id}T12:00:00Z")]).unwrap();
            c.execute("INSERT INTO lineup_snapshots(id,match_id,provider,provider_event_id,captured_at,lineup_status,home_team_id,away_team_id,is_official,source_hash,completeness_status) VALUES(?1,?1,'fixture',?2,?3,'OFFICIAL',1,2,1,?2,'COMPLETE_OFFICIAL')", params![id,format!("event-{id}"),format!("2025-01-0{id}T10:00:00Z")]).unwrap();
            c.execute("INSERT INTO lineup_feature_sets(id,match_id,lineup_snapshot_id,feature_version,quality_status,home_resolution_count,away_resolution_count,home_starter_count,away_starter_count,historical_match_sample,minutes_coverage,position_coverage,features_json) VALUES(?1,?1,?1,'lineup_features_v1','COMPLETE',11,11,11,11,1,1,1,'{}')", [id]).unwrap();
            c.execute("INSERT INTO predictions(id,match_id,market,selection,model_probability,confidence_bucket,kickoff_at,model_version_id) VALUES(?1,?1,'MATCH_RESULT','HOME',0.5,'fixture',?2,1)", params![id,format!("2025-01-0{id}T12:00:00Z")]).unwrap();
        }
        c.execute("INSERT INTO prediction_revisions(prediction_id,match_id,revision_type,parent_prediction_id,lineup_snapshot_id,lineup_feature_set_id,generated_at,revision_reason,model_version,revision_status) VALUES(1,1,'LINEUP_AWARE_PREMATCH',1,1,1,'2025-01-01T12:00:00Z','legacy','legacy','REVISION_PENDING_MODEL')", []).unwrap();
        let old = load_revision_core_payload(&c, 1).unwrap();
        assert!(old.base_home_lambda.is_none() && old.market_payload_json.is_none());
        let payload = RevisionCorePayload {
            prediction_id: 2,
            match_id: 2,
            lineup_snapshot_id: 2,
            lineup_feature_set_id: 2,
            generated_at: "2025-01-02T12:00:00Z".into(),
            revision_reason: "CORE_FIXTURE".into(),
            base_model_version: "base-v1".into(),
            base_model_hash: "base-hash".into(),
            base_calibration_version: "cal-v1".into(),
            base_calibration_hash: "cal-hash".into(),
            lineup_model_version: "lineup-v1".into(),
            lineup_model_hash: "lineup-hash".into(),
            base_home_lambda: Some(1.2),
            base_away_lambda: Some(0.8),
            revised_home_lambda: Some(1.4),
            revised_away_lambda: Some(0.7),
            payload: RevisionMarketPayloadDocument {
                payload_schema: REVISION_PAYLOAD_SCHEMA.into(),
                markets: vec![RevisionMarketPayload {
                    market: "MATCH_RESULT".into(),
                    selection: "HOME".into(),
                    line: None,
                    base_raw_probability: Some(0.5),
                    base_public_probability: Some(0.5),
                    revised_raw_probability: Some(0.6),
                    revised_calibrated_probability: Some(0.6),
                    final_public_probability: Some(0.6),
                    delta_percentage_points: Some(10.0),
                    family_status: "ACTIVE".into(),
                    calibration_status: "CALIBRATED".into(),
                    fallback_reason: None,
                }],
            },
        };
        let id = persist_revision_core_payload(&c, &payload).unwrap();
        assert_eq!(persist_revision_core_payload(&c, &payload).unwrap(), id);
        let saved = load_revision_core_payload(&c, id).unwrap();
        assert_eq!(saved.base_home_lambda, Some(1.2));
        assert_eq!(saved.revised_away_lambda, Some(0.7));
        assert_eq!(saved.base_model_hash.as_deref(), Some("base-hash"));
        assert_eq!(
            saved.payload_schema.as_deref(),
            Some(REVISION_PAYLOAD_SCHEMA)
        );
        let expected_hash = revision_payload_hash(&payload).unwrap();
        assert_eq!(saved.payload_hash.as_deref(), Some(expected_hash.as_str()));
        let json = saved.market_payload_json.unwrap();
        let decoded: RevisionMarketPayloadDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, payload.payload);
        let mut invalid = payload.clone();
        invalid.base_home_lambda = Some(0.0);
        assert_eq!(
            persist_revision_core_payload(&c, &invalid).unwrap_err(),
            "INVALID_REVISION_LAMBDA"
        );
        invalid = payload.clone();
        invalid.payload.payload_schema = "unknown".into();
        assert_eq!(
            persist_revision_core_payload(&c, &invalid).unwrap_err(),
            "REVISION_PAYLOAD_SCHEMA_UNSUPPORTED"
        );
        let mut insufficient = payload;
        insufficient.prediction_id = 1;
        insufficient.match_id = 1;
        insufficient.lineup_snapshot_id = 2;
        insufficient.lineup_feature_set_id = 2;
        insufficient.payload.markets[0].family_status = "INSUFFICIENT_DATA".into();
        insufficient.payload.markets[0].calibration_status =
            "UNCALIBRATED_INSUFFICIENT_DATA".into();
        insufficient.payload.markets[0].final_public_probability = Some(0.5);
        insufficient.payload.markets[0].delta_percentage_points = Some(0.0);
        insufficient.payload.markets[0].fallback_reason =
            Some("BASE_RETAINED:INSUFFICIENT_DATA".into());
        assert_eq!(
            family_gate(
                &CoreMetrics {
                    sample_count: 1,
                    ..CoreMetrics::default()
                },
                5
            ),
            "INSUFFICIENT_DATA"
        );
        let insufficient_id = persist_revision_core_payload(&c, &insufficient).unwrap();
        let insufficient_saved = load_revision_core_payload(&c, insufficient_id).unwrap();
        let insufficient_doc: RevisionMarketPayloadDocument =
            serde_json::from_str(&insufficient_saved.market_payload_json.unwrap()).unwrap();
        let market = &insufficient_doc.markets[0];
        assert_eq!(market.family_status, "INSUFFICIENT_DATA");
        assert_eq!(
            market.final_public_probability,
            market.base_public_probability
        );
        assert_eq!(
            market.fallback_reason.as_deref(),
            Some("BASE_RETAINED:INSUFFICIENT_DATA")
        );
    }

    #[test]
    fn persisted_oos_dataset_is_complete_and_trains_core() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        c.execute(
            "INSERT INTO competitions(id,name,country) VALUES(1,'Fixture League','TR')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO teams(id,normalized_name,country) VALUES(1,'home','TR'),(2,'away','TR')",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO model_versions(id,version_identifier,model_name,artifact_sha256) VALUES(1,'base-v1','fixture','base-hash')", []).unwrap();
        c.execute("INSERT INTO calibration_models(id,parent_model_version,parent_artifact_sha256,calibration_version,market_family,calibration_method,parameters_json,sample_count,validation_metrics_json,artifact_sha256,artifact_path,created_at) VALUES(1,'base-v1','base-hash','cal-v1','MULTI_FAMILY','FIXTURE','{}',35,'{}','cal-hash','fixture.json','2025-01-01')", []).unwrap();
        c.execute("INSERT INTO backtest_runs(id,model_family_version,feature_engine_version,feature_schema_version,label_version,started_at,completed_at,evaluation_start,evaluation_end,configuration_json,status,result_hash) VALUES(1,'base-v1','fe_v1','pred_features_v1','labels_v1','2025-01-01','2025-01-01','2025-01-01','2025-12-31','{}','COMPLETED','run-hash')", []).unwrap();
        let feature_json = r#"{"home":{"starter_continuity_count":8,"changed_starters":3,"regular_starter_presence":0.7,"goalkeeper_continuity":1,"prior_match_sample":4},"away":{"starter_continuity_count":6,"changed_starters":5,"regular_starter_presence":0.6,"goalkeeper_continuity":1,"prior_match_sample":4},"home_minus_away_continuity":2}"#;
        for i in 1..=35i64 {
            let kickoff = format!("2025-{i:02}-01T12:00:00Z");
            c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,final_home_goals,final_away_goals) VALUES(?1,1,'2025',1,2,?2,'finished',?3,?4)", params![i,kickoff,if i%3==0 {2}else{1},if i%2==0 {1}else{0}]).unwrap();
            c.execute("INSERT INTO lineup_snapshots(id,match_id,provider,provider_event_id,captured_at,lineup_status,home_team_id,away_team_id,is_official,source_hash,completeness_status) VALUES(?1,?1,'fixture',?2,?3,'OFFICIAL',1,2,1,?2,'COMPLETE_OFFICIAL')", params![i,format!("event-{i}"),format!("2025-{i:02}-01T10:00:00Z")]).unwrap();
            for n in 0..11i64 {
                c.execute("INSERT INTO lineup_players(lineup_snapshot_id,team_id,provider_player_name,side,role,position,goalkeeper) VALUES(?1,1,?2,'HOME','STARTER','DF',?3)", params![i,format!("H-{i}-{n}"),n==0]).unwrap();
                c.execute("INSERT INTO lineup_players(lineup_snapshot_id,team_id,provider_player_name,side,role,position,goalkeeper) VALUES(?1,2,?2,'AWAY','STARTER','DF',?3)", params![i,format!("A-{i}-{n}"),n==0]).unwrap();
            }
            c.execute("INSERT INTO lineup_feature_sets(id,match_id,lineup_snapshot_id,feature_version,quality_status,home_resolution_count,away_resolution_count,home_starter_count,away_starter_count,historical_match_sample,minutes_coverage,position_coverage,features_json) VALUES(?1,?1,?1,'lineup_features_v1','COMPLETE',11,11,11,11,8,1,1,?2)", params![i,feature_json]).unwrap();
            for (id, market, selection, probability) in [
                (i * 10 + 1, "MATCH_RESULT", "HOME", 0.5),
                (i * 10 + 2, "MATCH_RESULT", "DRAW", 0.25),
                (i * 10 + 3, "MATCH_RESULT", "AWAY", 0.25),
                (i * 10 + 4, "BTTS", "YES", 0.5),
            ] {
                c.execute("INSERT INTO backtest_predictions(id,run_id,match_id,fold,market,line_value,selection,raw_probability,actual_result,settlement,out_of_sample,generated_at,base_home_lambda,base_away_lambda) VALUES(?1,1,?2,1,?3,NULL,?4,?5,'true','WON',1,?6,1.4,0.8)", params![id,i,market,selection,probability,format!("2025-{i:02}-01T09:00:00Z")]).unwrap();
            }
        }
        let dataset = build_core_training_dataset(&c).unwrap();
        assert_eq!(dataset.rows.len(), 35);
        assert_eq!(dataset.diagnostics.accepted, 35);
        let row = &dataset.rows[0];
        assert_eq!(row.match_id, 1);
        assert_eq!(row.base_home_lambda, 1.4);
        assert_eq!(row.base_away_lambda, 0.8);
        assert_eq!(
            (
                row.base_home_probability,
                row.base_draw_probability,
                row.base_away_probability,
                row.base_btts_yes_probability
            ),
            (0.5, 0.25, 0.25, 0.5)
        );
        assert_eq!(row.lineup_snapshot_id, 1);
        assert_eq!(row.lineup_feature_set_id, 1);
        assert_eq!(row.lineup_feature_vector["home_starter_continuity"], 8.0);
        assert_eq!(row.actual_home_goals, 1);
        assert_eq!(row.actual_away_goals, 0);
        assert_eq!(row.match_result_label, "HOME");
        assert!(!row.btts_label && !row.total_over_15_label);
        assert!(!row.home_over_15_label && !row.away_over_05_label);
        assert_eq!(row.base_model_version, "base-v1");
        assert_eq!(row.base_calibration_version, "cal-v1");
        assert!(row.base_prediction_is_oos);
        assert_eq!(
            row.to_core_training_row().unwrap().base_result,
            [0.5, 0.25, 0.25]
        );
        let dir = tempfile::tempdir().unwrap();
        let report = train(
            &c,
            "lineup-core-fixture",
            &dir.path().join("artifact.json"),
            None,
            None,
        )
        .unwrap();
        let artifact = load(Path::new(&report.artifact_path)).unwrap();
        let core = artifact.core_model.as_ref().unwrap();
        assert!(!core.home_goal_offset.coefficients.is_empty());
        assert!(!core.away_goal_offset.coefficients.is_empty());
        assert!(!core.match_result.iter().all(|m| m.coefficients.is_empty()));
        assert!(!core.btts.coefficients.is_empty());
        assert!(core
            .home_goal_offset
            .coefficients
            .values()
            .all(|v| v.is_finite()));
        assert!(core
            .away_goal_offset
            .coefficients
            .values()
            .all(|v| v.is_finite()));
        assert!(core.match_result.iter().all(|m| {
            m.intercept.is_finite() && m.coefficients.values().all(|v| v.is_finite())
        }));
        assert!(
            core.btts.intercept.is_finite()
                && core.btts.coefficients.values().all(|v| v.is_finite())
        );
        let reloaded = load(Path::new(&report.artifact_path)).unwrap();
        assert_eq!(artifact.core_model, reloaded.core_model);
        let output = infer_core(
            core,
            None,
            1.4,
            0.8,
            [0.5, 0.25, 0.25],
            0.5,
            &row.lineup_feature_vector,
        );
        assert!(output.home_lambda.is_finite() && output.home_lambda > 0.0);
        assert!(output.away_lambda.is_finite() && output.away_lambda > 0.0);
        assert!((output.match_result.iter().sum::<f64>() - 1.0).abs() < 1e-10);
        assert!((0.0..=1.0).contains(&output.btts_yes));
    }

    #[test]
    fn deterministic_training_and_tamper_detection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lineup.json");
        let mut rows = Vec::new();
        for i in 0..60 {
            rows.push(complete_test_row(i));
        }
        let report = train_rows(&mut rows, "acceptance-lineup-v1", &path, None, None).unwrap();
        let artifact = load(&path).unwrap();
        assert_eq!(artifact.training_samples, 42);
        assert_eq!(artifact.validation_samples, 9);
        assert_eq!(artifact.test_samples, 9);
        assert_eq!(hash_artifact(&artifact).unwrap(), artifact.artifact_sha256);
        let mut tampered: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        tampered["markets"]["BTTS_YES"]["intercept"] = serde_json::json!(99.0);
        fs::write(&path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert_eq!(
            load(&path).unwrap_err(),
            "LINEUP_ADJUSTMENT_ARTIFACT_INVALID"
        );
        assert!(!report.artifact_sha256.is_empty());
    }

    #[test]
    fn insufficient_training_is_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let mut rows = vec![complete_test_row(0); 10];
        assert_eq!(
            train_rows(
                &mut rows,
                "too-small",
                &dir.path().join("x.json"),
                None,
                None
            )
            .unwrap_err(),
            "LINEUP_MODEL_INSUFFICIENT_DATA"
        );
    }

    #[test]
    fn poisson_offsets_and_derived_markets_are_coherent() {
        let mut rows = Vec::new();
        for i in 0..60 {
            let mut features = BTreeMap::new();
            features.insert(
                "home_starter_continuity".into(),
                if i % 2 == 0 { 1.0 } else { -1.0 },
            );
            rows.push(CoreTrainingRow {
                as_of: format!("2025-{i:03}"),
                base_home_lambda: 1.0,
                base_away_lambda: 0.9,
                base_result: [0.5, 0.25, 0.25],
                base_btts: 0.5,
                home_goals: if i % 2 == 0 { 2.0 } else { 0.0 },
                away_goals: if i % 3 == 0 { 2.0 } else { 0.0 },
                result_class: if i % 2 == 0 { 0 } else { 2 },
                btts: i % 3 == 0,
                features,
            });
        }
        let model = train_core(&mut rows).unwrap();
        let mut high = BTreeMap::new();
        high.insert("home_starter_continuity".into(), 1.0);
        let h = apply_offset(1.0, &model.home_goal_offset, &high);
        let a = apply_offset(0.9, &model.away_goal_offset, &high);
        assert!(h.is_finite() && h > 0.0 && a.is_finite() && a > 0.0);
        let markets = revised_goal_markets(h, a);
        assert!(markets.total_over[0] >= markets.total_over[1]);
        assert!(markets.total_over[1] >= markets.total_over[2]);
        assert!(markets.home_over[0] >= markets.home_over[1]);
        assert!(markets.away_over[0] >= markets.away_over[1]);
        assert!((markets.match_result.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        let result = revised_multiclass([0.5, 0.25, 0.25], &model.match_result, &high);
        assert!((result.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(model.btts.coefficients.values().all(|v| v.is_finite()));
    }

    #[test]
    fn persisted_core_inference_calibration_is_coherent() {
        let model = CoreModel {
            home_goal_offset: LinearCorrection {
                intercept: 0.05,
                coefficients: BTreeMap::new(),
            },
            away_goal_offset: LinearCorrection::default(),
            match_result: vec![LinearCorrection::default(); 3],
            btts: LinearCorrection::default(),
            match_result_blend: 0.5,
            btts_blend: 0.5,
        };
        let mut features = BTreeMap::new();
        features.insert("home_starter_continuity".into(), 2.0);
        let calibration = CoreCalibration {
            match_result_temperature: Some(1.1),
            binary: BTreeMap::from([(
                "TOTAL_GOALS".into(),
                PlattCalibration {
                    a: 1.0,
                    b: 0.0,
                    status: "CALIBRATED".into(),
                    sample_count: 20,
                },
            )]),
        };
        let output = infer_core(
            &model,
            Some(&calibration),
            1.2,
            0.9,
            [0.45, 0.25, 0.30],
            0.52,
            &features,
        );
        assert!(output.home_lambda.is_finite() && output.home_lambda > 0.0);
        assert!((output.match_result.iter().sum::<f64>() - 1.0).abs() < 1e-10);
        assert!(output.total_over[0] >= output.total_over[1]);
        assert!(output.total_over[1] >= output.total_over[2]);
        assert!(output.btts_yes.is_finite() && (0.0..=1.0).contains(&output.btts_yes));
    }

    #[test]
    fn validation_calibration_and_metrics_are_deterministic() {
        let rows = (0..30)
            .map(|i| CoreTrainingRow {
                as_of: format!("2025-{i:02}-01"),
                base_home_lambda: 1.2,
                base_away_lambda: 0.9,
                base_result: [0.5, 0.25, 0.25],
                base_btts: 0.5,
                home_goals: 1.0,
                away_goals: 0.0,
                result_class: i % 3,
                btts: i % 2 == 0,
                features: BTreeMap::new(),
            })
            .collect::<Vec<_>>();
        let mut training = rows.clone();
        let model = train_core(&mut training).unwrap();
        let temperature = fit_temperature(&rows, &model).unwrap();
        assert!(temperature.is_finite() && temperature > 0.0);
        let platt = fit_platt(&rows, |r| r.base_btts).unwrap();
        assert!(platt.a.is_finite() && platt.b.is_finite());
        let metrics = binary_metrics(&[(0.8, true), (0.2, false)]);
        assert_eq!(metrics.sample_count, 2);
        assert!(metrics.base_log_loss.is_finite() && metrics.base_brier.is_finite());
    }

    #[test]
    fn final_test_report_and_artifact_only_runtime_are_complete() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("signal-artifact.json");
        let mut rows = (0..60).map(complete_test_row).collect::<Vec<_>>();
        let report = train_rows(&mut rows, "signal-v1", &path, None, None).unwrap();
        let artifact = load(&path).unwrap();
        for key in [
            "MATCH_RESULT",
            "BTTS",
            "O1.5",
            "O2.5",
            "O3.5",
            "HOME_O0.5",
            "HOME_O1.5",
            "AWAY_O0.5",
            "AWAY_O1.5",
            "HOME_GOALS",
            "AWAY_GOALS",
        ] {
            let metrics = artifact.core_metrics.get(key).expect(key);
            assert!(metrics.sample_count > 0);
            assert!(metrics.base_log_loss.is_finite() || metrics.base_mae.unwrap().is_finite());
        }
        assert!(artifact.core_calibration.is_some());
        assert!(artifact.family_statuses.contains_key("MATCH_RESULT"));
        assert_eq!(artifact.family_statuses["CORNERS"], "NOT_SUPPORTED_V1");
        assert_eq!(artifact.family_statuses["CARDS"], "NOT_SUPPORTED_V1");
        let input = rows[0].to_core_training_row().unwrap();
        let output = infer_core(
            artifact.core_model.as_ref().unwrap(),
            artifact.core_calibration.as_ref(),
            input.base_home_lambda,
            input.base_away_lambda,
            input.base_result,
            input.base_btts,
            &input.features,
        );
        assert!(output.home_lambda.is_finite() && output.home_lambda > 0.0);
        assert!(output.away_lambda.is_finite() && output.away_lambda > 0.0);
        assert!((output.match_result.iter().sum::<f64>() - 1.0).abs() < 1e-10);
        assert!((0.0..=1.0).contains(&output.btts_yes));
        assert!(!report.artifact_sha256.is_empty());
    }

    #[test]
    fn complete_artifact_tamper_matrix_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tamper-artifact.json");
        let mut rows = (0..60).map(complete_test_row).collect::<Vec<_>>();
        train_rows(&mut rows, "tamper-v1", &path, None, None).unwrap();
        let pristine = fs::read(&path).unwrap();
        let mutations = [
            (
                "core_model.home_goal_offset.coefficients.base_logit",
                serde_json::json!(9.0),
            ),
            (
                "core_model.away_goal_offset.coefficients.base_logit",
                serde_json::json!(9.0),
            ),
            (
                "core_model.match_result.0.coefficients.base_logit",
                serde_json::json!(9.0),
            ),
            (
                "core_model.btts.coefficients.base_logit",
                serde_json::json!(9.0),
            ),
            ("preprocessing.means.base_logit", serde_json::json!(9.0)),
            ("preprocessing.stddevs.base_logit", serde_json::json!(9.0)),
            ("core_model.match_result_blend", serde_json::json!(0.9)),
            ("core_model.btts_blend", serde_json::json!(0.9)),
            (
                "core_calibration.match_result_temperature",
                serde_json::json!(1.9),
            ),
            ("core_calibration.binary.BTTS.a", serde_json::json!(1.9)),
            (
                "core_metrics.MATCH_RESULT.base_log_loss",
                serde_json::json!(9.0),
            ),
            (
                "family_statuses.MATCH_RESULT",
                serde_json::json!("NOT_BENEFICIAL"),
            ),
            (
                "benefit_gate_config.ece_tolerance",
                serde_json::json!("9.0"),
            ),
            ("parent_model_version", serde_json::json!("wrong-model")),
            (
                "parent_calibration_hash",
                serde_json::json!("wrong-calibration"),
            ),
            (
                "feature_registry_version",
                serde_json::json!("wrong-features"),
            ),
        ];
        for (path_expr, value) in mutations {
            let mut json: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            let mut current = &mut json;
            let parts = path_expr.split('.').collect::<Vec<_>>();
            for part in &parts[..parts.len() - 1] {
                current = match current {
                    serde_json::Value::Array(values) => &mut values[part.parse::<usize>().unwrap()],
                    _ => &mut current[*part],
                };
            }
            let last = parts[parts.len() - 1];
            match current {
                serde_json::Value::Array(values) => values[last.parse::<usize>().unwrap()] = value,
                _ => current[last] = value,
            }
            fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
            let error = match load(&path) {
                Ok(_) => panic!("tamper was accepted: {path_expr}"),
                Err(error) => error,
            };
            assert!(
                matches!(
                    error.as_str(),
                    "LINEUP_ADJUSTMENT_ARTIFACT_INVALID" | "LINEUP_FEATURE_REGISTRY_MISMATCH"
                ),
                "tamper {path_expr}: {error}"
            );
            fs::write(&path, &pristine).unwrap();
        }
    }

    #[test]
    fn artifact_only_runtime_is_deterministic_without_training_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("portable.json");
        let mut rows = (0..60).map(complete_test_row).collect::<Vec<_>>();
        train_rows(&mut rows, "portable-v1", &path, None, None).unwrap();
        let runtime_a = load(&path).unwrap();
        let runtime_b = load(&path).unwrap();
        let row = rows[0].to_core_training_row().unwrap();
        let mut features = row.features.clone();
        features.insert("future_participation_after_target".into(), 9999.0);
        let a = infer_core(
            runtime_a.core_model.as_ref().unwrap(),
            runtime_a.core_calibration.as_ref(),
            row.base_home_lambda,
            row.base_away_lambda,
            row.base_result,
            row.base_btts,
            &features,
        );
        let b = infer_core(
            runtime_b.core_model.as_ref().unwrap(),
            runtime_b.core_calibration.as_ref(),
            row.base_home_lambda,
            row.base_away_lambda,
            row.base_result,
            row.base_btts,
            &features,
        );
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::to_value(&b).unwrap()
        );
        assert!(a.home_lambda.is_finite() && a.away_lambda.is_finite());
        assert!((a.match_result.iter().sum::<f64>() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn active_inference_allows_a_zero_delta_without_cosmetic_adjustment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zero-delta.json");
        let mut rows = (0..60).map(complete_test_row).collect::<Vec<_>>();
        train_rows(&mut rows, "zero-delta-v1", &path, None, None).unwrap();
        let artifact = load(&path).unwrap();
        let model = artifact.core_model.as_ref().unwrap();
        let features = rows[0].lineup_feature_vector.clone();
        let mut base_result = [1.0 / 3.0; 3];
        let mut base_btts = 0.5;
        for _ in 0..256 {
            let output = infer_core(
                model,
                artifact.core_calibration.as_ref(),
                rows[0].base_home_lambda,
                rows[0].base_away_lambda,
                base_result,
                base_btts,
                &features,
            );
            base_result = output.match_result;
            base_btts = output.btts_yes;
        }
        let output = infer_core(
            model,
            artifact.core_calibration.as_ref(),
            rows[0].base_home_lambda,
            rows[0].base_away_lambda,
            base_result,
            base_btts,
            &features,
        );
        let result_delta = (output.match_result[0] - base_result[0]).abs();
        let btts_delta = (output.btts_yes - base_btts).abs();
        println!(
            "NO_FORCED_DELTA base_match_home={} revised_match_home={} delta_pp={} base_btts={} revised_btts={} btts_delta_pp={}",
            base_result[0],
            output.match_result[0],
            (output.match_result[0] - base_result[0]) * 100.0,
            base_btts,
            output.btts_yes,
            (output.btts_yes - base_btts) * 100.0,
        );
        assert!(result_delta <= 1e-10 || btts_delta <= 1e-10);
        if result_delta <= 1e-10 {
            assert!(((output.match_result[0] - base_result[0]) * 100.0).abs() <= 1e-8);
        } else {
            assert!(((output.btts_yes - base_btts) * 100.0).abs() <= 1e-8);
        }
    }

    #[test]
    fn prior_only_lineup_features_ignore_future_history_and_respond_to_prior_history() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        c.execute(
            "INSERT INTO competitions(id,name,country) VALUES(1,'Leakage League','TR')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO teams(id,normalized_name,country) VALUES(1,'home','TR'),(2,'away','TR')",
            [],
        )
        .unwrap();
        for id in 1..=22i64 {
            c.execute("INSERT INTO players(id,canonical_name,normalized_name,primary_team_id) VALUES(?1,?2,?2,?3)", params![id, format!("Player {id}"), if id <= 11 { 1 } else { 2 }]).unwrap();
        }
        for id in 23..=26i64 {
            c.execute("INSERT INTO players(id,canonical_name,normalized_name,primary_team_id) VALUES(?1,?2,?2,1)", params![id, format!("Bench History {id}")]).unwrap();
        }
        for (id, kickoff, status, home_goals, away_goals) in [
            (1, "2025-01-01T12:00:00Z", "finished", Some(1), Some(0)),
            (2, "2025-02-01T12:00:00Z", "finished", Some(0), Some(0)),
            (3, "2025-03-01T12:00:00Z", "finished", Some(2), Some(1)),
        ] {
            c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,final_home_goals,final_away_goals) VALUES(?1,1,'2025',1,2,?2,?3,?4,?5)", params![id, kickoff, status, home_goals, away_goals]).unwrap();
        }
        for (snapshot, match_id, captured, hash) in [
            (1, 1, "2025-01-01T10:00:00Z", "prior"),
            (2, 2, "2025-02-01T10:00:00Z", "target"),
            (3, 3, "2025-03-01T10:00:00Z", "future"),
        ] {
            c.execute("INSERT INTO lineup_snapshots(id,match_id,provider,provider_event_id,captured_at,lineup_status,home_team_id,away_team_id,is_official,source_hash,completeness_status) VALUES(?1,?2,'fixture',?3,?4,'OFFICIAL',1,2,1,?5,'COMPLETE_OFFICIAL')", params![snapshot, match_id, format!("event-{snapshot}"), captured, hash]).unwrap();
        }
        for player in 1..=22i64 {
            let side = if player <= 11 { "HOME" } else { "AWAY" };
            c.execute("INSERT INTO lineup_players(lineup_snapshot_id,team_id,player_id,provider_player_name,side,role,position,goalkeeper) VALUES(2,?1,?2,?3,?4,'STARTER','DF',?5)", params![if player <= 11 { 1 } else { 2 }, player, format!("Player {player}"), side, player == 1 || player == 12]).unwrap();
            c.execute("INSERT INTO player_match_participation(match_id,player_id,team_id,started,substitute,minutes_played,position,lineup_snapshot_id) VALUES(1,?1,?2,1,0,90,'DF',1)", params![player, if player <= 11 { 1 } else { 2 }]).unwrap();
        }
        for player in 23..=26i64 {
            c.execute("INSERT INTO player_match_participation(match_id,player_id,team_id,started,substitute,minutes_played,position,lineup_snapshot_id) VALUES(1,?1,1,1,0,90,'DF',1)", params![player]).unwrap();
        }
        let before = crate::repositories::lineup::generate_features(&c, 2).unwrap();
        let before_json = serde_json::to_string(&before.features).unwrap();
        let before_vector = lineup_feature_vector(&before_json).unwrap();
        let mut rows = (0..60).map(complete_test_row).collect::<Vec<_>>();
        for (i, row) in rows.iter_mut().enumerate() {
            let high = i % 2 == 0;
            row.lineup_feature_vector.insert(
                "home_starter_continuity".into(),
                if high { 11.0 } else { 0.0 },
            );
            row.match_result_label = if high { "HOME" } else { "AWAY" }.into();
            row.actual_home_goals = if high { 2 } else { 0 };
            row.actual_away_goals = 0;
        }
        let artifact_dir = tempfile::tempdir().unwrap();
        let artifact_path = artifact_dir.path().join("prior-only.json");
        train_rows(&mut rows, "prior-only-v1", &artifact_path, None, None).unwrap();
        let artifact = load(&artifact_path).unwrap();
        let infer = |features: &BTreeMap<String, f64>| {
            infer_core(
                artifact.core_model.as_ref().unwrap(),
                artifact.core_calibration.as_ref(),
                1.2,
                0.9,
                [0.5, 0.25, 0.25],
                0.5,
                features,
            )
        };
        let before_output = infer(&before_vector);

        c.execute("INSERT INTO player_match_participation(match_id,player_id,team_id,started,substitute,minutes_played,position,lineup_snapshot_id) VALUES(3,1,1,1,0,90,'DF',3)", []).unwrap();
        c.execute("INSERT INTO lineup_players(lineup_snapshot_id,team_id,player_id,provider_player_name,side,role,position,goalkeeper) VALUES(3,1,1,'Player 1','HOME','STARTER','DF',1)", []).unwrap();
        c.execute(
            "UPDATE matches SET final_home_goals=9,final_away_goals=9 WHERE id=3",
            [],
        )
        .unwrap();
        let after_future = crate::repositories::lineup::generate_features(&c, 2).unwrap();
        let after_future_json = serde_json::to_string(&after_future.features).unwrap();
        assert_eq!(before_json, after_future_json);
        assert_eq!(
            before_output,
            infer(&lineup_feature_vector(&after_future_json).unwrap())
        );

        let prior_player: i64 = c
            .query_row(
                "SELECT player_id FROM player_match_participation WHERE match_id=1 AND team_id=1 ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(c.execute("UPDATE player_match_participation SET started=0,substitute=1 WHERE match_id=1 AND player_id=?1", [prior_player]).unwrap(), 1);
        let after_prior = crate::repositories::lineup::generate_features(&c, 2).unwrap();
        let after_prior_json = serde_json::to_string(&after_prior.features).unwrap();
        assert_ne!(before_json, after_prior_json);
        let after_prior_vector = lineup_feature_vector(&after_prior_json).unwrap();
        println!("prior vectors before={before_vector:?} after={after_prior_vector:?}");
        assert_ne!(before_output, infer(&after_prior_vector));
    }

    #[test]
    fn persisted_signal_fixture_reports_real_final_test_evidence() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        c.execute(
            "INSERT INTO competitions(id,name,country) VALUES(1,'Signal League','TR')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO teams(id,normalized_name,country) VALUES(1,'home','TR'),(2,'away','TR')",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO model_versions(id,version_identifier,model_name,artifact_sha256) VALUES(1,'base-v1','fixture','base-hash')", []).unwrap();
        c.execute("INSERT INTO calibration_models(id,parent_model_version,parent_artifact_sha256,calibration_version,market_family,calibration_method,parameters_json,sample_count,validation_metrics_json,artifact_sha256,artifact_path,created_at) VALUES(1,'base-v1','base-hash','cal-v1','MULTI_FAMILY','FIXTURE','{}',60,'{}','cal-hash','fixture.json','2025-01-01')", []).unwrap();
        c.execute("INSERT INTO backtest_runs(id,model_family_version,feature_engine_version,feature_schema_version,label_version,started_at,completed_at,evaluation_start,evaluation_end,configuration_json,status,result_hash) VALUES(1,'base-v1','fe_v1','pred_features_v1','labels_v1','2025-01-01','2025-01-01','2025-01-01','2025-12-31','{}','COMPLETED','run-hash')", []).unwrap();
        let dir = tempfile::tempdir().unwrap();
        for i in 1..=60i64 {
            let kickoff = format!("2025-{i:02}-01T12:00:00Z");
            let high = i % 2 == 0;
            let home_goals = if high { 3 } else { 0 };
            c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,final_home_goals,final_away_goals) VALUES(?1,1,'2025',1,2,?2,'finished',?3,0)", params![i,kickoff,home_goals]).unwrap();
            c.execute("INSERT INTO lineup_snapshots(id,match_id,provider,provider_event_id,captured_at,lineup_status,home_team_id,away_team_id,is_official,source_hash,completeness_status) VALUES(?1,?1,'fixture',?2,?3,'OFFICIAL',1,2,1,?2,'COMPLETE_OFFICIAL')", params![i,format!("event-{i}"),format!("2025-{i:02}-01T10:00:00Z")]).unwrap();
            for n in 0..11i64 {
                c.execute("INSERT INTO lineup_players(lineup_snapshot_id,team_id,provider_player_name,side,role,position,goalkeeper) VALUES(?1,1,?2,'HOME','STARTER','DF',?3)", params![i,format!("H-{i}-{n}"),n==0]).unwrap();
                c.execute("INSERT INTO lineup_players(lineup_snapshot_id,team_id,provider_player_name,side,role,position,goalkeeper) VALUES(?1,2,?2,'AWAY','STARTER','DF',?3)", params![i,format!("A-{i}-{n}"),n==0]).unwrap();
            }
            let feature_json = format!(
                r#"{{"home":{{"starter_continuity_count":{},"changed_starters":{},"regular_starter_presence":1,"goalkeeper_continuity":1,"prior_match_sample":8}},"away":{{"starter_continuity_count":5,"changed_starters":5,"regular_starter_presence":0.5,"goalkeeper_continuity":1,"prior_match_sample":8}},"home_minus_away_continuity":{}}}"#,
                if high { 11 } else { 1 },
                if high { 0 } else { 10 },
                if high { 6 } else { -4 }
            );
            c.execute("INSERT INTO lineup_feature_sets(id,match_id,lineup_snapshot_id,feature_version,quality_status,home_resolution_count,away_resolution_count,home_starter_count,away_starter_count,historical_match_sample,minutes_coverage,position_coverage,features_json) VALUES(?1,?1,?1,'lineup_features_v1','COMPLETE',11,11,11,11,8,1,1,?2)", params![i,feature_json]).unwrap();
            for (id, market, selection, probability) in [
                (i * 10 + 1, "MATCH_RESULT", "HOME", 0.33),
                (i * 10 + 2, "MATCH_RESULT", "DRAW", 0.34),
                (i * 10 + 3, "MATCH_RESULT", "AWAY", 0.33),
                (i * 10 + 4, "BTTS", "YES", 0.25),
            ] {
                c.execute("INSERT INTO backtest_predictions(id,run_id,match_id,fold,market,line_value,selection,raw_probability,actual_result,settlement,out_of_sample,generated_at,base_home_lambda,base_away_lambda) VALUES(?1,1,?2,1,?3,NULL,?4,?5,'true','WON',1,?6,1.0,0.7)", params![id,i,market,selection,probability,format!("2025-{i:02}-01T09:00:00Z")]).unwrap();
            }
        }
        let dataset = build_core_training_dataset(&c).unwrap();
        assert_eq!(dataset.rows.len(), 60);
        let report = train(
            &c,
            "persisted-signal-v1",
            &dir.path().join("signal.json"),
            None,
            None,
        )
        .unwrap();
        let artifact = load(Path::new(&report.artifact_path)).unwrap();
        assert!(artifact.core_metrics["MATCH_RESULT"].sample_count > 0);
        assert!(artifact.core_metrics["BTTS"].sample_count > 0);
        assert!(artifact.family_statuses.values().any(|s| s == "ACTIVE"));
        assert_eq!(artifact.family_statuses["CORNERS"], "NOT_SUPPORTED_V1");
        assert_eq!(artifact.family_statuses["CARDS"], "NOT_SUPPORTED_V1");
        c.execute("INSERT INTO lineup_adjustment_models(version_identifier,artifact_path,artifact_sha256,parent_model_version,parent_model_hash,feature_version,registry_hash,training_samples,validation_samples,test_samples,status) VALUES(?1,?2,?3,?4,?5,?6,?3,?7,?8,?9,'ACTIVE')", params![artifact.model_version, report.artifact_path, artifact.artifact_sha256, artifact.parent_model_version, artifact.parent_model_hash, artifact.lineup_feature_version, report.training_samples, report.validation_samples, report.test_samples]).unwrap();
        c.execute("INSERT INTO prediction_runs(id,match_id,model_version_id,feature_engine_version,feature_schema_version,artifact_sha256,base_home_lambda,base_away_lambda) VALUES(2,1,1,'fe_v1','pred_features_v1','base-hash',1.0,0.7)", []).unwrap();
        for (id, market, selection, probability) in [
            (1001, "MATCH_RESULT", "HOME", 0.33),
            (1002, "MATCH_RESULT", "DRAW", 0.34),
            (1003, "MATCH_RESULT", "AWAY", 0.33),
            (1004, "BTTS", "YES", 0.25),
        ] {
            c.execute("INSERT INTO predictions(id,match_id,market,selection,model_probability,confidence_bucket,kickoff_at,model_version_id,prediction_run_id,raw_probability,public_probability,calibration_status,calibration_version) VALUES(?1,1,?2,?3,?4,'MODEL_OUTPUT','2025-01-01T12:00:00Z',1,2,?4,?4,'CALIBRATED','cal-v1')", params![id,market,selection,probability]).unwrap();
        }
        let revision = generate_revision(&c, 1001, 1).unwrap();
        assert!(revision.revised_probability.is_some());
        let saved = load_revision_core_payload(&c, revision.revision_id).unwrap();
        assert_eq!(
            saved.payload_schema.as_deref(),
            Some(REVISION_PAYLOAD_SCHEMA)
        );
        assert_eq!(saved.base_home_lambda, Some(1.0));
        assert_eq!(
            saved.lineup_model_version.as_deref(),
            Some("persisted-signal-v1")
        );
        assert!(saved.revised_home_lambda.unwrap() > 0.0);
        let payload_doc: RevisionMarketPayloadDocument =
            serde_json::from_str(&saved.market_payload_json.unwrap()).unwrap();
        assert!(payload_doc
            .markets
            .iter()
            .any(|m| m.market == "CORNERS" && m.family_status == "NOT_SUPPORTED_V1"));
        assert!(payload_doc
            .markets
            .iter()
            .filter(|m| m.family_status != "ACTIVE")
            .all(|m| m.final_public_probability == m.base_public_probability
                || m.market == "CORNERS"
                || m.market == "CARDS"));
        for market in &payload_doc.markets {
            if let (Some(base), Some(final_public), Some(delta)) = (
                market.base_public_probability,
                market.final_public_probability,
                market.delta_percentage_points,
            ) {
                if market.family_status == "ACTIVE" {
                    assert!((delta - (final_public - base) * 100.0).abs() < 1e-10);
                } else {
                    assert!((final_public - base).abs() < 1e-12);
                    assert!(delta.abs() < 1e-12);
                }
            }
        }
        println!(
            "RUNTIME REVISION id={} schema={:?} hash={:?} lineage=({:?},{:?},{:?},{:?},{:?},{:?}) lambdas=({:?},{:?})->({:?},{:?}) markets={:?}",
            saved.id,
            saved.payload_schema,
            saved.payload_hash,
            saved.base_model_version,
            saved.base_model_hash,
            saved.base_calibration_version,
            saved.base_calibration_hash,
            saved.lineup_model_version,
            saved.lineup_model_hash,
            saved.base_home_lambda,
            saved.base_away_lambda,
            saved.revised_home_lambda,
            saved.revised_away_lambda,
            payload_doc.markets,
        );
        let same = generate_revision(&c, 1001, 1).unwrap();
        assert_eq!(same.revision_id, revision.revision_id);
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,alternative_odd,captured_at) VALUES(1,'fixture','acceptance','MATCH_RESULT','HOME',9.99,8.88,'2025-01-01T11:30:00Z')", []).unwrap();
        c.execute("INSERT INTO popularity_snapshots(provider,match_id,provider_event_id,market_type,selection,metric_type,metric_value,raw_metric_name,captured_at) VALUES('fixture',1,'acceptance','MATCH_RESULT','HOME','COUNT',9999,'totalPlayed','2025-01-01T11:30:00Z')", []).unwrap();
        let odds_and_popularity_changed = generate_revision(&c, 1001, 1).unwrap();
        assert_eq!(
            odds_and_popularity_changed.revision_id,
            revision.revision_id
        );
        assert_eq!(
            load_revision_core_payload(&c, odds_and_popularity_changed.revision_id)
                .unwrap()
                .payload_hash,
            saved.payload_hash
        );
        c.execute("INSERT INTO lineup_snapshots(id,match_id,provider,provider_event_id,captured_at,lineup_status,home_team_id,away_team_id,is_official,source_hash,completeness_status) VALUES(100,1,'fixture','event-corrected','2025-01-01T11:00:00Z','OFFICIAL',1,2,1,'corrected','COMPLETE_OFFICIAL')", []).unwrap();
        for n in 0..11i64 {
            c.execute("INSERT INTO lineup_players(lineup_snapshot_id,team_id,provider_player_name,side,role,position,goalkeeper) VALUES(100,1,?1,'HOME','STARTER','DF',?2)", params![format!("H-corrected-{n}"),n==0]).unwrap();
            c.execute("INSERT INTO lineup_players(lineup_snapshot_id,team_id,provider_player_name,side,role,position,goalkeeper) VALUES(100,2,?1,'AWAY','STARTER','DF',?2)", params![format!("A-corrected-{n}"),n==0]).unwrap();
        }
        let corrected_features = r#"{"home":{"starter_continuity_count":1,"changed_starters":10,"regular_starter_presence":0.1,"goalkeeper_continuity":0,"prior_match_sample":8},"away":{"starter_continuity_count":5,"changed_starters":5,"regular_starter_presence":0.5,"goalkeeper_continuity":1,"prior_match_sample":8},"home_minus_away_continuity":-4}"#;
        c.execute("INSERT INTO lineup_feature_sets(id,match_id,lineup_snapshot_id,feature_version,quality_status,home_resolution_count,away_resolution_count,home_starter_count,away_starter_count,historical_match_sample,minutes_coverage,position_coverage,features_json) VALUES(100,1,100,'lineup_features_v1','COMPLETE',11,11,11,11,8,1,1,?1)", [corrected_features]).unwrap();
        let corrected = generate_revision(&c, 1001, 100).unwrap();
        assert_ne!(corrected.revision_id, revision.revision_id);
        let corrected_again = generate_revision(&c, 1001, 100).unwrap();
        assert_eq!(corrected_again.revision_id, corrected.revision_id);
        let first_payload = load_revision_core_payload(&c, revision.revision_id).unwrap();
        let corrected_payload = load_revision_core_payload(&c, corrected.revision_id).unwrap();
        assert_ne!(first_payload.payload_hash, corrected_payload.payload_hash);
        println!(
            "SIGNAL DATASET rows={} train={} validation={} test={} statuses={:?} metrics={:?}",
            dataset.rows.len(),
            artifact.training_samples,
            artifact.validation_samples,
            artifact.test_samples,
            artifact.family_statuses,
            artifact.core_metrics
        );
    }

    #[test]
    fn persisted_no_signal_and_overfit_fixtures_require_held_out_benefit() {
        for mode in ["NO_SIGNAL", "OVERFIT"] {
            let db = Database::open_in_memory().unwrap();
            let c = db.connection().unwrap();
            populate_persisted_acceptance_fixture(&c, mode);
            let dir = tempfile::tempdir().unwrap();
            let report = train(
                &c,
                &format!("{mode}-v1"),
                &dir.path().join("artifact.json"),
                None,
                None,
            )
            .unwrap();
            let artifact = load(Path::new(&report.artifact_path)).unwrap();
            println!(
                "{mode} status={} metrics={:?}",
                artifact.family_statuses["MATCH_RESULT"], artifact.core_metrics["MATCH_RESULT"]
            );
            assert_eq!(
                artifact.family_statuses["MATCH_RESULT"], "NOT_BENEFICIAL",
                "{mode} must use final-test evidence"
            );
            assert!(
                artifact.core_metrics["MATCH_RESULT"].lineup_log_loss
                    >= artifact.core_metrics["MATCH_RESULT"].base_log_loss
                    || artifact.family_statuses["MATCH_RESULT"] != "ACTIVE"
            );
            println!(
                "{mode} DATASET rows=60 train={} validation={} test={} status={} metrics={:?}",
                artifact.training_samples,
                artifact.validation_samples,
                artifact.test_samples,
                artifact.family_statuses["MATCH_RESULT"],
                artifact.core_metrics["MATCH_RESULT"]
            );
        }
    }
}
