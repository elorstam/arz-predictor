use rusqlite::{params, Connection};

use super::model_performance as subject;
use crate::database::Database;

const AS_OF: &str = "2026-01-31";

fn add_match(
    c: &Connection,
    id: i64,
    date: &str,
    competition: i64,
    status: &str,
    hg: Option<i64>,
    ag: Option<i64>,
) {
    c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,final_home_goals,final_away_goals,scheduled_local_date,kickoff_time_known) VALUES(?1,?2,'2025/26',?3,?4,?5,?6,?7,?8,substr(?5,1,10),1)",params![id,competition,id*2-1,id*2,format!("{date}T18:00:00Z"),status,hg,ag]).unwrap();
    if status == "finished" {
        c.execute("INSERT INTO match_statistics(match_id,home_corners,away_corners,home_yellow_cards,away_yellow_cards) VALUES(?1,6,4,2,2)",[id]).unwrap();
    }
}

#[allow(clippy::too_many_arguments)]
fn add_backtest(
    c: &Connection,
    run: i64,
    cal: i64,
    match_id: i64,
    market: &str,
    selection: &str,
    line: Option<f64>,
    p: f64,
    actual: bool,
    settlement: &str,
) {
    let kickoff: String = c
        .query_row(
            "SELECT kickoff_at FROM matches WHERE id=?1",
            [match_id],
            |row| row.get(0),
        )
        .unwrap();
    let generated = kickoff.replace("18:00:00", "12:00:00");
    c.execute("INSERT INTO backtest_calibration_predictions(run_id,calibration_model_id,match_id,market,line_value,selection,raw_probability,calibrated_probability,public_probability,calibration_status,calibration_bucket,bucket_sample_size,actual_result,settlement,generated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7,?7,'CALIBRATED_V1','fixture',100,?8,?9,?10)",params![run,cal,match_id,market,line,selection,p,actual.to_string(),settlement,generated]).unwrap();
}

fn add_odds(
    c: &Connection,
    match_id: i64,
    market: &str,
    selection: &str,
    line: Option<f64>,
    odd: f64,
) {
    let kickoff: String = c
        .query_row(
            "SELECT kickoff_at FROM matches WHERE id=?1",
            [match_id],
            |row| row.get(0),
        )
        .unwrap();
    c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,line_value,selection,odd,normalized_market_type,normalized_selection,captured_at) VALUES(?1,'iddaa',?2,?2,?3,?4,?5,?2,?4,?6)",params![match_id,market,line,selection,odd,kickoff.replace("18:00:00","11:00:00")]).unwrap();
}

fn fixture() -> Database {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    c.execute("INSERT INTO competitions(id,name,current_season) VALUES(1,'Premier League','2025/26'),(2,'La Liga','2025/26')",[]).unwrap();
    for id in 1..=70 {
        c.execute(
            "INSERT INTO teams(id,normalized_name) VALUES(?1,?2)",
            params![id, format!("Team {id}")],
        )
        .unwrap();
    }
    c.execute("INSERT INTO model_versions(id,version_identifier,model_name) VALUES(1,'model_v1','Fixture V1'),(2,'model_v2','Fixture V2')",[]).unwrap();
    for (id, version, model) in [(1, "cal_v1", "model_v1"), (2, "cal_v2", "model_v2")] {
        c.execute("INSERT INTO calibration_models(id,parent_model_version,parent_artifact_sha256,calibration_version,market_family,calibration_method,parameters_json,sample_count,validation_metrics_json,artifact_sha256,artifact_path,created_at) VALUES(?1,?2,'hash',?3,'MULTI_FAMILY','FIXTURE','{}',100,'{}',?3,'fixture','2025-01-01')",params![id,model,version]).unwrap();
        c.execute("INSERT INTO backtest_runs(id,model_family_version,feature_engine_version,feature_schema_version,label_version,started_at,completed_at,evaluation_start,evaluation_end,configuration_json,status,result_hash) VALUES(?1,?2,'fe_v1','schema_v1','labels_v1','2025-01-01','2026-01-31','2025-01-01','2026-01-31','{}','COMPLETED',?2)",params![id,model]).unwrap();
    }
    let dates = [
        "2025-12-01",
        "2025-12-02",
        "2025-12-03",
        "2025-12-04",
        "2026-01-05",
        "2026-01-07",
        "2026-01-09",
        "2026-01-11",
        "2026-01-13",
        "2026-01-15",
        "2026-01-17",
        "2026-01-19",
        "2026-01-21",
        "2026-01-22",
        "2026-01-23",
        "2026-01-24",
        "2026-01-25",
        "2026-01-26",
        "2026-01-27",
        "2026-01-28",
    ];
    let odds = [1.20, 1.40, 1.60, 1.90, 2.20, 2.70, 3.20];
    for (index, date) in dates.iter().enumerate() {
        let id = index as i64 + 1;
        let actual = index < 14;
        let (home_goals, away_goals) = if id == 20 {
            (0, 0)
        } else if actual {
            (2, 1)
        } else {
            (1, 0)
        };
        add_match(
            &c,
            id,
            date,
            if index < 10 { 1 } else { 2 },
            "finished",
            Some(home_goals),
            Some(away_goals),
        );
        add_backtest(
            &c,
            1,
            1,
            id,
            "BTTS",
            "YES",
            None,
            0.70,
            actual,
            if actual { "WON" } else { "LOST" },
        );
        add_odds(&c, id, "BTTS", "YES", None, odds[index % odds.len()]);
    }
    for (id, date) in [(21, "2026-01-26"), (22, "2026-01-27"), (23, "2026-01-28")] {
        add_match(&c, id, date, 2, "finished", Some(1), Some(1));
        add_backtest(&c, 2, 2, id, "BTTS", "YES", None, 0.80, true, "WON");
        add_odds(&c, id, "BTTS", "YES", None, 2.10);
    }
    add_match(&c, 24, "2026-01-29", 1, "scheduled", None, None);
    add_backtest(&c, 1, 1, 24, "BTTS", "YES", None, 0.99, true, "WON");
    add_match(&c, 25, "2026-01-29", 1, "cancelled", None, None);
    add_backtest(&c, 1, 1, 25, "BTTS", "YES", None, 0.99, true, "WON");
    add_match(&c, 26, "2026-01-29", 1, "finished", Some(1), Some(0));
    add_backtest(&c, 1, 1, 26, "BTTS", "YES", None, 0.99, true, "WON");
    add_match(&c, 27, "2026-01-29", 1, "finished", Some(1), Some(0));
    add_backtest(&c, 1, 1, 27, "BTTS", "YES", None, 0.99, false, "UNSETTLED");

    add_backtest(
        &c,
        1,
        1,
        1,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.55,
        true,
        "WON",
    );
    add_backtest(
        &c,
        1,
        1,
        1,
        "TOTAL_GOALS",
        "UNDER",
        Some(2.5),
        0.45,
        false,
        "LOST",
    );
    add_backtest(
        &c,
        1,
        1,
        2,
        "TOTAL_GOALS",
        "UNDER",
        Some(2.5),
        0.90,
        false,
        "LOST",
    );
    add_backtest(
        &c,
        1,
        1,
        2,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.65,
        true,
        "WON",
    );
    add_backtest(
        &c,
        1,
        1,
        3,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.85,
        true,
        "WON",
    );
    add_backtest(
        &c,
        1,
        1,
        4,
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        0.90,
        true,
        "WON",
    );
    add_backtest(
        &c,
        1,
        1,
        1,
        "HOME_TEAM_TOTAL_GOALS",
        "OVER",
        Some(1.5),
        0.75,
        true,
        "WON",
    );
    add_backtest(
        &c,
        1,
        1,
        1,
        "FULL_TIME_TOTAL_CORNERS",
        "OVER",
        Some(8.5),
        0.75,
        true,
        "WON",
    );
    add_backtest(
        &c,
        1,
        1,
        1,
        "FULL_TIME_TOTAL_CARDS",
        "OVER",
        Some(3.5),
        0.75,
        true,
        "WON",
    );
    for (selection, p, actual) in [
        ("HOME", 0.70, true),
        ("DRAW", 0.20, false),
        ("AWAY", 0.10, false),
    ] {
        add_backtest(
            &c,
            1,
            1,
            1,
            "MATCH_RESULT",
            selection,
            None,
            p,
            actual,
            if actual { "WON" } else { "LOST" },
        );
    }
    for (selection, p, actual) in [
        ("HOME", 0.60, false),
        ("DRAW", 0.30, true),
        ("AWAY", 0.10, false),
    ] {
        add_backtest(
            &c,
            1,
            1,
            20,
            "MATCH_RESULT",
            selection,
            None,
            p,
            actual,
            if actual { "WON" } else { "LOST" },
        );
    }

    c.execute("INSERT INTO prediction_runs(id,match_id,model_version_id,feature_engine_version,feature_schema_version,artifact_sha256,generated_at) VALUES(1,1,1,'fe_v1','schema_v1','hash','2025-12-01T12:00:00Z')",[]).unwrap();
    c.execute("INSERT INTO predictions(id,match_id,market,selection,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,availability,prediction_run_id) VALUES(1,1,'BTTS','YES',.7,'MODEL_OUTPUT','2025-12-01T18:00:00Z',1,.7,.7,'CALIBRATED_V1','cal_v1','AVAILABLE',1)",[]).unwrap();
    c.execute("INSERT INTO lineup_snapshots(id,match_id,provider,provider_event_id,captured_at,lineup_status,home_team_id,away_team_id,is_official,source_hash,completeness_status) VALUES(1,1,'fixture','event-1','2025-12-01T10:00:00Z','OFFICIAL',1,2,1,'lineup-hash','COMPLETE_OFFICIAL')",[]).unwrap();
    c.execute("INSERT INTO lineup_feature_sets(id,match_id,lineup_snapshot_id,feature_version,quality_status,home_resolution_count,away_resolution_count,home_starter_count,away_starter_count,historical_match_sample,minutes_coverage,position_coverage,features_json) VALUES(1,1,1,'lineup_features_v1','COMPLETE',11,11,11,11,100,1,1,'{}')",[]).unwrap();
    let payload = serde_json::json!({"payload_schema":"lineup_revision_payload_v1","markets":[{
        "market":"BTTS","selection":"YES","line":null,"base_raw_probability":0.7,
        "base_public_probability":0.7,"revised_raw_probability":0.6,
        "revised_calibrated_probability":0.6,"final_public_probability":0.6,
        "delta_percentage_points":-10.0,"family_status":"ACTIVE","calibration_status":"CALIBRATED",
        "fallback_reason":null}]})
    .to_string();
    c.execute("INSERT INTO prediction_revisions(id,prediction_id,match_id,revision_type,parent_prediction_id,lineup_snapshot_id,lineup_feature_set_id,generated_at,revision_reason,model_version,calibration_version,lineup_model_version,revision_status,base_model_version,base_calibration_version,payload_schema,market_payload_json) VALUES(1,1,1,'LINEUP_AWARE_PREMATCH',1,1,1,'2025-12-01T11:00:00Z','OFFICIAL','model_v1','cal_v1','lineup_v1','REVISION_AVAILABLE','model_v1','cal_v1','lineup_revision_payload_v1',?1)",[payload]).unwrap();
    c.execute("INSERT INTO candidate_engine_runs(id,business_date,timezone,generated_at,model_version,calibration_version,candidate_policy_version,feature_engine_version,odds_cutoff_at,configuration_hash,input_fingerprint,status,match_count_considered,prediction_context) VALUES(1,'2025-12-01','Europe/Istanbul','2025-12-01T12:00:00Z','model_v1','cal_v1','candidate_policy_v1','fe_v1','2025-12-01T12:00:00Z','cfg','candidate-fixture','COMPLETED',1,'BASE')",[]).unwrap();
    c.execute("INSERT INTO candidate_engine_candidates(id,run_id,prediction_id,match_id,competition_id,category,market,selection,raw_probability,public_probability,calibration_status,calibration_version,iddaa_odd,odds_snapshot_id,odds_captured_at,implied_probability,probability_edge,expected_value,data_quality,score,score_components_json,rank,same_match_group,correlation_type,compound_eligible,qualification_state,prediction_source,base_public_probability,final_public_probability) VALUES(1,1,1,1,1,'BTTS_YES','BTTS','YES',.7,.7,'CALIBRATED_V1','cal_v1',1.4,1,'2025-12-01T11:00:00Z',0.7142857142857143,-0.0142857142857143,-.02,'GOOD',.7,'{}',1,'1','BTTS_GOALS_CORRELATION',0,'QUALIFIED','BASE',.7,.7)",[]).unwrap();
    drop(c);
    db
}

fn request(window: subject::PerformanceWindow) -> subject::ModelPerformanceRequest {
    subject::ModelPerformanceRequest {
        window,
        market: Some("BTTS".into()),
        competition_id: None,
        prediction_context: Some(subject::PredictionContext::Base),
        model_version: Some("model_v1".into()),
        calibration_version: Some("cal_v1".into()),
        candidate_only: false,
        as_of: Some(AS_OF.into()),
    }
}

#[test]
fn settled_labels_and_all_time_windows_are_strict() {
    let db = fixture();
    let c = db.connection().unwrap();
    let all = subject::get(&c, &request(subject::PerformanceWindow::AllTime)).unwrap();
    let last30 = subject::get(&c, &request(subject::PerformanceWindow::Last30Days)).unwrap();
    let last7 = subject::get(&c, &request(subject::PerformanceWindow::Last7Days)).unwrap();
    let season = subject::get(&c, &request(subject::PerformanceWindow::Season)).unwrap();
    assert_eq!(
        (
            all.sample_count,
            last30.sample_count,
            last7.sample_count,
            season.sample_count
        ),
        (20, 16, 4, 20)
    );
    assert_eq!(last7.from_date.as_deref(), Some("2026-01-25"));
    assert_eq!(last30.from_date.as_deref(), Some("2026-01-02"));
    assert_eq!(all.to_date.as_deref(), Some("2026-01-28"));
}

#[test]
fn binary_metrics_buckets_and_ece_are_analytically_exact() {
    let db = fixture();
    let report = subject::get(
        &db.connection().unwrap(),
        &request(subject::PerformanceWindow::AllTime),
    )
    .unwrap();
    let metric = &report.by_market[0];
    assert_eq!(metric.sample_count, 20);
    assert_eq!(metric.status, subject::ReliabilityStatus::Sufficient);
    assert!((metric.average_predicted_probability.unwrap() - 0.7).abs() < 1e-12);
    assert!((metric.actual_hit_rate.unwrap() - 0.7).abs() < 1e-12);
    assert!((metric.brier_score.unwrap() - 0.21).abs() < 1e-12);
    let expected = -0.7f64 * 0.7f64.ln() - 0.3f64 * 0.3f64.ln();
    assert!((metric.log_loss.unwrap() - expected).abs() < 1e-12);
    assert!(metric.calibration_error_ece.unwrap().abs() < 1e-12);
    let bucket = report
        .confidence_buckets
        .iter()
        .find(|bucket| bucket.bucket == "70_79")
        .unwrap();
    assert_eq!(bucket.sample_count, 20);
    assert!((bucket.actual_hit_rate - 0.7).abs() < 1e-12);
    assert!(bucket.calibration_gap.abs() < 1e-12);
}

#[test]
fn match_result_uses_multiclass_metrics_and_accuracy() {
    let db = fixture();
    let mut r = request(subject::PerformanceWindow::AllTime);
    r.market = Some("MATCH_RESULT".into());
    let report = subject::get(&db.connection().unwrap(), &r).unwrap();
    let metric = &report.by_market[0];
    assert_eq!(metric.metric_type, subject::MetricType::Multiclass);
    assert_eq!(metric.sample_count, 2);
    assert_eq!(metric.actual_hit_rate, None);
    assert_eq!(metric.accuracy, Some(0.5));
    assert!((metric.brier_score.unwrap() - 0.5).abs() < 1e-12);
    assert!((metric.log_loss.unwrap() - ((-0.7f64.ln() - 0.3f64.ln()) / 2.)).abs() < 1e-12);
    assert_eq!(metric.status, subject::ReliabilityStatus::LowSample);
}

#[test]
fn league_odds_market_and_version_breakdowns_remain_labeled() {
    let db = fixture();
    let c = db.connection().unwrap();
    let mut r = request(subject::PerformanceWindow::AllTime);
    r.prediction_context = None;
    r.model_version = None;
    r.calibration_version = None;
    let report = subject::get(&c, &r).unwrap();
    assert!(report
        .by_competition
        .iter()
        .any(|x| x.competition_name.as_deref() == Some("Premier League") && x.sample_count == 10));
    assert!(report
        .by_competition
        .iter()
        .any(|x| x.competition_name.as_deref() == Some("La Liga")
            && x.model_version == "model_v1"
            && x.sample_count == 10));
    assert!(report
        .by_model_version
        .iter()
        .any(|x| x.model_version == "model_v2"
            && x.calibration_version.as_deref() == Some("cal_v2")
            && x.sample_count == 3));
    for band in [
        "1.01_1.29",
        "1.30_1.49",
        "1.50_1.79",
        "1.80_1.99",
        "2.00_2.49",
        "2.50_2.99",
        "3.00_PLUS",
    ] {
        assert!(report.by_odds_range.iter().any(|x| x.odds_range == band));
    }
    let mut all = request(subject::PerformanceWindow::AllTime);
    all.market = None;
    let all = subject::get(&c, &all).unwrap();
    for market in [
        "MATCH_RESULT",
        "BTTS",
        "TOTAL_GOALS",
        "TEAM_TOTAL_GOALS",
        "FULL_TIME_TOTAL_CORNERS",
        "FULL_TIME_TOTAL_CARDS",
    ] {
        assert!(all
            .by_market
            .iter()
            .any(|summary| summary.market.as_deref() == Some(market)));
    }
    let buckets = all
        .confidence_buckets
        .iter()
        .map(|bucket| bucket.bucket.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        buckets.contains("BELOW_50")
            && buckets.contains("50_59")
            && buckets.contains("60_69")
            && buckets.contains("80_89")
            && buckets.contains("90_100")
    );
}

#[test]
fn lineup_context_and_paired_comparison_are_explicit() {
    let db = fixture();
    let mut r = request(subject::PerformanceWindow::AllTime);
    r.prediction_context = Some(subject::PredictionContext::LineupAware);
    let report = subject::get(&db.connection().unwrap(), &r).unwrap();
    assert_eq!(report.sample_count, 1);
    assert_eq!(
        report.overall[0].prediction_context,
        subject::PredictionContext::LineupAware
    );
    assert_eq!(
        report.overall[0].lineup_model_version.as_deref(),
        Some("lineup_v1")
    );
    assert_eq!(report.overall[0].average_predicted_probability, Some(0.6));
    let comparison = &report.base_vs_lineup[0];
    assert_eq!(comparison.paired_sample_count, 1);
    assert_eq!(
        comparison.status,
        subject::ReliabilityStatus::InsufficientData
    );
    assert_eq!(comparison.base.average_predicted_probability, Some(0.7));
    assert_eq!(
        comparison.lineup_aware.average_predicted_probability,
        Some(0.6)
    );
    assert!(comparison.brier_delta_lineup_minus_base.is_some());
}

#[test]
fn candidate_subset_is_categorized_read_only_and_zero_samples_are_safe() {
    let db = fixture();
    let c = db.connection().unwrap();
    let before:(i64,i64,i64)=c.query_row("SELECT (SELECT COUNT(*) FROM predictions),(SELECT COUNT(*) FROM candidate_engine_candidates),(SELECT COUNT(*) FROM matches)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    let mut r = request(subject::PerformanceWindow::AllTime);
    r.candidate_only = true;
    let report = subject::get(&c, &r).unwrap();
    assert_eq!(report.sample_count, 1);
    assert_eq!(
        report.candidate_policy_summary[0].category.as_deref(),
        Some("BTTS_YES")
    );
    assert_eq!(
        report.candidate_policy_summary[0].metric_type,
        subject::MetricType::BinarySelection
    );
    let after:(i64,i64,i64)=c.query_row("SELECT (SELECT COUNT(*) FROM predictions),(SELECT COUNT(*) FROM candidate_engine_candidates),(SELECT COUNT(*) FROM matches)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(before, after);
    r.model_version = Some("missing".into());
    let empty = subject::get(&c, &r).unwrap();
    assert_eq!(empty.status, subject::ReliabilityStatus::InsufficientData);
    assert_eq!(empty.sample_count, 0);
}

#[test]
fn command_json_contract_has_no_financial_fields() {
    let parsed: subject::ModelPerformanceRequest = serde_json::from_value(serde_json::json!({
        "window":"LAST_30_DAYS","market":"BTTS","predictionContext":"BASE",
        "candidateOnly":false,"asOf":AS_OF}))
    .unwrap();
    assert_eq!(parsed.window, subject::PerformanceWindow::Last30Days);
    let db = fixture();
    let response = subject::get(&db.connection().unwrap(), &parsed).unwrap();
    let value = serde_json::to_value(&response).unwrap();
    fn keys(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    out.push(key.to_ascii_lowercase());
                    keys(value, out)
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    keys(value, out)
                }
            }
            _ => {}
        }
    }
    let mut fields = Vec::new();
    keys(&value, &mut fields);
    for forbidden in [
        "roi",
        "profit",
        "financial_loss",
        "pnl",
        "yield",
        "bankroll",
        "stake",
        "popularity",
    ] {
        assert!(!fields.iter().any(|field| field == forbidden));
    }
    let json = serde_json::to_string(&value).unwrap();
    assert!(json.contains("\"brier_score\"") && json.contains("\"calibration_error_ece\""));
}
