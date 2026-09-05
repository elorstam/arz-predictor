use super::{features, prediction_engine as pe};
use crate::database::Database;
use chrono::{Duration, TimeZone, Utc};
use rusqlite::params;
use std::{fs, time::Instant};

pub(crate) fn build_universe() -> (Database, i64) {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    c.execute("INSERT INTO competitions(id,name,country,current_season) VALUES(1,'ML League','TR','2025')", []).unwrap();
    for id in 1..=10 {
        c.execute(
            "INSERT INTO teams(id,normalized_name,country) VALUES(?1,?2,'TR')",
            params![id, format!("Team {id}")],
        )
        .unwrap();
    }
    let start = Utc.with_ymd_and_hms(2024, 1, 1, 15, 0, 0).unwrap();
    for i in 0..150i64 {
        let home = i % 10 + 1;
        let mut away = (i * 3 + 4) % 10 + 1;
        if away == home {
            away = away % 10 + 1
        }
        let kick = (start + Duration::days(i)).to_rfc3339();
        let date = (start + Duration::days(i)).format("%Y-%m-%d").to_string();
        let hg = (i * 7 + i / 3) % 5;
        let ag = (i * 5 + i / 4 + 1) % 4;
        c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,final_home_goals,final_away_goals,scheduled_local_date,kickoff_time_known) VALUES(?1,1,'2025',?2,?3,?4,'finished',?5,?6,?7,1)",params![i+1,home,away,kick,hg,ag,date]).unwrap();
        c.execute("INSERT INTO match_statistics(match_id,home_shots,away_shots,home_shots_on_target,away_shots_on_target,home_corners,away_corners,home_fouls,away_fouls,home_yellow_cards,away_yellow_cards,home_red_cards,away_red_cards) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,0,0)",params![i+1,8+i%8,7+(i*2)%7,3+i%5,2+(i*3)%5,3+i%8,2+(i*3)%7,8+i%10,9+(i*2)%9,1+i%5,1+(i*2)%4]).unwrap();
    }
    let upcoming = 1000;
    let kick = (start + Duration::days(151)).to_rfc3339();
    let date = (start + Duration::days(151)).format("%Y-%m-%d").to_string();
    c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(?1,1,'2025',1,2,?2,'scheduled',?3,1)",params![upcoming,kick,date]).unwrap();
    features::generate_dataset(&c, None, None).unwrap();
    drop(c);
    (db, upcoming)
}

fn probability(
    result: &pe::PredictionResult,
    market: &str,
    line: Option<f64>,
    selection: &str,
) -> f64 {
    result
        .markets
        .iter()
        .find(|m| m.market_type == market && m.line == line && m.selection == selection)
        .unwrap()
        .model_probability
        .unwrap()
}

#[test]
fn phase_6_registry_preprocessing_and_math_contracts() {
    assert_eq!(pe::FEATURE_SCHEMA_VERSION, "pred_features_v1");
    assert_eq!(pe::FEATURE_NAMES.len(), 36);
    for forbidden in [
        "match_id",
        "team_id",
        "odd",
        "popularity",
        "label",
        "provider",
    ] {
        assert!(!pe::FEATURE_NAMES.iter().any(|v| v.contains(forbidden)));
    }
    let train = vec![
        vec![Some(1.0), None, Some(9.0)],
        vec![Some(3.0), Some(4.0), Some(9.0)],
        vec![Some(5.0), Some(6.0), Some(9.0)],
    ];
    // Use registry-width rows while proving train-only median and zero-variance handling.
    let rows: Vec<Vec<Option<f64>>> = train
        .into_iter()
        .map(|v| {
            (0..pe::FEATURE_NAMES.len())
                .map(|i| if i < v.len() { v[i] } else { Some(i as f64) })
                .collect()
        })
        .collect();
    let p = pe::fit_preprocessing(&rows);
    assert_eq!(
        p.rules.iter().find(|r| r.source_index == 0).unwrap().median,
        3.0
    );
    assert!(p
        .dropped_all_null_or_zero_variance
        .contains(&"elo_difference".to_string()));
    let x = pe::transform(&p, &vec![None; pe::FEATURE_NAMES.len()]);
    assert!(x.iter().all(|v| v.is_finite()));
    assert!(x.iter().any(|v| *v == 1.0));
    let (one, g) = pe::goal_distribution(1.7, 1.1);
    assert!((one[0].iter().sum::<f64>() - 1.0).abs() < 1e-10);
    assert!(g["1.5"] >= g["2.5"] && g["2.5"] >= g["3.5"]);
    assert!(g["home_0.5"] >= g["home_1.5"] && g["away_0.5"] >= g["away_1.5"]);
}

#[test]
fn phase_6_controlled_model_acceptance() {
    let (db, upcoming) = build_universe();
    let c = db.connection().unwrap();
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let training_started = Instant::now();
    let report = pe::train(&c, "arz_glm_v1_acceptance", a.path()).unwrap();
    let training_time = training_started.elapsed();
    assert_eq!(report.rows_considered, 150);
    assert_eq!(report.split.train, 105);
    assert_eq!(report.split.validation, 22);
    assert_eq!(report.split.test, 23);
    assert!(report.metrics.match_log_loss.is_finite());
    assert!(report.metrics.btts_log_loss.is_finite());
    let load_started = Instant::now();
    let first = pe::load_artifact(&pe::artifact_path(a.path())).unwrap();
    let load_time = load_started.elapsed();
    fs::copy(pe::artifact_path(a.path()), pe::artifact_path(b.path())).unwrap();
    let second = pe::load_artifact(&pe::artifact_path(b.path())).unwrap();
    let snapshot = features::generate(&c, upcoming).unwrap();
    let p1 = pe::infer(&first, &snapshot).unwrap();
    let p2 = pe::infer(&second, &snapshot).unwrap();
    assert_eq!(p1.markets, p2.markets);
    assert!(
        (probability(&p1, "MATCH_RESULT", None, "HOME")
            + probability(&p1, "MATCH_RESULT", None, "DRAW")
            + probability(&p1, "MATCH_RESULT", None, "AWAY")
            - 1.0)
            .abs()
            < 1e-10
    );
    for market in [
        "TOTAL_GOALS",
        "FULL_TIME_TOTAL_CORNERS",
        "FULL_TIME_TOTAL_CARDS",
    ] {
        let mut over: Vec<_> = p1
            .markets
            .iter()
            .filter(|m| m.market_type == market && m.selection == "OVER")
            .map(|m| m.model_probability.unwrap())
            .collect();
        assert!(over.windows(2).all(|v| v[0] >= v[1]));
        over.clear();
    }
    for market in ["FIRST_HALF_TOTAL_CORNERS", "FIRST_HALF_TOTAL_CARDS"] {
        let m = p1.markets.iter().find(|m| m.market_type == market).unwrap();
        assert_eq!(m.model_probability, None);
        assert_eq!(m.availability, "SOURCE_NOT_AVAILABLE");
    }
    let mut changed = snapshot.clone();
    changed.home_elo += 350.0;
    changed.elo_difference += 350.0;
    changed.expected_home_result = 0.9;
    let p3 = pe::infer(&first, &changed).unwrap();
    assert_ne!(p1.markets, p3.markets);
    let t = Instant::now();
    for _ in 0..100 {
        pe::infer(&second, &snapshot).unwrap();
    }
    let infer = t.elapsed();
    let id = pe::register(&c, &pe::artifact_path(a.path())).unwrap();
    pe::activate(&c, "arz_glm_v1_acceptance").unwrap();
    let run1 = pe::persist_predictions(&c, &p1, id, &snapshot.cutoff_at).unwrap();
    let run2 = pe::persist_predictions(&c, &p1, id, &snapshot.cutoff_at).unwrap();
    assert_eq!(run1, run2);
    let persisted = pe::persisted_prediction_run(&c, run1).unwrap();
    assert_eq!(persisted.base_home_lambda, p1.base_home_lambda);
    assert_eq!(persisted.base_away_lambda, p1.base_away_lambda);
    assert!(persisted.base_home_lambda.unwrap() > 0.0);
    assert!(persisted.base_away_lambda.unwrap() > 0.0);
    let rows: i64 = c
        .query_row("SELECT COUNT(*) FROM prediction_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1);
    let before = p1.clone();
    c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,alternative_odd,captured_at) VALUES(?1,'iddaa','1X2','Result','HOME',2.0,1.9,'2024-01-01T00:00:00Z')",[upcoming]).unwrap();
    c.execute("INSERT INTO popularity_snapshots(provider,match_id,provider_event_id,market_type,selection,metric_type,metric_value,raw_metric_name,captured_at) VALUES('iddaa',?1,'future','MATCH_RESULT','HOME','COUNT',999,'count','2024-01-01T00:00:00Z')",[upcoming]).unwrap();
    let after = pe::infer(&second, &snapshot).unwrap();
    assert_eq!(before.markets, after.markets);
    let report2 = pe::train(&c, "arz_glm_v1_acceptance", b.path()).unwrap();
    assert_eq!(report.artifact_sha256, report2.artifact_sha256);
    println!("PHASE6_ACCEPTANCE data competitions=1 teams=10 finished=150 features=150 labels=300 split={}/{}/{} artifact={} bytes={} training_ms={} artifact_load_us={} infer_100_us={} metrics={:?} example={}",report.split.train,report.split.validation,report.split.test,report.artifact_sha256,fs::metadata(pe::artifact_path(a.path())).unwrap().len(),training_time.as_millis(),load_time.as_micros(),infer.as_micros(),report.metrics,serde_json::to_string(&p1).unwrap());
}

#[test]
fn artifact_integrity_and_registry_reject_corruption() {
    let (db, _) = build_universe();
    let c = db.connection().unwrap();
    let dir = tempfile::tempdir().unwrap();
    pe::train(&c, "integrity_v1", dir.path()).unwrap();
    let path = pe::artifact_path(dir.path());
    assert!(pe::validate_artifact(&path).valid);
    let mut text = fs::read_to_string(&path).unwrap();
    text = text.replace("ARZ Predictor", "ARX Predictor");
    fs::write(&path, text).unwrap();
    assert!(!pe::validate_artifact(&path).valid);
    assert!(pe::register(&c, &path).is_err());
}

#[test]
fn insufficient_training_and_runtime_are_explicitly_safe() {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    let dir = tempfile::tempdir().unwrap();
    assert!(pe::train(&c, "tiny", dir.path())
        .unwrap_err()
        .contains("INSUFFICIENT_TRAINING_DATA"));
    let status = pe::status(&c).unwrap();
    assert_eq!(status.active_model_version, None);
    assert!(!status.artifact_valid);
    let source = include_str!("prediction_engine.rs");
    for forbidden in [
        "reqwest",
        "Command::new",
        "python",
        "odds_snapshots",
        "popularity_snapshots",
    ] {
        assert!(
            !source.contains(forbidden),
            "runtime source contains forbidden dependency/query: {forbidden}"
        )
    }
}

#[test]
fn migration_0009_and_prediction_run_constraints_exist() {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    let max: i64 = c
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(max, 20);
    for table in ["prediction_runs"] {
        let n: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1)
    }
    let columns: Vec<String> = c
        .prepare("PRAGMA table_info(model_versions)")
        .unwrap()
        .query_map([], |r| r.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for name in [
        "artifact_path",
        "artifact_sha256",
        "training_cutoff",
        "feature_engine_version",
        "feature_schema_version",
        "label_versions_json",
        "metrics_json",
        "registry_status",
        "is_active",
    ] {
        assert!(columns.contains(&name.to_string()))
    }
    let err = c.execute(
        "INSERT INTO prediction_runs(match_id,model_version_id,feature_engine_version,feature_schema_version,artifact_sha256,base_home_lambda,base_away_lambda) VALUES(1,1,'fe_v1','pred_features_v1','invalid-lambda',0,-1)",
        [],
    );
    assert!(err.is_err());
}
