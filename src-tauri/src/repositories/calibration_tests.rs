use super::{calibration as cal, features, prediction_engine as pe};
use crate::database::Database;

fn obs(p: f64, actual: bool) -> cal::BacktestObservation {
    cal::BacktestObservation {
        match_id: 1,
        cutoff: "2025-01-01T00:00:00Z".into(),
        market: "BTTS".into(),
        line: None,
        selection: "YES".into(),
        raw_probability: p,
        calibrated_probability: None,
        actual,
        settlement: if actual { "WON" } else { "LOST" }.into(),
        fold: 0,
        out_of_sample: true,
        base_home_lambda: None,
        base_away_lambda: None,
    }
}

#[test]
fn migration_0010_and_oos_identity_are_present() {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    let max: i64 = c
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(max, 20);
    for table in [
        "backtest_runs",
        "backtest_predictions",
        "calibration_models",
        "backtest_calibration_predictions",
    ] {
        let n: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }
}

#[test]
fn buckets_metrics_and_threshold_are_deterministic() {
    let mut rows = Vec::new();
    for _ in 0..20 {
        rows.push(obs(0.62, true));
    }
    for _ in 0..20 {
        rows.push(obs(0.62, false));
    }
    let m = cal::metric(&rows, false);
    assert_eq!(m.sample_count, 40);
    assert!((m.observed_hit_rate - 0.5).abs() < 1e-12);
    assert!((m.mean_prediction - 0.62).abs() < 1e-12);
    assert!(m.ece.is_finite() && m.mce.is_finite());
    let b = cal::buckets(&rows, false);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].bucket, "60_64");
    assert_eq!(b[0].quality, "USABLE");
    assert!((b[0].observed_frequency - 0.5).abs() < 1e-12);
}

#[test]
fn walk_forward_is_chronological_and_calibration_is_portable() {
    let (db, _) = super::prediction_engine_tests::build_universe();
    let c = db.connection().unwrap();
    let report = cal::walk_forward(&c, "walk_v1").unwrap();
    assert!(report.out_of_sample);
    assert!(report.fold_count >= 5);
    assert_eq!(
        report.training_rows_by_fold[0],
        cal::MIN_INITIAL_TRAINING_ROWS
    );
    assert!(report
        .observations
        .iter()
        .all(|o| o.out_of_sample && o.fold < report.fold_count));
    assert!(report.observations.iter().all(|o| {
        o.base_home_lambda.is_some_and(|v| v.is_finite() && v > 0.0)
            && o.base_away_lambda.is_some_and(|v| v.is_finite() && v > 0.0)
    }));
    assert!(report
        .observations
        .windows(2)
        .all(|w| w[0].cutoff <= w[1].cutoff));
    let dir = tempfile::tempdir().unwrap();
    let base = pe::train(&c, "walk_base", dir.path()).unwrap();
    let artifact = pe::load_artifact(&pe::artifact_path(dir.path())).unwrap();
    let calibration_path = dir.path().join("calibration.json");
    let fitted = cal::fit_calibration(&artifact, &report, &calibration_path).unwrap();
    let loaded = cal::load_calibration(&calibration_path).unwrap();
    assert_eq!(fitted.artifact_sha256, loaded.artifact_sha256);
    assert_eq!(loaded.parent_artifact_sha256, base.artifact_sha256);
    println!("PHASE61_ACCEPTANCE folds={} oos={} training_rows={:?} evaluation={}..{} methods={:?} skipped={:?} raw={:?} calibrated={:?}", report.fold_count, report.oos_prediction_count, report.training_rows_by_fold, report.evaluation_start, report.evaluation_end, fitted.methods_fitted, fitted.skipped, fitted.raw_metrics, fitted.calibrated_metrics);
    assert!(loaded.feature_engine_version == "fe_v1");
    let mut predictions = pe::infer(&artifact, &features::generate(&c, 1000).unwrap()).unwrap();
    let raw = predictions.markets.clone();
    cal::apply(&mut predictions, &loaded);
    assert_eq!(raw.len(), predictions.markets.len());
    for m in &predictions.markets {
        if let Some(p) = m.model_probability {
            assert!((0.0..=1.0).contains(&p));
            assert_eq!(m.public_probability, Some(p));
            assert!(m.raw_probability.is_some());
        }
    }
    let one_x_two: Vec<f64> = predictions
        .markets
        .iter()
        .filter(|m| m.market_type == "MATCH_RESULT")
        .map(|m| m.model_probability.unwrap())
        .collect();
    assert_eq!(one_x_two.len(), 3);
    assert!((one_x_two.iter().sum::<f64>() - 1.0).abs() < 1e-10);
    let btts = predictions
        .markets
        .iter()
        .find(|m| m.market_type == "BTTS" && m.selection == "YES")
        .unwrap();
    assert_eq!(btts.calibration_status, "CALIBRATION_NOT_BENEFICIAL");
    assert_eq!(btts.public_probability, btts.raw_probability);
    let goals = predictions
        .markets
        .iter()
        .find(|m| m.market_type == "TOTAL_GOALS" && m.line == Some(2.5) && m.selection == "OVER")
        .unwrap();
    assert_eq!(goals.calibration_bucket.as_deref(), Some("70_74"));
    let mut insufficient = loaded.clone();
    insufficient.parameters.remove("BTTS");
    let mut fallback = pe::infer(&artifact, &features::generate(&c, 1000).unwrap()).unwrap();
    cal::apply(&mut fallback, &insufficient);
    let fallback_btts = fallback
        .markets
        .iter()
        .find(|m| m.market_type == "BTTS" && m.selection == "YES")
        .unwrap();
    assert_eq!(
        fallback_btts.public_probability,
        fallback_btts.raw_probability
    );
    let probability = |market: &str, line: f64, selection: &str| {
        predictions
            .markets
            .iter()
            .find(|m| m.market_type == market && m.line == Some(line) && m.selection == selection)
            .and_then(|m| m.public_probability)
            .unwrap()
    };
    assert!(probability("TOTAL_GOALS", 1.5, "OVER") >= probability("TOTAL_GOALS", 2.5, "OVER"));
    assert!(probability("TOTAL_GOALS", 2.5, "OVER") >= probability("TOTAL_GOALS", 3.5, "OVER"));
    for (market, lines) in [
        ("FULL_TIME_TOTAL_CORNERS", vec![7.5, 8.5, 9.5, 10.5, 11.5]),
        ("FULL_TIME_TOTAL_CARDS", vec![2.5, 3.5, 4.5, 5.5, 6.5]),
    ] {
        for pair in lines.windows(2) {
            assert!(probability(market, pair[0], "OVER") >= probability(market, pair[1], "OVER"));
        }
    }
    let copied_dir = tempfile::tempdir().unwrap();
    let copied_path = copied_dir.path().join("calibration-copy.json");
    std::fs::copy(&calibration_path, &copied_path).unwrap();
    let copied = cal::load_calibration(&copied_path).unwrap();
    let snapshot = features::generate(&c, 1000).unwrap();
    let mut copied_prediction = pe::infer(&artifact, &snapshot).unwrap();
    cal::apply(&mut copied_prediction, &copied);
    assert_eq!(predictions.markets, copied_prediction.markets);
    let pick = |market: &str, line: Option<f64>, selection: &str| {
        predictions
            .markets
            .iter()
            .find(|m| m.market_type == market && m.line == line && m.selection == selection)
            .and_then(|m| m.model_probability)
    };
    let raw_pick = |market: &str, line: Option<f64>, selection: &str| {
        raw.iter()
            .find(|m| m.market_type == market && m.line == line && m.selection == selection)
            .and_then(|m| m.model_probability)
    };
    println!("PHASE61_EXAMPLE raw_home={:?} calibrated_home={:?} raw_o25={:?} calibrated_o25={:?} raw_btts_yes={:?} calibrated_btts_yes={:?} raw_corners_o95={:?} calibrated_corners_o95={:?} raw_cards_o45={:?} calibrated_cards_o45={:?}", raw_pick("MATCH_RESULT", None, "HOME"), pick("MATCH_RESULT", None, "HOME"), raw_pick("TOTAL_GOALS", Some(2.5), "OVER"), pick("TOTAL_GOALS", Some(2.5), "OVER"), raw_pick("BTTS", None, "YES"), pick("BTTS", None, "YES"), raw_pick("FULL_TIME_TOTAL_CORNERS", Some(9.5), "OVER"), pick("FULL_TIME_TOTAL_CORNERS", Some(9.5), "OVER"), raw_pick("FULL_TIME_TOTAL_CARDS", Some(4.5), "OVER"), pick("FULL_TIME_TOTAL_CARDS", Some(4.5), "OVER"));
    let original = std::fs::read(&calibration_path).unwrap();
    for (label, mutate) in [
        (
            "numeric_parameter",
            (|v: &mut serde_json::Value| v["parameters"]["BTTS"]["a"] = serde_json::json!(0.123))
                as fn(&mut serde_json::Value),
        ),
        (
            "platt_slope",
            (|v: &mut serde_json::Value| v["parameters"]["BTTS"]["b"] = serde_json::json!(0.123))
                as fn(&mut serde_json::Value),
        ),
        (
            "temperature",
            (|v: &mut serde_json::Value| {
                v["parameters"]["MATCH_RESULT"]["temperature"] = serde_json::json!(1.9)
            }) as fn(&mut serde_json::Value),
        ),
        (
            "family_identifier",
            (|v: &mut serde_json::Value| {
                v["family_metadata"]["BTTS"] = serde_json::json!("TAMPERED")
            }) as fn(&mut serde_json::Value),
        ),
        (
            "parent_model_hash",
            (|v: &mut serde_json::Value| {
                v["parent_artifact_sha256"] = serde_json::json!("tampered")
            }) as fn(&mut serde_json::Value),
        ),
        (
            "missing_component",
            (|v: &mut serde_json::Value| {
                v["parameters"].as_object_mut().unwrap().remove("BTTS");
            }) as fn(&mut serde_json::Value),
        ),
    ] {
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        mutate(&mut value);
        std::fs::write(
            &calibration_path,
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        assert!(
            cal::load_calibration(&calibration_path).is_err(),
            "tampering must reject: {label}"
        );
        std::fs::write(&calibration_path, &original).unwrap();
        assert!(cal::load_calibration(&calibration_path).is_ok());
    }
    // Calibration/runtime source remains strictly local and does not inspect bookmaker tables.
    let source = include_str!("calibration.rs");
    assert!(!source.contains("reqwest"));
    assert!(!source.contains("odds_snapshots"));
    assert!(!source.contains("popularity_snapshots"));
}

#[test]
fn deliberately_miscalibrated_binary_and_multiclass_fixtures_are_legal() {
    let fit: Vec<_> = (0..100).map(|i| obs(0.85, i % 20 < 13)).collect();
    let eval: Vec<_> = (100..200).map(|i| obs(0.85, i % 20 < 13)).collect();
    let (a, b) = super::calibration::platt(
        &fit.iter()
            .map(|o| (o.raw_probability, o.actual))
            .collect::<Vec<_>>(),
    );
    let calibrated: Vec<_> = eval
        .iter()
        .cloned()
        .map(|mut o| {
            o.calibrated_probability =
                Some(super::calibration::cal_binary(o.raw_probability, a, b));
            o
        })
        .collect();
    let raw = super::calibration::metric(&eval, false);
    let public = super::calibration::metric(&calibrated, true);
    assert!(public.ece < raw.ece);
    assert!(public.brier <= raw.brier || public.log_loss <= raw.log_loss);
    println!("PHASE61_MISCALIBRATED_BINARY raw_ece={} calibrated_ece={} raw_brier={} calibrated_brier={} raw_logloss={} calibrated_logloss={}", raw.ece, public.ece, raw.brier, public.brier, raw.log_loss, public.log_loss);

    let raw = [0.85, 0.10, 0.05];
    let calibrated = super::calibration::temperature_probabilities(raw, 2.0);
    assert!(calibrated
        .iter()
        .all(|p| p.is_finite() && (0.0..=1.0).contains(p)));
    assert!((calibrated.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    assert!(calibrated[0] < raw[0]);
}

#[test]
fn backtest_persistence_is_immutable_and_explicitly_oos() {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    c.execute("INSERT INTO competitions(id,name) VALUES(1,'Test')", [])
        .unwrap();
    c.execute(
        "INSERT INTO teams(id,normalized_name) VALUES(1,'A'),(2,'B')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,final_home_goals,final_away_goals) VALUES(1,1,'s',1,2,'2025-01-01T00:00:00Z','finished',1,0)",[]).unwrap();
    let report = cal::BacktestReport {
        run_id: None,
        model_version: "m".into(),
        feature_engine_version: "fe_v1".into(),
        fold_count: 1,
        training_rows_by_fold: vec![100],
        oos_prediction_count: 1,
        evaluation_start: "2025-01-01".into(),
        evaluation_end: "2025-01-01".into(),
        out_of_sample: true,
        markets: vec![],
        confusion_matrix: Default::default(),
        configuration: [("oos_only".into(), "true".into())].into_iter().collect(),
        result_hash: "hash".into(),
        observations: vec![obs(0.7, true)],
    };
    let id = cal::persist_backtest(&c, &report).unwrap();
    assert_eq!(
        c.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM backtest_predictions WHERE run_id=?1 AND out_of_sample=1",
            [id],
            |r| r.get(0)
        )
        .unwrap(),
        1
    );
    assert!(c
        .execute(
            "UPDATE backtest_predictions SET raw_probability=0.1 WHERE run_id=?1",
            [id]
        )
        .is_err());
}

#[test]
fn calibrated_backtest_results_persist_idempotently_and_version_history_is_preserved() {
    let (db, _) = super::prediction_engine_tests::build_universe();
    let c = db.connection().unwrap();
    let report = cal::walk_forward(&c, "persist_base").unwrap();
    let run_id = cal::persist_backtest(&c, &report).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let _base = pe::train(&c, "persist_base_model", dir.path()).unwrap();
    let base_artifact = pe::load_artifact(&pe::artifact_path(dir.path())).unwrap();
    let cal1_path = dir.path().join("calibration-v1.json");
    let cal1 = cal::fit_calibration(&base_artifact, &report, &cal1_path).unwrap();
    let id1 = cal::persist_calibration_for_run(&c, &cal1, &base_artifact, run_id).unwrap();
    let id1_again = cal::persist_calibration_for_run(&c, &cal1, &base_artifact, run_id).unwrap();
    assert_eq!(id1, id1_again);
    let rows: i64 = c.query_row("SELECT COUNT(*) FROM backtest_calibration_predictions WHERE run_id=?1 AND calibration_model_id=?2", rusqlite::params![run_id,id1], |r| r.get(0)).unwrap();
    assert_eq!(rows as usize, report.observations.len());
    let first = &report.observations[0];
    let raw_before: f64 = c.query_row("SELECT raw_probability FROM backtest_predictions WHERE run_id=?1 AND match_id=?2 AND market=?3 AND selection=?4", rusqlite::params![run_id,first.match_id,first.market,first.selection], |r| r.get(0)).unwrap();
    let raw_after: f64 = c.query_row("SELECT raw_probability FROM backtest_calibration_predictions WHERE run_id=?1 AND calibration_model_id=?2 AND match_id=?3 AND market=?4 AND selection=?5", rusqlite::params![run_id,id1,first.match_id,first.market,first.selection], |r| r.get(0)).unwrap();
    assert_eq!(raw_before, raw_after);
    let cal2_path = dir.path().join("calibration-v2.json");
    let cal2 = cal::fit_calibration_version(
        &base_artifact,
        &report,
        &cal2_path,
        Some("persist_base_cal2"),
    )
    .unwrap();
    let id2 = cal::persist_calibration_for_run(&c, &cal2, &base_artifact, run_id).unwrap();
    assert_ne!(id1, id2);
    assert_eq!(
        c.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM calibration_models WHERE parent_model_version=?1",
            [&base_artifact.bundle.model_version],
            |r| r.get(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        c.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM backtest_calibration_predictions WHERE run_id=?1",
            [run_id],
            |r| r.get(0)
        )
        .unwrap(),
        (report.observations.len() * 2) as i64
    );
    assert_eq!(
        c.query_row::<String, _, _>(
            "SELECT calibration_version FROM calibration_models WHERE id=?1",
            [id1],
            |r| r.get(0)
        )
        .unwrap(),
        "persist_base_model_cal1"
    );
    assert_eq!(
        c.query_row::<String, _, _>(
            "SELECT calibration_version FROM calibration_models WHERE id=?1",
            [id2],
            |r| r.get(0)
        )
        .unwrap(),
        "persist_base_cal2"
    );
}
