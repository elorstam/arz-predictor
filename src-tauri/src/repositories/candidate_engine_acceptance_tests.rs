use super::candidate_engine::{self, GenerateRequest, GetRequest, LineupRevisionGenerateRequest};
use crate::database::Database;
use rusqlite::{params, Connection};
use std::time::Instant;

const DATE: &str = "2026-09-02";
const GENERATION: &str = "2026-09-02T12:00:00Z";

pub(super) fn db() -> Database {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    c.execute("INSERT INTO model_versions(version_identifier,model_name,registry_status,is_active) VALUES('model_v1','acceptance','ACTIVE',1)", []).unwrap();
    c.execute("INSERT INTO calibration_models(parent_model_version,parent_artifact_sha256,calibration_version,market_family,calibration_method,parameters_json,sample_count,validation_metrics_json,artifact_sha256,artifact_path,is_active,created_at) VALUES('model_v1','a','cal_v1','all','identity','{}',100,'{}','a','temp',1,?1)", [GENERATION]).unwrap();
    c.execute("INSERT INTO competitions(name,country,current_season) VALUES('Acceptance Premier','TR','2026/27'),('Acceptance 1. Lig','TR','2026/27')", []).unwrap();
    for i in 0..50 {
        c.execute(
            "INSERT INTO teams(normalized_name,country) VALUES(?1,'TR')",
            [format!("Acceptance Team {i}")],
        )
        .unwrap();
    }
    for i in 0..19 {
        let status = if i == 17 {
            "finished"
        } else if i == 18 {
            "in_progress"
        } else {
            "scheduled"
        };
        let kickoff = format!("2026-09-02T{:02}:00:00Z", 18 + (i % 3));
        c.execute("INSERT INTO matches(competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(?1,'2026/27',?2,?3,?4,?5,?6,1)", params![1 + i % 2, 1 + i * 2, 2 + i * 2, kickoff, status, DATE]).unwrap();
        let quality = if i == 16 {
            r#"{"history_matches_home":1,"history_matches_away":1,"corners_coverage":0.1}"#
        } else {
            r#"{"history_matches_home":8,"history_matches_away":8,"corners_coverage":0.9}"#
        };
        c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json) VALUES(?1,'fe_v1',?2,'{}',?3)", params![i + 1, GENERATION, quality]).unwrap();
    }
    // Every candidate below is a real persisted prediction DTO shape, including Phase 6.1 metadata.
    prediction(
        &c,
        1,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.80,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    prediction(
        &c,
        2,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.50,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    prediction(
        &c,
        3,
        "TOTAL_GOALS",
        "OVER",
        Some(3.5),
        0.60,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    prediction(
        &c,
        4,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.80,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    prediction(
        &c,
        4,
        "TOTAL_GOALS",
        "OVER",
        Some(3.5),
        0.48,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    prediction(
        &c,
        5,
        "BTTS",
        "YES",
        None,
        0.75,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    prediction(
        &c,
        6,
        "BTTS",
        "NO",
        None,
        0.80,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    for (line, p) in [
        (7.5, 0.80),
        (8.5, 0.75),
        (9.5, 0.70),
        (10.5, 0.65),
        (11.5, 0.60),
    ] {
        prediction(
            &c,
            7,
            "FULL_TIME_TOTAL_CORNERS",
            "OVER",
            Some(line),
            p,
            "CALIBRATED_V1",
            Some(25),
            "AVAILABLE",
        );
    }
    prediction(
        &c,
        8,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.82,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    prediction(
        &c,
        8,
        "TOTAL_GOALS",
        "OVER",
        Some(3.5),
        0.81,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    prediction(
        &c,
        9,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.85,
        "CALIBRATION_INSUFFICIENT_DATA",
        Some(5),
        "AVAILABLE",
    );
    prediction(
        &c,
        10,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.48,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    prediction(
        &c,
        11,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.18,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    for i in 12..=14 {
        prediction(
            &c,
            i,
            "TOTAL_GOALS",
            "OVER",
            Some(2.5),
            0.80,
            "CALIBRATED_V1",
            Some(25),
            "AVAILABLE",
        );
    }
    prediction(
        &c,
        15,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.80,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    prediction(
        &c,
        16,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.80,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    prediction(
        &c,
        17,
        "TEAM_CARDS",
        "OVER",
        Some(1.5),
        0.80,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    prediction(
        &c,
        18,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.80,
        "CALIBRATED_V1",
        Some(25),
        "UNAVAILABLE",
    );
    prediction(
        &c,
        19,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.80,
        "CALIBRATED_V1",
        Some(25),
        "AVAILABLE",
    );
    // Current, historical, and future odds: only the latest snapshot at/before cutoff is eligible.
    odds(
        &c,
        1,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        1.80,
        "2026-09-02T08:00:00Z",
    );
    odds(
        &c,
        1,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        2.00,
        "2026-09-02T10:00:00Z",
    );
    odds(
        &c,
        1,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        9.99,
        "2026-09-02T13:00:00Z",
    );
    odds(
        &c,
        2,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        2.00,
        "2026-09-02T10:00:00Z",
    );
    odds(
        &c,
        3,
        "TOTAL_GOALS",
        "OVER",
        Some(3.5),
        2.00,
        "2026-09-02T10:00:00Z",
    );
    odds(
        &c,
        4,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        2.00,
        "2026-09-02T10:00:00Z",
    );
    odds(
        &c,
        4,
        "TOTAL_GOALS",
        "OVER",
        Some(3.5),
        2.10,
        "2026-09-02T10:00:00Z",
    );
    odds(&c, 5, "BTTS", "YES", None, 1.80, "2026-09-02T10:00:00Z");
    odds(&c, 6, "BTTS", "NO", None, 2.00, "2026-09-02T10:00:00Z");
    for line in [7.5, 8.5, 9.5, 10.5, 11.5] {
        odds(
            &c,
            7,
            "FULL_TIME_TOTAL_CORNERS",
            "OVER",
            Some(line),
            2.00,
            "2026-09-02T10:00:00Z",
        );
    }
    for id in [8, 9, 10, 11, 12, 13, 14] {
        odds(
            &c,
            id,
            "TOTAL_GOALS",
            "OVER",
            Some(2.5),
            if id == 11 {
                3.2
            } else if id == 10 {
                2.6
            } else if id == 8 {
                1.6
            } else {
                1.3
            },
            "2026-09-02T10:00:00Z",
        );
    }
    odds(
        &c,
        15,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        2.00,
        "2026-09-01T00:00:00Z",
    );
    odds(
        &c,
        17,
        "TEAM_CARDS",
        "OVER",
        Some(1.5),
        2.00,
        "2026-09-02T10:00:00Z",
    );
    odds(
        &c,
        18,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        2.00,
        "2026-09-02T10:00:00Z",
    );
    odds(
        &c,
        19,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        2.00,
        "2026-09-02T10:00:00Z",
    );
    drop(c);
    db
}

fn prediction(
    c: &Connection,
    match_id: i64,
    market: &str,
    selection: &str,
    line: Option<f64>,
    p: f64,
    status: &str,
    n: Option<i64>,
    availability: &str,
) {
    c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,calibration_bucket,bucket_sample_size,bucket_observed_rate,availability) VALUES(?1,?2,?3,?4,?5,'acceptance',?6,1,?5,?5,?7,'cal_v1','0.8',?8,?5,?9)", params![match_id,market,selection,line,p,"2026-09-02T18:00:00Z",status,n,availability]).unwrap();
}
fn odds(
    c: &Connection,
    match_id: i64,
    market: &str,
    selection: &str,
    line: Option<f64>,
    odd: f64,
    captured: &str,
) {
    c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(?1,'iddaa','accept',?2,?3,?4,?2,?3,?5,?6)", params![match_id,market,selection,odd,line,captured]).unwrap();
}

fn print_report(run: &candidate_engine::DailyRun, status: &candidate_engine::Status) {
    println!("PHASE 7 CONTROLLED ACCEPTANCE\nBusiness date: {}\nTimezone: {}\nGeneration time: {}\nPolicy version: {}\nModel version: {}\nCalibration version: {:?}\nMatches considered: {}", run.business_date, run.timezone, run.generated_at, run.policy_version, run.model_version, run.calibration_version, run.match_count_considered);
    println!("CATEGORY TOTALS");
    for s in &run.category_summaries {
        println!(
            "{}\nqualified = {}\nrejected = {}",
            s.category, s.qualified_count, s.exclusion_count
        );
    }
    let mut reasons = std::collections::BTreeMap::new();
    for x in &run.exclusions {
        *reasons.entry(x.reason.clone()).or_insert(0usize) += 1;
    }
    println!("EXCLUSIONS BY REASON\n{:?}", reasons);
    for cat in [
        "CORNERS",
        "OVER_25",
        "OVER_35",
        "BTTS_YES",
        "HIGH_CONFIDENCE",
        "SURPRISE",
        "COMPOUND",
    ] {
        println!("TOP {}", cat);
        for x in run.candidates.iter().filter(|x| x.category == cat).take(3) {
            println!("rank={} match_id={} market={} selection={} line={:?} raw_probability={} public_probability={} calibration_status={} calibration_bucket={:?} bucket_sample_size={:?} bucket_observed_rate={:?} odd={} odds_snapshot_id={} odds_captured_at={} implied_probability={} edge={} expected_value={} score={} data_quality={} correlation={}", x.rank,x.match_id,x.market,x.selection,x.line,x.raw_probability,x.public_probability,x.calibration_status,x.calibration_bucket,x.bucket_sample_size,x.bucket_observed_rate,x.iddaa_odd,x.odds_snapshot_id,x.odds_captured_at,x.implied_probability,x.probability_edge,x.expected_value,x.score,x.data_quality,x.correlation_type);
        }
    }
    println!("STATUS {:?}", status);
}

#[test]
fn lineup_candidate_run_is_immutable_child_with_base_fallback() {
    let db = db();
    let c = db.connection().unwrap();
    let base = candidate_engine::generate(
        &c,
        &GenerateRequest {
            business_date: Some(DATE.into()),
            generation_time: Some(GENERATION.into()),
            category: Some("OVER_25".into()),
            dry_run: Some(false),
        },
    )
    .unwrap();
    let lineup = candidate_engine::generate_lineup_revision(
        &c,
        &LineupRevisionGenerateRequest {
            base_run_id: base.run_id,
            business_date: DATE.into(),
        },
    )
    .unwrap();
    assert_ne!(base.run_id, lineup.run_id);
    assert_eq!(lineup.prediction_context.as_deref(), Some("LINEUP_AWARE"));
    assert_eq!(lineup.parent_candidate_run_id, Some(base.run_id));
    assert!(lineup.revision_context_hash.is_some());
    assert!(lineup
        .candidates
        .iter()
        .all(|x| x.prediction_source.as_deref() == Some("BASE")));
    assert!(lineup
        .candidates
        .iter()
        .all(|x| x.final_public_probability == x.base_public_probability));
    let repeated = candidate_engine::generate_lineup_revision(
        &c,
        &LineupRevisionGenerateRequest {
            base_run_id: base.run_id,
            business_date: DATE.into(),
        },
    )
    .unwrap();
    assert_eq!(lineup.run_id, repeated.run_id);
    let base_again = candidate_engine::get(
        &c,
        &GetRequest {
            business_date: DATE.into(),
            run_id: Some(base.run_id),
            category: Some("OVER_25".into()),
        },
    )
    .unwrap();
    assert_eq!(
        base.candidates
            .iter()
            .map(|x| (x.prediction_id, x.public_probability))
            .collect::<Vec<_>>(),
        base_again
            .candidates
            .iter()
            .map(|x| (x.prediction_id, x.public_probability))
            .collect::<Vec<_>>()
    );
}

#[test]
fn phase_7_controlled_acceptance_and_100_match_benchmark() {
    let db = db();
    let c = db.connection().unwrap();
    let run = candidate_engine::generate(
        &c,
        &GenerateRequest {
            business_date: Some(DATE.into()),
            generation_time: Some(GENERATION.into()),
            category: None,
            dry_run: Some(false),
        },
    )
    .unwrap();
    let fetched = candidate_engine::get(
        &c,
        &GetRequest {
            business_date: DATE.into(),
            run_id: None,
            category: None,
        },
    )
    .unwrap();
    let filtered = candidate_engine::get(
        &c,
        &GetRequest {
            business_date: DATE.into(),
            run_id: Some(run.run_id),
            category: Some("OVER_25".into()),
        },
    )
    .unwrap();
    let status = candidate_engine::status(&c).unwrap();
    print_report(&fetched, &status);
    assert_eq!(run.run_id, fetched.run_id);
    assert_eq!(fetched.match_count_considered, 19);
    assert!(filtered.candidates.iter().all(|x| x.category == "OVER_25"));
    assert!(filtered.exclusions.iter().all(|x| x.category == "OVER_25"));
    for cat in [
        "CORNERS",
        "OVER_25",
        "OVER_35",
        "BTTS_YES",
        "HIGH_CONFIDENCE",
        "SURPRISE",
        "COMPOUND",
    ] {
        assert!(
            fetched.candidates.iter().any(|x| x.category == cat),
            "{cat} must qualify"
        );
    }
    assert_eq!(
        fetched
            .candidates
            .iter()
            .filter(|x| x.category == "COMPOUND")
            .count(),
        3
    );
    assert!(!fetched
        .candidates
        .iter()
        .any(|x| x.category == "BTTS_YES" && x.selection == "NO"));
    assert!(!fetched
        .candidates
        .iter()
        .any(|x| x.category == "HIGH_CONFIDENCE" && x.match_id == 9));
    assert!(!fetched
        .candidates
        .iter()
        .any(|x| x.category == "SURPRISE" && x.match_id == 11));
    assert_eq!(
        fetched
            .candidates
            .iter()
            .filter(|x| x.category == "CORNERS" && x.match_id == 7)
            .count(),
        1
    );
    assert!(fetched
        .exclusions
        .iter()
        .any(|x| x.reason == "CORRELATION_REDUCTION"));
    assert!(fetched.exclusions.iter().any(|x| x.reason == "STALE_ODDS"));
    assert!(fetched
        .exclusions
        .iter()
        .any(|x| x.reason == "ODDS_UNAVAILABLE"));
    assert!(fetched
        .exclusions
        .iter()
        .any(|x| x.reason == "CALIBRATION_INSUFFICIENT"));
    assert!(fetched
        .exclusions
        .iter()
        .any(|x| x.reason == "MODEL_UNAVAILABLE"));
    let corner: Vec<_> = fetched
        .candidates
        .iter()
        .filter(|x| x.category == "CORNERS" && x.match_id == 7)
        .collect();
    assert_eq!(corner[0].line, Some(7.5));
    assert_eq!(corner[0].iddaa_odd, 2.0);
    assert!((corner[0].implied_probability - 0.5).abs() < 1e-9);
    assert_eq!(status.matches_considered_in_latest_run, 19);
    assert_eq!(status.latest_run_id, Some(run.run_id));
    assert_eq!(status.total_rejected, fetched.exclusions.len());
    assert_eq!(
        status
            .category_counts
            .iter()
            .map(|x| x.qualified_count)
            .sum::<usize>(),
        fetched.candidates.len()
    );
    for category in &status.category_counts {
        let summary = fetched
            .category_summaries
            .iter()
            .find(|x| x.category == category.category)
            .unwrap();
        assert_eq!(category.qualified_count, summary.qualified_count);
        assert_eq!(category.rejected_count, summary.exclusion_count);
    }
    let again = candidate_engine::generate(
        &c,
        &GenerateRequest {
            business_date: Some(DATE.into()),
            generation_time: Some(GENERATION.into()),
            category: None,
            dry_run: Some(false),
        },
    )
    .unwrap();
    assert_eq!(again.run_id, run.run_id);
    c.execute("INSERT INTO popularity_snapshots(provider,match_id,provider_event_id,metric_type,metric_value,raw_metric_name,provider_raw_value,captured_at) VALUES('iddaa',1,'accept-1','COUNT',1,'totalPlayed','1',?1)", [GENERATION]).unwrap();
    let popularity_only = candidate_engine::generate(
        &c,
        &GenerateRequest {
            business_date: Some(DATE.into()),
            generation_time: Some(GENERATION.into()),
            category: None,
            dry_run: Some(false),
        },
    )
    .unwrap();
    assert_eq!(popularity_only.run_id, run.run_id);
    let before = fetched.candidates.clone();
    let before_fp: String = c
        .query_row(
            "SELECT input_fingerprint FROM candidate_engine_runs WHERE id=?1",
            [run.run_id],
            |r| r.get(0),
        )
        .unwrap();
    c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(1,'iddaa','accept','TOTAL_GOALS','OVER',2.20,'TOTAL_GOALS','OVER',2.5,'2026-09-02T11:00:00Z')",[]).unwrap();
    let changed = candidate_engine::generate(
        &c,
        &GenerateRequest {
            business_date: Some(DATE.into()),
            generation_time: Some(GENERATION.into()),
            category: None,
            dry_run: Some(false),
        },
    )
    .unwrap();
    assert_ne!(changed.run_id, run.run_id);
    let old = candidate_engine::get(
        &c,
        &GetRequest {
            business_date: DATE.into(),
            run_id: Some(run.run_id),
            category: None,
        },
    )
    .unwrap();
    assert_eq!(old.candidates, before);
    let after_fp: String = c
        .query_row(
            "SELECT input_fingerprint FROM candidate_engine_runs WHERE id=?1",
            [changed.run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(before_fp, after_fp);
    for i in 0..100 {
        let home = 51 + i * 2;
        let away = 52 + i * 2;
        c.execute(
            "INSERT INTO teams(normalized_name) VALUES(?1),(?2)",
            params![format!("Bench H{i}"), format!("Bench A{i}")],
        )
        .unwrap();
        c.execute("INSERT INTO matches(competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(1,'2026/27',?1,?2,'2026-09-03T18:00:00Z','scheduled','2026-09-03',1)",params![home,away]).unwrap();
        let id = 20 + i;
        prediction(
            &c,
            id,
            "TOTAL_GOALS",
            "OVER",
            Some(2.5),
            0.8,
            "CALIBRATED_V1",
            Some(25),
            "AVAILABLE",
        );
        odds(
            &c,
            id,
            "TOTAL_GOALS",
            "OVER",
            Some(2.5),
            2.0,
            "2026-09-03T10:00:00Z",
        );
    }
    let started = Instant::now();
    let bench = candidate_engine::generate(
        &c,
        &GenerateRequest {
            business_date: Some("2026-09-03".into()),
            generation_time: Some("2026-09-03T12:00:00Z".into()),
            category: Some("OVER_25".into()),
            dry_run: Some(false),
        },
    )
    .unwrap();
    let ms = started.elapsed().as_millis();
    println!("100-MATCH BENCHMARK\nmatches processed = {}\npotential selections evaluated = {}\nqualified = {}\nrejected = {}\ntotal ms = {}\naverage ms/match = {:.3}",bench.match_count_considered,bench.candidates.len()+bench.exclusions.len(),bench.candidates.len(),bench.exclusions.len(),ms,ms as f64/100.0);
    assert_eq!(bench.match_count_considered, 100);
    assert_eq!(bench.candidates.len(), 100);
}
