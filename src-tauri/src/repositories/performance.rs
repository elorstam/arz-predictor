//! Read-only performance and odds-band diagnostics over persisted OOS results.
//! Odds are intentionally isolated here; they never enter features, training,
//! calibration fitting, or probability generation.
use super::calibration::MIN_PERFORMANCE_GROUP_SAMPLE;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PerformancePeriod {
    Last7Days,
    Last30Days,
    CurrentSeason,
    AllTime,
    Custom,
}
impl Default for PerformancePeriod {
    fn default() -> Self {
        Self::AllTime
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceFilter {
    pub model_version: Option<String>,
    pub calibration_version: Option<String>,
    pub market: Option<String>,
    pub selection: Option<String>,
    pub line: Option<f64>,
    pub competition_id: Option<i64>,
    #[serde(default)]
    pub period: PerformancePeriod,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    /// Makes relative periods deterministic in tests and reports.
    pub as_of: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceGroup {
    pub competition_id: Option<i64>,
    pub competition_name: Option<String>,
    pub market: String,
    pub selection: Option<String>,
    pub line: Option<f64>,
    pub sample_count: usize,
    pub mean_raw_probability: Option<f64>,
    pub mean_public_probability: Option<f64>,
    pub observed_hit_rate: Option<f64>,
    pub raw_log_loss: Option<f64>,
    pub calibrated_log_loss: Option<f64>,
    pub raw_brier: Option<f64>,
    pub calibrated_brier: Option<f64>,
    pub raw_ece: Option<f64>,
    pub calibrated_ece: Option<f64>,
    pub calibration_gap: Option<f64>,
    pub won: Option<usize>,
    pub lost: Option<usize>,
    pub void_count: usize,
    pub unsettled_count: usize,
    pub quality: String,
    pub multiclass: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PerformanceReport {
    pub groups: Vec<PerformanceGroup>,
    pub filter: PerformanceFilter,
}

#[derive(Clone)]
struct Row {
    competition_id: i64,
    competition_name: String,
    match_id: i64,
    generated_at: String,
    market: String,
    line: Option<f64>,
    selection: String,
    raw: f64,
    public: f64,
    actual: bool,
    settlement: String,
}

pub const ODDS_BANDS: &[(&str, f64, Option<f64>)] = &[
    ("1.01-1.29", 1.01, Some(1.30)),
    ("1.30-1.49", 1.30, Some(1.50)),
    ("1.50-1.79", 1.50, Some(1.80)),
    ("1.80-1.99", 1.80, Some(2.00)),
    ("2.00-2.49", 2.00, Some(2.50)),
    ("2.50+", 2.50, None),
];
pub fn odds_band(odd: f64) -> Option<&'static str> {
    ODDS_BANDS
        .iter()
        .find(|(_, lo, hi)| odd >= *lo && hi.map(|x| odd < x).unwrap_or(true))
        .map(|x| x.0)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct OddsBandGroup {
    pub odds_band: String,
    pub sample_count: usize,
    pub observed_hit_rate: f64,
    pub mean_model_probability: f64,
    pub brier: f64,
    pub log_loss: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct OddsBandReport {
    pub bands: Vec<OddsBandGroup>,
    pub predictions_without_historical_odds: usize,
}

fn load_rows(c: &Connection, f: &PerformanceFilter) -> Result<Vec<Row>, String> {
    let as_of = f.as_of.clone().unwrap_or_else(|| Utc::now().to_rfc3339());
    let from = f.date_from.clone();
    let to = f.date_to.clone();
    let period = match f.period {
        PerformancePeriod::Last7Days => "LAST_7_DAYS",
        PerformancePeriod::Last30Days => "LAST_30_DAYS",
        PerformancePeriod::CurrentSeason => "CURRENT_SEASON",
        PerformancePeriod::AllTime => "ALL_TIME",
        PerformancePeriod::Custom => "CUSTOM",
    };
    let mut q = c.prepare("SELECT m.competition_id,co.name,b.match_id,b.generated_at,b.market,b.line_value,b.selection,b.raw_probability,b.public_probability,b.actual_result,b.settlement FROM backtest_calibration_predictions b JOIN backtest_runs br ON br.id=b.run_id JOIN calibration_models cm ON cm.id=b.calibration_model_id JOIN matches m ON m.id=b.match_id JOIN competitions co ON co.id=m.competition_id WHERE (?1 IS NULL OR br.model_family_version=?1) AND (?2 IS NULL OR cm.calibration_version=?2) AND (?2 IS NOT NULL OR cm.id=(SELECT MAX(cm2.id) FROM calibration_models cm2 WHERE cm2.parent_model_version=br.model_family_version)) AND (?3 IS NULL OR b.market=?3) AND (?4 IS NULL OR b.selection=?4) AND (?5 IS NULL OR b.line_value=?5) AND (?6 IS NULL OR m.competition_id=?6) AND (?7='ALL_TIME' OR (?7='LAST_7_DAYS' AND datetime(m.kickoff_at)>=datetime(?8,'-7 days') AND datetime(m.kickoff_at)<=datetime(?8)) OR (?7='LAST_30_DAYS' AND datetime(m.kickoff_at)>=datetime(?8,'-30 days') AND datetime(m.kickoff_at)<=datetime(?8)) OR (?7='CURRENT_SEASON' AND m.season=co.current_season) OR (?7='CUSTOM' AND (?9 IS NULL OR m.kickoff_at>=?9) AND (?10 IS NULL OR m.kickoff_at<=?10))) ORDER BY m.competition_id,m.kickoff_at,b.match_id,b.market,b.selection").map_err(|e| e.to_string())?;
    let rows = q
        .query_map(
            params![
                f.model_version,
                f.calibration_version,
                f.market,
                f.selection,
                f.line,
                f.competition_id,
                period,
                as_of,
                from,
                to
            ],
            |r| {
                Ok(Row {
                    competition_id: r.get(0)?,
                    competition_name: r.get(1)?,
                    match_id: r.get(2)?,
                    generated_at: r.get(3)?,
                    market: r.get(4)?,
                    line: r.get(5)?,
                    selection: r.get(6)?,
                    raw: r.get(7)?,
                    public: r.get(8)?,
                    actual: r.get::<_, String>(9)? == "true",
                    settlement: r.get(10)?,
                })
            },
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string());
    rows
}

type BinaryMetrics = (
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BinaryQualityMetrics {
    pub average_probability: f64,
    pub actual_rate: f64,
    pub brier: f64,
    pub log_loss: f64,
    pub ece: f64,
}

/// Shared Phase 6.1 probability-quality semantics. Callers decide whether the
/// sample size is large enough to publish ECE.
pub(crate) fn binary_quality_metrics(samples: &[(f64, bool)]) -> Option<BinaryQualityMetrics> {
    if samples.is_empty() {
        return None;
    }
    let n = samples.len() as f64;
    let average_probability = samples.iter().map(|(p, _)| p).sum::<f64>() / n;
    let actual_rate = samples.iter().filter(|(_, actual)| *actual).count() as f64 / n;
    let brier = samples
        .iter()
        .map(|(p, actual)| (p - if *actual { 1.0 } else { 0.0 }).powi(2))
        .sum::<f64>()
        / n;
    let log_loss = samples
        .iter()
        .map(|(p, actual)| {
            let y = if *actual { 1.0 } else { 0.0 };
            -y * p.max(1e-12).ln() - (1.0 - y) * (1.0 - p).max(1e-12).ln()
        })
        .sum::<f64>()
        / n;
    let mut buckets: BTreeMap<i32, (usize, f64, f64)> = BTreeMap::new();
    for (p, actual) in samples {
        let key = ((p * 100.0).floor() as i32 / 5 * 5).clamp(0, 95);
        let bucket = buckets.entry(key).or_insert((0, 0.0, 0.0));
        bucket.0 += 1;
        bucket.1 += *p;
        bucket.2 += if *actual { 1.0 } else { 0.0 };
    }
    let ece = buckets
        .values()
        .map(|bucket| {
            ((bucket.1 / bucket.0 as f64) - (bucket.2 / bucket.0 as f64)).abs() * bucket.0 as f64
                / n
        })
        .sum();
    Some(BinaryQualityMetrics {
        average_probability,
        actual_rate,
        brier,
        log_loss,
        ece,
    })
}

fn binary_metrics(rows: &[&Row]) -> BinaryMetrics {
    let settled: Vec<_> = rows
        .iter()
        .filter(|r| r.settlement == "WON" || r.settlement == "LOST")
        .collect();
    if settled.is_empty() {
        return (None, None, None, None, None, None, None, None, None);
    }
    let raw = binary_quality_metrics(
        &settled
            .iter()
            .map(|row| (row.raw, row.actual))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let public = binary_quality_metrics(
        &settled
            .iter()
            .map(|row| (row.public, row.actual))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    (
        Some(raw.average_probability),
        Some(public.average_probability),
        Some(public.actual_rate),
        Some(raw.log_loss),
        Some(public.log_loss),
        Some(raw.brier),
        Some(public.brier),
        Some(raw.ece),
        Some(public.ece),
    )
}
fn multiclass(rows: &[&Row]) -> PerformanceGroup {
    let mut matches: BTreeMap<i64, Vec<&Row>> = BTreeMap::new();
    for r in rows {
        matches.entry(r.match_id).or_default().push(r);
    }
    let mut rawll = 0.;
    let mut publl = 0.;
    let mut rawb = 0.;
    let mut pubb = 0.;
    let mut rawactual = 0.;
    let mut pubactual = 0.;
    let mut public_accuracy = 0.;
    let mut n = 0.;
    let mut raw_conf = Vec::new();
    let mut pub_conf = Vec::new();
    for rs in matches.values() {
        let complete = rs
            .iter()
            .filter(|r| ["HOME", "DRAW", "AWAY"].contains(&r.selection.as_str()))
            .count()
            == 3;
        if !complete {
            continue;
        }
        let actual = rs
            .iter()
            .find(|r| r.actual)
            .map(|r| r.selection.as_str())
            .unwrap_or("AWAY");
        let raw_actual = rs
            .iter()
            .find(|r| r.selection == actual)
            .map(|r| r.raw)
            .unwrap_or(1. / 3.);
        let pub_actual = rs
            .iter()
            .find(|r| r.selection == actual)
            .map(|r| r.public)
            .unwrap_or(1. / 3.);
        rawll -= raw_actual.max(1e-12).ln();
        publl -= pub_actual.max(1e-12).ln();
        for r in rs {
            let y = if r.selection == actual { 1. } else { 0. };
            rawb += (r.raw - y).powi(2);
            pubb += (r.public - y).powi(2);
        }
        rawactual += raw_actual;
        pubactual += pub_actual;
        let raw_pick = rs.iter().max_by(|a, b| a.raw.total_cmp(&b.raw)).unwrap();
        let pub_pick = rs
            .iter()
            .max_by(|a, b| a.public.total_cmp(&b.public))
            .unwrap();
        public_accuracy += if pub_pick.selection == actual { 1. } else { 0. };
        raw_conf.push((raw_pick.raw, raw_pick.selection == actual));
        pub_conf.push((pub_pick.public, pub_pick.selection == actual));
        n += 1.;
    }
    let ece = |xs: &Vec<(f64, bool)>| {
        let mut b: BTreeMap<i32, (usize, f64, f64)> = BTreeMap::new();
        for (p, y) in xs {
            let k = ((p * 100.).floor() as i32 / 5 * 5).clamp(0, 95);
            let e = b.entry(k).or_insert((0, 0., 0.));
            e.0 += 1;
            e.1 += *p;
            e.2 += if *y { 1. } else { 0. };
        }
        if n == 0. {
            0.
        } else {
            b.values()
                .map(|x| ((x.1 / x.0 as f64) - (x.2 / x.0 as f64)).abs() * x.0 as f64 / n)
                .sum()
        }
    };
    let first = rows.first().unwrap();
    PerformanceGroup {
        competition_id: Some(first.competition_id),
        competition_name: Some(first.competition_name.clone()),
        market: first.market.clone(),
        selection: None,
        line: first.line,
        sample_count: n as usize,
        mean_raw_probability: (n > 0.).then_some(rawactual / n),
        mean_public_probability: (n > 0.).then_some(pubactual / n),
        observed_hit_rate: (n > 0.).then_some(public_accuracy / n),
        raw_log_loss: (n > 0.).then_some(rawll / n),
        calibrated_log_loss: (n > 0.).then_some(publl / n),
        raw_brier: (n > 0.).then_some(rawb / n),
        calibrated_brier: (n > 0.).then_some(pubb / n),
        raw_ece: (n > 0.).then_some(ece(&raw_conf)),
        calibrated_ece: (n > 0.).then_some(ece(&pub_conf)),
        calibration_gap: (n > 0.).then_some(pubactual / n - public_accuracy / n),
        won: None,
        lost: None,
        void_count: 0,
        unsettled_count: 0,
        quality: if n as usize >= MIN_PERFORMANCE_GROUP_SAMPLE {
            "USABLE".into()
        } else {
            "INSUFFICIENT_SAMPLE".into()
        },
        multiclass: true,
    }
}
pub fn prediction_model_performance(
    c: &Connection,
    f: &PerformanceFilter,
) -> Result<PerformanceReport, String> {
    let rows = load_rows(c, f)?;
    let mut groups: BTreeMap<(i64, String, Option<String>, Option<String>), Vec<&Row>> =
        BTreeMap::new();
    for r in &rows {
        let selection = if r.market == "MATCH_RESULT" && f.selection.is_none() {
            None
        } else {
            Some(r.selection.clone())
        };
        groups
            .entry((
                r.competition_id,
                r.market.clone(),
                r.line.map(|x| x.to_string()),
                selection,
            ))
            .or_default()
            .push(r);
    }
    let mut out = Vec::new();
    for ((cid, market, line, selection), rs) in groups {
        if market == "MATCH_RESULT" && selection.is_none() {
            out.push(multiclass(&rs));
            continue;
        }
        let first = rs[0];
        let (rawmean, pubmean, hit, rawll, publl, rawb, pubb, rawece, pubece) = binary_metrics(&rs);
        let settled = rs
            .iter()
            .filter(|r| r.settlement == "WON" || r.settlement == "LOST")
            .count();
        out.push(PerformanceGroup {
            competition_id: Some(cid),
            competition_name: Some(first.competition_name.clone()),
            market,
            selection,
            line: line.and_then(|x| x.parse().ok()),
            sample_count: settled,
            mean_raw_probability: rawmean,
            mean_public_probability: pubmean,
            observed_hit_rate: hit,
            raw_log_loss: rawll,
            calibrated_log_loss: publl,
            raw_brier: rawb,
            calibrated_brier: pubb,
            raw_ece: rawece,
            calibrated_ece: pubece,
            calibration_gap: match (hit, pubmean) {
                (Some(h), Some(p)) => Some(h - p),
                _ => None,
            },
            won: Some(rs.iter().filter(|r| r.settlement == "WON").count()),
            lost: Some(rs.iter().filter(|r| r.settlement == "LOST").count()),
            void_count: rs.iter().filter(|r| r.settlement == "VOID").count(),
            unsettled_count: rs.iter().filter(|r| r.settlement == "UNSETTLED").count(),
            quality: if settled >= MIN_PERFORMANCE_GROUP_SAMPLE {
                "USABLE".into()
            } else {
                "INSUFFICIENT_SAMPLE".into()
            },
            multiclass: false,
        });
    }
    Ok(PerformanceReport {
        groups: out,
        filter: f.clone(),
    })
}

fn latest_odd(c: &Connection, r: &Row) -> Result<Option<f64>, String> {
    let mut q=c.prepare("SELECT odd FROM odds_snapshots WHERE match_id=?1 AND captured_at<=?2 AND (UPPER(COALESCE(normalized_market_type,''))=UPPER(?3) OR UPPER(market_name)=UPPER(?3)) AND (UPPER(COALESCE(normalized_selection,''))=UPPER(?4) OR UPPER(selection)=UPPER(?4)) AND (line_value IS ?5 OR line_value=?5) ORDER BY captured_at DESC,id DESC LIMIT 1").map_err(|e|e.to_string())?;
    q.query_row(
        params![r.match_id, r.generated_at, r.market, r.selection, r.line],
        |x| x.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}
pub fn prediction_odds_band_performance(
    c: &Connection,
    f: &PerformanceFilter,
) -> Result<OddsBandReport, String> {
    let rows = load_rows(c, f)?;
    let mut groups: BTreeMap<String, Vec<(f64, f64, bool)>> = BTreeMap::new();
    let mut missing = 0;
    for r in &rows {
        let odd = latest_odd(c, r)?;
        if let Some(o) = odd {
            if let Some(b) = odds_band(o) {
                groups
                    .entry(b.into())
                    .or_default()
                    .push((o, r.public, r.actual));
            } else {
                missing += 1;
            }
        } else {
            missing += 1;
        }
    }
    let bands = ODDS_BANDS
        .iter()
        .filter_map(|(name, _, _)| {
            groups.get(*name).map(|xs| {
                let n = xs.len() as f64;
                OddsBandGroup {
                    odds_band: (*name).into(),
                    sample_count: xs.len(),
                    observed_hit_rate: xs.iter().filter(|x| x.2).count() as f64 / n,
                    mean_model_probability: xs.iter().map(|x| x.1).sum::<f64>() / n,
                    brier: xs
                        .iter()
                        .map(|x| (x.1 - if x.2 { 1. } else { 0. }).powi(2))
                        .sum::<f64>()
                        / n,
                    log_loss: xs
                        .iter()
                        .map(|x| {
                            let y = if x.2 { 1. } else { 0. };
                            -y * x.1.max(1e-12).ln() - (1. - y) * (1. - x.1).max(1e-12).ln()
                        })
                        .sum::<f64>()
                        / n,
                }
            })
        })
        .collect();
    Ok(OddsBandReport {
        bands,
        predictions_without_historical_odds: missing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;

    fn fixture() -> Database {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        c.execute("INSERT INTO competitions(id,name,current_season) VALUES(1,'Competition A','2025/26'),(2,'Competition B','2024/25')", []).unwrap();
        for id in 1..=40 {
            c.execute(
                "INSERT INTO teams(id,normalized_name) VALUES(?1,?2)",
                params![id, format!("Team {id}")],
            )
            .unwrap();
        }
        c.execute("INSERT INTO backtest_runs(id,model_family_version,feature_engine_version,feature_schema_version,label_version,started_at,completed_at,evaluation_start,evaluation_end,configuration_json,status,result_hash) VALUES(1,'model_v1','fe_v1','pred_features_v1','labels_v1','2025-01-01','2025-01-01','2025-01-01','2025-02-10','{}','COMPLETED','fixture')", []).unwrap();
        for (id, version) in [(1, "calibration_v1"), (2, "calibration_v2")] {
            c.execute("INSERT INTO calibration_models(id,parent_model_version,parent_artifact_sha256,calibration_version,market_family,calibration_method,parameters_json,fitted_from,fitted_to,sample_count,validation_metrics_json,artifact_sha256,artifact_path,created_at) VALUES(?1,'model_v1','parent',?2,'MULTI_FAMILY','MULTI_METHOD','{}','2025-01-01','2025-01-01',100,'{}',?2,'fixture.json','2025-01-01')", params![id,version]).unwrap();
        }
        let dates = [
            "2025-01-01T12:00:00Z",
            "2025-01-15T12:00:00Z",
            "2025-02-01T12:00:00Z",
            "2025-02-04T12:00:00Z",
            "2025-02-08T12:00:00Z",
            "2025-02-09T12:00:00Z",
            "2025-02-10T12:00:00Z",
            "2025-02-11T12:00:00Z",
        ];
        for (i, date) in dates.iter().enumerate() {
            let id = (i + 1) as i64;
            let comp = if i < 6 { 1 } else { 2 };
            let season = if comp == 1 {
                "2025/26"
            } else if i == 6 {
                "2024/25"
            } else {
                "2023/24"
            };
            c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,final_home_goals,final_away_goals) VALUES(?1,?2,?3,?4,?5,?6,'finished',1,0)", params![id,comp,season,id*2-1,id*2,date]).unwrap();
            let actual = i % 2 == 0;
            c.execute("INSERT INTO backtest_calibration_predictions(run_id,calibration_model_id,match_id,market,line_value,selection,raw_probability,calibrated_probability,public_probability,calibration_status,calibration_bucket,bucket_sample_size,actual_result,settlement,generated_at) VALUES(1,1,?1,'BTTS',NULL,'YES',0.70,0.65,0.65,'CALIBRATED_V1','65_69',100,?2,?3,?4)", params![id,if actual{"true"}else{"false"},if actual{"WON"}else{"LOST"},date]).unwrap();
            if i == 0 {
                c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,captured_at) VALUES(?1,'iddaa','btts','BTTS','YES',1.30,'BTTS','YES','2025-01-01T11:00:00Z')", [id]).unwrap();
            }
            if i == 1 {
                c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,captured_at) VALUES(?1,'iddaa','btts','BTTS','YES',1.30,'BTTS','YES','2025-01-15T11:00:00Z'),(?1,'iddaa','btts','BTTS','YES',2.50,'BTTS','YES','2025-01-15T13:00:00Z')", [id]).unwrap();
            }
            if i >= 2 && i <= 5 {
                let odd = [1.50, 1.80, 2.00, 2.50][i - 2];
                c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,captured_at) VALUES(?1,'iddaa','btts','BTTS','YES',?2,'BTTS','YES',?3)", params![id,odd,date.replace("12:00","11:00")]).unwrap();
            }
        }
        drop(c);
        db
    }

    fn filter(period: PerformancePeriod) -> PerformanceFilter {
        PerformanceFilter {
            model_version: Some("model_v1".into()),
            calibration_version: Some("calibration_v1".into()),
            market: Some("BTTS".into()),
            selection: Some("YES".into()),
            line: None,
            competition_id: None,
            period,
            date_from: None,
            date_to: None,
            as_of: Some("2025-02-11T23:59:59Z".into()),
        }
    }

    #[test]
    fn performance_filters_competitions_periods_and_odds_are_deterministic() {
        let db = fixture();
        let c = db.connection().unwrap();
        let all = prediction_model_performance(&c, &filter(PerformancePeriod::AllTime)).unwrap();
        let last30 =
            prediction_model_performance(&c, &filter(PerformancePeriod::Last30Days)).unwrap();
        let last7 =
            prediction_model_performance(&c, &filter(PerformancePeriod::Last7Days)).unwrap();
        let season =
            prediction_model_performance(&c, &filter(PerformancePeriod::CurrentSeason)).unwrap();
        let mut custom = filter(PerformancePeriod::Custom);
        custom.date_from = Some("2025-02-04T00:00:00Z".into());
        custom.date_to = Some("2025-02-10T23:59:59Z".into());
        let custom = prediction_model_performance(&c, &custom).unwrap();
        let count = |r: &PerformanceReport| r.groups.iter().map(|g| g.sample_count).sum::<usize>();
        assert_eq!(count(&all), 8);
        assert_eq!(count(&last30), 7);
        assert_eq!(count(&last7), 4);
        assert_eq!(count(&season), 7);
        assert_eq!(count(&custom), 4);
        let mut a = filter(PerformancePeriod::AllTime);
        a.competition_id = Some(1);
        let a = prediction_model_performance(&c, &a).unwrap();
        let mut b = filter(PerformancePeriod::AllTime);
        b.competition_id = Some(2);
        let b = prediction_model_performance(&c, &b).unwrap();
        assert_eq!(count(&a), 6);
        assert_eq!(count(&b), 2);
        assert!(a.groups.iter().all(|g| g.competition_id == Some(1)));
        assert!(b.groups.iter().all(|g| g.competition_id == Some(2)));
        assert_eq!(all.groups[0].quality, "INSUFFICIENT_SAMPLE");
        let odds =
            prediction_odds_band_performance(&c, &filter(PerformancePeriod::AllTime)).unwrap();
        assert_eq!(odds.predictions_without_historical_odds, 2);
        assert_eq!(odds.bands.iter().map(|b| b.sample_count).sum::<usize>(), 6);
        assert!(odds.bands.iter().any(|b| b.odds_band == "1.30-1.49"));
        assert!(odds.bands.iter().any(|b| b.odds_band == "2.50+"));
        let probabilities_before = all.groups.clone();
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,captured_at) VALUES(1,'iddaa','btts','BTTS','YES',9.99,'BTTS','YES','2025-02-12T00:00:00Z')", []).unwrap();
        let after_odds =
            prediction_model_performance(&c, &filter(PerformancePeriod::AllTime)).unwrap();
        assert_eq!(probabilities_before, after_odds.groups);
        for (p, expected) in [
            (1.29, Some("1.01-1.29")),
            (1.30, Some("1.30-1.49")),
            (1.49, Some("1.30-1.49")),
            (1.50, Some("1.50-1.79")),
            (1.79, Some("1.50-1.79")),
            (1.80, Some("1.80-1.99")),
            (1.99, Some("1.80-1.99")),
            (2.00, Some("2.00-2.49")),
            (2.49, Some("2.00-2.49")),
            (2.50, Some("2.50+")),
        ] {
            assert_eq!(odds_band(p), expected);
        }
        println!("PHASE61_PERFORMANCE all_time={:?} last30={:?} last7={:?} competition_a={:?} competition_b={:?}",all.groups,last30.groups,last7.groups,a.groups,b.groups);
        println!(
            "PHASE61_ODDS bands={:?} predictions_without_historical_odds={}",
            odds.bands, odds.predictions_without_historical_odds
        );
    }

    #[test]
    fn market_selection_line_and_calibration_version_filters_are_applied() {
        let db = fixture();
        let c = db.connection().unwrap();
        let mut f = filter(PerformancePeriod::AllTime);
        f.market = Some("NOT_A_MARKET".into());
        assert!(prediction_model_performance(&c, &f)
            .unwrap()
            .groups
            .is_empty());
        f.market = Some("BTTS".into());
        f.selection = Some("NO".into());
        assert!(prediction_model_performance(&c, &f)
            .unwrap()
            .groups
            .is_empty());
        f.selection = Some("YES".into());
        f.line = Some(2.5);
        assert!(prediction_model_performance(&c, &f)
            .unwrap()
            .groups
            .is_empty());
        f.line = None;
        f.calibration_version = Some("calibration_v2".into());
        assert!(prediction_model_performance(&c, &f)
            .unwrap()
            .groups
            .is_empty());
    }
}
