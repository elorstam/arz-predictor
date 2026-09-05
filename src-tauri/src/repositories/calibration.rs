//! Walk-forward evaluation and conservative probability calibration.
use super::{
    features::{FeatureSnapshot, TrainingLabel, FEATURE_ENGINE_VERSION, LABEL_VERSION},
    prediction_engine as pe,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

static BACKTEST_RUN_NONCE: AtomicUsize = AtomicUsize::new(0);

pub const MIN_INITIAL_TRAINING_ROWS: usize = 100;
pub const TEST_WINDOW: usize = 10;
pub const MIN_CALIBRATION_SAMPLES: usize = 20;
pub const MIN_BUCKET_SAMPLE: usize = 20;
pub const MIN_PERFORMANCE_GROUP_SAMPLE: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BacktestObservation {
    pub match_id: i64,
    pub cutoff: String,
    pub market: String,
    pub line: Option<f64>,
    pub selection: String,
    pub raw_probability: f64,
    pub calibrated_probability: Option<f64>,
    pub actual: bool,
    pub settlement: String,
    pub fold: usize,
    pub out_of_sample: bool,
    #[serde(default)]
    pub base_home_lambda: Option<f64>,
    #[serde(default)]
    pub base_away_lambda: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MetricSummary {
    pub sample_count: usize,
    pub log_loss: f64,
    pub brier: f64,
    pub mean_prediction: f64,
    pub observed_hit_rate: f64,
    pub ece: f64,
    pub mce: f64,
    pub accuracy: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibrationBucket {
    pub bucket: String,
    pub predictions: usize,
    pub mean_predicted: f64,
    pub observed_frequency: f64,
    pub calibration_gap: f64,
    pub standard_error: Option<f64>,
    pub quality: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketReport {
    pub market: String,
    pub line: Option<f64>,
    pub selection: String,
    pub raw: MetricSummary,
    pub calibrated: Option<MetricSummary>,
    pub buckets: Vec<CalibrationBucket>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ConfusionMatrix {
    pub home: [usize; 3],
    pub draw: [usize; 3],
    pub away: [usize; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BacktestReport {
    pub run_id: Option<i64>,
    pub model_version: String,
    pub feature_engine_version: String,
    pub fold_count: usize,
    pub training_rows_by_fold: Vec<usize>,
    pub oos_prediction_count: usize,
    pub evaluation_start: String,
    pub evaluation_end: String,
    pub out_of_sample: bool,
    pub markets: Vec<MarketReport>,
    pub confusion_matrix: ConfusionMatrix,
    pub configuration: BTreeMap<String, String>,
    pub result_hash: String,
    pub observations: Vec<BacktestObservation>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibrationParam {
    pub method: String,
    pub a: f64,
    pub b: f64,
    pub temperature: Option<f64>,
    pub sample_count: usize,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibrationArtifact {
    pub calibration_schema_version: String,
    pub calibration_version: String,
    pub parent_model_version: String,
    pub parent_artifact_sha256: String,
    pub feature_engine_version: String,
    pub feature_schema_version: String,
    pub fitted_from: String,
    pub fitted_to: String,
    pub parameters: BTreeMap<String, CalibrationParam>,
    pub raw_metrics: BTreeMap<String, MetricSummary>,
    pub calibrated_metrics: BTreeMap<String, MetricSummary>,
    #[serde(default)]
    pub calibration_configuration: BTreeMap<String, String>,
    #[serde(default)]
    pub family_metadata: BTreeMap<String, String>,
    #[serde(default)]
    pub component_hashes: BTreeMap<String, String>,
    pub created_at: String,
    pub artifact_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibrationReport {
    pub artifact_path: String,
    pub artifact_sha256: String,
    pub methods_fitted: Vec<String>,
    pub skipped: BTreeMap<String, String>,
    pub raw_metrics: BTreeMap<String, MetricSummary>,
    pub calibrated_metrics: BTreeMap<String, MetricSummary>,
}

#[derive(Clone)]
struct SourceRow {
    id: i64,
    cutoff: String,
    json: String,
    label_json: String,
    count_json: String,
    label: TrainingLabel,
    snapshot: FeatureSnapshot,
}
fn sha<T: Serialize>(v: &T) -> Result<String, String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(v).map_err(|e| e.to_string())?)
    ))
}
// Integrity hashes the canonical compact JSON object of every deterministic
// inference payload field. Only created_at and the self-referential hash are
// removed; parameter, metric, cutoff, config, family, component, parent, and
// compatibility fields remain covered.
fn calibration_hash(a: &CalibrationArtifact) -> Result<String, String> {
    let mut value = serde_json::to_value(a).map_err(|e| e.to_string())?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "calibration artifact must serialize as an object".to_string())?;
    object.remove("created_at");
    object.remove("artifact_sha256");
    fn canonicalize(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Number(number) => {
                if let Some(float) = number.as_f64() {
                    *value = serde_json::Value::String(format!("{float:.15e}"));
                }
            }
            serde_json::Value::Array(values) => values.iter_mut().for_each(canonicalize),
            serde_json::Value::Object(values) => values.values_mut().for_each(canonicalize),
            _ => {}
        }
    }
    canonicalize(&mut value);
    let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    let digest = format!("{:x}", Sha256::digest(bytes));
    Ok(digest)
}
fn family(m: &str) -> &'static str {
    if m == "MATCH_RESULT" {
        "MATCH_RESULT"
    } else if m == "BTTS" {
        "BTTS"
    } else if m == "TOTAL_GOALS" {
        "TOTAL_GOALS"
    } else if m.contains("TOTAL_GOALS") {
        "TEAM_TOTAL_GOALS"
    } else if m.contains("CORNERS") {
        if m.contains("TEAM") {
            "TEAM_CORNERS"
        } else {
            "TOTAL_CORNERS"
        }
    } else if m.contains("CARDS") {
        "TOTAL_CARDS"
    } else {
        "OTHER"
    }
}
fn source_rows(c: &Connection) -> Result<Vec<SourceRow>, String> {
    let mut q=c.prepare("SELECT f.match_id,f.cutoff_at,f.feature_json,l.label_json,x.label_json FROM feature_sets f JOIN training_labels l ON l.match_id=f.match_id AND l.label_version=?1 JOIN training_labels x ON x.match_id=f.match_id AND x.label_version=?2 WHERE f.feature_engine_version=?3 ORDER BY f.cutoff_at,f.match_id").map_err(|e|e.to_string())?;
    let rs = q
        .query_map(
            params![
                LABEL_VERSION,
                pe::COUNT_LABEL_VERSION,
                FEATURE_ENGINE_VERSION
            ],
            |r| {
                let id: i64 = r.get(0)?;
                let json: String = r.get(2)?;
                let lj: String = r.get(3)?;
                Ok((id, r.get::<_, String>(1)?, json, lj, r.get::<_, String>(4)?))
            },
        )
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rs {
        let (id, cutoff, json, lj, cj) = r.map_err(|e| e.to_string())?;
        out.push(SourceRow {
            id,
            cutoff,
            json: json.clone(),
            label_json: lj.clone(),
            count_json: cj,
            label: serde_json::from_str(&lj).map_err(|e| e.to_string())?,
            snapshot: serde_json::from_str(&json).map_err(|e| e.to_string())?,
        })
    }
    Ok(out)
}
fn training_connection(rows: &[SourceRow]) -> Result<Connection, String> {
    let c = Connection::open_in_memory().map_err(|e| e.to_string())?;
    c.execute_batch("CREATE TABLE feature_sets(id INTEGER PRIMARY KEY,match_id INTEGER,feature_engine_version TEXT,cutoff_at TEXT,calculated_at TEXT,feature_json TEXT,data_quality_json TEXT);CREATE TABLE training_labels(id INTEGER PRIMARY KEY,match_id INTEGER,label_version TEXT,label_json TEXT);CREATE TABLE matches(id INTEGER PRIMARY KEY,final_home_goals INTEGER,final_away_goals INTEGER,status TEXT,kickoff_at TEXT);CREATE TABLE match_statistics(match_id INTEGER PRIMARY KEY,home_corners INTEGER,away_corners INTEGER,home_yellow_cards INTEGER,away_yellow_cards INTEGER);").map_err(|e|e.to_string())?;
    for (r, i) in rows.iter().zip(1..) {
        c.execute("INSERT INTO matches(id,final_home_goals,final_away_goals,status,kickoff_at) VALUES(?1,?2,?3,'finished',?4)",params![r.id,0,0,r.cutoff]).map_err(|e|e.to_string())?;
        c.execute("INSERT INTO feature_sets(id,match_id,feature_engine_version,cutoff_at,calculated_at,feature_json,data_quality_json) VALUES(?1,?2,?3,?4,?4,?5,'{}')",params![i,r.id,FEATURE_ENGINE_VERSION,r.cutoff,r.json]).map_err(|e|e.to_string())?;
        c.execute(
            "INSERT INTO training_labels(match_id,label_version,label_json) VALUES(?1,?2,?3)",
            params![r.id, LABEL_VERSION, r.label_json],
        )
        .map_err(|e| e.to_string())?;
        c.execute(
            "INSERT INTO training_labels(match_id,label_version,label_json) VALUES(?1,?2,?3)",
            params![r.id, pe::COUNT_LABEL_VERSION, r.count_json],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(c)
}
fn threshold(v: Option<i64>, line: Option<f64>, over: bool) -> Option<bool> {
    v.zip(line)
        .map(|(v, l)| if over { v as f64 > l } else { v as f64 <= l })
}
fn actual(l: &TrainingLabel, c: &pe::CountLabel, m: &pe::MarketPrediction) -> Option<bool> {
    let over = m.selection == "OVER";
    match m.market_type.as_str() {
        "MATCH_RESULT" => Some(l.match_result == m.selection),
        "BTTS" => Some(if m.selection == "YES" {
            l.btts
        } else {
            !l.btts
        }),
        "TOTAL_GOALS" => Some(match m.line {
            Some(1.5) => {
                if over {
                    l.over_1_5
                } else {
                    !l.over_1_5
                }
            }
            Some(2.5) => {
                if over {
                    l.over_2_5
                } else {
                    !l.over_2_5
                }
            }
            Some(3.5) => {
                if over {
                    l.over_3_5
                } else {
                    !l.over_3_5
                }
            }
            _ => return None,
        }),
        "HOME_TEAM_TOTAL_GOALS" => Some(if m.line == Some(0.5) {
            if over {
                l.home_over_0_5
            } else {
                !l.home_over_0_5
            }
        } else {
            if over {
                l.home_over_1_5
            } else {
                !l.home_over_1_5
            }
        }),
        "AWAY_TEAM_TOTAL_GOALS" => Some(if m.line == Some(0.5) {
            if over {
                l.away_over_0_5
            } else {
                !l.away_over_0_5
            }
        } else {
            if over {
                l.away_over_1_5
            } else {
                !l.away_over_1_5
            }
        }),
        "FULL_TIME_TOTAL_CORNERS" => threshold(c.total_corners, m.line, over),
        "FULL_TIME_TOTAL_CARDS" => threshold(c.total_yellow_cards, m.line, over),
        "HOME_TEAM_CORNERS" => threshold(c.home_corners, m.line, over),
        "AWAY_TEAM_CORNERS" => threshold(c.away_corners, m.line, over),
        _ => None,
    }
}
fn logit(p: f64) -> f64 {
    let p = p.clamp(1e-9, 1.0 - 1e-9);
    (p / (1.0 - p)).ln()
}
pub(crate) fn platt(xs: &[(f64, bool)]) -> (f64, f64) {
    let (mut a, mut b) = (1.0, 0.0);
    for _ in 0..500 {
        let (mut ga, mut gb) = (0.0, 0.0);
        for (p, y) in xs {
            let q = 1.0 / (1.0 + (-(a * logit(*p) + b).clamp(-35.0, 35.0)).exp());
            let e = q - if *y { 1.0 } else { 0.0 };
            ga += e * logit(*p);
            gb += e;
        }
        let n = xs.len() as f64;
        a = (a - 0.03 * ga / n).clamp(0.05, 5.0);
        b = (b - 0.03 * gb / n).clamp(-5.0, 5.0)
    }
    (a, b)
}
pub(crate) fn cal_binary(p: f64, a: f64, b: f64) -> f64 {
    1.0 / (1.0 + (-(a * logit(p) + b).clamp(-35.0, 35.0)).exp())
}
fn bucket_name(p: f64) -> String {
    let lo = ((p * 100.0).floor() as i32 / 5 * 5).clamp(0, 95);
    format!("{}_{}", lo, lo + 4)
}
pub fn buckets(obs: &[BacktestObservation], cal: bool) -> Vec<CalibrationBucket> {
    let mut map: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    for o in obs {
        let p = if cal {
            o.calibrated_probability.unwrap_or(o.raw_probability)
        } else {
            o.raw_probability
        };
        map.entry(bucket_name(p))
            .or_default()
            .push((p, if o.actual { 1.0 } else { 0.0 }));
    }
    map.into_iter()
        .map(|(bucket, v)| {
            let n = v.len();
            let mean = v.iter().map(|x| x.0).sum::<f64>() / n as f64;
            let observed = v.iter().map(|x| x.1).sum::<f64>() / n as f64;
            CalibrationBucket {
                bucket,
                predictions: n,
                mean_predicted: mean,
                observed_frequency: observed,
                calibration_gap: observed - mean,
                standard_error: Some((observed * (1.0 - observed) / n as f64).sqrt()),
                quality: if n >= MIN_BUCKET_SAMPLE {
                    "USABLE".into()
                } else {
                    "INSUFFICIENT_SAMPLE".into()
                },
            }
        })
        .collect()
}
pub fn metric(obs: &[BacktestObservation], cal: bool) -> MetricSummary {
    if obs.is_empty() {
        return MetricSummary::default();
    }
    let n = obs.len() as f64;
    let mut m = MetricSummary::default();
    for o in obs {
        let p = if cal {
            o.calibrated_probability.unwrap_or(o.raw_probability)
        } else {
            o.raw_probability
        };
        let y = if o.actual { 1.0 } else { 0.0 };
        m.log_loss -= y * p.max(1e-12).ln() + (1.0 - y) * (1.0 - p).max(1e-12).ln();
        m.brier += (p - y).powi(2);
        m.mean_prediction += p;
        m.observed_hit_rate += y;
    }
    m.sample_count = obs.len();
    m.log_loss /= n;
    m.brier /= n;
    m.mean_prediction /= n;
    m.observed_hit_rate /= n;
    let bs = buckets(obs, cal);
    m.ece = bs
        .iter()
        .filter(|b| b.quality == "USABLE")
        .map(|b| b.calibration_gap.abs() * b.predictions as f64 / n)
        .sum();
    m.mce = bs
        .iter()
        .filter(|b| b.quality == "USABLE")
        .map(|b| b.calibration_gap.abs())
        .fold(0.0, f64::max);
    m
}

pub fn walk_forward(c: &Connection, model_version: &str) -> Result<BacktestReport, String> {
    let ids: Vec<i64> = {
        let mut q = c
            .prepare("SELECT match_id FROM feature_sets WHERE feature_engine_version=?1")
            .map_err(|e| e.to_string())?;
        let mapped = q
            .query_map([FEATURE_ENGINE_VERSION], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        mapped
    };
    for id in ids {
        pe::ensure_count_label(c, id)?;
    }
    let rows = source_rows(c)?;
    if rows.len() < MIN_INITIAL_TRAINING_ROWS + TEST_WINDOW {
        return Err("INSUFFICIENT_BACKTEST_DATA".into());
    }
    let mut obs = Vec::new();
    let mut sizes = Vec::new();
    let (mut fold, mut start) = (0, MIN_INITIAL_TRAINING_ROWS);
    while start < rows.len() {
        let end = (start + TEST_WINDOW).min(rows.len());
        let tc = training_connection(&rows[..start])?;
        let nonce = BACKTEST_RUN_NONCE.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("arz-bt-{}-{}-{}", std::process::id(), nonce, fold));
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let tr = pe::train(&tc, &format!("{model_version}_fold{fold}"), &dir)?;
        let art = pe::load_artifact(Path::new(&tr.artifact_path))?;
        sizes.push(start);
        for r in &rows[start..end] {
            let count: pe::CountLabel =
                serde_json::from_str(&r.count_json).map_err(|e| e.to_string())?;
            let result = pe::infer(&art, &r.snapshot)?;
            for m in result
                .markets
                .iter()
                .filter(|m| m.model_probability.is_some())
            {
                if let Some(y) = actual(&r.label, &count, m) {
                    obs.push(BacktestObservation {
                        match_id: r.id,
                        cutoff: r.cutoff.clone(),
                        market: m.market_type.clone(),
                        line: m.line,
                        selection: m.selection.clone(),
                        raw_probability: m.model_probability.unwrap(),
                        calibrated_probability: None,
                        actual: y,
                        settlement: if y { "WON".into() } else { "LOST".into() },
                        fold,
                        out_of_sample: true,
                        base_home_lambda: result.base_home_lambda,
                        base_away_lambda: result.base_away_lambda,
                    });
                }
            }
        }
        let _ = fs::remove_dir_all(&dir);
        fold += 1;
        start = end;
    }
    let result_hash = sha(&obs)?;
    let mut groups: BTreeMap<(String, Option<String>, String), Vec<BacktestObservation>> =
        BTreeMap::new();
    for o in &obs {
        groups
            .entry((
                o.market.clone(),
                o.line.map(|v| v.to_string()),
                o.selection.clone(),
            ))
            .or_default()
            .push(o.clone());
    }
    let reports = groups
        .into_iter()
        .map(|((market, line, selection), v)| MarketReport {
            market,
            line: line.and_then(|s| s.parse().ok()),
            selection,
            raw: metric(&v, false),
            calibrated: None,
            buckets: buckets(&v, false),
        })
        .collect();
    let mut cfg = BTreeMap::new();
    cfg.insert(
        "initial_training_rows".into(),
        MIN_INITIAL_TRAINING_ROWS.to_string(),
    );
    cfg.insert("test_window".into(), TEST_WINDOW.to_string());
    cfg.insert("oos_only".into(), "true".into());
    Ok(BacktestReport {
        run_id: None,
        model_version: model_version.into(),
        feature_engine_version: FEATURE_ENGINE_VERSION.into(),
        fold_count: fold,
        training_rows_by_fold: sizes,
        oos_prediction_count: obs.len(),
        evaluation_start: obs.first().map(|o| o.cutoff.clone()).unwrap_or_default(),
        evaluation_end: obs.last().map(|o| o.cutoff.clone()).unwrap_or_default(),
        out_of_sample: true,
        markets: reports,
        confusion_matrix: ConfusionMatrix::default(),
        configuration: cfg,
        result_hash,
        observations: obs,
    })
}

pub fn fit_calibration(
    base: &pe::ArtifactFile,
    report: &BacktestReport,
    path: &Path,
) -> Result<CalibrationReport, String> {
    fit_calibration_version(base, report, path, None)
}

pub fn fit_calibration_version(
    base: &pe::ArtifactFile,
    report: &BacktestReport,
    path: &Path,
    requested_version: Option<&str>,
) -> Result<CalibrationReport, String> {
    let ordered_matches: Vec<i64> = report
        .observations
        .iter()
        .map(|o| o.match_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let split = ordered_matches.len() / 2;
    let fit_matches: BTreeSet<i64> = ordered_matches[..split].iter().copied().collect();
    let (fit, eval): (Vec<_>, Vec<_>) = report
        .observations
        .iter()
        .partition(|o| fit_matches.contains(&o.match_id));
    let mut params: BTreeMap<String, CalibrationParam> = BTreeMap::new();
    let mut skipped = BTreeMap::new();
    let mut raw_metrics = BTreeMap::new();
    let mut calibrated_metrics = BTreeMap::new();
    let mut result_groups: BTreeMap<i64, ([f64; 3], usize)> = BTreeMap::new();
    for o in fit.iter().filter(|o| o.market == "MATCH_RESULT") {
        let entry = result_groups.entry(o.match_id).or_insert(([0.0; 3], 0));
        let index = match o.selection.as_str() {
            "HOME" => 0,
            "DRAW" => 1,
            _ => 2,
        };
        entry.0[index] = o.raw_probability;
        if o.actual {
            entry.1 = index;
        }
    }
    if result_groups.len() >= MIN_CALIBRATION_SAMPLES {
        let mut best = (1.0, f64::INFINITY);
        for step in 5..=20 {
            let t = step as f64 / 10.0;
            let loss = result_groups
                .values()
                .map(|(p, y)| {
                    let z = [
                        p[0].max(1e-9).ln() / t,
                        p[1].max(1e-9).ln() / t,
                        p[2].max(1e-9).ln() / t,
                    ];
                    let mx = z.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    let den = z.iter().map(|v| (v - mx).exp()).sum::<f64>();
                    -((z[*y] - mx).exp() / den).max(1e-12).ln()
                })
                .sum::<f64>();
            if loss < best.1 {
                best = (t, loss);
            }
        }
        params.insert(
            "MATCH_RESULT".into(),
            CalibrationParam {
                method: "TEMPERATURE_SCALING".into(),
                a: 1.0,
                b: 0.0,
                temperature: Some(best.0),
                sample_count: result_groups.len(),
                status: "CALIBRATED_V1".into(),
            },
        );
    } else {
        skipped.insert(
            "MATCH_RESULT".into(),
            "CALIBRATION_INSUFFICIENT_DATA".into(),
        );
    }
    for f in [
        "BTTS",
        "TOTAL_GOALS",
        "TEAM_TOTAL_GOALS",
        "TOTAL_CORNERS",
        "TEAM_CORNERS",
        "TOTAL_CARDS",
    ] {
        let samples: Vec<_> = fit
            .iter()
            .filter(|o| family(&o.market) == f)
            .map(|o| (o.raw_probability, o.actual))
            .collect();
        if samples.len() < MIN_CALIBRATION_SAMPLES {
            skipped.insert(f.into(), "CALIBRATION_INSUFFICIENT_DATA".into());
            continue;
        }
        let (a, b) = platt(&samples);
        let raw: Vec<_> = eval
            .iter()
            .filter(|o| family(&o.market) == f)
            .map(|o| (*o).clone())
            .collect();
        let cal: Vec<_> = raw
            .iter()
            .cloned()
            .map(|mut x| {
                x.calibrated_probability = Some(cal_binary(x.raw_probability, a, b));
                x
            })
            .collect();
        let rm = metric(&raw, false);
        let cm = metric(&cal, true);
        raw_metrics.insert(f.into(), rm.clone());
        calibrated_metrics.insert(f.into(), cm.clone());
        let status = if cm.log_loss <= rm.log_loss && cm.ece <= rm.ece + 0.02 {
            "CALIBRATED_V1"
        } else {
            "CALIBRATION_NOT_BENEFICIAL"
        };
        params.insert(
            f.into(),
            CalibrationParam {
                method: "PLATT_LOGISTIC".into(),
                a,
                b,
                temperature: None,
                sample_count: samples.len(),
                status: status.into(),
            },
        );
    }
    let component_hashes = params
        .iter()
        .map(|(name, parameter)| Ok((name.clone(), sha(parameter)?)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let calibration_configuration = [
        ("fit_partition".into(), "chronological_first_half".into()),
        (
            "evaluation_partition".into(),
            "untouched_second_half".into(),
        ),
        (
            "min_calibration_samples".into(),
            MIN_CALIBRATION_SAMPLES.to_string(),
        ),
        ("min_bucket_sample".into(), MIN_BUCKET_SAMPLE.to_string()),
    ]
    .into_iter()
    .collect();
    let family_metadata = [
        (
            "MATCH_RESULT".into(),
            "multiclass_temperature_scaling".into(),
        ),
        ("BTTS".into(), "binary_platt_logistic".into()),
        ("TOTAL_GOALS".into(), "binary_platt_logistic".into()),
        ("TEAM_TOTAL_GOALS".into(), "binary_platt_logistic".into()),
        ("TOTAL_CORNERS".into(), "binary_platt_logistic".into()),
        ("TEAM_CORNERS".into(), "binary_platt_logistic".into()),
        ("TOTAL_CARDS".into(), "binary_platt_logistic".into()),
    ]
    .into_iter()
    .collect();
    let mut a = CalibrationArtifact {
        calibration_schema_version: "calibration_v1".into(),
        calibration_version: requested_version
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{}_cal1", base.bundle.model_version)),
        parent_model_version: base.bundle.model_version.clone(),
        parent_artifact_sha256: sha(&base.bundle)?,
        feature_engine_version: base.bundle.feature_engine_version.clone(),
        feature_schema_version: base.bundle.feature_schema_version.clone(),
        fitted_from: fit.first().map(|o| o.cutoff.clone()).unwrap_or_default(),
        fitted_to: fit.last().map(|o| o.cutoff.clone()).unwrap_or_default(),
        parameters: params.clone(),
        raw_metrics,
        calibrated_metrics,
        calibration_configuration,
        family_metadata,
        component_hashes,
        created_at: Utc::now().to_rfc3339(),
        artifact_sha256: String::new(),
    };
    a = serde_json::from_slice(&serde_json::to_vec(&a).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    a.artifact_sha256 = String::new();
    a.artifact_sha256 = calibration_hash(&a)?;
    fs::write(
        path,
        serde_json::to_vec_pretty(&a).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(CalibrationReport {
        artifact_path: path.to_string_lossy().into(),
        artifact_sha256: a.artifact_sha256,
        methods_fitted: params.keys().cloned().collect(),
        skipped,
        raw_metrics: a.raw_metrics.clone(),
        calibrated_metrics: a.calibrated_metrics.clone(),
    })
}
pub fn load_calibration(path: &Path) -> Result<CalibrationArtifact, String> {
    let a: CalibrationArtifact =
        serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut x = a.clone();
    x.artifact_sha256 = String::new();
    let actual = calibration_hash(&x)?;
    if actual != a.artifact_sha256 {
        return Err(format!(
            "calibration artifact SHA-256 mismatch expected={} actual={}",
            a.artifact_sha256, actual
        ));
    }
    Ok(a)
}
pub(crate) fn temperature_probabilities(raw: [f64; 3], temperature: f64) -> [f64; 3] {
    let z = [
        raw[0].max(1e-9).ln() / temperature.max(1e-9),
        raw[1].max(1e-9).ln() / temperature.max(1e-9),
        raw[2].max(1e-9).ln() / temperature.max(1e-9),
    ];
    let mx = z.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let den = z.iter().map(|v| (v - mx).exp()).sum::<f64>();
    [
        (z[0] - mx).exp() / den,
        (z[1] - mx).exp() / den,
        (z[2] - mx).exp() / den,
    ]
}
fn public_bucket(p: f64) -> String {
    bucket_name(p.clamp(0.0, 1.0))
}
pub fn calibrated_observations(
    report: &BacktestReport,
    cal: &CalibrationArtifact,
) -> Vec<BacktestObservation> {
    let mut out = report.observations.clone();
    let mut result_groups: BTreeMap<i64, [f64; 3]> = BTreeMap::new();
    for o in &out {
        if o.market == "MATCH_RESULT" {
            let i = match o.selection.as_str() {
                "HOME" => 0,
                "DRAW" => 1,
                _ => 2,
            };
            result_groups.entry(o.match_id).or_insert([0.0; 3])[i] = o.raw_probability;
        }
    }
    let temp = cal.parameters.get("MATCH_RESULT");
    for o in &mut out {
        let mut calibrated = None;
        let mut status = "CALIBRATION_INSUFFICIENT_DATA".to_string();
        let mut sample_size = None;
        if o.market == "MATCH_RESULT" {
            if let (Some(q), Some(raw)) = (temp, result_groups.get(&o.match_id)) {
                if let Some(t) = q.temperature {
                    let i = match o.selection.as_str() {
                        "HOME" => 0,
                        "DRAW" => 1,
                        _ => 2,
                    };
                    status = q.status.clone();
                    sample_size = Some(q.sample_count);
                    if q.status == "CALIBRATED_V1" {
                        calibrated = Some(temperature_probabilities(*raw, t)[i]);
                    }
                }
            }
        } else if let Some(q) = cal.parameters.get(family(&o.market)) {
            status = q.status.clone();
            sample_size = Some(q.sample_count);
            if q.method == "PLATT_LOGISTIC" && q.status == "CALIBRATED_V1" {
                calibrated = Some(if o.selection == "OVER" || o.selection == "YES" {
                    cal_binary(o.raw_probability, q.a, q.b)
                } else if o.selection == "UNDER" || o.selection == "NO" {
                    1.0 - cal_binary(1.0 - o.raw_probability, q.a, q.b)
                } else {
                    cal_binary(o.raw_probability, q.a, q.b)
                });
            }
        }
        o.calibrated_probability = calibrated;
        // BacktestObservation deliberately has no public field. Its calibrated
        // value is None when the public contract falls back to raw.
        let _ = (status, sample_size);
    }
    out
}
pub fn apply(result: &mut pe::PredictionResult, cal: &CalibrationArtifact) {
    let mut result_raw = [1.0 / 3.0; 3];
    for m in result.markets.iter() {
        if m.market_type == "MATCH_RESULT" {
            let i = match m.selection.as_str() {
                "HOME" => 0,
                "DRAW" => 1,
                _ => 2,
            };
            result_raw[i] = m
                .raw_probability
                .or(m.model_probability)
                .unwrap_or(result_raw[i]);
        }
    }
    if let Some(q) = cal.parameters.get("MATCH_RESULT") {
        if let Some(t) = q.temperature {
            let values = temperature_probabilities(result_raw, t);
            for m in result
                .markets
                .iter_mut()
                .filter(|m| m.market_type == "MATCH_RESULT")
            {
                let i = match m.selection.as_str() {
                    "HOME" => 0,
                    "DRAW" => 1,
                    _ => 2,
                };
                let public = if q.status == "CALIBRATED_V1" {
                    values[i]
                } else {
                    result_raw[i]
                };
                m.model_probability = Some(public);
                m.public_probability = Some(public);
                m.calibration_status = q.status.clone();
                m.calibration_bucket = Some(public_bucket(public));
                m.bucket_sample_size = Some(q.sample_count);
            }
        }
    }
    for m in &mut result.markets {
        let Some(raw) = m.raw_probability.or(m.model_probability) else {
            continue;
        };
        let mut public = raw;
        if let Some(q) = cal.parameters.get(family(&m.market_type)) {
            if q.status == "CALIBRATED_V1" {
                public = if m.selection == "OVER" || m.selection == "YES" {
                    cal_binary(raw, q.a, q.b)
                } else if m.selection == "UNDER" || m.selection == "NO" {
                    1.0 - cal_binary(1.0 - raw, q.a, q.b)
                } else {
                    cal_binary(raw, q.a, q.b)
                };
            }
            m.calibration_status = q.status.clone();
            m.bucket_sample_size = Some(q.sample_count);
        }
        m.model_probability = Some(public);
        m.public_probability = Some(public);
        m.calibration_bucket = Some(public_bucket(public));
        m.calibration_version = Some(cal.calibration_version.clone());
    }
}
pub fn persist_backtest(c: &Connection, r: &BacktestReport) -> Result<i64, String> {
    let cfg = serde_json::to_string(&r.configuration).map_err(|e| e.to_string())?;
    c.execute("INSERT INTO backtest_runs(model_family_version,feature_engine_version,feature_schema_version,label_version,started_at,completed_at,evaluation_start,evaluation_end,configuration_json,status,result_hash) VALUES(?1,?2,?3,?4,?5,?5,?6,?7,?8,'COMPLETED',?9)",params![r.model_version,FEATURE_ENGINE_VERSION,pe::FEATURE_SCHEMA_VERSION,LABEL_VERSION,Utc::now().to_rfc3339(),r.evaluation_start,r.evaluation_end,cfg,r.result_hash]).map_err(|e|e.to_string())?;
    let id = c.last_insert_rowid();
    for o in &r.observations {
        c.execute("INSERT INTO backtest_predictions(run_id,match_id,fold,market,line_value,selection,raw_probability,actual_result,settlement,generated_at,base_home_lambda,base_away_lambda) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![id,o.match_id,o.fold,o.market,o.line,o.selection,o.raw_probability,if o.actual{"true"}else{"false"},o.settlement,o.cutoff,o.base_home_lambda,o.base_away_lambda]).map_err(|e|e.to_string())?;
    }
    Ok(id)
}

fn persist_calibration_model(
    c: &Connection,
    report: &CalibrationReport,
    base: &pe::ArtifactFile,
) -> Result<i64, String> {
    let artifact = load_calibration(Path::new(&report.artifact_path))?;
    let params_json = serde_json::to_string(&artifact.parameters).map_err(|e| e.to_string())?;
    let validation = serde_json::json!({
        "raw": artifact.raw_metrics,
        "calibrated": artifact.calibrated_metrics,
        "configuration": artifact.calibration_configuration,
    });
    c.execute("INSERT OR IGNORE INTO calibration_models(parent_model_version,parent_artifact_sha256,calibration_version,market_family,calibration_method,parameters_json,fitted_from,fitted_to,sample_count,validation_metrics_json,artifact_sha256,artifact_path,created_at) VALUES(?1,?2,?3,'MULTI_FAMILY','MULTI_METHOD',?4,?5,?6,?7,?8,?9,?10,?11)", params![base.bundle.model_version,artifact.parent_artifact_sha256,artifact.calibration_version,params_json,artifact.fitted_from,artifact.fitted_to,artifact.parameters.values().map(|p|p.sample_count).sum::<usize>(),validation.to_string(),artifact.artifact_sha256,report.artifact_path,artifact.created_at]).map_err(|e|e.to_string())?;
    let row: (i64, String, String) = c.query_row(
        "SELECT id, artifact_sha256, parent_artifact_sha256 FROM calibration_models WHERE calibration_version=?1",
        [&artifact.calibration_version],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).map_err(|e| e.to_string())?;
    if row.1 != artifact.artifact_sha256 || row.2 != artifact.parent_artifact_sha256 {
        return Err("calibration version already exists with different payload".into());
    }
    Ok(row.0)
}

pub fn persist_calibration(
    c: &Connection,
    report: &CalibrationReport,
    base: &pe::ArtifactFile,
) -> Result<i64, String> {
    persist_calibration_model(c, report, base)
}

pub fn load_backtest_report(c: &Connection, run_id: i64) -> Result<BacktestReport, String> {
    let model_version: String = c
        .query_row(
            "SELECT model_family_version FROM backtest_runs WHERE id=?1",
            [run_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut q = c.prepare("SELECT match_id,generated_at,market,line_value,selection,raw_probability,actual_result,settlement,fold,out_of_sample,base_home_lambda,base_away_lambda FROM backtest_predictions WHERE run_id=?1 ORDER BY generated_at,match_id,market,selection").map_err(|e| e.to_string())?;
    let observations = q
        .query_map([run_id], |r| {
            Ok(BacktestObservation {
                match_id: r.get(0)?,
                cutoff: r.get(1)?,
                market: r.get(2)?,
                line: r.get(3)?,
                selection: r.get(4)?,
                raw_probability: r.get(5)?,
                calibrated_probability: None,
                actual: r.get::<_, String>(6)? == "true",
                settlement: r.get(7)?,
                fold: r.get::<_, i64>(8)? as usize,
                out_of_sample: r.get::<_, i64>(9)? == 1,
                base_home_lambda: r.get(10)?,
                base_away_lambda: r.get(11)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let start = observations
        .first()
        .map(|o| o.cutoff.clone())
        .unwrap_or_default();
    let end = observations
        .last()
        .map(|o| o.cutoff.clone())
        .unwrap_or_default();
    Ok(BacktestReport {
        run_id: Some(run_id),
        model_version,
        feature_engine_version: FEATURE_ENGINE_VERSION.into(),
        fold_count: observations.iter().map(|o| o.fold).max().unwrap_or(0) + 1,
        training_rows_by_fold: vec![],
        oos_prediction_count: observations.len(),
        evaluation_start: start,
        evaluation_end: end,
        out_of_sample: true,
        markets: vec![],
        confusion_matrix: Default::default(),
        configuration: BTreeMap::new(),
        result_hash: String::new(),
        observations,
    })
}

pub fn persist_calibration_for_run(
    c: &Connection,
    report: &CalibrationReport,
    base: &pe::ArtifactFile,
    run_id: i64,
) -> Result<i64, String> {
    let calibration_model_id = persist_calibration_model(c, report, base)?;
    let artifact = load_calibration(Path::new(&report.artifact_path))?;
    for o in calibrated_observations(&load_backtest_report(c, run_id)?, &artifact) {
        let parameter = if o.market == "MATCH_RESULT" {
            artifact.parameters.get("MATCH_RESULT")
        } else {
            artifact.parameters.get(family(&o.market))
        };
        let status = parameter
            .map(|p| p.status.clone())
            .unwrap_or_else(|| "CALIBRATION_INSUFFICIENT_DATA".into());
        let public = o.calibrated_probability.unwrap_or(o.raw_probability);
        let sample_size = parameter.map(|p| p.sample_count).unwrap_or(0);
        c.execute("INSERT OR IGNORE INTO backtest_calibration_predictions(run_id,calibration_model_id,match_id,market,line_value,selection,raw_probability,calibrated_probability,public_probability,calibration_status,calibration_bucket,bucket_sample_size,actual_result,settlement,generated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)", params![run_id, calibration_model_id, o.match_id, o.market, o.line, o.selection, o.raw_probability, o.calibrated_probability, public, status, public_bucket(public), sample_size, if o.actual { "true" } else { "false" }, o.settlement, o.cutoff]).map_err(|e| e.to_string())?;
    }
    Ok(calibration_model_id)
}

pub fn activate_calibration(c: &Connection, version: &str) -> Result<(), String> {
    let path: String = c
        .query_row(
            "SELECT artifact_path FROM calibration_models WHERE calibration_version=?1",
            [version],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    load_calibration(Path::new(&path))?;
    c.execute("UPDATE calibration_models SET is_active=0", [])
        .map_err(|e| e.to_string())?;
    c.execute(
        "UPDATE calibration_models SET is_active=1 WHERE calibration_version=?1",
        [version],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn active_calibration(c: &Connection) -> Result<Option<CalibrationArtifact>, String> {
    let path: Option<String> = c
        .query_row(
            "SELECT artifact_path FROM calibration_models WHERE is_active=1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    path.map(|p| load_calibration(Path::new(&p))).transpose()
}
