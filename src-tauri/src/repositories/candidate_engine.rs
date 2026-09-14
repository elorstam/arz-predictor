//! Deterministic Phase 7 candidate pools. Odds are downstream diagnostics only.
use chrono::{DateTime, FixedOffset, Utc};
use chrono_tz::Europe::Istanbul;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const POLICY_VERSION: &str = "candidate_policy_v2_daily_goal_ranking";
pub const BUSINESS_TIMEZONE: &str = "Europe/Istanbul";
pub const MAX_DAILY_ODDS_AGE_HOURS: i64 = 24;
pub const TARGET_DISPLAY_RANGE: &str = "5-10";

#[cfg(test)]
mod daily_goal_ranking_tests {
    use super::*;

    fn input(category: &str) -> (P, O) {
        (
            P {
                id: 1,
                match_id: 1,
                competition_id: 1,
                market: if category == "BTTS_YES" {
                    "BTTS"
                } else {
                    "TOTAL_GOALS"
                }
                .into(),
                selection: if category == "BTTS_YES" {
                    "YES"
                } else {
                    "OVER"
                }
                .into(),
                line: if category == "BTTS_YES" {
                    None
                } else {
                    Some(2.5)
                },
                raw: 0.58,
                public: 0.58,
                status: "UNCALIBRATED_V1".into(),
                cal: None,
                bucket: None,
                observed: None,
                n: None,
                gap: None,
                availability: "AVAILABLE".into(),
                generated: "2026-09-02T12:00:00Z".into(),
                match_status: "scheduled".into(),
                quality_json: r#"{"history_matches_home":10,"history_matches_away":10}"#.into(),
                goal_identity_valid: true,
                kickoff_known: true,
            },
            O {
                id: 1,
                odd: 1.5,
                captured: "2026-09-02T11:00:00Z".into(),
            },
        )
    }

    #[test]
    fn daily_goals_admit_low_probability_negative_ev_without_weakening_high_confidence() {
        for category in ["OVER_25", "BTTS_YES"] {
            let (mut p, o) = input(category);
            assert!(p.public * o.odd < 1.0);
            assert_eq!(reject(&p, category, &o, &policy()), None);
            assert_eq!(
                reject(&p, "HIGH_CONFIDENCE", &o, &policy()).as_deref(),
                Some("LOW_PUBLIC_PROBABILITY")
            );
            p.public = 0.75;
            assert_eq!(
                reject(&p, "HIGH_CONFIDENCE", &o, &policy()).as_deref(),
                Some("CALIBRATION_INSUFFICIENT")
            );
            p.status = "CALIBRATION_NOT_BENEFICIAL".into();
            p.n = Some(20);
            assert_eq!(reject(&p, "HIGH_CONFIDENCE", &o, &policy()), None);
            p.n = Some(19);
            assert!(reject(&p, "HIGH_CONFIDENCE", &o, &policy()).is_some());
        }
    }

    #[test]
    fn daily_goals_reject_invalid_stale_unresolved_and_proven_bad_inputs() {
        for category in ["OVER_25", "BTTS_YES"] {
            let (p, o) = input(category);
            let mut x = p.clone();
            x.goal_identity_valid = false;
            assert_eq!(
                reject(&x, category, &o, &policy()).as_deref(),
                Some("RESOLUTION_FAILED")
            );
            x = p.clone();
            x.kickoff_known = false;
            assert_eq!(
                reject(&x, category, &o, &policy()).as_deref(),
                Some("KICKOFF_UNKNOWN")
            );
            x = p.clone();
            x.quality_json = r#"{"history_matches_home":4,"history_matches_away":10}"#.into();
            assert_eq!(
                reject(&x, category, &o, &policy()).as_deref(),
                Some("INSUFFICIENT_HISTORY")
            );
            x = p.clone();
            x.public = f64::NAN;
            assert_eq!(
                reject(&x, category, &o, &policy()).as_deref(),
                Some("INVALID_PROBABILITY")
            );
            x = p.clone();
            x.public = 0.80;
            x.observed = Some(0.40);
            x.n = Some(100);
            assert_eq!(
                reject(&x, category, &o, &policy()).as_deref(),
                Some("NEGATIVE_MODEL_QUALITY")
            );
            x.n = Some(10);
            assert_eq!(reject(&x, category, &o, &policy()), None);
            let mut quote = o.clone();
            quote.captured = "2026-09-01T11:00:00Z".into();
            assert_eq!(
                reject(&p, category, &quote, &policy()).as_deref(),
                Some("STALE_ODDS")
            );
            quote.captured = "2026-09-02T13:00:00Z".into();
            assert_eq!(
                reject(&p, category, &quote, &policy()).as_deref(),
                Some("INVALID_ODDS")
            );
        }
    }

    #[test]
    fn daily_goal_rank_uses_probability_then_ev_confidence_and_history() {
        for category in ["OVER_25", "BTTS_YES"] {
            let (mut p, o) = input(category);
            let low = make(&p, &o, category);
            p.public = 0.60;
            let mut high = make(&p, &o, category);
            high.expected_value = -0.8;
            assert!(compare_candidates(&high, &low).is_lt());
            let mut better = low.clone();
            better.expected_value += 0.1;
            assert!(compare_candidates(&better, &low).is_lt());
            better = low.clone();
            better.score_components.insert("confidence".into(), 0.9);
            assert!(compare_candidates(&better, &low).is_lt());
            better = low.clone();
            better
                .score_components
                .insert("history_quality".into(), 0.5);
            assert!(compare_candidates(&low, &better).is_lt());
        }
    }
}
pub(crate) const CATEGORIES: [&str; 7] = [
    "CORNERS",
    "OVER_25",
    "OVER_35",
    "BTTS_YES",
    "HIGH_CONFIDENCE",
    "SURPRISE",
    "COMPOUND",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidatePolicy {
    pub version: String,
    pub high_confidence_min_public_probability: f64,
    pub high_confidence_min_bucket_sample: usize,
    pub over_25_min_public_probability: f64,
    pub over_35_min_public_probability: f64,
    pub btts_yes_min_public_probability: f64,
    pub surprise_min_odd: f64,
    pub surprise_min_public_probability: f64,
    pub surprise_min_edge: f64,
    pub compound_min_public_probability: f64,
    pub compound_min_bucket_sample: usize,
    pub compound_odd_range: [f64; 2],
    pub score_weights: [f64; 5],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_newer_base_publication_cannot_hide_populated_base() {
        let db = fixture();
        let c = db.connection().unwrap();
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(1,'iddaa','test','TOTAL_GOALS','OVER',2.0,'TOTAL_GOALS','OVER',2.5,'2026-09-02T11:00:00Z')",[]).unwrap();
        let base = generate(
            &c,
            &GenerateRequest {
                business_date: Some("2026-09-02".into()),
                generation_time: Some("2026-09-02T12:00:00Z".into()),
                category: None,
                dry_run: Some(false),
            },
        )
        .unwrap();
        assert!(!base.candidates.is_empty());
        c.execute("INSERT INTO candidate_engine_runs(business_date,timezone,generated_at,model_version,candidate_policy_version,feature_engine_version,odds_cutoff_at,configuration_hash,input_fingerprint,status,match_count_considered,prediction_context) SELECT business_date,timezone,'2026-09-02T13:00:00Z',model_version,candidate_policy_version,feature_engine_version,odds_cutoff_at,configuration_hash,'empty-base-publication','COMPLETED',0,'BASE' FROM candidate_engine_runs WHERE id=?1",[base.run_id]).unwrap();
        let empty = c.last_insert_rowid();
        publish(&c, "2026-09-02", empty, false).unwrap();
        assert_eq!(
            selected_run_id(&c, "2026-09-02").unwrap(),
            Some(base.run_id)
        );
    }
    use crate::database::Database;

    fn fixture() -> Database {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        c.execute("INSERT INTO model_versions(version_identifier,model_name,registry_status,is_active) VALUES('model_v1','test','ACTIVE',1)", []).unwrap();
        c.execute("INSERT INTO competitions(name,country,current_season) VALUES('A','TR','2026/27'),('B','TR','2026/27')", []).unwrap();
        c.execute(
            "INSERT INTO teams(normalized_name) VALUES('Home'),('Away')",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO matches(competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(1,'2026/27',1,2,'2026-09-02T18:00:00Z','scheduled','2026-09-02',1)", []).unwrap();
        c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,created_at) VALUES(1,'TOTAL_GOALS','OVER',2.5,.80,'MODEL_OUTPUT','2026-09-02T18:00:00Z',1,.80,.80,'UNCALIBRATED_V1','2026-09-02T11:00:00Z')", []).unwrap();
        c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json,calculated_at) VALUES(1,'fe_v1','2026-09-02T09:00:00Z','{}','{\"history_matches_home\":10,\"history_matches_away\":10}','2026-09-02T09:00:00Z')", []).unwrap();
        drop(c);
        db
    }
    #[test]
    fn daily_odds_expire_at_exact_24_hour_boundary() {
        let db = fixture();
        let c = db.connection().unwrap();
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(1,'iddaa','g','TOTAL_GOALS','OVER',2.0,'TOTAL_GOALS','OVER',2.5,'2026-09-01T11:59:59Z')",[]).unwrap();
        let request = GenerateRequest {
            business_date: Some("2026-09-02".into()),
            generation_time: Some("2026-09-02T12:00:00Z".into()),
            category: Some("OVER_25".into()),
            dry_run: Some(false),
        };
        let stale = generate(&c, &request).unwrap();
        assert!(stale.candidates.is_empty());
        assert!(stale.exclusions.iter().any(|e| e.reason == "STALE_ODDS"));
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(1,'iddaa','g','TOTAL_GOALS','OVER',2.0,'TOTAL_GOALS','OVER',2.5,'2026-09-01T12:00:00Z')",[]).unwrap();
        assert_eq!(generate(&c, &request).unwrap().candidates.len(), 1);
    }

    #[test]
    fn provider_corner_btts_mapping_and_cross_season_history_reach_policy() {
        let db = fixture();
        let c = db.connection().unwrap();
        c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json,calculated_at) VALUES(1,'fe_v1','2026-09-02T10:00:00Z',?1,?2,'2026-09-02T10:00:00Z')", params![r#"{"home":{"overall_last10":{"sample_size":10}},"away":{"overall_last10":{"sample_size":10}}}"#,r#"{"history_matches_home":0,"history_matches_away":0,"corners_coverage":1.0}"#]).unwrap();
        for (market, selection, line, code, provider_market, raw_selection) in [
            (
                "FULL_TIME_TOTAL_CORNERS",
                "OVER",
                Some(9.5),
                "48",
                "UNKNOWN",
                "Üst",
            ),
            ("BTTS", "YES", None, "btts", "BOTH_TEAMS_TO_SCORE", "YES"),
        ] {
            c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,created_at) VALUES(1,?1,?2,?3,.8,'test','2026-09-02T18:00:00Z',1,.8,.8,'UNCALIBRATED_V1','2026-09-02T11:00:00Z')", params![market,selection,line]).unwrap();
            c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(1,'iddaa',?1,?2,?3,2.0,?2,?4,?5,'2026-09-02T10:00:00Z')", params![code,provider_market,raw_selection,selection,line]).unwrap();
        }
        let request = GenerateRequest {
            business_date: Some("2026-09-02".into()),
            generation_time: Some("2026-09-02T12:00:00Z".into()),
            category: None,
            dry_run: Some(false),
        };
        let run = generate(&c, &request).unwrap();
        for category in ["CORNERS", "BTTS_YES"] {
            assert_eq!(
                run.candidates
                    .iter()
                    .filter(|x| x.category == category)
                    .count(),
                1
            );
        }
        // A provider's non-bettable 1.00 price must never enter any pool.
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(1,'iddaa','48','UNKNOWN','Üst',1.0,'UNKNOWN','OVER',9.5,'2026-09-02T11:30:00Z')",[]).unwrap();
        let rerun = generate(&c, &request).unwrap();
        assert!(!rerun.candidates.iter().any(|x| x.category == "CORNERS"));
        assert!(c.query_row::<i64,_,_>("SELECT count(*) FROM candidate_engine_exclusions WHERE run_id=?1 AND category='CORNERS' AND reason='INVALID_ODDS'",[rerun.run_id],|r|r.get(0)).unwrap()>0);
    }

    #[test]
    fn policy_timezone_and_value_contract_are_deterministic() {
        assert_eq!(policy().version, POLICY_VERSION);
        assert_eq!(day("2026-09-01T21:30:00Z").unwrap(), "2026-09-02");
        let db = fixture();
        let c = db.connection().unwrap();
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(1,'iddaa','g','TOTAL_GOALS','OVER',2.00,'TOTAL_GOALS','OVER',2.5,'2026-09-02T10:00:00Z'),(1,'iddaa','g','TOTAL_GOALS','OVER',9.99,'TOTAL_GOALS','OVER',2.5,'2026-09-02T20:00:00Z')",[]).unwrap();
        let r = GenerateRequest {
            business_date: Some("2026-09-02".into()),
            generation_time: Some("2026-09-02T12:00:00Z".into()),
            category: Some("OVER_25".into()),
            dry_run: Some(false),
        };
        let a = generate(&c, &r).unwrap();
        assert_eq!(a.candidates.len(), 1);
        assert_eq!(a.prediction_context.as_deref(), Some("BASE"));
        let x = &a.candidates[0];
        assert_eq!(x.prediction_source.as_deref(), Some("BASE"));
        assert_eq!(x.base_public_probability, Some(x.public_probability));
        assert_eq!(x.final_public_probability, Some(x.public_probability));
        assert_eq!(x.delta_percentage_points, Some(0.0));
        assert_eq!(x.iddaa_odd, 2.0);
        assert!((x.implied_probability - 0.5).abs() < 1e-9);
        assert!((x.probability_edge - 0.3).abs() < 1e-9);
        assert!((x.expected_value - 0.6).abs() < 1e-9);
        let b = generate(&c, &r).unwrap();
        assert_eq!(a.run_id, b.run_id);
        assert_eq!(
            c.query_row::<i64, _, _>("SELECT COUNT(*) FROM candidate_engine_runs", [], |x| x
                .get(0))
                .unwrap(),
            1
        );
        let run_lineage: (Option<String>, Option<i64>, Option<String>) = c
            .query_row(
                "SELECT prediction_context,parent_candidate_run_id,model_version FROM candidate_engine_runs WHERE id=?1",
                [a.run_id],
                |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            run_lineage,
            (Some("BASE".into()), None, Some("model_v1".into()))
        );
        let invalid_context = c.execute("INSERT INTO candidate_engine_runs(business_date,timezone,generated_at,model_version,candidate_policy_version,feature_engine_version,odds_cutoff_at,configuration_hash,input_fingerprint,status,match_count_considered,prediction_context) VALUES('2026-09-02','Europe/Istanbul','2026-09-02T12:00:00Z','model_v1','candidate_policy_v1','fe_v1','2026-09-02T12:00:00Z','invalid','invalid','COMPLETED',1,'INVALID')", []);
        assert!(invalid_context.is_err());
    }
}
impl Default for CandidatePolicy {
    fn default() -> Self {
        Self {
            version: POLICY_VERSION.into(),
            high_confidence_min_public_probability: 0.75,
            high_confidence_min_bucket_sample: 20,
            // Daily goal pools are ranked, not threshold-selected.
            over_25_min_public_probability: 0.0,
            over_35_min_public_probability: 0.52,
            btts_yes_min_public_probability: 0.0,
            surprise_min_odd: 2.0,
            surprise_min_public_probability: 0.42,
            surprise_min_edge: 0.05,
            compound_min_public_probability: 0.72,
            compound_min_bucket_sample: 20,
            compound_odd_range: [1.10, 1.50],
            score_weights: [0.55, 0.25, 0.15, 0.04, 0.01],
        }
    }
}
pub fn policy() -> CandidatePolicy {
    CandidatePolicy::default()
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Candidate {
    pub id: i64,
    pub prediction_id: i64,
    pub match_id: i64,
    pub competition_id: i64,
    pub category: String,
    pub market: String,
    pub selection: String,
    pub line: Option<f64>,
    pub raw_probability: f64,
    pub public_probability: f64,
    pub calibration_status: String,
    pub calibration_version: Option<String>,
    pub calibration_bucket: Option<String>,
    pub bucket_observed_rate: Option<f64>,
    pub bucket_sample_size: Option<usize>,
    pub bucket_calibration_gap: Option<f64>,
    pub iddaa_odd: f64,
    pub odds_snapshot_id: i64,
    pub odds_captured_at: String,
    pub implied_probability: f64,
    pub probability_edge: f64,
    pub expected_value: f64,
    pub data_quality: String,
    pub score: f64,
    pub score_components: BTreeMap<String, f64>,
    pub rank: i64,
    pub same_match_group: String,
    pub correlation_type: String,
    pub compound_eligible: bool,
    pub qualification_state: String,
    #[serde(default)]
    pub prediction_source: Option<String>,
    #[serde(default)]
    pub lineup_revision_id: Option<i64>,
    #[serde(default)]
    pub lineup_snapshot_id: Option<i64>,
    #[serde(default)]
    pub base_public_probability: Option<f64>,
    #[serde(default)]
    pub final_public_probability: Option<f64>,
    #[serde(default)]
    pub delta_percentage_points: Option<f64>,
    #[serde(default)]
    pub lineup_family_status: Option<String>,
    #[serde(default)]
    pub lineup_fallback_reason: Option<String>,
    #[serde(default)]
    pub lineup_model_version: Option<String>,
    #[serde(default)]
    pub lineup_model_hash: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Exclusion {
    pub prediction_id: Option<i64>,
    pub match_id: Option<i64>,
    pub business_date: String,
    pub competition_id: Option<i64>,
    pub category: String,
    pub market: Option<String>,
    pub selection: Option<String>,
    pub line: Option<f64>,
    pub reason: String,
    pub generated_at: String,
    pub details: Option<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CategorySummary {
    pub category: String,
    pub qualified_count: usize,
    pub exclusion_count: usize,
    pub target_display_range: String,
    pub shortfall_reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DailyRun {
    pub run_id: i64,
    pub business_date: String,
    pub timezone: String,
    pub generated_at: String,
    pub model_version: String,
    pub calibration_version: Option<String>,
    pub policy_version: String,
    pub match_count_considered: usize,
    pub category_summaries: Vec<CategorySummary>,
    pub candidates: Vec<Candidate>,
    pub exclusions: Vec<Exclusion>,
    #[serde(default)]
    pub prediction_context: Option<String>,
    #[serde(default)]
    pub parent_candidate_run_id: Option<i64>,
    #[serde(default)]
    pub lineup_model_version: Option<String>,
    #[serde(default)]
    pub lineup_model_hash: Option<String>,
    #[serde(default)]
    pub revision_context_hash: Option<String>,
    #[serde(default)]
    pub base_model_hash: Option<String>,
    #[serde(default)]
    pub base_calibration_hash: Option<String>,
    #[serde(default)]
    pub lineup_aware_match_count: usize,
    #[serde(default)]
    pub base_fallback_match_count: usize,
    #[serde(default)]
    pub newly_qualified_count: usize,
    #[serde(default)]
    pub no_longer_qualified_count: usize,
    #[serde(default)]
    pub still_qualified_count: usize,
    #[serde(default)]
    pub unchanged_count: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenerateRequest {
    pub business_date: Option<String>,
    pub generation_time: Option<String>,
    pub category: Option<String>,
    pub dry_run: Option<bool>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetRequest {
    pub business_date: String,
    pub run_id: Option<i64>,
    pub category: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Status {
    pub active_model_version: Option<String>,
    pub calibration_version: Option<String>,
    pub calibration_status: Option<String>,
    pub candidate_policy_version: String,
    pub business_date: Option<String>,
    pub timezone: String,
    pub latest_run_id: Option<i64>,
    pub latest_generation_time: Option<String>,
    pub matches_eligible_today: usize,
    pub matches_considered_in_latest_run: usize,
    pub category_counts: Vec<StatusCategory>,
    pub total_rejected: usize,
    pub active_model: Option<String>,
    pub active_calibration: Option<String>,
    pub policy_version: String,
    pub latest_run: Option<i64>,
    pub eligible_match_count: usize,
    pub qualified_counts: BTreeMap<String, usize>,
    pub missing_odds_count: usize,
    pub stale_odds_count: usize,
    pub model_unavailable_count: usize,
    pub low_data_quality_count: usize,
    pub calibration_insufficient_count: usize,
    pub latest_business_date: Option<String>,
    pub latest_generated_at: Option<String>,
    pub matches_considered: usize,
    pub rejected_counts: BTreeMap<String, usize>,
    pub rejected_counts_by_category: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusCategory {
    pub category: String,
    pub qualified_count: usize,
    pub rejected_count: usize,
}

#[derive(Clone)]
struct P {
    id: i64,
    match_id: i64,
    competition_id: i64,
    market: String,
    selection: String,
    line: Option<f64>,
    raw: f64,
    public: f64,
    status: String,
    cal: Option<String>,
    bucket: Option<String>,
    observed: Option<f64>,
    n: Option<usize>,
    gap: Option<f64>,
    availability: String,
    generated: String,
    match_status: String,
    quality_json: String,
    goal_identity_valid: bool,
    kickoff_known: bool,
}
#[derive(Clone)]
struct O {
    id: i64,
    odd: f64,
    captured: String,
}
fn ts(s: &str) -> Result<DateTime<FixedOffset>, String> {
    DateTime::parse_from_rfc3339(s).map_err(|e| e.to_string())
}
fn now() -> String {
    Utc::now().to_rfc3339()
}
fn day(s: &str) -> Result<String, String> {
    Ok(ts(s)?.with_timezone(&Istanbul).date_naive().to_string())
}
fn sha(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("{:x}", h.finalize())
}
fn load(c: &Connection, date: &str, cutoff: &str) -> Result<Vec<P>, String> {
    // prediction_runs.generated_at is the immutable feature-information cutoff
    // (normally kickoff), while predictions.created_at is the actual inference
    // time. Candidate as-of filtering and odds freshness must use the latter.
    let mut q=c.prepare("SELECT p.id,p.match_id,m.competition_id,p.market,p.selection,p.line_value,COALESCE(p.raw_probability,p.model_probability),COALESCE(p.public_probability,p.model_probability),COALESCE(p.calibration_status,'UNCALIBRATED_V1'),p.calibration_version,p.calibration_bucket,p.bucket_observed_rate,p.bucket_sample_size,p.bucket_calibration_gap,COALESCE(p.availability,'AVAILABLE'),p.created_at,CASE WHEN m.status IN ('scheduled','finished','live') THEN CASE WHEN julianday(m.kickoff_at)<=julianday(?2) THEN 'started' ELSE 'scheduled' END ELSE m.status END,CASE WHEN json_extract(fs.feature_json,'$.home.overall_last10.sample_size') IS NOT NULL THEN json_set(COALESCE(fs.data_quality_json,'{}'),'$.history_matches_home',json_extract(fs.feature_json,'$.home.overall_last10.sample_size'),'$.history_matches_away',json_extract(fs.feature_json,'$.away.overall_last10.sample_size')) ELSE COALESCE(fs.data_quality_json,'{}') END FROM predictions p JOIN matches m ON m.id=p.match_id LEFT JOIN feature_sets fs ON fs.id=(SELECT f.id FROM feature_sets f WHERE f.match_id=p.match_id AND julianday(f.calculated_at)<=julianday(?2) ORDER BY julianday(f.calculated_at) DESC,f.id DESC LIMIT 1) WHERE m.scheduled_local_date=?1 AND julianday(p.created_at)<=julianday(?2) AND p.model_version_id=(SELECT id FROM model_versions WHERE is_active=1) AND NOT EXISTS(SELECT 1 FROM predictions newer WHERE newer.match_id=p.match_id AND newer.market=p.market AND newer.selection=p.selection AND newer.line_value IS p.line_value AND newer.model_version_id=p.model_version_id AND julianday(newer.created_at)<=julianday(?2) AND (julianday(newer.created_at)>julianday(p.created_at) OR (julianday(newer.created_at)=julianday(p.created_at) AND newer.id>p.id))) ORDER BY m.kickoff_at,m.id,p.market,p.selection,p.line_value,p.id").map_err(|e|e.to_string())?;
    let mut v = q
        .query_map(params![date, cutoff], |r| {
            Ok(P {
                id: r.get(0)?,
                match_id: r.get(1)?,
                competition_id: r.get(2)?,
                market: r.get(3)?,
                selection: r.get(4)?,
                line: r.get(5)?,
                raw: r.get(6)?,
                public: r.get(7)?,
                status: r.get(8)?,
                cal: r.get(9)?,
                bucket: r.get(10)?,
                observed: r.get(11)?,
                n: r.get::<_, Option<i64>>(12)?.map(|x| x as usize),
                gap: r.get(13)?,
                availability: r.get(14)?,
                generated: cutoff.to_string(),
                match_status: r.get(16)?,
                quality_json: r.get(17)?,
                goal_identity_valid: true,
                kickoff_known: true,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut identity = BTreeMap::new();
    for p in &mut v {
        let (resolved, known) = if let Some(value) = identity.get(&p.match_id) {
            *value
        } else {
            // Existing canonical predictions are sufficient for non-bulletin
            // fixtures. Iddaa fixtures must also retain their model mappings.
            let value: (bool, bool) = c.query_row("SELECT NOT EXISTS(SELECT 1 FROM provider_match_mappings pm WHERE pm.match_id=m.id AND pm.provider='iddaa') OR (EXISTS(SELECT 1 FROM provider_competition_mappings pc WHERE pc.competition_id=m.competition_id AND pc.provider='football-data.co.uk') AND EXISTS(SELECT 1 FROM provider_team_mappings pt WHERE pt.team_id=m.home_team_id AND pt.provider='football-data.co.uk') AND EXISTS(SELECT 1 FROM provider_team_mappings pt WHERE pt.team_id=m.away_team_id AND pt.provider='football-data.co.uk')), m.kickoff_time_known FROM matches m WHERE m.id=?1", [p.match_id], |r| Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?;
            identity.insert(p.match_id, value);
            value
        };
        p.goal_identity_valid = resolved;
        p.kickoff_known = known;
    }
    Ok(v)
}
fn odd(c: &Connection, p: &P, cutoff: &str) -> Result<Option<O>, String> {
    c.query_row("SELECT id,odd,captured_at FROM iddaa_model_odds WHERE provider='iddaa' AND match_id=?1 AND julianday(captured_at)<=julianday(?2) AND UPPER(model_market_type) IN (UPPER(?3),CASE ?3 WHEN 'BTTS' THEN 'BOTH_TEAMS_TO_SCORE' WHEN 'FULL_TIME_TOTAL_CORNERS' THEN 'CORNERS_TOTAL' ELSE UPPER(?3) END) AND UPPER(model_selection)=UPPER(?4) AND (line_value IS ?5 OR line_value=?5) ORDER BY julianday(captured_at) DESC,id DESC LIMIT 1",params![p.match_id,cutoff,p.market,p.selection,p.line],|r|Ok(O{id:r.get(0)?,odd:r.get(1)?,captured:r.get(2)?})).optional().map_err(|e|e.to_string())
}
fn good(p: &P) -> bool {
    let v: serde_json::Value = serde_json::from_str(&p.quality_json).unwrap_or_default();
    let h = v
        .get("history_matches_home")
        .and_then(|x| x.as_i64())
        .unwrap_or(0)
        .min(
            v.get("history_matches_away")
                .and_then(|x| x.as_i64())
                .unwrap_or(0),
        );
    let c = if p.market == "FULL_TIME_TOTAL_CORNERS" {
        v.get("corners_coverage")
            .and_then(|x| x.as_f64())
            .unwrap_or(0.0)
    } else {
        1.0
    };
    h >= 5 && c >= 0.5
}
fn corr(p: &P) -> &'static str {
    if p.market == "FULL_TIME_TOTAL_CORNERS" {
        "CORNERS_SAME_MATCH"
    } else if p.market == "TOTAL_GOALS" {
        "NESTED_TOTAL_LINE"
    } else if p.market == "BTTS" {
        "BTTS_GOALS_CORRELATION"
    } else {
        "NONE"
    }
}
fn reject(p: &P, cat: &str, o: &O, pol: &CandidatePolicy) -> Option<String> {
    if !o.odd.is_finite() || o.odd <= 1.0 {
        return Some("INVALID_ODDS".into());
    }
    if p.availability != "AVAILABLE" {
        return Some("MODEL_UNAVAILABLE".into());
    }
    if daily_goal_category(cat) {
        if !p.goal_identity_valid {
            return Some("RESOLUTION_FAILED".into());
        }
        if !p.kickoff_known {
            return Some("KICKOFF_UNKNOWN".into());
        }
        if !p.public.is_finite() || !(0.0..=1.0).contains(&p.public) {
            return Some("INVALID_PROBABILITY".into());
        }
        if !good(p) {
            return Some("INSUFFICIENT_HISTORY".into());
        }
        // Uncalibrated / small-sample models are uncertain, not proven bad.
        // Reject explicit failures or severe, well-supported overconfidence.
        if matches!(p.status.as_str(), "INVALID" | "MODEL_INVALID" | "REJECTED")
            || p.observed.is_some_and(|observed| {
                observed.is_finite()
                    && (0.0..=1.0).contains(&observed)
                    && p.n.unwrap_or(0) >= 50
                    && p.public - observed > 0.20
                    && p.public - observed
                        > 3.0 * (p.public * (1.0 - p.public) / p.n.unwrap() as f64).sqrt()
            })
        {
            return Some("NEGATIVE_MODEL_QUALITY".into());
        }
    }
    if p.match_status != "scheduled" {
        return Some(if p.match_status == "finished" {
            "MATCH_FINISHED".into()
        } else {
            "MATCH_STARTED".into()
        });
    }
    if !good(p) && (cat == "CORNERS" || cat == "HIGH_CONFIDENCE" || cat == "COMPOUND") {
        return Some("LOW_DATA_QUALITY".into());
    }
    let age = ts(&p.generated)
        .ok()
        .and_then(|a| ts(&o.captured).ok().map(|b| (a - b).num_seconds()))
        .unwrap_or(i64::MAX);
    if age > MAX_DAILY_ODDS_AGE_HOURS * 3600 {
        return Some("STALE_ODDS".into());
    }
    if daily_goal_category(cat) && age < 0 {
        return Some("INVALID_ODDS".into());
    }
    let edge = p.public - 1.0 / o.odd;
    match cat {
        "CORNERS" => {
            if p.market != "FULL_TIME_TOTAL_CORNERS"
                || p.selection != "OVER"
                || !matches!(p.line, Some(7.5 | 8.5 | 9.5 | 10.5 | 11.5))
            {
                Some("ODDS_MARKET_UNAVAILABLE".into())
            } else if p.public < 0.50 {
                Some("LOW_PUBLIC_PROBABILITY".into())
            } else {
                None
            }
        }
        "OVER_25" => {
            if p.market != "TOTAL_GOALS" || p.selection != "OVER" || p.line != Some(2.5) {
                Some("UNSUPPORTED_MARKET".into())
            } else {
                None
            }
        }
        "OVER_35" => {
            if p.market != "TOTAL_GOALS" || p.selection != "OVER" || p.line != Some(3.5) {
                Some("UNSUPPORTED_MARKET".into())
            } else if p.public < pol.over_35_min_public_probability {
                Some("LOW_PUBLIC_PROBABILITY".into())
            } else if edge <= 0.0 {
                Some("LOW_EDGE".into())
            } else {
                None
            }
        }
        "BTTS_YES" => {
            if p.market != "BTTS" || p.selection != "YES" {
                Some("UNSUPPORTED_MARKET".into())
            } else {
                None
            }
        }
        "HIGH_CONFIDENCE" => {
            if p.public < pol.high_confidence_min_public_probability {
                Some("LOW_PUBLIC_PROBABILITY".into())
            } else if p.status != "CALIBRATED_V1"
                && !(p.status == "CALIBRATION_NOT_BENEFICIAL"
                    && p.n.unwrap_or(0) >= pol.high_confidence_min_bucket_sample)
            {
                Some("CALIBRATION_INSUFFICIENT".into())
            } else if p.n.unwrap_or(0) < pol.high_confidence_min_bucket_sample {
                Some("INSUFFICIENT_BUCKET_SAMPLE".into())
            } else {
                None
            }
        }
        "SURPRISE" => {
            if o.odd < pol.surprise_min_odd || p.public < pol.surprise_min_public_probability {
                Some("LOW_PUBLIC_PROBABILITY".into())
            } else if edge < pol.surprise_min_edge {
                Some("LOW_EDGE".into())
            } else {
                None
            }
        }
        "COMPOUND" => {
            if p.public < pol.compound_min_public_probability {
                Some("LOW_PUBLIC_PROBABILITY".into())
            } else if o.odd < pol.compound_odd_range[0] || o.odd > pol.compound_odd_range[1] {
                Some("ODDS_OUTSIDE_COMPOUND_RANGE".into())
            } else if !matches!(
                p.status.as_str(),
                "CALIBRATED_V1" | "CALIBRATION_NOT_BENEFICIAL"
            ) || p.n.unwrap_or(0) < pol.compound_min_bucket_sample
            {
                Some("CALIBRATION_INSUFFICIENT".into())
            } else {
                None
            }
        }
        _ => Some("UNSUPPORTED_MARKET".into()),
    }
}
fn daily_goal_category(category: &str) -> bool {
    matches!(category, "OVER_25" | "BTTS_YES")
}

/// Keep daily probability primary; EV, confidence and history break ties.
/// Other pools retain their existing weighted-score order.
pub(crate) fn compare_candidates(a: &Candidate, b: &Candidate) -> std::cmp::Ordering {
    a.category
        .cmp(&b.category)
        .then_with(|| {
            if daily_goal_category(&a.category) {
                let component =
                    |x: &Candidate, key: &str| x.score_components.get(key).copied().unwrap_or(0.0);
                b.public_probability
                    .total_cmp(&a.public_probability)
                    .then_with(|| b.expected_value.total_cmp(&a.expected_value))
                    .then_with(|| component(b, "confidence").total_cmp(&component(a, "confidence")))
                    .then_with(|| {
                        component(b, "history_quality").total_cmp(&component(a, "history_quality"))
                    })
            } else {
                b.score.total_cmp(&a.score)
            }
        })
        .then_with(|| a.match_id.cmp(&b.match_id))
        .then_with(|| a.prediction_id.cmp(&b.prediction_id))
}

fn make(p: &P, o: &O, cat: &str) -> Candidate {
    let implied = 1.0 / o.odd;
    let edge = p.public - implied;
    let ev = p.public * o.odd - 1.0;
    let dq = if good(p) { 1.0 } else { 0.0 };
    let cs = if p.status == "CALIBRATED_V1" {
        1.0
    } else {
        0.0
    };
    let score = if daily_goal_category(cat) {
        p.public
    } else {
        0.55 * p.public
            + 0.25 * edge.clamp(0.0, 1.0)
            + 0.15 * ev.clamp(0.0, 1.0)
            + 0.04 * cs
            + 0.01 * dq
    };
    let mut comp = BTreeMap::new();
    comp.insert("public_probability".into(), p.public);
    comp.insert("edge".into(), edge);
    comp.insert("expected_value".into(), ev);
    comp.insert("calibration_support".into(), p.n.unwrap_or(0) as f64);
    comp.insert("data_quality".into(), dq);
    if daily_goal_category(cat) {
        let n = p.n.unwrap_or(0) as f64;
        let gap = p
            .gap
            .filter(|v| v.is_finite())
            .map(f64::abs)
            .or_else(|| {
                p.observed
                    .filter(|v| v.is_finite())
                    .map(|v| (p.public - v).abs())
            })
            .unwrap_or(0.0);
        let quality: serde_json::Value = serde_json::from_str(&p.quality_json).unwrap_or_default();
        let history = ["history_matches_home", "history_matches_away"]
            .iter()
            .map(|key| quality.get(*key).and_then(|v| v.as_u64()).unwrap_or(0))
            .min()
            .unwrap_or(0);
        comp.insert(
            "confidence".into(),
            n / (n + 20.0) * (1.0 - gap).clamp(0.0, 1.0),
        );
        comp.insert(
            "history_quality".into(),
            (history as f64 / 10.0).clamp(0.0, 1.0),
        );
    }
    Candidate {
        id: 0,
        prediction_id: p.id,
        match_id: p.match_id,
        competition_id: p.competition_id,
        category: cat.into(),
        market: p.market.clone(),
        selection: p.selection.clone(),
        line: p.line,
        raw_probability: p.raw,
        public_probability: p.public,
        calibration_status: p.status.clone(),
        calibration_version: p.cal.clone(),
        calibration_bucket: p.bucket.clone(),
        bucket_observed_rate: p.observed,
        bucket_sample_size: p.n,
        bucket_calibration_gap: p.gap,
        iddaa_odd: o.odd,
        odds_snapshot_id: o.id,
        odds_captured_at: o.captured.clone(),
        implied_probability: implied,
        probability_edge: edge,
        expected_value: ev,
        data_quality: if dq > 0.0 {
            "GOOD".into()
        } else {
            "LOW_DATA_QUALITY".into()
        },
        score,
        score_components: comp,
        rank: 0,
        same_match_group: p.match_id.to_string(),
        correlation_type: corr(p).into(),
        compound_eligible: cat == "COMPOUND",
        qualification_state: "QUALIFIED".into(),
        prediction_source: Some("BASE".into()),
        lineup_revision_id: None,
        lineup_snapshot_id: None,
        base_public_probability: Some(p.public),
        final_public_probability: Some(p.public),
        delta_percentage_points: Some(0.0),
        lineup_family_status: None,
        lineup_fallback_reason: None,
        lineup_model_version: None,
        lineup_model_hash: None,
    }
}

fn exclusion(p: &P, date: &str, cat: &str, reason: &str, generated: &str) -> Exclusion {
    Exclusion {
        prediction_id: Some(p.id),
        match_id: Some(p.match_id),
        business_date: date.into(),
        competition_id: Some(p.competition_id),
        category: cat.into(),
        market: Some(p.market.clone()),
        selection: Some(p.selection.clone()),
        line: p.line,
        reason: reason.into(),
        generated_at: generated.into(),
        details: None,
    }
}

fn state_fingerprint(c: &Connection, ps: &[P], cutoff: &str) -> Result<String, String> {
    let mut state = String::new();
    for p in ps {
        state.push_str(&format!(
            "{}|{}|{}|{}|{:?}|{}|{}|{};",
            p.id,
            p.match_id,
            p.market,
            p.selection,
            p.line.map(f64::to_bits),
            p.raw.to_bits(),
            p.public.to_bits(),
            p.status
        ));
        state.push_str(&format!("{}|{};", p.match_status, p.quality_json));
        state.push_str(&format!(
            "{}|{}|{:?}|{:?}|{:?};",
            p.goal_identity_valid, p.kickoff_known, p.observed, p.n, p.gap
        ));
        if let Some(o) = odd(c, p, cutoff)? {
            state.push_str(&format!("o:{}|{}|{};", o.id, o.odd.to_bits(), o.captured));
        }
    }
    Ok(sha(&state))
}

pub(crate) fn insert_candidate(c: &Connection, run_id: i64, x: &Candidate) -> Result<(), String> {
    c.execute("INSERT INTO candidate_engine_candidates(run_id,prediction_id,match_id,competition_id,category,market,selection,line_value,raw_probability,public_probability,calibration_status,calibration_version,calibration_bucket,bucket_observed_rate,bucket_sample_size,bucket_calibration_gap,iddaa_odd,odds_snapshot_id,odds_captured_at,implied_probability,probability_edge,expected_value,data_quality,score,score_components_json,rank,same_match_group,correlation_type,compound_eligible,qualification_state,prediction_source,lineup_revision_id,lineup_snapshot_id,base_public_probability,final_public_probability,delta_percentage_points,lineup_family_status,lineup_fallback_reason,lineup_model_version,lineup_model_hash)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?35,?36,?37,?38,?39,?40)", params![run_id,x.prediction_id,x.match_id,x.competition_id,x.category,x.market,x.selection,x.line,x.raw_probability,x.public_probability,x.calibration_status,x.calibration_version,x.calibration_bucket,x.bucket_observed_rate,x.bucket_sample_size.map(|n| n as i64),x.bucket_calibration_gap,x.iddaa_odd,x.odds_snapshot_id,x.odds_captured_at,x.implied_probability,x.probability_edge,x.expected_value,x.data_quality,x.score,serde_json::to_string(&x.score_components).map_err(|e| e.to_string())?,x.rank,x.same_match_group,x.correlation_type,x.compound_eligible as i64,x.qualification_state,x.prediction_source,x.lineup_revision_id,x.lineup_snapshot_id,x.base_public_probability,x.final_public_probability,x.delta_percentage_points,x.lineup_family_status,x.lineup_fallback_reason,x.lineup_model_version,x.lineup_model_hash]).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineupRevisionGenerateRequest {
    pub base_run_id: i64,
    pub business_date: String,
}

#[derive(Debug, Clone)]
struct EffectiveRevision {
    id: i64,
    snapshot_id: Option<i64>,
    lineup_model_version: Option<String>,
    lineup_model_hash: Option<String>,
    payload_hash: Option<String>,
    payload: crate::repositories::lineup_model::RevisionMarketPayloadDocument,
}

fn latest_revision(c: &Connection, match_id: i64) -> Result<Option<EffectiveRevision>, String> {
    let row: Option<(i64, Option<i64>, Option<String>, Option<String>, Option<String>, Option<String>, String)> = c.query_row("SELECT id,lineup_snapshot_id,lineup_model_version,lineup_model_hash,payload_hash,payload_schema,market_payload_json FROM prediction_revisions WHERE match_id=?1 AND revision_type='LINEUP_AWARE_PREMATCH' AND payload_schema=?2 ORDER BY id DESC LIMIT 1", params![match_id, crate::repositories::lineup_model::REVISION_PAYLOAD_SCHEMA], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(|e| e.to_string())?;
    let Some((
        id,
        snapshot_id,
        lineup_model_version,
        lineup_model_hash,
        payload_hash,
        schema,
        json,
    )) = row
    else {
        return Ok(None);
    };
    if schema.as_deref() != Some(crate::repositories::lineup_model::REVISION_PAYLOAD_SCHEMA) {
        return Ok(None);
    }
    let payload: crate::repositories::lineup_model::RevisionMarketPayloadDocument =
        serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(Some(EffectiveRevision {
        id,
        snapshot_id,
        lineup_model_version,
        lineup_model_hash,
        payload_hash,
        payload,
    }))
}

fn lineup_family(category: &str) -> &'static str {
    match category {
        "BTTS_YES" => "BTTS",
        "OVER_25" | "OVER_35" => "TOTAL_GOALS",
        "CORNERS" => "CORNERS",
        "HIGH_CONFIDENCE" => "MATCH_RESULT",
        "COMPOUND" => "TOTAL_GOALS",
        _ => "MATCH_RESULT",
    }
}

fn revision_market<'a>(
    r: &'a EffectiveRevision,
    p: &P,
) -> Option<&'a crate::repositories::lineup_model::RevisionMarketPayload> {
    r.payload.markets.iter().find(|x| {
        x.market.eq_ignore_ascii_case(&p.market)
            && x.selection.eq_ignore_ascii_case(&p.selection)
            && x.line.map(f64::to_bits) == p.line.map(f64::to_bits)
    })
}

fn lineup_candidate(
    c: &Connection,
    p: &P,
    o: &O,
    category: &str,
) -> Result<(Candidate, Option<EffectiveRevision>), String> {
    let revision = latest_revision(c, p.match_id)?;
    let base_probability = p.public;
    let mut selected_probability = base_probability;
    let mut selected_raw = p.raw;
    let mut source = "BASE";
    let mut family_status = None;
    let mut fallback_reason = None;
    let mut snapshot_id = None;
    let mut revision_id = None;
    let mut lineup_model_version = None;
    let mut lineup_model_hash = None;
    if let Some(r) = revision.as_ref() {
        if let Some(m) = revision_market(r, p) {
            family_status = Some(m.family_status.clone());
            fallback_reason = m.fallback_reason.clone();
            snapshot_id = r.snapshot_id;
            revision_id = Some(r.id);
            lineup_model_version = r.lineup_model_version.clone();
            lineup_model_hash = r.lineup_model_hash.clone();
            if m.family_status == "ACTIVE" {
                selected_probability = m.final_public_probability.unwrap_or(base_probability);
                selected_raw = m.revised_raw_probability.unwrap_or(selected_probability);
                source = "LINEUP_AWARE";
            } else {
                source = "BASE_RETAINED";
            }
        }
    }
    let mut adjusted = p.clone();
    adjusted.raw = selected_raw;
    adjusted.public = selected_probability;
    let mut out = make(&adjusted, o, category);
    out.prediction_source = Some(source.into());
    out.lineup_revision_id = revision_id;
    out.lineup_snapshot_id = snapshot_id;
    out.base_public_probability = Some(base_probability);
    out.final_public_probability = Some(selected_probability);
    out.delta_percentage_points = Some((selected_probability - base_probability) * 100.0);
    out.lineup_family_status = family_status.or_else(|| Some(lineup_family(category).into()));
    out.lineup_fallback_reason = fallback_reason.or_else(|| {
        (source == "BASE_RETAINED").then(|| format!("BASE_RETAINED:{}", lineup_family(category)))
    });
    out.lineup_model_version = lineup_model_version;
    out.lineup_model_hash = lineup_model_hash;
    Ok((out, revision))
}

pub fn generate_lineup_revision(
    c: &Connection,
    r: &LineupRevisionGenerateRequest,
) -> Result<DailyRun, String> {
    let base: (String,String,String,Option<String>,String) = c.query_row("SELECT business_date,generated_at,model_version,calibration_version,COALESCE(prediction_context,'BASE') FROM candidate_engine_runs WHERE id=?1", [r.base_run_id], |x| Ok((x.get(0)?,x.get(1)?,x.get(2)?,x.get(3)?,x.get(4)?))).map_err(|e| e.to_string())?;
    if base.0 != r.business_date || base.4 != "BASE" {
        return Err("BASE_CANDIDATE_RUN_REQUIRED".into());
    }
    let ps = load(c, &base.0, &base.1)?;
    let (base_model_hash, base_calibration_hash): (Option<String>, Option<String>) = ps
        .first()
        .and_then(|p| {
            c.query_row("SELECT mv.artifact_sha256,cm.artifact_sha256 FROM predictions p JOIN model_versions mv ON mv.id=p.model_version_id LEFT JOIN calibration_models cm ON cm.calibration_version=p.calibration_version WHERE p.id=?1", [p.id], |x| Ok((x.get(0)?, x.get(1)?))).optional().ok().flatten()
        })
        .unwrap_or((None, None));
    let pol = policy();
    let mut cs = Vec::new();
    let mut ex = Vec::new();
    let mut context = String::new();
    let mut lineup_matches = BTreeMap::new();
    for p in &ps {
        if let Some(rev) = latest_revision(c, p.match_id)? {
            lineup_matches.insert(p.match_id, rev);
        }
        for cat in CATEGORIES {
            let Some(o) = odd(c, p, &base.1)? else {
                continue;
            };
            let (candidate, rev) = lineup_candidate(c, p, &o, cat)?;
            if let Some(rev) = rev {
                context.push_str(&format!(
                    "{}:{}:{};",
                    p.match_id,
                    rev.id,
                    rev.payload_hash.unwrap_or_default()
                ));
            }
            let mut adjusted = p.clone();
            adjusted.public = candidate.public_probability;
            adjusted.raw = candidate.raw_probability;
            if let Some(reason) = reject(&adjusted, cat, &o, &pol) {
                ex.push(exclusion(&adjusted, &base.0, cat, &reason, &base.1));
            } else {
                cs.push(candidate);
            }
        }
    }
    // Keep the strongest qualified selection per match/category, not whichever
    // market happened to be loaded first from SQLite.
    cs.sort_by(compare_candidates);
    for cat in CATEGORIES {
        let mut seen = BTreeMap::new();
        cs.retain(|x| x.category != cat || seen.insert(x.match_id, true).is_none());
    }
    cs.sort_by(compare_candidates);
    let mut ranks = BTreeMap::new();
    for x in &mut cs {
        *ranks.entry(x.category.clone()).or_insert(0) += 1;
        x.rank = ranks[&x.category];
    }
    let revision_context_hash = sha(&context);
    let fp = sha(&format!(
        "LINEUP_AWARE|{}|{}|{}|{}|{}",
        r.base_run_id, r.business_date, base.1, pol.version, revision_context_hash
    ));
    if let Some(id) = c
        .query_row(
            "SELECT id FROM candidate_engine_runs WHERE input_fingerprint=?1",
            [&fp],
            |x| x.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        return get(
            c,
            &GetRequest {
                business_date: r.business_date.clone(),
                run_id: Some(id),
                category: None,
            },
        );
    }
    let first = lineup_matches.values().next();
    let (lineup_version, lineup_hash) = first
        .map(|x| (x.lineup_model_version.clone(), x.lineup_model_hash.clone()))
        .unwrap_or((None, None));
    c.execute("INSERT INTO candidate_engine_runs(business_date,timezone,generated_at,model_version,calibration_version,candidate_policy_version,feature_engine_version,odds_cutoff_at,configuration_hash,input_fingerprint,status,match_count_considered,prediction_context,parent_candidate_run_id,lineup_model_version,lineup_model_hash,revision_context_hash,base_model_hash,base_calibration_hash)VALUES(?1,?2,?3,?4,?5,?6,'fe_v1',?3,?7,?8,'COMPLETED',?9,'LINEUP_AWARE',?10,?11,?12,?13,?14,?15)", params![r.business_date,BUSINESS_TIMEZONE,base.1,base.2,base.3,pol.version,sha(&serde_json::to_string(&pol).unwrap()),fp,ps.iter().map(|p| p.match_id).collect::<std::collections::BTreeSet<_>>().len(),r.base_run_id,lineup_version,lineup_hash,revision_context_hash,base_model_hash,base_calibration_hash]).map_err(|e| e.to_string())?;
    let id = c.last_insert_rowid();
    for x in &cs {
        insert_candidate(c, id, x)?;
    }
    for x in &ex {
        c.execute("INSERT INTO candidate_engine_exclusions(run_id,prediction_id,match_id,business_date,competition_id,category,market,selection,line_value,reason,details_json,generated_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![id,x.prediction_id,x.match_id,x.business_date,x.competition_id,x.category,x.market,x.selection,x.line,x.reason,x.details.as_ref().map(|v|v.to_string()),x.generated_at]).map_err(|e| e.to_string())?;
    }
    let mut out = get(
        c,
        &GetRequest {
            business_date: r.business_date.clone(),
            run_id: Some(id),
            category: None,
        },
    )?;
    out.lineup_aware_match_count = lineup_matches.len();
    out.base_fallback_match_count = ps
        .iter()
        .map(|x| x.match_id)
        .collect::<std::collections::BTreeSet<_>>()
        .difference(&lineup_matches.keys().copied().collect())
        .count();
    Ok(out)
}

/// Commit candidates, their coupons and the visible publication together.
pub fn generate_daily_output(c: &Connection, r: &GenerateRequest) -> Result<DailyRun, String> {
    if r.dry_run.unwrap_or(false) {
        return generate(c, r);
    }
    // Reserve the writer before reading the publication/input snapshot. The
    // independent logo worker must not invalidate a deferred read transaction.
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let run = generate(&tx, r)?;
    super::coupon_engine::generate_daily(
        &tx,
        &super::coupon_engine::GenerateRequest {
            business_date: run.business_date.clone(),
            candidate_run_id: Some(run.run_id),
            coupon_type: None,
            unit_stake_cents: None,
        },
    )?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(run)
}

pub fn generate(c: &Connection, r: &GenerateRequest) -> Result<DailyRun, String> {
    // A category is a view of a complete run, never a partially persisted run.
    if r.category.is_some() {
        let mut full = r.clone();
        full.category = None;
        let mut result = generate(c, &full)?;
        result
            .candidates
            .retain(|x| Some(&x.category) == r.category.as_ref());
        result
            .exclusions
            .retain(|x| Some(&x.category) == r.category.as_ref());
        result
            .category_summaries
            .retain(|x| Some(&x.category) == r.category.as_ref());
        return Ok(result);
    }
    let generated = r.generation_time.clone().unwrap_or_else(now);
    let date = r.business_date.clone().unwrap_or(day(&generated)?);
    if r.generation_time.is_none() && date < day(&generated)? {
        // Past-date navigation/re-generation reopens its immutable publication.
        // A new replay requires an explicit information cutoff.
        return get(
            c,
            &GetRequest {
                business_date: date,
                run_id: None,
                category: None,
            },
        )
        .map_err(|_| "HISTORICAL_REPLAY_REQUIRES_AS_OF".into());
    }
    chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|_| "INVALID_BUSINESS_DATE")?;
    let generated = ts(&generated)?.with_timezone(&Utc).to_rfc3339();
    let pol = policy();
    let (model,cal):(String,Option<String>)=c.query_row("SELECT version_identifier,(SELECT calibration_version FROM calibration_models WHERE is_active=1) FROM model_versions WHERE is_active=1",[],|x|Ok((x.get(0)?,x.get(1)?))).optional().map_err(|e|e.to_string())?.ok_or("no active model")?;
    let cfg = serde_json::to_string(&pol).map_err(|e| e.to_string())?;
    let ps = load(c, &date, &generated)?;
    let input_state = state_fingerprint(c, &ps, &generated)?;
    let fp = sha(&format!(
        "daily-v7|{date}|{generated}|{model}|{:?}|{cfg}|{input_state}",
        cal
    ));
    if let Some(id) = c
        .query_row(
            "SELECT id FROM candidate_engine_runs WHERE input_fingerprint=?1",
            [&fp],
            |x| x.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        if !r.dry_run.unwrap_or(false) {
            publish(c, &date, id, r.generation_time.is_some())?;
        }
        return get(
            c,
            &GetRequest {
                business_date: date,
                run_id: Some(id),
                category: r.category.clone(),
            },
        );
    }
    let mut cs = Vec::new();
    let mut ex = Vec::new();
    // Explain bulletin matches that cannot reach inference, instead of silently
    // omitting them from the daily screen and its considered-match count.
    let mut considered: std::collections::BTreeSet<i64> = ps.iter().map(|p| p.match_id).collect();
    let mut scope = c.prepare("SELECT m.id,m.competition_id FROM matches m WHERE m.scheduled_local_date=?1 AND EXISTS(SELECT 1 FROM provider_match_mappings pm WHERE pm.match_id=m.id AND pm.provider='iddaa')").map_err(|e|e.to_string())?;
    let bulletin = scope
        .query_map([&date], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (id, competition_id) in bulletin {
        if !considered.insert(id) {
            continue;
        }
        let coverage = super::model_coverage::classify(c, id).map_err(|e| e.to_string())?;
        let reason = match coverage {
            super::model_coverage::CoverageState::ModelUnsupported => "MODEL_UNSUPPORTED",
            super::model_coverage::CoverageState::ResolutionFailed => "RESOLUTION_FAILED",
            super::model_coverage::CoverageState::ModelInputMissing => "INSUFFICIENT_HISTORY",
            _ => "MODEL_PREDICTION_UNAVAILABLE",
        };
        for cat in CATEGORIES
            .iter()
            .filter(|cat| r.category.as_deref().is_none_or(|x| x == **cat))
        {
            ex.push(Exclusion {
                prediction_id: None,
                match_id: Some(id),
                business_date: date.clone(),
                competition_id: Some(competition_id),
                category: (*cat).into(),
                market: None,
                selection: None,
                line: None,
                reason: reason.into(),
                generated_at: generated.clone(),
                details: None,
            });
        }
    }
    for p in &ps {
        let odds = odd(c, p, &generated)?;
        for cat in CATEGORIES {
            if r.category.as_deref().is_some_and(|x| x != cat) {
                continue;
            }
            match &odds {
                None => ex.push(exclusion(p, &date, cat, "ODDS_UNAVAILABLE", &generated)),
                Some(o) => {
                    if let Some(reason) = reject(p, cat, &o, &pol) {
                        ex.push(exclusion(p, &date, cat, &reason, &generated))
                    } else {
                        cs.push(make(p, &o, cat));
                    }
                }
            }
        }
    }
    cs.sort_by(compare_candidates);
    for cat in CATEGORIES {
        let mut seen = BTreeMap::new();
        cs.retain(|x| {
            if x.category != cat {
                true
            } else if seen.insert(x.match_id, true).is_some() {
                ex.push(Exclusion {
                    prediction_id: Some(x.prediction_id),
                    match_id: Some(x.match_id),
                    business_date: date.clone(),
                    competition_id: Some(x.competition_id),
                    category: cat.into(),
                    market: Some(x.market.clone()),
                    selection: Some(x.selection.clone()),
                    line: x.line,
                    reason: "CORRELATION_REDUCTION".into(),
                    generated_at: generated.clone(),
                    details: Some(serde_json::json!({"correlation_type": x.correlation_type})),
                });
                false
            } else {
                true
            }
        });
    }
    cs.sort_by(compare_candidates);
    let mut ranks: BTreeMap<String, i64> = BTreeMap::new();
    for x in &mut cs {
        let n = ranks.entry(x.category.clone()).or_insert(0);
        *n += 1;
        x.rank = *n;
    }
    let run_id = if r.dry_run.unwrap_or(false) {
        0
    } else {
        c.execute("INSERT INTO candidate_engine_runs(business_date,timezone,generated_at,model_version,calibration_version,candidate_policy_version,feature_engine_version,odds_cutoff_at,configuration_hash,input_fingerprint,status,match_count_considered,prediction_context)VALUES(?1,?2,?3,?4,?5,?6,'fe_v1',?3,?7,?8,'COMPLETED',?9,'BASE')",params![date,BUSINESS_TIMEZONE,generated,model,cal,pol.version,sha(&cfg),fp,considered.len()]).map_err(|e|e.to_string())?;
        let id = c.last_insert_rowid();
        for x in &cs {
            insert_candidate(c, id, x)?;
        }
        for x in &ex {
            c.execute("INSERT INTO candidate_engine_exclusions(run_id,prediction_id,match_id,business_date,competition_id,category,market,selection,line_value,reason,details_json,generated_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![id,x.prediction_id,x.match_id,x.business_date,x.competition_id,x.category,x.market,x.selection,x.line,x.reason,x.details.as_ref().map(|v|v.to_string()),x.generated_at]).map_err(|e|e.to_string())?;
        }
        c.execute(
            "INSERT INTO candidate_run_execution(run_id,purpose) VALUES(?1,?2)",
            params![
                id,
                if r.generation_time.is_some() {
                    "REPLAY"
                } else {
                    "LIVE"
                }
            ],
        )
        .map_err(|e| e.to_string())?;
        publish(c, &date, id, r.generation_time.is_some())?;
        id
    };
    if run_id > 0 {
        return get(
            c,
            &GetRequest {
                business_date: date,
                run_id: Some(run_id),
                category: r.category.clone(),
            },
        );
    }
    let sums = CATEGORIES
        .iter()
        .filter(|cat| r.category.as_deref().is_none_or(|x| x == **cat))
        .map(|cat| {
            let n = cs.iter().filter(|x| x.category == *cat).count();
            CategorySummary {
                category: (*cat).into(),
                qualified_count: n,
                exclusion_count: ex.iter().filter(|x| x.category == *cat).count(),
                target_display_range: TARGET_DISPLAY_RANGE.into(),
                shortfall_reason: (n < 5)
                    .then(|| format!("Only {n} selections passed current quality policy.")),
            }
        })
        .collect();
    Ok(DailyRun {
        run_id,
        business_date: date,
        timezone: BUSINESS_TIMEZONE.into(),
        generated_at: generated,
        model_version: model,
        calibration_version: cal,
        policy_version: pol.version,
        match_count_considered: considered.len(),
        category_summaries: sums,
        candidates: cs,
        exclusions: ex,
        prediction_context: Some("BASE".into()),
        parent_candidate_run_id: None,
        lineup_model_version: None,
        lineup_model_hash: None,
        revision_context_hash: None,
        base_model_hash: None,
        base_calibration_hash: None,
        lineup_aware_match_count: 0,
        base_fallback_match_count: 0,
        newly_qualified_count: 0,
        no_longer_qualified_count: 0,
        still_qualified_count: 0,
        unchanged_count: 0,
    })
}

pub fn selected_run_id(c: &Connection, date: &str) -> Result<Option<i64>, String> {
    let selected: Option<i64> = c
        .query_row(
            "SELECT candidate_run_id FROM daily_output_publications WHERE business_date=?1",
            [date],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some(id) = selected else { return Ok(None) };
    let header: Option<(String, bool)> = c.query_row("SELECT COALESCE(prediction_context,'BASE'),EXISTS(SELECT 1 FROM candidate_engine_candidates WHERE run_id=r.id AND qualification_state='QUALIFIED') FROM candidate_engine_runs r WHERE id=?1 AND business_date=?2 AND status='COMPLETED'", params![id,date], |r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
    let Some((context, populated)) = header else {
        return Ok(None);
    };
    if populated && (context == "BASE" || valid_lineup_run(c, id)?) {
        return Ok(Some(id));
    }
    // Recover only full BASE output; category-only and un-published replay runs
    // must not become the daily publication just because their ID is newer.
    let base: Option<i64> = c.query_row("SELECT r.id FROM candidate_engine_runs r WHERE r.business_date=?1 AND COALESCE(r.prediction_context,'BASE')='BASE' AND r.status='COMPLETED' AND EXISTS(SELECT 1 FROM candidate_engine_candidates x WHERE x.run_id=r.id AND x.qualification_state='QUALIFIED') AND (r.id=(SELECT parent_candidate_run_id FROM candidate_engine_runs WHERE id=?2) OR EXISTS(SELECT 1 FROM candidate_run_execution e WHERE e.run_id=r.id AND e.purpose='LIVE')) ORDER BY (r.id=(SELECT parent_candidate_run_id FROM candidate_engine_runs WHERE id=?2)) DESC,julianday(r.generated_at) DESC,r.id DESC LIMIT 1",params![date,id],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
    Ok(base.or(if context == "BASE" { Some(id) } else { None }))
}

pub(crate) fn valid_lineup_run(c: &Connection, id: i64) -> Result<bool, String> {
    c.query_row("SELECT EXISTS(SELECT 1 FROM candidate_engine_candidates x JOIN candidate_engine_runs r ON r.id=x.run_id JOIN prediction_revisions pr ON pr.id=x.lineup_revision_id AND pr.match_id=x.match_id JOIN lineup_snapshots ls ON ls.id=pr.lineup_snapshot_id AND ls.id=x.lineup_snapshot_id AND ls.match_id=x.match_id WHERE r.id=?1 AND r.prediction_context='LINEUP_AWARE' AND r.status='COMPLETED' AND x.prediction_source='LINEUP_AWARE' AND x.qualification_state='QUALIFIED' AND pr.revision_type='LINEUP_AWARE_PREMATCH' AND pr.revision_status='REVISION_AVAILABLE' AND pr.payload_schema=?2 AND pr.lineup_model_hash IS NOT NULL AND ls.is_official=1 AND ls.lineup_status='OFFICIAL' AND ls.completeness_status='COMPLETE_OFFICIAL')",params![id,super::lineup_model::REVISION_PAYLOAD_SCHEMA],|r|r.get(0)).map_err(|e|e.to_string())
}

fn publish(c: &Connection, date: &str, id: i64, replay: bool) -> Result<(), String> {
    let (context, populated): (String,bool) = c.query_row("SELECT COALESCE(prediction_context,'BASE'),EXISTS(SELECT 1 FROM candidate_engine_candidates x WHERE x.run_id=r.id AND x.qualification_state='QUALIFIED') FROM candidate_engine_runs r WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?;
    if context == "LINEUP_AWARE" && !valid_lineup_run(c, id)? {
        return Ok(());
    }
    if !populated && selected_run_id(c, date)?.is_some() {
        return Ok(());
    }
    if replay {
        c.execute("INSERT OR IGNORE INTO daily_output_publications(business_date,candidate_run_id) VALUES(?1,?2)", params![date,id]).map_err(|e|e.to_string())?;
    } else {
        c.execute("INSERT INTO daily_output_publications(business_date,candidate_run_id) VALUES(?1,?2) ON CONFLICT(business_date) DO UPDATE SET candidate_run_id=excluded.candidate_run_id,published_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE julianday((SELECT generated_at FROM candidate_engine_runs WHERE id=excluded.candidate_run_id))>=julianday((SELECT generated_at FROM candidate_engine_runs WHERE id=daily_output_publications.candidate_run_id))", params![date,id]).map_err(|e|e.to_string())?;
    }
    Ok(())
}

pub fn get(c: &Connection, r: &GetRequest) -> Result<DailyRun, String> {
    let id = match r.run_id {
        Some(id) => id,
        None => selected_run_id(c, &r.business_date)?.ok_or("no candidate run")?,
    };
    let h:(String,String,String,String,Option<String>,String,usize)=c.query_row("SELECT business_date,timezone,generated_at,model_version,calibration_version,candidate_policy_version,match_count_considered FROM candidate_engine_runs WHERE id=?1",[id],|x|Ok((x.get(0)?,x.get(1)?,x.get(2)?,x.get(3)?,x.get(4)?,x.get(5)?,x.get::<_,i64>(6)? as usize))).map_err(|e|e.to_string())?;
    if h.0 != r.business_date {
        return Err("SOURCE_RUN_DATE_MISMATCH".into());
    }
    let lineage:(Option<String>,Option<i64>,Option<String>,Option<String>,Option<String>,Option<String>,Option<String>)=c.query_row("SELECT prediction_context,parent_candidate_run_id,lineup_model_version,lineup_model_hash,revision_context_hash,base_model_hash,base_calibration_hash FROM candidate_engine_runs WHERE id=?1",[id],|x|Ok((x.get(0)?,x.get(1)?,x.get(2)?,x.get(3)?,x.get(4)?,x.get(5)?,x.get(6)?))).map_err(|e|e.to_string())?;
    let mut q=c.prepare("SELECT id,prediction_id,match_id,competition_id,category,market,selection,line_value,raw_probability,public_probability,calibration_status,calibration_version,calibration_bucket,bucket_observed_rate,bucket_sample_size,bucket_calibration_gap,iddaa_odd,odds_snapshot_id,odds_captured_at,implied_probability,probability_edge,expected_value,data_quality,score,score_components_json,rank,same_match_group,correlation_type,compound_eligible,qualification_state,prediction_source,lineup_revision_id,lineup_snapshot_id,base_public_probability,final_public_probability,delta_percentage_points,lineup_family_status,lineup_fallback_reason,lineup_model_version,lineup_model_hash FROM candidate_engine_candidates WHERE run_id=?1 AND (?2 IS NULL OR category=?2) ORDER BY category,rank").map_err(|e|e.to_string())?;
    let candidates = q
        .query_map(params![id, r.category], |x| {
            let j: String = x.get(24)?;
            Ok(Candidate {
                id: x.get(0)?,
                prediction_id: x.get(1)?,
                match_id: x.get(2)?,
                competition_id: x.get(3)?,
                category: x.get(4)?,
                market: x.get(5)?,
                selection: x.get(6)?,
                line: x.get(7)?,
                raw_probability: x.get(8)?,
                public_probability: x.get(9)?,
                calibration_status: x.get(10)?,
                calibration_version: x.get(11)?,
                calibration_bucket: x.get(12)?,
                bucket_observed_rate: x.get(13)?,
                bucket_sample_size: x.get::<_, Option<i64>>(14)?.map(|n| n as usize),
                bucket_calibration_gap: x.get(15)?,
                iddaa_odd: x.get(16)?,
                odds_snapshot_id: x.get(17)?,
                odds_captured_at: x.get(18)?,
                implied_probability: x.get(19)?,
                probability_edge: x.get(20)?,
                expected_value: x.get(21)?,
                data_quality: x.get(22)?,
                score: x.get(23)?,
                score_components: serde_json::from_str(&j).unwrap_or_default(),
                rank: x.get(25)?,
                same_match_group: x.get(26)?,
                correlation_type: x.get(27)?,
                compound_eligible: x.get::<_, i64>(28)? == 1,
                qualification_state: x.get(29)?,
                prediction_source: x.get(30)?,
                lineup_revision_id: x.get(31)?,
                lineup_snapshot_id: x.get(32)?,
                base_public_probability: x.get(33)?,
                final_public_probability: x.get(34)?,
                delta_percentage_points: x.get(35)?,
                lineup_family_status: x.get(36)?,
                lineup_fallback_reason: x.get(37)?,
                lineup_model_version: x.get(38)?,
                lineup_model_hash: x.get(39)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut exq = c.prepare("SELECT prediction_id,match_id,business_date,competition_id,category,market,selection,line_value,reason,generated_at,details_json FROM candidate_engine_exclusions WHERE run_id=?1 AND (?2 IS NULL OR category=?2) ORDER BY category,id").map_err(|e| e.to_string())?;
    let exclusions = exq
        .query_map(params![id, r.category], |x| {
            let details: Option<String> = x.get(10)?;
            Ok(Exclusion {
                prediction_id: x.get(0)?,
                match_id: x.get(1)?,
                business_date: x.get(2)?,
                competition_id: x.get(3)?,
                category: x.get(4)?,
                market: x.get(5)?,
                selection: x.get(6)?,
                line: x.get(7)?,
                reason: x.get(8)?,
                generated_at: x.get(9)?,
                details: details.and_then(|v| serde_json::from_str(&v).ok()),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let lineup_aware_match_count = candidates
        .iter()
        .filter(|x| x.prediction_source.as_deref() == Some("LINEUP_AWARE"))
        .map(|x| x.match_id)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let base_fallback_match_count = candidates
        .iter()
        .filter(|x| x.prediction_source.as_deref() != Some("LINEUP_AWARE"))
        .map(|x| x.match_id)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let (lineup_aware_match_count, base_fallback_match_count) = if lineage.0.as_deref()
        == Some("LINEUP_AWARE")
    {
        let lineup_matches: usize = c
            .query_row(
                "SELECT COUNT(DISTINCT pr.match_id) FROM prediction_revisions pr JOIN matches m ON m.id=pr.match_id WHERE m.scheduled_local_date=?1 AND pr.revision_type='LINEUP_AWARE_PREMATCH' AND pr.payload_schema=?2",
                params![h.0, crate::repositories::lineup_model::REVISION_PAYLOAD_SCHEMA],
                |x| x.get::<_, i64>(0),
            )
            .unwrap_or(0) as usize;
        let considered = h.6;
        (lineup_matches, considered.saturating_sub(lineup_matches))
    } else {
        (lineup_aware_match_count, base_fallback_match_count)
    };
    let category_summaries = CATEGORIES
        .iter()
        .filter(|cat| r.category.as_deref().is_none_or(|x| x == **cat))
        .map(|cat| {
            let qualified_count = candidates.iter().filter(|x| x.category == *cat).count();
            let exclusion_count = exclusions.iter().filter(|x| x.category == *cat).count();
            CategorySummary {
                category: (*cat).into(),
                qualified_count,
                exclusion_count,
                target_display_range: TARGET_DISPLAY_RANGE.into(),
                shortfall_reason: (qualified_count < 5).then(|| {
                    format!("Only {qualified_count} selections passed current quality policy.")
                }),
            }
        })
        .collect();
    Ok(DailyRun {
        run_id: id,
        business_date: h.0,
        timezone: h.1,
        generated_at: h.2,
        model_version: h.3,
        calibration_version: h.4,
        policy_version: h.5,
        match_count_considered: h.6,
        category_summaries,
        candidates,
        exclusions,
        prediction_context: lineage.0,
        parent_candidate_run_id: lineage.1,
        lineup_model_version: lineage.2,
        lineup_model_hash: lineage.3,
        revision_context_hash: lineage.4,
        base_model_hash: lineage.5,
        base_calibration_hash: lineage.6,
        lineup_aware_match_count,
        base_fallback_match_count,
        newly_qualified_count: 0,
        no_longer_qualified_count: 0,
        still_qualified_count: 0,
        unchanged_count: 0,
    })
}
pub fn status(c: &Connection) -> Result<Status, String> {
    let model = c
        .query_row(
            "SELECT version_identifier FROM model_versions WHERE is_active=1",
            [],
            |x| x.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let cal = c
        .query_row(
            "SELECT calibration_version FROM calibration_models WHERE is_active=1",
            [],
            |x| x.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let latest = c
        .query_row(
            "SELECT id FROM candidate_engine_runs ORDER BY id DESC LIMIT 1",
            [],
            |x| x.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let mut counts = BTreeMap::new();
    let mut rejected_counts = BTreeMap::new();
    let mut rejected_by_category = BTreeMap::new();
    let mut category_counts = Vec::new();
    let mut latest_business_date = None;
    let mut latest_generated_at = None;
    let mut matches_considered = 0;
    if let Some(id) = latest {
        let h: (String, String, i64) = c.query_row("SELECT business_date,generated_at,match_count_considered FROM candidate_engine_runs WHERE id=?1", [id], |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?))).map_err(|e| e.to_string())?;
        latest_business_date = Some(h.0);
        latest_generated_at = Some(h.1);
        matches_considered = h.2 as usize;
        let mut q=c.prepare("SELECT category,COUNT(*) FROM candidate_engine_candidates WHERE run_id=?1 GROUP BY category").map_err(|e|e.to_string())?;
        for row in q
            .query_map([id], |x| Ok((x.get::<_, String>(0)?, x.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?
        {
            let (k, v) = row.map_err(|e| e.to_string())?;
            counts.insert(k, v as usize);
        }
        let mut q=c.prepare("SELECT reason,COUNT(*) FROM candidate_engine_exclusions WHERE run_id=?1 GROUP BY reason").map_err(|e|e.to_string())?;
        for row in q
            .query_map([id], |x| Ok((x.get::<_, String>(0)?, x.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?
        {
            let (k, v) = row.map_err(|e| e.to_string())?;
            rejected_counts.insert(k, v as usize);
        }
        let mut q=c.prepare("SELECT category,COUNT(*) FROM candidate_engine_exclusions WHERE run_id=?1 GROUP BY category").map_err(|e|e.to_string())?;
        for row in q
            .query_map([id], |x| Ok((x.get::<_, String>(0)?, x.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?
        {
            let (k, v) = row.map_err(|e| e.to_string())?;
            rejected_by_category.insert(k, v as usize);
        }
        for cat in CATEGORIES {
            category_counts.push(StatusCategory {
                category: cat.into(),
                qualified_count: *counts.get(cat).unwrap_or(&0),
                rejected_count: *rejected_by_category.get(cat).unwrap_or(&0),
            });
        }
    }
    let today = day(&now()).unwrap_or_default();
    let eligible_match_count = c
        .query_row(
            "SELECT COUNT(*) FROM matches WHERE scheduled_local_date=?1 AND status='scheduled'",
            [&today],
            |x| x.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())? as usize;
    Ok(Status {
        active_model_version: model.clone(),
        calibration_version: cal.clone(),
        calibration_status: cal.as_ref().map(|_| "ACTIVE".to_string()),
        candidate_policy_version: POLICY_VERSION.into(),
        business_date: latest_business_date.clone(),
        timezone: BUSINESS_TIMEZONE.into(),
        latest_run_id: latest,
        latest_generation_time: latest_generated_at.clone(),
        matches_eligible_today: eligible_match_count,
        matches_considered_in_latest_run: matches_considered,
        category_counts,
        total_rejected: rejected_counts.values().sum(),
        active_model: model,
        active_calibration: cal,
        policy_version: POLICY_VERSION.into(),
        latest_run: latest,
        eligible_match_count,
        qualified_counts: counts,
        missing_odds_count: rejected_counts
            .get("ODDS_UNAVAILABLE")
            .copied()
            .unwrap_or(0),
        stale_odds_count: rejected_counts.get("STALE_ODDS").copied().unwrap_or(0),
        model_unavailable_count: rejected_counts
            .get("MODEL_UNAVAILABLE")
            .copied()
            .unwrap_or(0),
        low_data_quality_count: *rejected_counts.get("LOW_DATA_QUALITY").unwrap_or(&0),
        calibration_insufficient_count: rejected_counts
            .get("CALIBRATION_INSUFFICIENT")
            .copied()
            .unwrap_or(0)
            + rejected_counts
                .get("INSUFFICIENT_BUCKET_SAMPLE")
                .copied()
                .unwrap_or(0),
        latest_business_date,
        latest_generated_at,
        matches_considered,
        rejected_counts,
        rejected_counts_by_category: rejected_by_category,
    })
}
