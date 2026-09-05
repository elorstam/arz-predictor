//! Rust-native Phase 6 training and inference. Runtime code in this module is
//! intentionally filesystem/CPU-only: no HTTP client, process, Python, odds,
//! or popularity dependency participates in a feature vector or prediction.
use super::features::{FeatureSnapshot, TrainingLabel, FEATURE_ENGINE_VERSION, LABEL_VERSION};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub const FEATURE_SCHEMA_VERSION: &str = "pred_features_v1";
pub const ARTIFACT_SCHEMA_VERSION: &str = "arz_glm_artifact_v1";
pub const COUNT_LABEL_VERSION: &str = "label_v2_counts";
pub const MIN_TRAINING_SAMPLES: usize = 100;
const TRAIN_FRACTION: f64 = 0.70;
const VALIDATION_FRACTION: f64 = 0.15;
const EPS: f64 = 1e-12;

pub const FEATURE_NAMES: &[&str] = &[
    "home_elo",
    "away_elo",
    "elo_difference",
    "expected_home_result",
    "home_form5_ppm",
    "away_form5_ppm",
    "home_form10_ppm",
    "away_form10_ppm",
    "home_form5_gf",
    "away_form5_gf",
    "home_form5_ga",
    "away_form5_ga",
    "home_venue5_ppm",
    "away_venue5_ppm",
    "home_venue5_gf",
    "away_venue5_gf",
    "home_shots5_for",
    "away_shots5_for",
    "home_sot5_for",
    "away_sot5_for",
    "home_corners5_for",
    "away_corners5_for",
    "home_cards5_for",
    "away_cards5_for",
    "home_fouls5",
    "away_fouls5",
    "home_rest_days",
    "away_rest_days",
    "home_opponent_elo5",
    "away_opponent_elo5",
    "league_goals",
    "league_btts",
    "league_corners",
    "league_cards",
    "h2h_goals",
    "h2h_btts",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CountLabel {
    pub match_id: i64,
    pub home_goals: i64,
    pub away_goals: i64,
    pub home_corners: Option<i64>,
    pub away_corners: Option<i64>,
    pub total_corners: Option<i64>,
    pub home_yellow_cards: Option<i64>,
    pub away_yellow_cards: Option<i64>,
    pub total_yellow_cards: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeatureRule {
    pub source_index: usize,
    pub name: String,
    pub median: f64,
    pub mean: f64,
    pub std: f64,
    pub add_missing_indicator: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Preprocessing {
    pub rules: Vec<FeatureRule>,
    pub dropped_all_null_or_zero_variance: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BinaryModel {
    pub intercept: f64,
    pub weights: Vec<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoftmaxModel {
    pub intercepts: [f64; 3],
    pub weights: Vec<[f64; 3]>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PoissonModel {
    pub intercept: f64,
    pub weights: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Models {
    pub match_result: SoftmaxModel,
    pub home_goals: PoissonModel,
    pub away_goals: PoissonModel,
    pub btts: BinaryModel,
    pub total_corners: Option<PoissonModel>,
    pub home_corners: Option<PoissonModel>,
    pub away_corners: Option<PoissonModel>,
    pub total_cards: Option<PoissonModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MetricSet {
    pub match_log_loss: f64,
    pub match_brier: f64,
    pub match_accuracy: f64,
    pub btts_log_loss: f64,
    pub btts_brier: f64,
    pub btts_accuracy: f64,
    pub home_goals_mae: f64,
    pub away_goals_mae: f64,
    pub corners_mae: Option<f64>,
    pub cards_mae: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SplitSummary {
    pub train: usize,
    pub validation: usize,
    pub test: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtifactBundle {
    pub product: String,
    pub artifact_schema_version: String,
    pub model_version: String,
    pub model_family: String,
    pub feature_engine_version: String,
    pub feature_schema_version: String,
    pub label_versions: Vec<String>,
    pub ordered_feature_names: Vec<String>,
    pub training_start: String,
    pub training_cutoff: String,
    pub validation_period: [String; 2],
    pub test_period: [String; 2],
    pub split: SplitSummary,
    pub market_training_examples: BTreeMap<String, usize>,
    pub preprocessing: Preprocessing,
    pub preprocessing_sha256: String,
    pub models: Models,
    pub component_sha256: BTreeMap<String, String>,
    pub match_result_softmax_weight: f64,
    pub btts_logistic_weight: f64,
    pub blend_selection: String,
    pub training_configuration: BTreeMap<String, String>,
    pub deterministic_seed: u64,
    pub metrics: MetricSet,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtifactFile {
    pub artifact_sha256: String,
    pub bundle: ArtifactBundle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainReport {
    pub rows_considered: usize,
    pub rows_usable: usize,
    pub split: SplitSummary,
    pub trained: Vec<String>,
    pub skipped: BTreeMap<String, String>,
    pub metrics: MetricSet,
    pub artifact_path: String,
    pub artifact_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketPrediction {
    pub market_type: String,
    pub line: Option<f64>,
    pub selection: String,
    pub model_probability: Option<f64>,
    pub raw_probability: Option<f64>,
    /// The probability exposed to callers. `model_probability` remains the
    /// historical public field for compatibility and mirrors this value.
    pub public_probability: Option<f64>,
    pub availability: String,
    pub source_model: String,
    pub sample_quality: String,
    pub calibration_status: String,
    pub calibration_version: Option<String>,
    pub calibration_bucket: Option<String>,
    pub bucket_observed_rate: Option<f64>,
    pub bucket_sample_size: Option<usize>,
    pub bucket_calibration_gap: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PredictionResult {
    pub match_id: i64,
    pub model_version: String,
    pub feature_engine_version: String,
    pub feature_schema_version: String,
    pub artifact_hash: String,
    pub generated_at: String,
    #[serde(default)]
    pub base_home_lambda: Option<f64>,
    #[serde(default)]
    pub base_away_lambda: Option<f64>,
    pub markets: Vec<MarketPrediction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedPredictionRun {
    pub id: i64,
    pub match_id: i64,
    pub model_version_id: i64,
    pub artifact_sha256: String,
    pub base_home_lambda: Option<f64>,
    pub base_away_lambda: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtifactValidation {
    pub valid: bool,
    pub version: Option<String>,
    pub feature_compatible: bool,
    pub hash_valid: bool,
    pub available_models: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone)]
struct Row {
    cutoff: String,
    raw: Vec<Option<f64>>,
    label: TrainingLabel,
    counts: CountLabel,
}

pub fn raw_vector(s: &FeatureSnapshot) -> Vec<Option<f64>> {
    let h = &s.home;
    let a = &s.away;
    vec![
        Some(s.home_elo),
        Some(s.away_elo),
        Some(s.elo_difference),
        Some(s.expected_home_result),
        h.overall_last5.points_per_match,
        a.overall_last5.points_per_match,
        h.overall_last10.points_per_match,
        a.overall_last10.points_per_match,
        h.overall_last5.goals_for_avg,
        a.overall_last5.goals_for_avg,
        h.overall_last5.goals_against_avg,
        a.overall_last5.goals_against_avg,
        h.venue_last5.points_per_match,
        a.venue_last5.points_per_match,
        h.venue_last5.goals_for_avg,
        a.venue_last5.goals_for_avg,
        h.shooting_last5.shots_for_avg,
        a.shooting_last5.shots_for_avg,
        h.shooting_last5.shots_on_target_for_avg,
        a.shooting_last5.shots_on_target_for_avg,
        h.corners_last5.corners_for_avg,
        a.corners_last5.corners_for_avg,
        h.cards_last5.yellow_for_avg,
        a.cards_last5.yellow_for_avg,
        h.fouls_last5.fouls_committed_avg,
        a.fouls_last5.fouls_committed_avg,
        h.days_since_last_match,
        a.days_since_last_match,
        h.average_opponent_elo_last5,
        a.average_opponent_elo_last5,
        s.league.goals_per_match,
        s.league.btts_rate,
        s.league.average_total_corners,
        s.league.average_total_yellow_cards,
        s.h2h.goals_avg,
        s.h2h.btts_rate,
    ]
}

fn hash<T: Serialize>(v: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(v).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn finite(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}
fn clamp_probability(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.5
    }
}
fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

pub fn fit_preprocessing(train: &[Vec<Option<f64>>]) -> Preprocessing {
    let mut rules = Vec::new();
    let mut dropped = Vec::new();
    for i in 0..FEATURE_NAMES.len() {
        let vals: Vec<f64> = train
            .iter()
            .filter_map(|r| r[i].filter(|v| v.is_finite()))
            .collect();
        if vals.is_empty() {
            dropped.push(FEATURE_NAMES[i].into());
            continue;
        }
        let med = median(vals.clone());
        let filled: Vec<f64> = train
            .iter()
            .map(|r| r[i].filter(|v| v.is_finite()).unwrap_or(med))
            .collect();
        let mean = filled.iter().sum::<f64>() / filled.len() as f64;
        let variance = filled.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / filled.len() as f64;
        if variance <= 1e-14 {
            dropped.push(FEATURE_NAMES[i].into());
            continue;
        }
        rules.push(FeatureRule {
            source_index: i,
            name: FEATURE_NAMES[i].into(),
            median: med,
            mean,
            std: variance.sqrt(),
            add_missing_indicator: true,
        });
    }
    Preprocessing {
        rules,
        dropped_all_null_or_zero_variance: dropped,
    }
}
pub fn transform(p: &Preprocessing, raw: &[Option<f64>]) -> Vec<f64> {
    let mut out = Vec::with_capacity(p.rules.len() * 2);
    for r in &p.rules {
        let missing = raw[r.source_index].map(|v| !v.is_finite()).unwrap_or(true);
        let v = raw[r.source_index]
            .filter(|v| v.is_finite())
            .unwrap_or(r.median);
        out.push(finite((v - r.mean) / r.std));
        if r.add_missing_indicator {
            out.push(if missing { 1.0 } else { 0.0 });
        }
    }
    out
}

fn sigmoid(z: f64) -> f64 {
    if z >= 0.0 {
        1.0 / (1.0 + (-z.min(35.0)).exp())
    } else {
        let e = z.max(-35.0).exp();
        e / (1.0 + e)
    }
}
pub fn binary_predict(m: &BinaryModel, x: &[f64]) -> f64 {
    clamp_probability(sigmoid(
        m.intercept + m.weights.iter().zip(x).map(|(w, x)| w * x).sum::<f64>(),
    ))
}
pub fn poisson_predict(m: &PoissonModel, x: &[f64]) -> f64 {
    finite(
        (m.intercept + m.weights.iter().zip(x).map(|(w, x)| w * x).sum::<f64>())
            .clamp(-5.0, 5.0)
            .exp(),
    )
    .max(EPS)
}
pub fn softmax_predict(m: &SoftmaxModel, x: &[f64]) -> [f64; 3] {
    let mut z = m.intercepts;
    for (i, w) in m.weights.iter().enumerate() {
        for c in 0..3 {
            z[c] += w[c] * x[i]
        }
    }
    let max = z.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let e = [(z[0] - max).exp(), (z[1] - max).exp(), (z[2] - max).exp()];
    let sum = e.iter().sum::<f64>();
    [e[0] / sum, e[1] / sum, e[2] / sum]
}

fn train_binary(x: &[Vec<f64>], y: &[f64]) -> BinaryModel {
    let d = x[0].len();
    let mut m = BinaryModel {
        intercept: 0.0,
        weights: vec![0.0; d],
    };
    for _ in 0..700 {
        let mut gi = 0.0;
        let mut g = vec![0.0; d];
        for (r, t) in x.iter().zip(y) {
            let e = binary_predict(&m, r) - t;
            gi += e;
            for j in 0..d {
                g[j] += e * r[j]
            }
        }
        let n = x.len() as f64;
        m.intercept -= 0.08 * gi / n;
        for j in 0..d {
            m.weights[j] -= 0.08 * (g[j] / n + 0.002 * m.weights[j]);
        }
    }
    m
}
fn train_poisson(x: &[Vec<f64>], y: &[f64]) -> PoissonModel {
    let d = x[0].len();
    let mean = (y.iter().sum::<f64>() / y.len() as f64).max(0.05);
    let mut m = PoissonModel {
        intercept: mean.ln(),
        weights: vec![0.0; d],
    };
    for _ in 0..800 {
        let mut gi = 0.0;
        let mut g = vec![0.0; d];
        for (r, t) in x.iter().zip(y) {
            let e = poisson_predict(&m, r) - t;
            gi += e;
            for j in 0..d {
                g[j] += e * r[j]
            }
        }
        let n = x.len() as f64;
        m.intercept = (m.intercept - 0.015 * gi / n).clamp(-5.0, 5.0);
        for j in 0..d {
            m.weights[j] =
                (m.weights[j] - 0.015 * (g[j] / n + 0.002 * m.weights[j])).clamp(-5.0, 5.0);
        }
    }
    m
}
fn train_softmax(x: &[Vec<f64>], y: &[usize]) -> SoftmaxModel {
    let d = x[0].len();
    let mut m = SoftmaxModel {
        intercepts: [0.0; 3],
        weights: vec![[0.0; 3]; d],
    };
    for _ in 0..800 {
        let mut gi = [0.0; 3];
        let mut g = vec![[0.0; 3]; d];
        for (r, t) in x.iter().zip(y) {
            let p = softmax_predict(&m, r);
            for c in 0..3 {
                let e = p[c] - if *t == c { 1.0 } else { 0.0 };
                gi[c] += e;
                for j in 0..d {
                    g[j][c] += e * r[j]
                }
            }
        }
        let n = x.len() as f64;
        for c in 0..3 {
            m.intercepts[c] -= 0.06 * gi[c] / n;
            for j in 0..d {
                m.weights[j][c] -= 0.06 * (g[j][c] / n + 0.002 * m.weights[j][c]);
            }
        }
    }
    m
}

fn poisson_pmf(lambda: f64, max: usize) -> Vec<f64> {
    let mut p = vec![0.0; max + 1];
    p[0] = (-lambda).exp();
    for k in 1..=max {
        p[k] = p[k - 1] * lambda / k as f64
    }
    let sum = p.iter().sum::<f64>();
    p[max] += 1.0 - sum;
    p
}
pub fn goal_distribution(home: f64, away: f64) -> ([[f64; 3]; 1], BTreeMap<String, f64>) {
    let hp = poisson_pmf(home, 18);
    let ap = poisson_pmf(away, 18);
    let mut one = [0.0; 3];
    let mut totals = BTreeMap::new();
    let mut total_dist = vec![0.0; 37];
    for (i, a) in hp.iter().enumerate() {
        for (j, b) in ap.iter().enumerate() {
            let q = a * b;
            if i > j {
                one[0] += q
            } else if i == j {
                one[1] += q
            } else {
                one[2] += q
            }
            total_dist[i + j] += q;
        }
    }
    for (line, k) in [("1.5", 1), ("2.5", 2), ("3.5", 3)] {
        totals.insert(line.into(), 1.0 - total_dist[..=k].iter().sum::<f64>());
    }
    totals.insert("btts".into(), (1.0 - hp[0]) * (1.0 - ap[0]));
    totals.insert("home_0.5".into(), 1.0 - hp[0]);
    totals.insert("home_1.5".into(), 1.0 - hp[..=1].iter().sum::<f64>());
    totals.insert("away_0.5".into(), 1.0 - ap[0]);
    totals.insert("away_1.5".into(), 1.0 - ap[..=1].iter().sum::<f64>());
    ([one], totals)
}
fn count_over(lambda: f64, line: f64) -> f64 {
    let k = line.floor() as usize;
    let p = poisson_pmf(lambda, k + 40);
    clamp_probability(1.0 - p[..=k].iter().sum::<f64>())
}
fn blend3(a: [f64; 3], b: [f64; 3], w: f64) -> [f64; 3] {
    let mut p = [0.0; 3];
    for i in 0..3 {
        p[i] = w * a[i] + (1.0 - w) * b[i]
    }
    let s = p.iter().sum::<f64>();
    [p[0] / s, p[1] / s, p[2] / s]
}
fn select_blend3(soft: &[[f64; 3]], pois: &[[f64; 3]], y: &[usize]) -> f64 {
    (0..=20)
        .map(|i| i as f64 * 0.05)
        .min_by(|a, b| {
            let loss = |w: f64| {
                soft.iter()
                    .zip(pois)
                    .zip(y)
                    .map(|((s, p), y)| -blend3(*s, *p, w)[*y].max(EPS).ln())
                    .sum::<f64>()
            };
            loss(*a).total_cmp(&loss(*b))
        })
        .unwrap_or(0.5)
}
fn select_blend2(ml: &[f64], pois: &[f64], y: &[f64]) -> f64 {
    (0..=20)
        .map(|i| i as f64 * 0.05)
        .min_by(|a, b| {
            let loss = |w: f64| {
                ml.iter()
                    .zip(pois)
                    .zip(y)
                    .map(|((m, p), y)| {
                        let q = (w * m + (1.0 - w) * p).clamp(EPS, 1.0 - EPS);
                        -y * q.ln() - (1.0 - y) * (1.0 - q).ln()
                    })
                    .sum::<f64>()
            };
            loss(*a).total_cmp(&loss(*b))
        })
        .unwrap_or(0.5)
}

pub(crate) fn ensure_count_label(c: &Connection, match_id: i64) -> Result<CountLabel, String> {
    if let Some(json) = c
        .query_row(
            "SELECT label_json FROM training_labels WHERE match_id=?1 AND label_version=?2",
            params![match_id, COUNT_LABEL_VERSION],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        return serde_json::from_str(&json).map_err(|e| e.to_string());
    }
    let v=c.query_row("SELECT m.final_home_goals,m.final_away_goals,s.home_corners,s.away_corners,s.home_yellow_cards,s.away_yellow_cards FROM matches m LEFT JOIN match_statistics s ON s.match_id=m.id WHERE m.id=?1 AND m.status='finished'",[match_id],|r|{let h:i64=r.get(0)?;let a:i64=r.get(1)?;let hc:Option<i64>=r.get(2)?;let ac:Option<i64>=r.get(3)?;let hy:Option<i64>=r.get(4)?;let ay:Option<i64>=r.get(5)?;Ok(CountLabel{match_id,home_goals:h,away_goals:a,home_corners:hc,away_corners:ac,total_corners:hc.zip(ac).map(|(x,y)|x+y),home_yellow_cards:hy,away_yellow_cards:ay,total_yellow_cards:hy.zip(ay).map(|(x,y)|x+y)})}).map_err(|e|e.to_string())?;
    let json = serde_json::to_string(&v).map_err(|e| e.to_string())?;
    c.execute(
        "INSERT OR IGNORE INTO training_labels(match_id,label_version,label_json) VALUES(?1,?2,?3)",
        params![match_id, COUNT_LABEL_VERSION, json],
    )
    .map_err(|e| e.to_string())?;
    Ok(v)
}
fn load_rows(c: &Connection) -> Result<Vec<Row>, String> {
    let mut st=c.prepare("SELECT match_id,cutoff_at,feature_json FROM feature_sets WHERE feature_engine_version=?1 ORDER BY cutoff_at,match_id").map_err(|e|e.to_string())?;
    let data = st
        .query_map([FEATURE_ENGINE_VERSION], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (id, cutoff, json) in data {
        let label_json: Option<String> = c
            .query_row(
                "SELECT label_json FROM training_labels WHERE match_id=?1 AND label_version=?2",
                params![id, LABEL_VERSION],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(l) = label_json {
            let snapshot: FeatureSnapshot =
                serde_json::from_str(&json).map_err(|e| e.to_string())?;
            out.push(Row {
                cutoff,
                raw: raw_vector(&snapshot),
                label: serde_json::from_str(&l).map_err(|e| e.to_string())?,
                counts: ensure_count_label(c, id)?,
            });
        }
    }
    Ok(out)
}

fn metric(models: &Models, p: &Preprocessing, test: &[Row], wm: f64, wb: f64) -> MetricSet {
    let mut m = MetricSet::default();
    if test.is_empty() {
        return m;
    }
    let n = test.len() as f64;
    for r in test {
        let x = transform(p, &r.raw);
        let hg = poisson_predict(&models.home_goals, &x);
        let ag = poisson_predict(&models.away_goals, &x);
        let (pd, goal) = goal_distribution(hg, ag);
        let y = match r.label.match_result.as_str() {
            "HOME" => 0,
            "DRAW" => 1,
            _ => 2,
        };
        let q = blend3(softmax_predict(&models.match_result, &x), pd[0], wm);
        m.match_log_loss -= q[y].max(EPS).ln();
        m.match_brier += (0..3)
            .map(|i| (q[i] - if i == y { 1.0 } else { 0.0 }).powi(2))
            .sum::<f64>()
            / 3.0;
        m.match_accuracy += if q
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0
            == y
        {
            1.0
        } else {
            0.0
        };
        let by = if r.label.btts { 1.0 } else { 0.0 };
        let bp = wb * binary_predict(&models.btts, &x) + (1.0 - wb) * goal["btts"];
        m.btts_log_loss -= by * bp.max(EPS).ln() + (1.0 - by) * (1.0 - bp).max(EPS).ln();
        m.btts_brier += (bp - by).powi(2);
        m.btts_accuracy += if (bp >= 0.5) == r.label.btts {
            1.0
        } else {
            0.0
        };
        m.home_goals_mae += (hg - r.counts.home_goals as f64).abs();
        m.away_goals_mae += (ag - r.counts.away_goals as f64).abs();
        if let (Some(model), Some(y)) = (&models.total_corners, r.counts.total_corners) {
            *m.corners_mae.get_or_insert(0.0) += (poisson_predict(model, &x) - y as f64).abs()
        }
        if let (Some(model), Some(y)) = (&models.total_cards, r.counts.total_yellow_cards) {
            *m.cards_mae.get_or_insert(0.0) += (poisson_predict(model, &x) - y as f64).abs()
        }
    }
    m.match_log_loss /= n;
    m.match_brier /= n;
    m.match_accuracy /= n;
    m.btts_log_loss /= n;
    m.btts_brier /= n;
    m.btts_accuracy /= n;
    m.home_goals_mae /= n;
    m.away_goals_mae /= n;
    if let Some(v) = &mut m.corners_mae {
        *v /= test
            .iter()
            .filter(|r| r.counts.total_corners.is_some())
            .count()
            .max(1) as f64
    }
    if let Some(v) = &mut m.cards_mae {
        *v /= test
            .iter()
            .filter(|r| r.counts.total_yellow_cards.is_some())
            .count()
            .max(1) as f64
    }
    m
}

pub fn train(c: &Connection, model_version: &str, output: &Path) -> Result<TrainReport, String> {
    let rows = load_rows(c)?;
    if rows.len() < MIN_TRAINING_SAMPLES {
        return Err(format!(
            "INSUFFICIENT_TRAINING_DATA: {} < {}",
            rows.len(),
            MIN_TRAINING_SAMPLES
        ));
    }
    let n = rows.len();
    let tr = (n as f64 * TRAIN_FRACTION).floor() as usize;
    let va = (n as f64 * VALIDATION_FRACTION).floor() as usize;
    let train = &rows[..tr];
    let valid = &rows[tr..tr + va];
    let test = &rows[tr + va..];
    let prep = fit_preprocessing(&train.iter().map(|r| r.raw.clone()).collect::<Vec<_>>());
    if prep.rules.is_empty() {
        return Err("MODEL_NOT_TRAINABLE: no usable features".into());
    }
    let x: Vec<_> = train.iter().map(|r| transform(&prep, &r.raw)).collect();
    let yc: Vec<_> = train
        .iter()
        .map(|r| match r.label.match_result.as_str() {
            "HOME" => 0,
            "DRAW" => 1,
            _ => 2,
        })
        .collect();
    if (0..3).any(|k| yc.iter().filter(|v| **v == k).count() < 10) {
        return Err("MODEL_NOT_TRAINABLE: insufficient result class samples".into());
    }
    let yb: Vec<_> = train
        .iter()
        .map(|r| if r.label.btts { 1.0 } else { 0.0 })
        .collect();
    let yh: Vec<_> = train.iter().map(|r| r.counts.home_goals as f64).collect();
    let ya: Vec<_> = train.iter().map(|r| r.counts.away_goals as f64).collect();
    let corner_rows: Vec<_> = train
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.counts.total_corners.map(|v| (x[i].clone(), v as f64)))
        .collect();
    let home_corner_rows: Vec<_> = train
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.counts.home_corners.map(|v| (x[i].clone(), v as f64)))
        .collect();
    let away_corner_rows: Vec<_> = train
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.counts.away_corners.map(|v| (x[i].clone(), v as f64)))
        .collect();
    let card_rows: Vec<_> = train
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            r.counts
                .total_yellow_cards
                .map(|v| (x[i].clone(), v as f64))
        })
        .collect();
    let models = Models {
        match_result: train_softmax(&x, &yc),
        home_goals: train_poisson(&x, &yh),
        away_goals: train_poisson(&x, &ya),
        btts: train_binary(&x, &yb),
        total_corners: if corner_rows.len() >= MIN_TRAINING_SAMPLES / 2 {
            Some(train_poisson(
                &corner_rows.iter().map(|v| v.0.clone()).collect::<Vec<_>>(),
                &corner_rows.iter().map(|v| v.1).collect::<Vec<_>>(),
            ))
        } else {
            None
        },
        home_corners: if home_corner_rows.len() >= MIN_TRAINING_SAMPLES / 2 {
            Some(train_poisson(
                &home_corner_rows
                    .iter()
                    .map(|v| v.0.clone())
                    .collect::<Vec<_>>(),
                &home_corner_rows.iter().map(|v| v.1).collect::<Vec<_>>(),
            ))
        } else {
            None
        },
        away_corners: if away_corner_rows.len() >= MIN_TRAINING_SAMPLES / 2 {
            Some(train_poisson(
                &away_corner_rows
                    .iter()
                    .map(|v| v.0.clone())
                    .collect::<Vec<_>>(),
                &away_corner_rows.iter().map(|v| v.1).collect::<Vec<_>>(),
            ))
        } else {
            None
        },
        total_cards: if card_rows.len() >= MIN_TRAINING_SAMPLES / 2 {
            Some(train_poisson(
                &card_rows.iter().map(|v| v.0.clone()).collect::<Vec<_>>(),
                &card_rows.iter().map(|v| v.1).collect::<Vec<_>>(),
            ))
        } else {
            None
        },
    };
    let vx: Vec<_> = valid.iter().map(|r| transform(&prep, &r.raw)).collect();
    let mut soft = Vec::new();
    let mut pois = Vec::new();
    let mut vy = Vec::new();
    let mut bml = Vec::new();
    let mut bp = Vec::new();
    let mut by = Vec::new();
    for (r, x) in valid.iter().zip(&vx) {
        let (pd, g) = goal_distribution(
            poisson_predict(&models.home_goals, x),
            poisson_predict(&models.away_goals, x),
        );
        soft.push(softmax_predict(&models.match_result, x));
        pois.push(pd[0]);
        vy.push(match r.label.match_result.as_str() {
            "HOME" => 0,
            "DRAW" => 1,
            _ => 2,
        });
        bml.push(binary_predict(&models.btts, x));
        bp.push(g["btts"]);
        by.push(if r.label.btts { 1.0 } else { 0.0 });
    }
    let wm = if valid.len() >= 15 {
        select_blend3(&soft, &pois, &vy)
    } else {
        0.5
    };
    let wb = if valid.len() >= 15 {
        select_blend2(&bml, &bp, &by)
    } else {
        0.5
    };
    let metrics = metric(&models, &prep, test, wm, wb);
    let mut counts = BTreeMap::new();
    counts.insert("MATCH_RESULT".into(), tr);
    counts.insert("HOME_GOALS".into(), tr);
    counts.insert("AWAY_GOALS".into(), tr);
    counts.insert("BTTS".into(), tr);
    counts.insert("FULL_TIME_TOTAL_CORNERS".into(), corner_rows.len());
    counts.insert("FULL_TIME_TOTAL_CARDS".into(), card_rows.len());
    let mut component = BTreeMap::new();
    component.insert("match_result".into(), hash(&models.match_result)?);
    component.insert("home_goals".into(), hash(&models.home_goals)?);
    component.insert("away_goals".into(), hash(&models.away_goals)?);
    component.insert("btts".into(), hash(&models.btts)?);
    if let Some(v) = &models.total_corners {
        component.insert("corners".into(), hash(v)?);
    }
    if let Some(v) = &models.home_corners {
        component.insert("home_corners".into(), hash(v)?);
    }
    if let Some(v) = &models.away_corners {
        component.insert("away_corners".into(), hash(v)?);
    }
    if let Some(v) = &models.total_cards {
        component.insert("cards".into(), hash(v)?);
    }
    let mut cfg = BTreeMap::new();
    cfg.insert("split".into(), "chronological 70/15/15".into());
    cfg.insert(
        "optimizer".into(),
        "deterministic batch gradient descent, L2=0.002".into(),
    );
    cfg.insert(
        "goal_distribution".into(),
        "independent Poisson; tail retained in final bucket".into(),
    );
    let split = SplitSummary {
        train: tr,
        validation: va,
        test: n - tr - va,
    };
    let bundle = ArtifactBundle {
        product: "ARZ Predictor".into(),
        artifact_schema_version: ARTIFACT_SCHEMA_VERSION.into(),
        model_version: model_version.into(),
        model_family: "regularized GLM v1 (softmax/logistic/Poisson)".into(),
        feature_engine_version: FEATURE_ENGINE_VERSION.into(),
        feature_schema_version: FEATURE_SCHEMA_VERSION.into(),
        label_versions: vec![LABEL_VERSION.into(), COUNT_LABEL_VERSION.into()],
        ordered_feature_names: FEATURE_NAMES.iter().map(|v| v.to_string()).collect(),
        training_start: rows[0].cutoff.clone(),
        training_cutoff: rows[n - 1].cutoff.clone(),
        validation_period: [rows[tr].cutoff.clone(), rows[tr + va - 1].cutoff.clone()],
        test_period: [rows[tr + va].cutoff.clone(), rows[n - 1].cutoff.clone()],
        split: split.clone(),
        market_training_examples: counts,
        preprocessing_sha256: hash(&prep)?,
        preprocessing: prep,
        models,
        component_sha256: component,
        match_result_softmax_weight: wm,
        btts_logistic_weight: wb,
        blend_selection: if valid.len() >= 15 {
            "validation grid 0.00..1.00 step 0.05".into()
        } else {
            "documented 0.50 fallback: insufficient validation".into()
        },
        training_configuration: cfg,
        deterministic_seed: 0,
        metrics: metrics.clone(),
        created_at: rows[n - 1].cutoff.clone(),
    };
    let mut bundle: ArtifactBundle =
        serde_json::from_slice(&serde_json::to_vec(&bundle).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    bundle.preprocessing_sha256 = hash(&bundle.preprocessing)?;
    bundle
        .component_sha256
        .insert("match_result".into(), hash(&bundle.models.match_result)?);
    bundle
        .component_sha256
        .insert("home_goals".into(), hash(&bundle.models.home_goals)?);
    bundle
        .component_sha256
        .insert("away_goals".into(), hash(&bundle.models.away_goals)?);
    bundle
        .component_sha256
        .insert("btts".into(), hash(&bundle.models.btts)?);
    if let Some(v) = &bundle.models.total_corners {
        bundle.component_sha256.insert("corners".into(), hash(v)?);
    }
    if let Some(v) = &bundle.models.home_corners {
        bundle
            .component_sha256
            .insert("home_corners".into(), hash(v)?);
    }
    if let Some(v) = &bundle.models.away_corners {
        bundle
            .component_sha256
            .insert("away_corners".into(), hash(v)?);
    }
    if let Some(v) = &bundle.models.total_cards {
        bundle.component_sha256.insert("cards".into(), hash(v)?);
    }
    let bundle: ArtifactBundle =
        serde_json::from_slice(&serde_json::to_vec(&bundle).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let artifact_sha256 = hash(&bundle)?;
    let file = ArtifactFile {
        artifact_sha256: artifact_sha256.clone(),
        bundle,
    };
    fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let path = output.join("artifact.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut skipped = BTreeMap::new();
    skipped.insert(
        "TEAM_CARDS".into(),
        "team-card market semantics are not established".into(),
    );
    skipped.insert(
        "FIRST_HALF_TOTAL_CORNERS".into(),
        "SOURCE_NOT_AVAILABLE".into(),
    );
    skipped.insert(
        "FIRST_HALF_TOTAL_CARDS".into(),
        "SOURCE_NOT_AVAILABLE".into(),
    );
    let mut trained = vec![
        "MATCH_RESULT".into(),
        "HOME_GOALS".into(),
        "AWAY_GOALS".into(),
        "BTTS".into(),
    ];
    if file.bundle.models.total_corners.is_some() {
        trained.push("FULL_TIME_TOTAL_CORNERS".into())
    }
    if file.bundle.models.home_corners.is_some() && file.bundle.models.away_corners.is_some() {
        trained.push("TEAM_CORNERS".into())
    }
    if file.bundle.models.total_cards.is_some() {
        trained.push("FULL_TIME_TOTAL_CARDS".into())
    }
    Ok(TrainReport {
        rows_considered: n,
        rows_usable: n,
        split,
        trained,
        skipped,
        metrics,
        artifact_path: path.to_string_lossy().into(),
        artifact_sha256,
    })
}

pub fn load_artifact(path: &Path) -> Result<ArtifactFile, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let file: ArtifactFile = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if file.bundle.product != "ARZ Predictor"
        || file.bundle.artifact_schema_version != ARTIFACT_SCHEMA_VERSION
    {
        return Err("unsupported artifact schema/product".into());
    }
    if file.bundle.feature_engine_version != FEATURE_ENGINE_VERSION
        || file.bundle.feature_schema_version != FEATURE_SCHEMA_VERSION
    {
        return Err("incompatible feature schema".into());
    }
    if file.bundle.ordered_feature_names
        != FEATURE_NAMES
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
    {
        return Err("feature registry mismatch".into());
    }
    if hash(&file.bundle)? != file.artifact_sha256 {
        return Err("artifact SHA-256 mismatch".into());
    }
    if hash(&file.bundle.preprocessing)? != file.bundle.preprocessing_sha256 {
        return Err("preprocessing SHA-256 mismatch".into());
    }
    for (name, h) in &file.bundle.component_sha256 {
        let actual = match name.as_str() {
            "match_result" => hash(&file.bundle.models.match_result)?,
            "home_goals" => hash(&file.bundle.models.home_goals)?,
            "away_goals" => hash(&file.bundle.models.away_goals)?,
            "btts" => hash(&file.bundle.models.btts)?,
            "corners" => hash(
                file.bundle
                    .models
                    .total_corners
                    .as_ref()
                    .ok_or("missing corners component")?,
            )?,
            "home_corners" => hash(
                file.bundle
                    .models
                    .home_corners
                    .as_ref()
                    .ok_or("missing home corners component")?,
            )?,
            "away_corners" => hash(
                file.bundle
                    .models
                    .away_corners
                    .as_ref()
                    .ok_or("missing away corners component")?,
            )?,
            "cards" => hash(
                file.bundle
                    .models
                    .total_cards
                    .as_ref()
                    .ok_or("missing cards component")?,
            )?,
            _ => return Err("unknown component".into()),
        };
        if actual != *h {
            return Err(format!("component SHA-256 mismatch: {name}"));
        }
    }
    Ok(file)
}
pub fn validate_artifact(path: &Path) -> ArtifactValidation {
    match load_artifact(path) {
        Ok(v) => ArtifactValidation {
            valid: true,
            version: Some(v.bundle.model_version.clone()),
            feature_compatible: true,
            hash_valid: true,
            available_models: available_models(&v.bundle),
            error: None,
        },
        Err(e) => ArtifactValidation {
            valid: false,
            version: None,
            feature_compatible: !e.contains("feature"),
            hash_valid: !e.contains("SHA-256"),
            available_models: vec![],
            error: Some(e),
        },
    }
}
fn available_models(b: &ArtifactBundle) -> Vec<String> {
    let mut v = vec![
        "MATCH_RESULT".into(),
        "TOTAL_GOALS".into(),
        "BTTS".into(),
        "TEAM_TOTAL_GOALS".into(),
    ];
    if b.models.total_corners.is_some() {
        v.push("FULL_TIME_TOTAL_CORNERS".into())
    }
    if b.models.home_corners.is_some() && b.models.away_corners.is_some() {
        v.push("TEAM_CORNERS".into())
    }
    if b.models.total_cards.is_some() {
        v.push("FULL_TIME_TOTAL_CARDS".into())
    }
    v
}

fn add_binary(
    out: &mut Vec<MarketPrediction>,
    market: &str,
    line: Option<f64>,
    yes: &str,
    no: &str,
    p: f64,
    source: &str,
) {
    let p = clamp_probability(p);
    for (selection, q) in [(yes, p), (no, 1.0 - p)] {
        out.push(MarketPrediction {
            market_type: market.into(),
            line,
            selection: selection.into(),
            model_probability: Some(q),
            raw_probability: Some(p),
            public_probability: Some(q),
            availability: "AVAILABLE".into(),
            source_model: source.into(),
            sample_quality: "MODEL_TRAINED".into(),
            calibration_status: "UNCALIBRATED_V1".into(),
            calibration_version: None,
            calibration_bucket: None,
            bucket_observed_rate: None,
            bucket_sample_size: None,
            bucket_calibration_gap: None,
        })
    }
}
fn unavailable(out: &mut Vec<MarketPrediction>, market: &str, reason: &str) {
    out.push(MarketPrediction {
        market_type: market.into(),
        line: None,
        selection: "UNAVAILABLE".into(),
        model_probability: None,
        raw_probability: None,
        public_probability: None,
        availability: reason.into(),
        source_model: "NONE".into(),
        sample_quality: reason.into(),
        calibration_status: "CALIBRATION_PENDING".into(),
        calibration_version: None,
        calibration_bucket: None,
        bucket_observed_rate: None,
        bucket_sample_size: None,
        bucket_calibration_gap: None,
    })
}
pub fn infer(file: &ArtifactFile, s: &FeatureSnapshot) -> Result<PredictionResult, String> {
    if s.feature_engine_version != FEATURE_ENGINE_VERSION {
        return Err("incompatible feature snapshot".into());
    }
    let x = transform(&file.bundle.preprocessing, &raw_vector(s));
    let m = &file.bundle.models;
    let hl = poisson_predict(&m.home_goals, &x);
    let al = poisson_predict(&m.away_goals, &x);
    let (pd, g) = goal_distribution(hl, al);
    let one = blend3(
        softmax_predict(&m.match_result, &x),
        pd[0],
        file.bundle.match_result_softmax_weight,
    );
    let mut markets = Vec::new();
    for (sel, p) in [("HOME", one[0]), ("DRAW", one[1]), ("AWAY", one[2])] {
        markets.push(MarketPrediction {
            market_type: "MATCH_RESULT".into(),
            line: None,
            selection: sel.into(),
            model_probability: Some(p),
            raw_probability: Some(p),
            public_probability: Some(p),
            availability: "AVAILABLE".into(),
            source_model: "SOFTMAX_POISSON_ENSEMBLE".into(),
            sample_quality: "MODEL_TRAINED".into(),
            calibration_status: "UNCALIBRATED_V1".into(),
            calibration_version: None,
            calibration_bucket: None,
            bucket_observed_rate: None,
            bucket_sample_size: None,
            bucket_calibration_gap: None,
        })
    }
    for line in [1.5, 2.5, 3.5] {
        add_binary(
            &mut markets,
            "TOTAL_GOALS",
            Some(line),
            "OVER",
            "UNDER",
            g[&format!("{line}")],
            "GOAL_POISSON",
        )
    }
    let b = file.bundle.btts_logistic_weight * binary_predict(&m.btts, &x)
        + (1.0 - file.bundle.btts_logistic_weight) * g["btts"];
    add_binary(
        &mut markets,
        "BTTS",
        None,
        "YES",
        "NO",
        b,
        "LOGISTIC_POISSON_ENSEMBLE",
    );
    for (side, key) in [("HOME", "home"), ("AWAY", "away")] {
        for line in [0.5, 1.5] {
            add_binary(
                &mut markets,
                &format!("{side}_TEAM_TOTAL_GOALS"),
                Some(line),
                "OVER",
                "UNDER",
                g[&format!("{key}_{line}")],
                "GOAL_POISSON",
            )
        }
    }
    if let Some(model) = &m.total_corners {
        let l = poisson_predict(model, &x);
        for line in [7.5, 8.5, 9.5, 10.5, 11.5] {
            add_binary(
                &mut markets,
                "FULL_TIME_TOTAL_CORNERS",
                Some(line),
                "OVER",
                "UNDER",
                count_over(l, line),
                "POISSON_COUNT",
            )
        }
    } else {
        unavailable(
            &mut markets,
            "FULL_TIME_TOTAL_CORNERS",
            "INSUFFICIENT_TRAINING_DATA",
        )
    }
    if let Some(model) = &m.total_cards {
        let l = poisson_predict(model, &x);
        for line in [2.5, 3.5, 4.5, 5.5, 6.5] {
            add_binary(
                &mut markets,
                "FULL_TIME_TOTAL_CARDS",
                Some(line),
                "OVER",
                "UNDER",
                count_over(l, line),
                "POISSON_YELLOW_COUNT",
            )
        }
    } else {
        unavailable(
            &mut markets,
            "FULL_TIME_TOTAL_CARDS",
            "INSUFFICIENT_TRAINING_DATA",
        )
    }
    if let (Some(home), Some(away)) = (&m.home_corners, &m.away_corners) {
        for (market, model) in [("HOME_TEAM_CORNERS", home), ("AWAY_TEAM_CORNERS", away)] {
            let lambda = poisson_predict(model, &x);
            for line in [2.5, 3.5, 4.5, 5.5] {
                add_binary(
                    &mut markets,
                    market,
                    Some(line),
                    "OVER",
                    "UNDER",
                    count_over(lambda, line),
                    "POISSON_TEAM_CORNER_COUNT",
                );
            }
        }
    } else {
        unavailable(&mut markets, "TEAM_CORNERS", "INSUFFICIENT_TRAINING_DATA");
    }
    unavailable(&mut markets, "TEAM_CARDS", "SOURCE_NOT_AVAILABLE");
    unavailable(
        &mut markets,
        "FIRST_HALF_TOTAL_CORNERS",
        "SOURCE_NOT_AVAILABLE",
    );
    unavailable(
        &mut markets,
        "FIRST_HALF_TOTAL_CARDS",
        "SOURCE_NOT_AVAILABLE",
    );
    Ok(PredictionResult {
        match_id: s.match_id,
        model_version: file.bundle.model_version.clone(),
        feature_engine_version: FEATURE_ENGINE_VERSION.into(),
        feature_schema_version: FEATURE_SCHEMA_VERSION.into(),
        artifact_hash: file.artifact_sha256.clone(),
        generated_at: s.cutoff_at.clone(),
        base_home_lambda: Some(hl),
        base_away_lambda: Some(al),
        markets,
    })
}

pub fn register(c: &Connection, path: &Path) -> Result<i64, String> {
    let f = load_artifact(path)?;
    let b = &f.bundle;
    let metrics = serde_json::to_string(&b.metrics).map_err(|e| e.to_string())?;
    let labels = serde_json::to_string(&b.label_versions).map_err(|e| e.to_string())?;
    c.execute("INSERT INTO model_versions(version_identifier,model_name,model_type,config_json,artifact_path,artifact_sha256,training_cutoff,feature_engine_version,feature_schema_version,label_versions_json,metrics_json,registry_status) VALUES(?1,'ARZ Predictor Local ML','GLM',?2,?3,?4,?5,?6,?7,?8,?9,'VALIDATED')",params![b.model_version,serde_json::to_string(&b.training_configuration).unwrap(),path.to_string_lossy(),f.artifact_sha256,b.training_cutoff,b.feature_engine_version,b.feature_schema_version,labels,metrics]).map_err(|e|e.to_string())?;
    Ok(c.last_insert_rowid())
}
pub fn activate(c: &Connection, version: &str) -> Result<(), String> {
    let path:String=c.query_row("SELECT artifact_path FROM model_versions WHERE version_identifier=?1 AND registry_status IN ('VALIDATED','ACTIVE')",[version],|r|r.get(0)).map_err(|e|e.to_string())?;
    load_artifact(Path::new(&path))?;
    let tx = c.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("UPDATE model_versions SET is_active=0,registry_status=CASE WHEN registry_status='ACTIVE' THEN 'VALIDATED' ELSE registry_status END WHERE is_active=1",[]).map_err(|e|e.to_string())?;
    tx.execute("UPDATE model_versions SET is_active=1,registry_status='ACTIVE' WHERE version_identifier=?1",[version]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
pub fn active(c: &Connection) -> Result<(i64, ArtifactFile), String> {
    let (id, path) = c
        .query_row(
            "SELECT id,artifact_path FROM model_versions WHERE is_active=1",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )
        .map_err(|e| e.to_string())?;
    Ok((id, load_artifact(Path::new(&path))?))
}
pub fn persist_predictions(
    c: &Connection,
    r: &PredictionResult,
    model_id: i64,
    kickoff: &str,
) -> Result<i64, String> {
    let calibration_version = r
        .markets
        .iter()
        .find_map(|p| p.calibration_version.as_deref());
    let run_hash = calibration_version
        .map(|version| format!("{}|calibration:{}", r.artifact_hash, version))
        .unwrap_or_else(|| r.artifact_hash.clone());
    c.execute("INSERT OR IGNORE INTO prediction_runs(match_id,model_version_id,feature_engine_version,feature_schema_version,artifact_sha256,generated_at,base_home_lambda,base_away_lambda) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![r.match_id,model_id,r.feature_engine_version,r.feature_schema_version,run_hash,r.generated_at,r.base_home_lambda,r.base_away_lambda]).map_err(|e|e.to_string())?;
    let run:i64=c.query_row("SELECT id FROM prediction_runs WHERE match_id=?1 AND model_version_id=?2 AND artifact_sha256=?3",params![r.match_id,model_id,run_hash],|x|x.get(0)).map_err(|e|e.to_string())?;
    for p in r.markets.iter().filter(|p| p.model_probability.is_some()) {
        c.execute("INSERT OR IGNORE INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,prediction_run_id,raw_probability,public_probability,calibration_status,calibration_version,calibration_bucket,bucket_observed_rate,bucket_sample_size,bucket_calibration_gap,availability,sample_quality) VALUES(?1,?2,?3,?4,?5,'MODEL_OUTPUT',?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",params![r.match_id,p.market_type,p.selection,p.line,p.raw_probability.or(p.model_probability),kickoff,model_id,run,p.raw_probability.or(p.model_probability),p.public_probability.or(p.model_probability),p.calibration_status,p.calibration_version,p.calibration_bucket,p.bucket_observed_rate,p.bucket_sample_size.map(|v|v as i64),p.bucket_calibration_gap,p.availability,p.sample_quality]).map_err(|e|e.to_string())?;
    }
    Ok(run)
}

pub fn persisted_prediction_run(
    c: &Connection,
    run_id: i64,
) -> Result<PersistedPredictionRun, String> {
    c.query_row(
        "SELECT id,match_id,model_version_id,artifact_sha256,base_home_lambda,base_away_lambda FROM prediction_runs WHERE id=?1",
        [run_id],
        |r| Ok(PersistedPredictionRun {
            id: r.get(0)?,
            match_id: r.get(1)?,
            model_version_id: r.get(2)?,
            artifact_sha256: r.get(3)?,
            base_home_lambda: r.get(4)?,
            base_away_lambda: r.get(5)?,
        }),
    )
    .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EngineStatus {
    pub active_model_version: Option<String>,
    pub artifact_valid: bool,
    pub feature_compatible: bool,
    pub supported_markets: Vec<String>,
    pub training_cutoff: Option<String>,
    pub calibration_status: String,
}
pub fn status(c: &Connection) -> Result<EngineStatus, String> {
    let row:Option<(String,String,String)>=c.query_row("SELECT version_identifier,artifact_path,training_cutoff FROM model_versions WHERE is_active=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;
    if let Some((v, p, cut)) = row {
        let val = validate_artifact(Path::new(&p));
        Ok(EngineStatus {
            active_model_version: Some(v),
            artifact_valid: val.valid,
            feature_compatible: val.feature_compatible,
            supported_markets: val.available_models,
            training_cutoff: Some(cut),
            calibration_status: "UNCALIBRATED_V1".into(),
        })
    } else {
        Ok(EngineStatus {
            active_model_version: None,
            artifact_valid: false,
            feature_compatible: false,
            supported_markets: vec![],
            training_cutoff: None,
            calibration_status: "CALIBRATION_PENDING".into(),
        })
    }
}

pub fn artifact_path(dir: &Path) -> PathBuf {
    dir.join("artifact.json")
}
