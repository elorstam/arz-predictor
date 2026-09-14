use super::{candidate_engine, candidate_engine_acceptance_tests, coupon_engine};
use crate::database::Database;
use rusqlite::params;

fn fixture() -> Database {
    fixture_with_odd(2.60)
}

fn midnight_fixture() -> Database {
    midnight_fixture_at(false)
}

fn midnight_fixture_at(live: bool) -> Database {
    let db = fixture_with_odd(1.3);
    let c = db.connection().unwrap();
    c.execute("UPDATE matches SET kickoff_at='2026-09-02T17:00:00Z'", [])
        .unwrap();
    let (date, generated) = if live {
        let now = chrono::Utc::now();
        let generated = (now - chrono::Duration::minutes(2)).to_rfc3339();
        let kickoff = (now - chrono::Duration::minutes(1)).to_rfc3339();
        let cutoff = (now - chrono::Duration::minutes(5)).to_rfc3339();
        let date = crate::business_clock::date();
        c.execute(
            "UPDATE matches SET scheduled_local_date=?1,kickoff_at=?2",
            params![date, kickoff],
        )
        .unwrap();
        c.execute(
            "INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability,created_at) SELECT match_id,market,selection,line_value,model_probability,confidence_bucket,?1,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability,?2 FROM predictions",
            params![kickoff, cutoff],
        )
        .unwrap();
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) SELECT match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,?1 FROM odds_snapshots", [&cutoff])
            .unwrap();
        c.execute(
            "INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json,calculated_at) SELECT match_id,feature_engine_version,?1,feature_json,data_quality_json,?1 FROM feature_sets",
            [&cutoff],
        )
        .unwrap();
        (date, generated)
    } else {
        ("2026-09-02".to_owned(), "2026-09-02T15:00:00Z".to_owned())
    };
    // Add BTTS before immutable candidate generation so category overlays are covered.
    c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability,created_at) SELECT match_id,'BTTS','YES',NULL,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability,created_at FROM predictions", []).unwrap();
    c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,captured_at) SELECT match_id,provider,'btts','BTTS','YES',odd,'BTTS','YES',captured_at FROM odds_snapshots", []).unwrap();
    let run = candidate_engine::generate(
        &c,
        &candidate_engine::GenerateRequest {
            business_date: Some(date),
            generation_time: Some(generated),
            category: None,
            dry_run: Some(false),
        },
    )
    .unwrap();
    c.execute(
        "UPDATE candidate_run_execution SET purpose='LIVE' WHERE run_id=?1",
        [run.run_id],
    )
    .unwrap();
    coupon_engine::generate_daily(
        &c,
        &coupon_engine::GenerateRequest {
            business_date: run.business_date,
            candidate_run_id: Some(run.run_id),
            coupon_type: None,
            unit_stake_cents: Some(100),
        },
    )
    .unwrap();
    let artifact = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/production/base-model.json");
    c.execute(
        "UPDATE model_versions SET artifact_path=?1",
        [artifact.to_str().unwrap()],
    )
    .unwrap();
    drop(c);
    db
}

#[test]
fn started_daily_snapshot_is_not_replaced_by_later_valid_candidates() {
    let db = midnight_fixture();
    let c = db.connection().unwrap();
    let original = coupon_engine::get_daily(&c, "2026-09-02", Some("DAILY_OVER_25"))
        .unwrap()
        .remove(0);
    c.execute("UPDATE matches SET kickoff_at='2026-09-02T20:30:00Z' WHERE id NOT IN (SELECT match_id FROM phase8_coupon_selections WHERE coupon_id=?1)",[original.id]).unwrap();
    let run = candidate_engine::generate(
        &c,
        &candidate_engine::GenerateRequest {
            business_date: Some("2026-09-02".into()),
            generation_time: Some("2026-09-02T17:05:00Z".into()),
            category: None,
            dry_run: Some(false),
        },
    )
    .unwrap();
    assert!(
        run.candidates
            .iter()
            .filter(|x| x.category == "OVER_25")
            .count()
            >= 3
    );
    c.execute(
        "UPDATE candidate_run_execution SET purpose='LIVE' WHERE run_id=?1",
        [run.run_id],
    )
    .unwrap();
    coupon_engine::generate_daily(
        &c,
        &coupon_engine::GenerateRequest {
            business_date: run.business_date,
            candidate_run_id: Some(run.run_id),
            coupon_type: Some("DAILY_OVER_25".into()),
            unit_stake_cents: None,
        },
    )
    .unwrap();
    assert_eq!(
        coupon_engine::get_daily(&c, "2026-09-02", Some("DAILY_OVER_25")).unwrap(),
        vec![original]
    );
}

#[test]
fn settled_katlama_card_remains_alongside_same_day_next_step() {
    let db = midnight_fixture();
    let c = db.connection().unwrap();
    let run_id:i64=c.query_row("SELECT candidate_run_id FROM daily_output_publications WHERE business_date='2026-09-02'",[],|r|r.get(0)).unwrap();
    let series = coupon_engine::start_series(&c, "2026-09-02", 10000).unwrap();
    let original = coupon_engine::generate_compound_step(&c, "2026-09-02", Some(run_id)).unwrap();
    c.execute("UPDATE matches SET status='finished',final_home_goals=0,final_away_goals=0 WHERE id IN (SELECT match_id FROM phase8_coupon_selections WHERE coupon_id=?1)",[original.id]).unwrap();
    c.execute(
        "UPDATE matches SET kickoff_at='2026-09-02T20:30:00Z' WHERE status='scheduled'",
        [],
    )
    .unwrap();
    super::coupon_settlement::run(&c).unwrap();
    let run = candidate_engine::generate(
        &c,
        &candidate_engine::GenerateRequest {
            business_date: Some("2026-09-02".into()),
            generation_time: Some("2026-09-02T19:00:00Z".into()),
            category: None,
            dry_run: Some(false),
        },
    )
    .unwrap();
    c.execute(
        "UPDATE candidate_run_execution SET purpose='LIVE' WHERE run_id=?1",
        [run.run_id],
    )
    .unwrap();
    coupon_engine::publish_active_compound(&c, "2026-09-02", run.run_id).unwrap();
    let visible = coupon_engine::get_daily(&c, "2026-09-02", Some("DAILY_COMPOUND")).unwrap();
    assert_eq!(visible.len(), 2);
    assert_eq!(
        visible
            .iter()
            .find(|x| x.id == original.id)
            .unwrap()
            .settlement_result
            .as_deref(),
        Some("LOST")
    );
    assert_eq!(
        visible
            .iter()
            .filter(|x| x.publication_status == "READY")
            .count(),
        1
    );
    assert_eq!(coupon_engine::series(&c, series.id).unwrap().reset_count, 1);
    coupon_engine::publish_active_compound(&c, "2026-09-02", run.run_id).unwrap();
    assert_eq!(
        coupon_engine::get_daily(&c, "2026-09-02", Some("DAILY_COMPOUND")).unwrap(),
        visible
    );
}

#[test]
fn published_daily_coupons_survive_kickoff_settlement_reopen_and_date_switch() {
    for home_goals in [0, 3] {
        let db = midnight_fixture();
        let c = db.connection().unwrap();
        let initial = coupon_engine::get_daily(&c, "2026-09-02", None).unwrap();
        assert!(initial.iter().any(|x| x.coupon_type == "DAILY_BTTS"));
        let ids: Vec<_> = initial.iter().map(|x| x.id).collect();
        // 20:05 Istanbul: candidate generation correctly drops started matches.
        let run = candidate_engine::generate(
            &c,
            &candidate_engine::GenerateRequest {
                business_date: Some("2026-09-02".into()),
                generation_time: Some("2026-09-02T17:05:00Z".into()),
                category: None,
                dry_run: Some(false),
            },
        )
        .unwrap();
        assert!(run.candidates.is_empty());
        c.execute(
            "UPDATE candidate_run_execution SET purpose='LIVE' WHERE run_id=?1",
            [run.run_id],
        )
        .unwrap();
        let request = coupon_engine::GenerateRequest {
            business_date: run.business_date,
            candidate_run_id: Some(run.run_id),
            coupon_type: None,
            unit_stake_cents: Some(100),
        };
        coupon_engine::generate_daily(&c, &request).unwrap();
        assert_eq!(
            coupon_engine::get_daily(&c, "2026-09-02", None)
                .unwrap()
                .iter()
                .map(|x| x.id)
                .collect::<Vec<_>>(),
            ids
        );
        c.execute(
            "UPDATE matches SET status='finished',final_home_goals=?1,final_away_goals=0",
            [home_goals],
        )
        .unwrap();
        super::coupon_settlement::run(&c).unwrap();
        let settled = coupon_engine::get_daily(&c, "2026-09-02", None).unwrap();
        assert_eq!(settled.iter().map(|x| x.id).collect::<Vec<_>>(), ids);
        assert!(settled.iter().all(|x| x.publication_status == "SETTLED"));
        let goals = settled
            .iter()
            .find(|x| x.coupon_type == "DAILY_OVER_25")
            .unwrap();
        assert_eq!(
            goals.settlement_result.as_deref(),
            Some(if home_goals == 0 { "LOST" } else { "WON" })
        );
        let count: i64 = c
            .query_row("SELECT COUNT(*) FROM phase8_coupons", [], |r| r.get(0))
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("restart.sqlite3");
        c.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
            .unwrap();
        let reopened = Database::open(path).unwrap();
        let rc = reopened.connection().unwrap();
        coupon_engine::generate_daily(&rc, &request).unwrap();
        assert_eq!(
            rc.query_row("SELECT COUNT(*) FROM phase8_coupons", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            count
        );
        assert_eq!(
            coupon_engine::get_daily(&rc, "2026-09-02", None).unwrap(),
            settled
        );
        assert!(coupon_engine::get_daily(&rc, "2026-09-03", None)
            .unwrap()
            .is_empty());
        assert!(coupon_engine::get_coupon(&rc, goals.id)
            .unwrap()
            .settlement_result
            .is_some());
    }
}

#[test]
#[ignore = "controlled desktop export; set ARZ_MIDNIGHT_ACCEPTANCE_DIR"]
fn export_midnight_persistence_desktop() {
    let root = std::path::PathBuf::from(std::env::var("ARZ_MIDNIGHT_ACCEPTANCE_DIR").unwrap());
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("football-predictor.sqlite3");
    assert!(!path.exists());
    midnight_fixture_at(std::env::var("ARZ_MIDNIGHT_LIVE_FIXTURE").as_deref() == Ok("1"))
        .connection()
        .unwrap()
        .execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    std::fs::write(root.join("first-run-bootstrap.json"),serde_json::json!({"version":crate::first_run::BOOTSTRAP_VERSION,"completed":[],"reports":{},"durations_ms":{},"started_at":"2026-09-02T15:00:00Z","completed_at":"2026-09-02T15:00:00Z","attempts":1,"error":null}).to_string()).unwrap();
}

// A real persisted coupon from yesterday, with today's independent, eligible selections.
fn katlama_live_fixture(no_combo: bool, step: i64) -> (Database, i64) {
    let db = fixture_with_odd(1.30);
    let c = db.connection().unwrap();
    let run = phase7_run(&c);
    c.execute("UPDATE candidate_run_execution SET purpose='LIVE'", [])
        .unwrap();
    let series = coupon_engine::start_series(&c, &run.business_date, 10000).unwrap();
    c.execute("UPDATE phase8_compound_series SET current_step=?1", [step])
        .unwrap();
    let coupon =
        coupon_engine::generate_compound_step(&c, &run.business_date, Some(run.run_id)).unwrap();
    let today = crate::business_clock::date();
    let now = chrono::Utc::now();
    let cutoff = (now - chrono::Duration::minutes(10)).to_rfc3339();
    let kickoff = (now + chrono::Duration::minutes(45)).to_rfc3339();
    c.execute("UPDATE matches SET scheduled_local_date=?1,kickoff_at=?2 WHERE id NOT IN (SELECT match_id FROM phase8_coupon_selections WHERE coupon_id=?3)", params![today,kickoff,coupon.id]).unwrap();
    c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability,created_at) SELECT match_id,market,selection,line_value,model_probability,confidence_bucket,?1,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability,?2 FROM predictions",params![kickoff,cutoff]).unwrap();
    c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) SELECT match_id,provider,market_code,market_name,selection,?1,normalized_market_type,normalized_selection,line_value,?2 FROM odds_snapshots",params![if no_combo {2.6} else {1.3},cutoff]).unwrap();
    let artifact = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/production/base-model.json");
    c.execute(
        "UPDATE model_versions SET artifact_path=?1",
        [artifact.to_str().unwrap()],
    )
    .unwrap();
    drop(c);
    (db, series.id)
}

#[test]
fn katlama_known_results_publish_next_step_in_the_same_flow_and_reopen_once() {
    for (outcome, no_combo, step, expected) in [
        ("WON", false, 1, 2),
        ("LOST", false, 1, 1),
        ("WON", true, 1, 2),
        ("PENDING", false, 1, 1),
        ("WON", false, 7, 1),
    ] {
        let (db, series_id) = katlama_live_fixture(no_combo, step);
        let c = db.connection().unwrap();
        let previous = coupon_engine::series(&c, series_id)
            .unwrap()
            .latest_coupon_id
            .unwrap();
        if outcome != "PENDING" {
            c.execute("UPDATE matches SET status='finished',final_home_goals=?1,final_away_goals=1 WHERE id IN (SELECT match_id FROM phase8_coupon_selections WHERE coupon_id=?2)",params![if outcome=="LOST" {0} else {2},previous]).unwrap();
        }
        super::coupon_settlement::run(&c).unwrap();
        let flow = super::current_flow::run(&c).unwrap();
        let value = coupon_engine::series(&c, series_id).unwrap();
        assert_eq!(
            value.current_step, expected,
            "{outcome}, no_combo={no_combo}"
        );
        assert_eq!(
            value.history[0].result,
            if outcome == "PENDING" {
                "UNSETTLED"
            } else {
                outcome
            }
        );
        let next_expected = outcome != "PENDING" && !no_combo;
        assert_eq!(
            value.history.len(),
            if next_expected { 2 } else { 1 },
            "{outcome}, no_combo={no_combo}; {flow:?}"
        );
        if next_expected {
            let next = coupon_engine::get_daily(
                &c,
                &crate::business_clock::date(),
                Some("DAILY_COMPOUND"),
            )
            .unwrap();
            assert_eq!(next.len(), 1);
            assert_eq!(next[0].publication_status, "READY");
            assert_eq!(next[0].step_number, Some(expected));
            assert!((2..=3).contains(&next[0].selections.len()));
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reopen.sqlite3");
        c.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
            .unwrap();
        let reopened = Database::open(path).unwrap();
        let rc = reopened.connection().unwrap();
        super::coupon_settlement::run(&rc).unwrap();
        super::current_flow::run(&rc).unwrap();
        assert_eq!(
            coupon_engine::series(&rc, series_id).unwrap(),
            value,
            "restart {outcome}"
        );
    }
}

#[test]
#[ignore = "controlled real desktop fixture export; set ARZ_KATLAMA_ACCEPTANCE_DIR"]
fn export_katlama_desktop_scenarios() {
    let root = std::path::PathBuf::from(std::env::var("ARZ_KATLAMA_ACCEPTANCE_DIR").unwrap());
    for case in ["win", "loss", "pending", "no-combo"] {
        let (db, _) = katlama_live_fixture(case == "no-combo", 1);
        let dir = root.join(case);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("football-predictor.sqlite3");
        assert!(!path.exists());
        db.connection()
            .unwrap()
            .execute("VACUUM INTO ?1", [path.to_str().unwrap()])
            .unwrap();
        // Explicit acceptance fixture, not a zero-to-ready test. No runtime bypass of settlement.
        std::fs::write(dir.join("first-run-bootstrap.json"),serde_json::json!({"version":crate::first_run::BOOTSTRAP_VERSION,"completed":[],"reports":{},"durations_ms":{},"started_at":"2026-09-14T00:00:00Z","completed_at":"2026-09-14T00:00:00Z","attempts":1,"error":null}).to_string()).unwrap();
    }
}

#[test]
fn automatic_coupon_outcome_without_stake_never_invents_realized_money() {
    let db = fixture();
    let c = db.connection().unwrap();
    let run = phase7_run(&c);
    c.execute(
        "UPDATE candidate_run_execution SET purpose='LIVE' WHERE run_id=?1",
        [run.run_id],
    )
    .unwrap();
    let coupon = coupon_engine::generate_daily(
        &c,
        &coupon_engine::GenerateRequest {
            business_date: run.business_date.clone(),
            candidate_run_id: Some(run.run_id),
            coupon_type: Some("DAILY_OVER_25".into()),
            unit_stake_cents: None,
        },
    )
    .unwrap()
    .remove(0);
    c.execute(
        "UPDATE matches SET status='finished',final_home_goals=2,final_away_goals=1",
        [],
    )
    .unwrap();
    assert!(super::coupon_settlement::run(&c).unwrap() > 0);
    let settled = coupon_engine::get_coupon(&c, coupon.id).unwrap();
    assert_eq!(settled.status, "SETTLED");
    assert!(settled.total_stake_cents.is_none());
    assert_eq!(settled.metadata["financial_state"], "STAKE_NOT_RECORDED");
    assert!(settled.metadata["settlement"].is_null());
    assert_eq!(
        c.query_row(
            "SELECT settlement_result FROM phase8_coupons WHERE id=?1",
            [coupon.id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "WON"
    );
    assert_eq!(super::coupon_settlement::run(&c).unwrap(), 0);
}

#[test]
fn automatic_corner_coupon_preserves_line_and_waits_for_actual_final_statistics() {
    let db = fixture();
    let c = db.connection().unwrap();
    c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json,calculated_at) SELECT match_id,'fe_v1','2026-09-02T11:01:00Z','{}',json_set(data_quality_json,'$.corners_coverage',1.0),'2026-09-02T11:01:00Z' FROM feature_sets",[]).unwrap();
    c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,bucket_sample_size,availability,created_at) SELECT match_id,'FULL_TIME_TOTAL_CORNERS','OVER',8.5,0.6,'corner',kickoff_at,model_version_id,0.6,0.6,'CALIBRATED_V1',25,'AVAILABLE',created_at FROM predictions",[]).unwrap();
    c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) SELECT id,'iddaa','corner','FULL_TIME_TOTAL_CORNERS','OVER',1.6,'FULL_TIME_TOTAL_CORNERS','OVER',8.5,'2026-09-02T10:00:00Z' FROM matches",[]).unwrap();
    let run = phase7_run(&c);
    c.execute(
        "UPDATE candidate_run_execution SET purpose='LIVE' WHERE run_id=?1",
        [run.run_id],
    )
    .unwrap();
    let coupon = coupon_engine::generate_daily(
        &c,
        &coupon_engine::GenerateRequest {
            business_date: run.business_date.clone(),
            candidate_run_id: Some(run.run_id),
            coupon_type: Some("DAILY_CORNERS".into()),
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap()
    .remove(0);
    c.execute(
        "UPDATE matches SET status='finished',final_home_goals=2,final_away_goals=1",
        [],
    )
    .unwrap();
    assert_eq!(super::coupon_settlement::run(&c).unwrap(), 0);
    let pending = coupon_engine::get_coupon(&c, coupon.id).unwrap();
    assert_ne!(pending.status, "SETTLED");
    assert!(pending.selections.iter().all(|s| s.line == Some(8.5)
        && s.selection == "OVER"
        && s.settlement_state == "PENDING_DATA"
        && s.settlement_reason.as_deref() == Some("FINAL_CORNERS_UNAVAILABLE")));
    c.execute("INSERT INTO match_statistics(match_id,home_corners,away_corners) SELECT id,5,4 FROM matches",[]).unwrap();
    assert!(super::coupon_settlement::run(&c).unwrap() > 0);
    assert_eq!(
        coupon_engine::get_coupon(&c, coupon.id).unwrap().metadata["settlement"]["status"],
        "WON"
    );
}

#[test]
fn automatic_final_scores_settle_published_coupons_and_drive_katlama() {
    for (home, away, step, expected, next) in [
        (2, 1, 1, "WON", 2),
        (0, 0, 1, "LOST", 1),
        (2, 1, 7, "WON", 1),
    ] {
        let db = fixture_with_odd(1.3);
        let c = db.connection().unwrap();
        let run = phase7_run(&c);
        c.execute(
            "UPDATE candidate_run_execution SET purpose='LIVE' WHERE run_id=?1",
            [run.run_id],
        )
        .unwrap();
        let coupon = coupon_engine::generate_daily(
            &c,
            &coupon_engine::GenerateRequest {
                business_date: run.business_date.clone(),
                candidate_run_id: Some(run.run_id),
                coupon_type: Some("DAILY_OVER_25".into()),
                unit_stake_cents: Some(1000),
            },
        )
        .unwrap()
        .remove(0);
        let series = coupon_engine::start_series(&c, &run.business_date, 1000).unwrap();
        c.execute(
            "UPDATE phase8_compound_series SET current_step=?2 WHERE id=?1",
            params![series.id, step],
        )
        .unwrap();
        let compound =
            coupon_engine::generate_compound_step(&c, &run.business_date, Some(run.run_id))
                .unwrap();
        c.execute("UPDATE matches SET status='cancelled'", [])
            .unwrap();
        assert_eq!(super::coupon_settlement::run(&c).unwrap(), 0);
        assert_eq!(
            coupon_engine::series(&c, series.id).unwrap().current_step,
            step as usize
        );
        assert!(
            c.query_row(
                "SELECT count(*) FROM coupon_selection_settlement_audit WHERE state='PENDING_DATA'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap()
                > 0
        );
        c.execute(
            "UPDATE matches SET status='finished',final_home_goals=?1,final_away_goals=?2",
            params![home, away],
        )
        .unwrap();
        assert!(super::coupon_settlement::run(&c).unwrap() > 0);
        for id in [coupon.id, compound.id] {
            assert_eq!(
                c.query_row(
                    "SELECT settlement_result FROM phase8_coupons WHERE id=?1",
                    [id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                expected
            );
        }
        assert_eq!(
            coupon_engine::series(&c, series.id).unwrap().current_step,
            next
        );
        assert_eq!(super::coupon_settlement::run(&c).unwrap(), 0);
        let saved = coupon_engine::get_coupon(&c, coupon.id).unwrap();
        assert_eq!(saved.combined_decimal_odd, coupon.combined_decimal_odd);
        assert_eq!(saved.selections[0].odd, coupon.selections[0].odd);
    }
}

#[test]
fn compound_search_finds_pairs_triples_and_rejects_correlation() {
    let db = fixture_with_odd(1.3);
    let c = db.connection().unwrap();
    let run = phase7_run(&c);
    let mut pool: Vec<_> = run
        .candidates
        .into_iter()
        .filter(|x| x.category == "COMPOUND")
        .take(3)
        .collect();
    assert_eq!(pool.len(), 3);
    pool[0].iddaa_odd = 1.2;
    pool[1].iddaa_odd = 1.5;
    let pair = coupon_engine::search_compound(&pool[..2]);
    assert_eq!(pair.pairs_evaluated, 1);
    assert!((pair.chosen.unwrap().combined_odd - 1.8).abs() < 1e-9);
    for p in &mut pool {
        p.iddaa_odd = 1.21;
    }
    let triple = coupon_engine::search_compound(&pool);
    assert_eq!(triple.pairs_evaluated, 3);
    assert_eq!(triple.triples_evaluated, 1);
    assert_eq!(triple.chosen.as_ref().unwrap().candidate_ids.len(), 3);
    assert!((triple.chosen.unwrap().combined_odd - 1.771561).abs() < 1e-9);
    pool[0].iddaa_odd = 1.2;
    pool[1].iddaa_odd = 1.5;
    pool[1].match_id = pool[0].match_id;
    let rejected = coupon_engine::search_compound(&pool[..2]);
    assert!(rejected.chosen.is_none());
    assert_eq!(rejected.correlated_rejected, 1);
    assert_eq!(rejected.reason, "ALL_COMBINATIONS_CORRELATED");
    pool[1].match_id += 100;
    pool[0].iddaa_odd = 1.1;
    pool[1].iddaa_odd = 1.1;
    let too_low = coupon_engine::search_compound(&pool[..2]);
    assert!(too_low.chosen.is_none());
    assert!((too_low.closest[3].1.as_ref().unwrap().combined_odd - 1.21).abs() < 1e-9);
}

#[test]
fn compound_accepts_validated_raw_fallback_without_weakening_sample_gate() {
    let db = fixture_with_odd(1.3);
    let c = db.connection().unwrap();
    c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability,created_at) SELECT match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,'CALIBRATION_NOT_BENEFICIAL',calibration_version,25,availability,'2026-09-02T11:01:00Z' FROM predictions",[]).unwrap();
    assert!(phase7_run(&c)
        .candidates
        .iter()
        .any(|x| x.category == "COMPOUND"));
    c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability,created_at) SELECT match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,0,availability,'2026-09-02T11:02:00Z' FROM predictions WHERE created_at='2026-09-02T11:01:00Z'",[]).unwrap();
    assert!(!phase7_run(&c)
        .candidates
        .iter()
        .any(|x| x.category == "COMPOUND"));
}

fn fixture_with_odd(odd: f64) -> Database {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    c.execute("INSERT INTO model_versions(version_identifier,model_name,registry_status,is_active) VALUES('m8','phase8','ACTIVE',1)",[]).unwrap();
    c.execute(
        "INSERT INTO competitions(name,country,current_season) VALUES('P8 A','TR','2026/27')",
        [],
    )
    .unwrap();
    for i in 0..14 {
        c.execute(
            "INSERT INTO teams(normalized_name) VALUES(?1),(?2)",
            params![format!("P8 Home {i}"), format!("P8 Away {i}")],
        )
        .unwrap();
        c.execute("INSERT INTO matches(competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(1,'2026/27',?1,?2,'2026-09-02T18:00:00Z','scheduled','2026-09-02',1)",params![i*2+1,i*2+2]).unwrap();
        let id = i + 1;
        c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json,calculated_at) VALUES(?1,'fe_v1','2026-09-02T11:00:00Z','{}',?2,'2026-09-02T10:00:00Z')",params![id, r#"{"history_matches_home":10,"history_matches_away":10}"#]).unwrap();
        c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability,created_at) VALUES(?1,'TOTAL_GOALS','OVER',2.5,0.80,'p8','2026-09-02T18:00:00Z',1,0.80,0.80,'CALIBRATED_V1','cal8',25,'AVAILABLE','2026-09-02T11:00:00Z')",[id]).unwrap();
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(?1,'iddaa','p8','TOTAL_GOALS','OVER',?2,'TOTAL_GOALS','OVER',2.5,'2026-09-02T10:00:00Z')",params![id, odd]).unwrap();
    }
    drop(c);
    db
}

fn phase7_run(c: &rusqlite::Connection) -> candidate_engine::DailyRun {
    candidate_engine::generate(
        c,
        &candidate_engine::GenerateRequest {
            business_date: Some("2026-09-02".into()),
            generation_time: Some("2026-09-02T12:00:00Z".into()),
            category: None,
            dry_run: Some(false),
        },
    )
    .unwrap()
}

#[test]
fn daily_candidates_recovery_shares_coupon_pools_and_ignores_empty_lineup() {
    use super::daily_selections;
    let db = fixture_with_odd(1.3);
    let c = db.connection().unwrap();
    c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json,calculated_at) SELECT match_id,'fe_v1','2026-09-02T11:01:00Z','{}',json_set(data_quality_json,'$.corners_coverage',1.0),'2026-09-02T11:01:00Z' FROM feature_sets",[]).unwrap();
    c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,bucket_sample_size,availability,created_at) SELECT match_id,'FULL_TIME_TOTAL_CORNERS','OVER',8.5,0.6,'corner',kickoff_at,model_version_id,0.6,0.6,'CALIBRATED_V1',25,'AVAILABLE',created_at FROM predictions",[]).unwrap();
    c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) SELECT id,'iddaa','corner','FULL_TIME_TOTAL_CORNERS','OVER',1.6,'FULL_TIME_TOTAL_CORNERS','OVER',8.5,'2026-09-02T10:00:00Z' FROM matches",[]).unwrap();
    let base = phase7_run(&c);
    coupon_engine::generate_daily(
        &c,
        &coupon_engine::GenerateRequest {
            business_date: base.business_date.clone(),
            candidate_run_id: Some(base.run_id),
            coupon_type: None,
            unit_stake_cents: None,
        },
    )
    .unwrap();
    let revision = candidate_engine::generate_lineup_revision(
        &c,
        &candidate_engine::LineupRevisionGenerateRequest {
            base_run_id: base.run_id,
            business_date: base.business_date.clone(),
        },
    )
    .unwrap();
    assert!(!candidate_engine::valid_lineup_run(&c, revision.run_id).unwrap());
    c.execute("INSERT INTO candidate_engine_runs(business_date,timezone,generated_at,model_version,candidate_policy_version,feature_engine_version,odds_cutoff_at,configuration_hash,input_fingerprint,status,match_count_considered,prediction_context,parent_candidate_run_id) SELECT business_date,timezone,'2026-09-02T13:00:00Z',model_version,candidate_policy_version,feature_engine_version,odds_cutoff_at,configuration_hash,'empty-newer-lineup','COMPLETED',0,'LINEUP_AWARE',id FROM candidate_engine_runs WHERE id=?1",[base.run_id]).unwrap();
    let empty_id = c.last_insert_rowid();
    // Reproduce a corrupt publication pointer to the newer empty comparison.
    c.execute(
        "UPDATE daily_output_publications SET candidate_run_id=?1 WHERE business_date=?2",
        params![empty_id, base.business_date],
    )
    .unwrap();
    let output = daily_selections::get(&c, &base.business_date).unwrap();
    assert_eq!(output.run.run.run_id, base.run_id);
    assert!(!output.run.lineup_verified);
    assert_eq!(output.run.run.prediction_context.as_deref(), Some("BASE"));
    for (kind, cat) in [
        ("DAILY_CORNERS", "CORNERS"),
        ("DAILY_HIGH_CONFIDENCE", "HIGH_CONFIDENCE"),
        ("DAILY_OVER_25", "OVER_25"),
    ] {
        let coupon = output
            .coupons
            .iter()
            .find(|x| x.coupon_type == kind)
            .unwrap_or_else(|| panic!("missing {kind}, counts {:?}", output.run.category_counts));
        assert_eq!(
            coupon.source_candidate_run_id,
            output.publication.category_run_ids[cat]
        );
        assert!(coupon.selections.iter().all(|s| output
            .run
            .run
            .candidates
            .iter()
            .any(|x| x.id == s.candidate_id && x.category == cat)));
    }
    assert!(output.run.category_counts["ALL"] < output.run.run.candidates.len());
    for x in &output.run.run.candidates {
        if x.category == "OVER_25" {
            assert_eq!(
                output.run.selection_sources[&x.id].selection_status,
                "DAILY_RANKED"
            );
            assert!(!output.run.selection_sources[&x.id].strict_qualified);
        }
    }
}

#[test]
fn daily_goal_top_seven_keeps_all_valid_low_probability_candidates_in_pool() {
    for (market, selection, line, category, kind) in [
        ("TOTAL_GOALS", "OVER", Some(2.5), "OVER_25", "DAILY_OVER_25"),
        ("BTTS", "YES", None, "BTTS_YES", "DAILY_BTTS"),
    ] {
        let db = fixture_with_odd(1.5);
        let c = db.connection().unwrap();
        c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,bucket_sample_size,availability,created_at) SELECT match_id,?1,?2,?3,0.58,'daily',kickoff_at,model_version_id,0.58,0.58,'UNCALIBRATED_V1',0,'AVAILABLE','2026-09-02T11:30:00Z' FROM predictions",params![market,selection,line]).unwrap();
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) SELECT id,'iddaa','daily',?1,?2,1.5,?1,?2,?3,'2026-09-02T11:00:00Z' FROM matches",params![market,selection,line]).unwrap();
        let run = phase7_run(&c);
        let pool: Vec<_> = run
            .candidates
            .iter()
            .filter(|x| x.category == category)
            .collect();
        assert_eq!(pool.len(), 14);
        assert!(pool
            .iter()
            .all(|x| x.public_probability < 0.64 && x.expected_value < 0.0));
        let publication = super::daily_selections::publication(&c, &run.business_date).unwrap();
        let visible = super::daily_selections::view(&c, &publication).unwrap();
        for x in &pool {
            assert_eq!(
                visible.selection_sources[&x.id].selection_status,
                "DAILY_RANKED"
            );
            assert!(!visible.selection_sources[&x.id].strict_qualified);
        }
        let coupons = coupon_engine::generate_daily(
            &c,
            &coupon_engine::GenerateRequest {
                business_date: run.business_date.clone(),
                candidate_run_id: Some(run.run_id),
                coupon_type: Some(kind.into()),
                unit_stake_cents: None,
            },
        )
        .unwrap();
        assert_eq!(coupons.len(), 1);
        assert_eq!(
            coupons[0].candidate_ids,
            pool.iter()
                .take(if kind == "DAILY_OVER_25" { 5 } else { 7 })
                .map(|x| x.id)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            coupons[0].selections.len(),
            if kind == "DAILY_OVER_25" { 5 } else { 7 }
        );
    }
}

#[test]
fn inspecting_coupon_without_lineups_preserves_base_coupons() {
    let db = fixture();
    let c = db.connection().unwrap();
    let run = phase7_run(&c);
    let coupons = coupon_engine::generate_daily(
        &c,
        &coupon_engine::GenerateRequest {
            business_date: run.business_date.clone(),
            candidate_run_id: Some(run.run_id),
            coupon_type: Some("DAILY_OVER_25".into()),
            unit_stake_cents: None,
        },
    )
    .unwrap();
    assert!(!coupons.is_empty());
    let before: i64 = c
        .query_row("SELECT COUNT(*) FROM candidate_engine_runs", [], |r| {
            r.get(0)
        })
        .unwrap();
    let impact = coupon_engine::lineup_revision_impact(
        &c,
        &coupon_engine::LineupRevisionImpactRequest {
            coupon_id: coupons[0].id,
        },
    )
    .unwrap();
    assert!(impact.latest_lineup_candidate_run_id.is_none());
    let after: i64 = c
        .query_row("SELECT COUNT(*) FROM candidate_engine_runs", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(before, after);
    // An older read-side comparison run must not suppress a valid BASE draft either.
    candidate_engine::generate_lineup_revision(
        &c,
        &candidate_engine::LineupRevisionGenerateRequest {
            base_run_id: run.run_id,
            business_date: run.business_date.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        coupon_engine::get_daily(&c, &run.business_date, None)
            .unwrap()
            .len(),
        coupons.len()
    );
}

fn clone_candidate_for_conflict(
    c: &rusqlite::Connection,
    source_id: i64,
    category: &str,
    market: &str,
    selection: &str,
    line: Option<f64>,
    odd: f64,
) -> i64 {
    let (_run_id, match_id): (i64, i64) = c
        .query_row(
            "SELECT run_id,match_id FROM candidate_engine_candidates WHERE id=?1",
            [source_id],
            |x| Ok((x.get(0)?, x.get(1)?)),
        )
        .unwrap();
    c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,calibration_bucket,bucket_sample_size,availability,created_at) VALUES(?1,?2,?3,?4,0.80,'phase8-conflict','2026-09-02T12:00:00Z',1,0.80,0.80,'CALIBRATED_V1','cal_v1','0.8',25,'AVAILABLE','2026-09-02T11:00:00Z')", rusqlite::params![match_id, market, selection, line]).unwrap();
    let prediction_id = c.last_insert_rowid();
    c.execute("INSERT INTO candidate_engine_candidates(run_id,prediction_id,match_id,competition_id,category,market,selection,line_value,raw_probability,public_probability,calibration_status,calibration_version,calibration_bucket,bucket_observed_rate,bucket_sample_size,bucket_calibration_gap,iddaa_odd,odds_snapshot_id,odds_captured_at,implied_probability,probability_edge,expected_value,data_quality,score,score_components_json,rank,same_match_group,correlation_type,compound_eligible,qualification_state) SELECT run_id,?2,match_id,competition_id,?3,?4,?5,?6,raw_probability,public_probability,calibration_status,calibration_version,calibration_bucket,bucket_observed_rate,bucket_sample_size,bucket_calibration_gap,?7,odds_snapshot_id,odds_captured_at,1.0/?7,public_probability-1.0/?7,public_probability*?7-1.0,data_quality,score,score_components_json,?8,same_match_group,correlation_type,compound_eligible,'QUALIFIED' FROM candidate_engine_candidates WHERE id=?1", rusqlite::params![source_id, prediction_id, category, market, selection, line, odd, 1000 + prediction_id]).unwrap();
    c.last_insert_rowid()
}

#[test]
fn phase8_system_snapshot_and_katlama_state_contract() {
    let db = fixture();
    let c = db.connection().unwrap();
    let run = phase7_run(&c);
    let generated = coupon_engine::generate_daily(
        &c,
        &coupon_engine::GenerateRequest {
            business_date: "2026-09-02".into(),
            candidate_run_id: Some(run.run_id),
            coupon_type: Some("DAILY_SURPRISE_SYSTEM".into()),
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap();
    assert_eq!(generated.len(), 1);
    let coupon = &generated[0];
    assert_eq!(coupon.system_sizes, vec![5, 6, 7]);
    assert_eq!(coupon.columns.len(), 29);
    assert_eq!(
        coupon.columns.iter().filter(|x| x.system_size == 5).count(),
        21
    );
    assert_eq!(
        coupon.columns.iter().filter(|x| x.system_size == 6).count(),
        7
    );
    assert_eq!(
        coupon.columns.iter().filter(|x| x.system_size == 7).count(),
        1
    );
    assert_eq!(coupon.total_stake_cents, Some(29000));
    let finalized = coupon_engine::finalize(&c, coupon.id).unwrap();
    assert_eq!(finalized.status, "FINALIZED");
    assert!(coupon_engine::finalize(&c, coupon.id).is_ok());
    assert!(coupon_engine::update_draft(
        &c,
        &coupon_engine::UpdateRequest {
            coupon_id: coupon.id,
            candidate_ids: vec![]
        }
    )
    .is_err());
    let s = coupon_engine::start_series(&c, "2026-09-02", 10000).unwrap();
    assert!(coupon_engine::settle_series(&c, s.id, true, 16000).is_err());
    assert_eq!(coupon_engine::series(&c, s.id).unwrap().current_step, 1);
}

#[test]
fn phase8_accumulator_system_void_and_idempotent_settlement() {
    let db = fixture();
    let c = db.connection().unwrap();
    let run = candidate_engine::generate(
        &c,
        &candidate_engine::GenerateRequest {
            business_date: Some("2026-09-02".into()),
            generation_time: Some("2026-09-02T12:00:00Z".into()),
            category: None,
            dry_run: Some(false),
        },
    )
    .unwrap();
    let source = candidate_engine::get(
        &c,
        &candidate_engine::GetRequest {
            business_date: "2026-09-02".into(),
            run_id: Some(run.run_id),
            category: None,
        },
    )
    .unwrap();
    let _one = source
        .candidates
        .iter()
        .find(|x| x.category == "OVER_25")
        .unwrap()
        .id;
    let five: Vec<i64> = source
        .candidates
        .iter()
        .filter(|x| x.category == "OVER_25")
        .take(5)
        .map(|x| x.id)
        .collect();
    let draft = coupon_engine::create_draft(
        &c,
        &coupon_engine::DraftRequest {
            business_date: "2026-09-02".into(),
            candidate_run_id: run.run_id,
            coupon_type: "DAILY_OVER_25".into(),
            candidate_ids: five.clone(),
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap();
    coupon_engine::finalize(&c, draft.id).unwrap();
    let won = coupon_engine::settle(
        &c,
        &coupon_engine::SettlementRequest {
            coupon_id: draft.id,
            outcomes: five
                .iter()
                .map(|id| coupon_engine::SelectionOutcome {
                    candidate_id: *id,
                    result: "WON".into(),
                })
                .collect(),
            settled_at: "2026-09-02T13:00:00Z".into(),
        },
    )
    .unwrap();
    assert_eq!(won.status, "WON");
    assert_eq!(won.gross_return_cents, 118814);
    assert_eq!(
        coupon_engine::settle(
            &c,
            &coupon_engine::SettlementRequest {
                coupon_id: draft.id,
                outcomes: vec![],
                settled_at: "2026-09-02T14:00:00Z".into()
            }
        )
        .unwrap(),
        won
    );
    let _high = source
        .candidates
        .iter()
        .find(|x| x.category == "HIGH_CONFIDENCE")
        .unwrap()
        .id;
    let five_high: Vec<i64> = source
        .candidates
        .iter()
        .filter(|x| x.category == "HIGH_CONFIDENCE")
        .take(5)
        .map(|x| x.id)
        .collect();
    let void_draft = coupon_engine::create_draft(
        &c,
        &coupon_engine::DraftRequest {
            business_date: "2026-09-02".into(),
            candidate_run_id: run.run_id,
            coupon_type: "DAILY_HIGH_CONFIDENCE".into(),
            candidate_ids: five_high.clone(),
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap();
    coupon_engine::finalize(&c, void_draft.id).unwrap();
    let voided = coupon_engine::settle(
        &c,
        &coupon_engine::SettlementRequest {
            coupon_id: void_draft.id,
            outcomes: five_high
                .iter()
                .map(|id| coupon_engine::SelectionOutcome {
                    candidate_id: *id,
                    result: "VOID".into(),
                })
                .collect(),
            settled_at: "2026-09-02T13:00:00Z".into(),
        },
    )
    .unwrap();
    assert_eq!(voided.status, "VOID");
    assert_eq!(voided.gross_return_cents, 1000);
    let sys = coupon_engine::generate_daily(
        &c,
        &coupon_engine::GenerateRequest {
            business_date: "2026-09-02".into(),
            candidate_run_id: Some(run.run_id),
            coupon_type: Some("DAILY_SURPRISE_SYSTEM".into()),
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap()
    .pop()
    .unwrap();
    coupon_engine::finalize(&c, sys.id).unwrap();
    let all = coupon_engine::settle(
        &c,
        &coupon_engine::SettlementRequest {
            coupon_id: sys.id,
            outcomes: sys
                .candidate_ids
                .iter()
                .map(|id| coupon_engine::SelectionOutcome {
                    candidate_id: *id,
                    result: "WON".into(),
                })
                .collect(),
            settled_at: "2026-09-02T13:00:00Z".into(),
        },
    )
    .unwrap();
    assert_eq!(all.winning_columns, 29);
    assert_eq!(all.total_stake_cents, 29000);
    assert!(all.gross_return_cents > all.total_stake_cents);
}

#[test]
fn phase8_exact_system_settlement_payouts() {
    for (count, sizes, expected_columns, expected_all, expected_partial) in [
        (6usize, vec![5, 6], 7usize, 256000i64, 32000i64),
        (7usize, vec![5, 6, 7], 29usize, 1248000i64, 32000i64),
    ] {
        let db = fixture_with_odd(2.0);
        let c = db.connection().unwrap();
        let run = phase7_run(&c);
        let source = candidate_engine::get(
            &c,
            &candidate_engine::GetRequest {
                business_date: "2026-09-02".into(),
                run_id: Some(run.run_id),
                category: None,
            },
        )
        .unwrap();
        let ids: Vec<i64> = source
            .candidates
            .iter()
            .filter(|x| x.category == "SURPRISE")
            .take(count)
            .map(|x| x.id)
            .collect();
        assert_eq!(ids.len(), count);
        let draft = coupon_engine::create_draft(
            &c,
            &coupon_engine::DraftRequest {
                business_date: "2026-09-02".into(),
                candidate_run_id: run.run_id,
                coupon_type: "DAILY_SURPRISE_SYSTEM".into(),
                candidate_ids: ids.clone(),
                unit_stake_cents: Some(1000),
            },
        )
        .unwrap();
        assert_eq!(draft.system_sizes, sizes);
        assert_eq!(draft.columns.len(), expected_columns);
        coupon_engine::finalize(&c, draft.id).unwrap();
        let all = coupon_engine::settle(
            &c,
            &coupon_engine::SettlementRequest {
                coupon_id: draft.id,
                outcomes: ids
                    .iter()
                    .map(|id| coupon_engine::SelectionOutcome {
                        candidate_id: *id,
                        result: "WON".into(),
                    })
                    .collect(),
                settled_at: "2026-09-02T13:00:00Z".into(),
            },
        )
        .unwrap();
        assert_eq!(all.gross_return_cents, expected_all);
        assert_eq!(all.total_stake_cents, if count == 6 { 7000 } else { 29000 });
        assert_eq!(all.profit_loss_cents, expected_all - all.total_stake_cents);
        let partial_ids = ids.iter().take(5).copied().collect::<Vec<_>>();
        let db2 = fixture_with_odd(2.0);
        let c2 = db2.connection().unwrap();
        let run2 = phase7_run(&c2);
        let src2 = candidate_engine::get(
            &c2,
            &candidate_engine::GetRequest {
                business_date: "2026-09-02".into(),
                run_id: Some(run2.run_id),
                category: None,
            },
        )
        .unwrap();
        let ids2: Vec<i64> = src2
            .candidates
            .iter()
            .filter(|x| x.category == "SURPRISE")
            .take(count)
            .map(|x| x.id)
            .collect();
        let d2 = coupon_engine::create_draft(
            &c2,
            &coupon_engine::DraftRequest {
                business_date: "2026-09-02".into(),
                candidate_run_id: run2.run_id,
                coupon_type: "DAILY_SURPRISE_SYSTEM".into(),
                candidate_ids: ids2.clone(),
                unit_stake_cents: Some(1000),
            },
        )
        .unwrap();
        coupon_engine::finalize(&c2, d2.id).unwrap();
        let r2 = coupon_engine::settle(
            &c2,
            &coupon_engine::SettlementRequest {
                coupon_id: d2.id,
                outcomes: ids2
                    .iter()
                    .map(|id| coupon_engine::SelectionOutcome {
                        candidate_id: *id,
                        result: if partial_ids.contains(id) {
                            "WON".into()
                        } else {
                            "LOST".into()
                        },
                    })
                    .collect(),
                settled_at: "2026-09-02T13:00:00Z".into(),
            },
        )
        .unwrap();
        assert_eq!(r2.gross_return_cents, expected_partial);
        let d3 = coupon_engine::create_draft(
            &c2,
            &coupon_engine::DraftRequest {
                business_date: "2026-09-03".into(),
                candidate_run_id: run2.run_id,
                coupon_type: "DAILY_SURPRISE_SYSTEM".into(),
                candidate_ids: ids2.clone(),
                unit_stake_cents: Some(1000),
            },
        );
        assert!(d3.is_err());
    }
}

#[test]
fn phase8_system_void_and_unsettled_matrix() {
    for (count, expected_void, expected_all_void) in
        [(6usize, 144000i64, 7000i64), (7usize, 752000i64, 29000i64)]
    {
        let db = fixture_with_odd(2.0);
        let c = db.connection().unwrap();
        let run = phase7_run(&c);
        let src = candidate_engine::get(
            &c,
            &candidate_engine::GetRequest {
                business_date: "2026-09-02".into(),
                run_id: Some(run.run_id),
                category: None,
            },
        )
        .unwrap();
        let ids: Vec<i64> = src
            .candidates
            .iter()
            .filter(|x| x.category == "SURPRISE")
            .take(count)
            .map(|x| x.id)
            .collect();
        let d = coupon_engine::create_draft(
            &c,
            &coupon_engine::DraftRequest {
                business_date: "2026-09-02".into(),
                candidate_run_id: run.run_id,
                coupon_type: "DAILY_SURPRISE_SYSTEM".into(),
                candidate_ids: ids.clone(),
                unit_stake_cents: Some(1000),
            },
        )
        .unwrap();
        coupon_engine::finalize(&c, d.id).unwrap();
        let won_count = count - 1;
        let mixed = coupon_engine::settle(
            &c,
            &coupon_engine::SettlementRequest {
                coupon_id: d.id,
                outcomes: ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| coupon_engine::SelectionOutcome {
                        candidate_id: *id,
                        result: if i < won_count {
                            "WON".into()
                        } else {
                            "VOID".into()
                        },
                    })
                    .collect(),
                settled_at: "2026-09-02T13:00:00Z".into(),
            },
        )
        .unwrap();
        assert_eq!(mixed.gross_return_cents, expected_void);
        assert_eq!(
            mixed.total_stake_cents,
            if count == 6 { 7000 } else { 29000 }
        );
        let db2 = fixture_with_odd(2.0);
        let c2 = db2.connection().unwrap();
        let run2 = phase7_run(&c2);
        let src2 = candidate_engine::get(
            &c2,
            &candidate_engine::GetRequest {
                business_date: "2026-09-02".into(),
                run_id: Some(run2.run_id),
                category: None,
            },
        )
        .unwrap();
        let ids2: Vec<i64> = src2
            .candidates
            .iter()
            .filter(|x| x.category == "SURPRISE")
            .take(count)
            .map(|x| x.id)
            .collect();
        let d2 = coupon_engine::create_draft(
            &c2,
            &coupon_engine::DraftRequest {
                business_date: "2026-09-02".into(),
                candidate_run_id: run2.run_id,
                coupon_type: "DAILY_SURPRISE_SYSTEM".into(),
                candidate_ids: ids2.clone(),
                unit_stake_cents: Some(1000),
            },
        )
        .unwrap();
        coupon_engine::finalize(&c2, d2.id).unwrap();
        let voided = coupon_engine::settle(
            &c2,
            &coupon_engine::SettlementRequest {
                coupon_id: d2.id,
                outcomes: ids2
                    .iter()
                    .map(|id| coupon_engine::SelectionOutcome {
                        candidate_id: *id,
                        result: "VOID".into(),
                    })
                    .collect(),
                settled_at: "2026-09-02T13:00:00Z".into(),
            },
        )
        .unwrap();
        assert_eq!(voided.gross_return_cents, expected_all_void);
        assert_eq!(voided.profit_loss_cents, 0);
        let db3 = fixture_with_odd(2.0);
        let c3 = db3.connection().unwrap();
        let run3 = phase7_run(&c3);
        let src3 = candidate_engine::get(
            &c3,
            &candidate_engine::GetRequest {
                business_date: "2026-09-02".into(),
                run_id: Some(run3.run_id),
                category: None,
            },
        )
        .unwrap();
        let ids3: Vec<i64> = src3
            .candidates
            .iter()
            .filter(|x| x.category == "SURPRISE")
            .take(count)
            .map(|x| x.id)
            .collect();
        let d3 = coupon_engine::create_draft(
            &c3,
            &coupon_engine::DraftRequest {
                business_date: "2026-09-02".into(),
                candidate_run_id: run3.run_id,
                coupon_type: "DAILY_SURPRISE_SYSTEM".into(),
                candidate_ids: ids3.clone(),
                unit_stake_cents: Some(1000),
            },
        )
        .unwrap();
        coupon_engine::finalize(&c3, d3.id).unwrap();
        let pending = coupon_engine::settle(
            &c3,
            &coupon_engine::SettlementRequest {
                coupon_id: d3.id,
                outcomes: vec![coupon_engine::SelectionOutcome {
                    candidate_id: ids3[0],
                    result: "WON".into(),
                }],
                settled_at: "2026-09-02T13:00:00Z".into(),
            },
        )
        .unwrap();
        assert!(pending.unsettled_columns > 0);
        assert_eq!(pending.status, "UNSETTLED");
    }
}

#[test]
fn phase8_all_ordinary_coupon_types_share_settlement_router() {
    for (kind, category) in [
        ("DAILY_CORNERS", "CORNERS"),
        ("DAILY_OVER_25", "OVER_25"),
        ("DAILY_OVER_35", "OVER_35"),
        ("DAILY_BTTS", "BTTS_YES"),
        ("DAILY_HIGH_CONFIDENCE", "HIGH_CONFIDENCE"),
        ("DAILY_COMPOUND", "COMPOUND"),
    ] {
        for result in ["WON", "LOST", "VOID", "UNSETTLED"] {
            let db = candidate_engine_acceptance_tests::db();
            let c = db.connection().unwrap();
            let run = phase7_run(&c);
            let src = candidate_engine::get(
                &c,
                &candidate_engine::GetRequest {
                    business_date: "2026-09-02".into(),
                    run_id: Some(run.run_id),
                    category: None,
                },
            )
            .unwrap();
            let ids: Vec<i64> = src
                .candidates
                .iter()
                .filter(|x| x.category == category)
                .take(if kind == "DAILY_COMPOUND" { 2 } else { 5 })
                .map(|x| x.id)
                .collect();
            if ids.len() < if kind == "DAILY_COMPOUND" { 2 } else { 5 } {
                continue;
            }
            let draft = coupon_engine::create_draft(
                &c,
                &coupon_engine::DraftRequest {
                    business_date: "2026-09-02".into(),
                    candidate_run_id: run.run_id,
                    coupon_type: kind.into(),
                    candidate_ids: ids.clone(),
                    unit_stake_cents: Some(10000),
                },
            )
            .unwrap();
            coupon_engine::finalize(&c, draft.id).unwrap();
            let outcomes = if result == "UNSETTLED" {
                vec![]
            } else {
                ids.iter()
                    .map(|id| coupon_engine::SelectionOutcome {
                        candidate_id: *id,
                        result: result.into(),
                    })
                    .collect()
            };
            let settled = coupon_engine::settle(
                &c,
                &coupon_engine::SettlementRequest {
                    coupon_id: draft.id,
                    outcomes,
                    settled_at: "2026-09-02T13:00:00Z".into(),
                },
            )
            .unwrap();
            assert_eq!(settled.status, result, "routing failed for {kind}/{result}");
        }
    }
}

#[test]
fn phase8_replacement_and_correlation_acceptance_matrix() {
    let db = candidate_engine_acceptance_tests::db();
    let c = db.connection().unwrap();
    let run = phase7_run(&c);
    let source = candidate_engine::get(
        &c,
        &candidate_engine::GetRequest {
            business_date: "2026-09-02".into(),
            run_id: Some(run.run_id),
            category: None,
        },
    )
    .unwrap();
    let goals: Vec<i64> = source
        .candidates
        .iter()
        .filter(|x| x.category == "OVER_25")
        .map(|x| x.id)
        .collect();
    assert!(goals.len() >= 3);
    let draft = coupon_engine::create_draft(
        &c,
        &coupon_engine::DraftRequest {
            business_date: "2026-09-02".into(),
            candidate_run_id: run.run_id,
            coupon_type: "DAILY_OVER_25".into(),
            candidate_ids: vec![goals[0]],
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap();
    let replaced = coupon_engine::update_draft(
        &c,
        &coupon_engine::UpdateRequest {
            coupon_id: draft.id,
            candidate_ids: vec![goals[1]],
        },
    )
    .unwrap();
    assert_eq!(replaced.candidate_ids, vec![goals[1]]);
    assert_eq!(replaced.status, "DRAFT");
    let before = coupon_engine::get_coupon(&c, draft.id).unwrap();
    assert_eq!(
        coupon_engine::update_draft(
            &c,
            &coupon_engine::UpdateRequest {
                coupon_id: draft.id,
                candidate_ids: vec![999999]
            }
        )
        .unwrap_err(),
        "CANDIDATE_NOT_FOUND"
    );
    assert_eq!(coupon_engine::get_coupon(&c, draft.id).unwrap(), before);
    assert_eq!(
        coupon_engine::update_draft(
            &c,
            &coupon_engine::UpdateRequest {
                coupon_id: draft.id,
                candidate_ids: vec![goals[1], goals[1]]
            }
        )
        .unwrap_err(),
        "DUPLICATE_CANDIDATE"
    );
    let btts = source
        .candidates
        .iter()
        .find(|x| x.category == "BTTS_YES")
        .unwrap()
        .id;
    assert_eq!(
        coupon_engine::update_draft(
            &c,
            &coupon_engine::UpdateRequest {
                coupon_id: draft.id,
                candidate_ids: vec![btts]
            }
        )
        .unwrap_err(),
        "WRONG_CATEGORY"
    );
    // The Phase 7 category reduction normally removes nested alternatives. The
    // production validator still maps an explicitly supplied same-match pair
    // to the domain error; candidate snapshots are immutable, so use the real
    // rows when they are available and otherwise retain the generic safeguard.
    let nested_candidate = source.candidates.iter().find(|x| {
        x.category == "OVER_25"
            && x.match_id
                == source
                    .candidates
                    .iter()
                    .find(|y| y.id == goals[1])
                    .unwrap()
                    .match_id
            && x.id != goals[1]
    });
    if let Some(conflict) = nested_candidate {
        assert_eq!(
            coupon_engine::update_draft(
                &c,
                &coupon_engine::UpdateRequest {
                    coupon_id: draft.id,
                    candidate_ids: vec![goals[1], conflict.id]
                }
            )
            .unwrap_err(),
            "NESTED_TOTAL_CONFLICT"
        );
    }
    assert_eq!(
        coupon_engine::update_draft(
            &c,
            &coupon_engine::UpdateRequest {
                coupon_id: draft.id,
                candidate_ids: vec![-987654]
            }
        )
        .unwrap_err(),
        "CANDIDATE_NOT_FOUND"
    );
    let run_b = {
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(1,'iddaa','replace','TOTAL_GOALS','OVER',2.10,'TOTAL_GOALS','OVER',2.5,'2026-09-02T11:30:00Z')", []).unwrap();
        phase7_run(&c)
    };
    let other_run_candidate = candidate_engine::get(
        &c,
        &candidate_engine::GetRequest {
            business_date: "2026-09-02".into(),
            run_id: Some(run_b.run_id),
            category: None,
        },
    )
    .unwrap()
    .candidates
    .iter()
    .find(|x| x.category == "OVER_25")
    .unwrap()
    .id;
    assert_eq!(
        coupon_engine::update_draft(
            &c,
            &coupon_engine::UpdateRequest {
                coupon_id: draft.id,
                candidate_ids: vec![other_run_candidate]
            }
        )
        .unwrap_err(),
        "CANDIDATE_FROM_DIFFERENT_RUN"
    );
    coupon_engine::update_draft(
        &c,
        &coupon_engine::UpdateRequest {
            coupon_id: draft.id,
            candidate_ids: goals.iter().take(5).copied().collect(),
        },
    )
    .unwrap();
    let finalized = coupon_engine::finalize(&c, draft.id).unwrap();
    assert_eq!(
        coupon_engine::update_draft(
            &c,
            &coupon_engine::UpdateRequest {
                coupon_id: finalized.id,
                candidate_ids: vec![goals[0]]
            }
        )
        .unwrap_err(),
        "COUPON_NOT_DRAFT"
    );
}

#[test]
fn phase8_final_replacement_conflict_matrix() {
    let db = fixture_with_odd(2.60);
    let c = db.connection().unwrap();
    let run = phase7_run(&c);
    let source = candidate_engine::get(
        &c,
        &candidate_engine::GetRequest {
            business_date: "2026-09-02".into(),
            run_id: Some(run.run_id),
            category: None,
        },
    )
    .unwrap();
    let surprise: Vec<i64> = source
        .candidates
        .iter()
        .filter(|x| x.category == "SURPRISE")
        .map(|x| x.id)
        .collect();
    assert!(surprise.len() >= 8);
    let draft = coupon_engine::create_draft(
        &c,
        &coupon_engine::DraftRequest {
            business_date: "2026-09-02".into(),
            candidate_run_id: run.run_id,
            coupon_type: "DAILY_SURPRISE_SYSTEM".into(),
            candidate_ids: surprise[..7].to_vec(),
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap();
    let initial_columns = draft.columns.clone();
    let replacement = coupon_engine::update_draft(
        &c,
        &coupon_engine::UpdateRequest {
            coupon_id: draft.id,
            candidate_ids: surprise[1..8].to_vec(),
        },
    )
    .unwrap();
    assert_eq!(replacement.candidate_ids.len(), 7);
    assert!(!replacement.candidate_ids.contains(&surprise[0]));
    assert!(replacement.candidate_ids.contains(&surprise[7]));
    assert_eq!(replacement.system_sizes, vec![5, 6, 7]);
    assert_eq!(replacement.columns.len(), 29);
    assert_eq!(
        replacement
            .columns
            .iter()
            .map(|x| x.candidate_ids.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        29
    );
    assert_ne!(initial_columns, replacement.columns);
    assert_eq!(replacement.total_stake_cents, Some(29000));
    let before = coupon_engine::get_coupon(&c, draft.id).unwrap();
    for (ids, code) in [
        (
            vec![
                surprise[1],
                source
                    .candidates
                    .iter()
                    .find(|x| x.category == "OVER_25")
                    .unwrap()
                    .id,
            ],
            "WRONG_CATEGORY",
        ),
        (vec![surprise[1], surprise[1]], "DUPLICATE_CANDIDATE"),
    ] {
        assert_eq!(
            coupon_engine::update_draft(
                &c,
                &coupon_engine::UpdateRequest {
                    coupon_id: draft.id,
                    candidate_ids: ids
                }
            )
            .unwrap_err(),
            code
        );
        assert_eq!(coupon_engine::get_coupon(&c, draft.id).unwrap(), before);
    }
    let run_b = {
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(1,'iddaa','replace','TOTAL_GOALS','OVER',2.10,'TOTAL_GOALS','OVER',2.5,'2026-09-02T11:30:00Z')", []).unwrap();
        phase7_run(&c)
    };
    let other = candidate_engine::get(
        &c,
        &candidate_engine::GetRequest {
            business_date: "2026-09-02".into(),
            run_id: Some(run_b.run_id),
            category: None,
        },
    )
    .unwrap()
    .candidates
    .iter()
    .find(|x| x.category == "SURPRISE")
    .unwrap()
    .id;
    assert_eq!(
        coupon_engine::update_draft(
            &c,
            &coupon_engine::UpdateRequest {
                coupon_id: draft.id,
                candidate_ids: vec![surprise[1], other]
            }
        )
        .unwrap_err(),
        "CANDIDATE_FROM_DIFFERENT_RUN"
    );
    assert_eq!(coupon_engine::get_coupon(&c, draft.id).unwrap(), before);
    let same = clone_candidate_for_conflict(
        &c,
        surprise[1],
        "SURPRISE",
        "TOTAL_GOALS",
        "OVER",
        Some(3.5),
        2.60,
    );
    assert_eq!(
        coupon_engine::update_draft(
            &c,
            &coupon_engine::UpdateRequest {
                coupon_id: draft.id,
                candidate_ids: vec![surprise[1], same]
            }
        )
        .unwrap_err(),
        "NESTED_TOTAL_CONFLICT"
    );
    assert_eq!(coupon_engine::get_coupon(&c, draft.id).unwrap(), before);
    coupon_engine::update_draft(
        &c,
        &coupon_engine::UpdateRequest {
            coupon_id: draft.id,
            candidate_ids: surprise[1..8].to_vec(),
        },
    )
    .unwrap();
    let finalized = coupon_engine::finalize(&c, draft.id).unwrap();
    assert_eq!(
        coupon_engine::update_draft(
            &c,
            &coupon_engine::UpdateRequest {
                coupon_id: finalized.id,
                candidate_ids: replacement.candidate_ids.clone()
            }
        )
        .unwrap_err(),
        "COUPON_NOT_DRAFT"
    );

    let db2 = fixture_with_odd(1.30);
    let c2 = db2.connection().unwrap();
    let run2 = phase7_run(&c2);
    let src2 = candidate_engine::get(
        &c2,
        &candidate_engine::GetRequest {
            business_date: "2026-09-02".into(),
            run_id: Some(run2.run_id),
            category: None,
        },
    )
    .unwrap();
    let compounds: Vec<i64> = src2
        .candidates
        .iter()
        .filter(|x| x.category == "COMPOUND")
        .map(|x| x.id)
        .collect();
    let cd = coupon_engine::create_draft(
        &c2,
        &coupon_engine::DraftRequest {
            business_date: "2026-09-02".into(),
            candidate_run_id: run2.run_id,
            coupon_type: "DAILY_COMPOUND".into(),
            candidate_ids: compounds[..2].to_vec(),
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap();
    let valid = coupon_engine::update_draft(
        &c2,
        &coupon_engine::UpdateRequest {
            coupon_id: cd.id,
            candidate_ids: vec![compounds[1], compounds[2]],
        },
    )
    .unwrap();
    assert!((valid.combined_decimal_odd.unwrap() - 1.69).abs() < 1e-9);
    let low = clone_candidate_for_conflict(
        &c2,
        compounds[0],
        "COMPOUND",
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        1.0,
    );
    assert_eq!(
        coupon_engine::update_draft(
            &c2,
            &coupon_engine::UpdateRequest {
                coupon_id: cd.id,
                candidate_ids: vec![compounds[1], low]
            }
        )
        .unwrap_err(),
        "COMPOUND_ODDS_BELOW_MINIMUM"
    );
    let high = clone_candidate_for_conflict(
        &c2,
        compounds[0],
        "COMPOUND",
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        2.0,
    );
    assert_eq!(
        coupon_engine::update_draft(
            &c2,
            &coupon_engine::UpdateRequest {
                coupon_id: cd.id,
                candidate_ids: vec![compounds[1], high]
            }
        )
        .unwrap_err(),
        "COMPOUND_ODDS_ABOVE_MAXIMUM"
    );
    let corr = clone_candidate_for_conflict(
        &c2,
        compounds[1],
        "COMPOUND",
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        1.30,
    );
    assert_eq!(
        coupon_engine::update_draft(
            &c2,
            &coupon_engine::UpdateRequest {
                coupon_id: cd.id,
                candidate_ids: vec![compounds[1], corr]
            }
        )
        .unwrap_err(),
        "SAME_MATCH_CONFLICT"
    );

    let db3 = candidate_engine_acceptance_tests::db();
    let c3 = db3.connection().unwrap();
    let run3 = phase7_run(&c3);
    let src3 = candidate_engine::get(
        &c3,
        &candidate_engine::GetRequest {
            business_date: "2026-09-02".into(),
            run_id: Some(run3.run_id),
            category: None,
        },
    )
    .unwrap();
    let btts = src3
        .candidates
        .iter()
        .find(|x| x.category == "BTTS_YES")
        .unwrap();
    let btts_conflict = clone_candidate_for_conflict(
        &c3,
        btts.id,
        "BTTS_YES",
        "TOTAL_GOALS",
        "OVER",
        Some(2.5),
        1.8,
    );
    let bd = coupon_engine::create_draft(
        &c3,
        &coupon_engine::DraftRequest {
            business_date: "2026-09-02".into(),
            candidate_run_id: run3.run_id,
            coupon_type: "DAILY_BTTS".into(),
            candidate_ids: vec![btts.id],
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap();
    let bbefore = coupon_engine::get_coupon(&c3, bd.id).unwrap();
    assert_eq!(
        coupon_engine::update_draft(
            &c3,
            &coupon_engine::UpdateRequest {
                coupon_id: bd.id,
                candidate_ids: vec![btts.id, btts_conflict]
            }
        )
        .unwrap_err(),
        "BTTS_GOALS_CONFLICT"
    );
    assert_eq!(coupon_engine::get_coupon(&c3, bd.id).unwrap(), bbefore);
    let corner = src3
        .candidates
        .iter()
        .find(|x| x.category == "CORNERS")
        .unwrap();
    let c1 = clone_candidate_for_conflict(
        &c3,
        corner.id,
        "CORNERS",
        "FULL_TIME_TOTAL_CORNERS",
        "OVER",
        Some(8.5),
        2.0,
    );
    let c2x = clone_candidate_for_conflict(
        &c3,
        corner.id,
        "CORNERS",
        "FULL_TIME_TOTAL_CORNERS",
        "OVER",
        Some(9.5),
        2.0,
    );
    let corners = coupon_engine::create_draft(
        &c3,
        &coupon_engine::DraftRequest {
            business_date: "2026-09-02".into(),
            candidate_run_id: run3.run_id,
            coupon_type: "DAILY_CORNERS".into(),
            candidate_ids: vec![corner.id],
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap();
    assert_eq!(
        coupon_engine::update_draft(
            &c3,
            &coupon_engine::UpdateRequest {
                coupon_id: corners.id,
                candidate_ids: vec![c1, c2x]
            }
        )
        .unwrap_err(),
        "NESTED_CORNERS_CONFLICT"
    );
}

#[test]
fn phase8_katlama_generation_void_and_cancellation() {
    let db = fixture_with_odd(1.30);
    let c = db.connection().unwrap();
    let run = phase7_run(&c);
    let series = coupon_engine::start_series(&c, "2026-09-02", 10000).unwrap();
    let coupon = coupon_engine::generate_compound_step(&c, "2026-09-02", Some(run.run_id)).unwrap();
    assert_eq!(coupon.series_id, Some(series.id));
    assert_eq!(coupon.step_number, Some(1));
    assert_eq!(coupon.candidate_ids.len(), 2);
    assert!(coupon.combined_decimal_odd.unwrap() >= 1.50);
    assert!(coupon.combined_decimal_odd.unwrap() <= 1.80);
    coupon_engine::finalize(&c, coupon.id).unwrap();
    let won = coupon_engine::settle(
        &c,
        &coupon_engine::SettlementRequest {
            coupon_id: coupon.id,
            outcomes: coupon
                .candidate_ids
                .iter()
                .map(|id| coupon_engine::SelectionOutcome {
                    candidate_id: *id,
                    result: "WON".into(),
                })
                .collect(),
            settled_at: "2026-09-02T13:00:00Z".into(),
        },
    )
    .unwrap();
    assert_eq!(won.status, "WON");
    let after_win = coupon_engine::series(&c, series.id).unwrap();
    assert_eq!(after_win.current_step, 2);
    assert_eq!(after_win.current_stake_cents, won.gross_return_cents);
    assert!(coupon_engine::settle_series_result(&c, series.id, "VOID", 0).is_err());
    let after_void = coupon_engine::series(&c, series.id).unwrap();
    assert_eq!(after_void.current_step, 2);
    assert_eq!(
        after_void.current_stake_cents,
        after_win.current_stake_cents
    );
    assert!(coupon_engine::start_series(&c, "2026-09-02", 10000).is_err());
    let cancelled = coupon_engine::cancel_series(&c, series.id).unwrap();
    assert_eq!(cancelled.status, "CANCELLED");
    assert!(coupon_engine::generate_compound_step(&c, "2026-09-03", Some(run.run_id)).is_err());
    assert_eq!(
        coupon_engine::cancel_series(&c, series.id).unwrap().status,
        "CANCELLED"
    );
}

#[test]
fn phase8_controlled_daily_coupon_acceptance_report() {
    let db = candidate_engine_acceptance_tests::db();
    let c = db.connection().unwrap();
    // Extend the deterministic Phase 7 universe to provide six valid surprise legs
    // without replacing the compound fixture candidates.
    for id in 20..=24 {
        let home = (id - 20) * 2 + 39;
        c.execute("INSERT INTO matches(competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(1,'2026/27',?1,?2,?3,'scheduled','2026-09-02',1)", rusqlite::params![home, home + 1, format!("2026-09-02T{}:00:00Z", 16 + id - 20)]).unwrap();
        c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json,calculated_at) VALUES(?1,'fe_v1','2026-09-02T12:00:00Z','{}','{\"history_matches_home\":8,\"history_matches_away\":8}','2026-09-02T10:00:00Z')", [id]).unwrap();
        c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability,created_at) VALUES(?1,'TOTAL_GOALS','OVER',2.5,0.80,'p8','2026-09-02T12:00:00Z',1,0.80,0.80,'CALIBRATED_V1','cal_v1',25,'AVAILABLE','2026-09-02T11:00:00Z')", [id]).unwrap();
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(?1,'iddaa','phase8','TOTAL_GOALS','OVER',2.40,'TOTAL_GOALS','OVER',2.5,'2026-09-02T11:00:00Z')", [id]).unwrap();
    }
    let started = std::time::Instant::now();
    let run = candidate_engine::generate(
        &c,
        &candidate_engine::GenerateRequest {
            business_date: Some("2026-09-02".into()),
            generation_time: Some("2026-09-02T12:00:00Z".into()),
            category: None,
            dry_run: Some(false),
        },
    )
    .unwrap();
    let coupons = coupon_engine::generate_daily(
        &c,
        &coupon_engine::GenerateRequest {
            business_date: "2026-09-02".into(),
            candidate_run_id: Some(run.run_id),
            coupon_type: None,
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap();
    let elapsed = started.elapsed().as_millis();
    let competitions: i64 = c
        .query_row("SELECT COUNT(*) FROM competitions", [], |x| x.get(0))
        .unwrap();
    let teams: i64 = c
        .query_row("SELECT COUNT(*) FROM teams", [], |x| x.get(0))
        .unwrap();
    let upcoming: i64 = c.query_row("SELECT COUNT(*) FROM matches WHERE scheduled_local_date='2026-09-02' AND status='scheduled'", [], |x| x.get(0)).unwrap();
    println!("PHASE 8 CONTROLLED ACCEPTANCE\nBusiness date: {}\nTimezone: Europe/Istanbul\nGeneration time: {}\nPolicy version: {}\nModel version: {}\nCalibration version: {:?}\nCompetitions: {}\nTeams: {}\nUpcoming matches: {}\nMatches considered: {}", run.business_date, run.generated_at, coupon_engine::COUPON_POLICY_VERSION, run.model_version, run.calibration_version, competitions, teams, upcoming, run.match_count_considered);
    for kind in [
        "DAILY_CORNERS",
        "DAILY_OVER_25",
        "DAILY_OVER_35",
        "DAILY_BTTS",
        "DAILY_HIGH_CONFIDENCE",
        "DAILY_SURPRISE_SYSTEM",
        "DAILY_COMPOUND",
    ] {
        let x = coupons.iter().find(|x| x.coupon_type == kind);
        println!(
            "{} qualified={} combined_odd={:?} columns={}",
            kind,
            x.map(|v| v.candidate_ids.len()).unwrap_or(0),
            x.and_then(|v| v.combined_decimal_odd),
            x.map(|v| v.columns.len()).unwrap_or(0)
        );
        if let Some(x) = x {
            for id in x.candidate_ids.iter().take(3) {
                println!(
                    "TOP {} candidate_id={} rank_snapshot_source_run={}",
                    kind, id, x.source_candidate_run_id
                );
            }
        }
    }
    let mut reasons = std::collections::BTreeMap::new();
    for x in &run.exclusions {
        *reasons.entry(x.reason.clone()).or_insert(0usize) += 1;
    }
    println!(
        "EXCLUSIONS BY REASON {:?}\nPHASE 8 GENERATION MS {}",
        reasons, elapsed
    );
    assert_eq!(coupons.len(), 4);
    assert!(coupons.iter().all(coupon_engine::rule_compliant));
    let surprise = coupons
        .iter()
        .find(|x| x.coupon_type == "DAILY_SURPRISE_SYSTEM")
        .unwrap();
    assert_eq!(surprise.system_sizes, vec![5, 6, 7]);
    assert_eq!(surprise.columns.len(), 29);
}

#[test]
fn phase8_100_match_coupon_benchmark() {
    let db = candidate_engine_acceptance_tests::db();
    let c = db.connection().unwrap();
    for i in 0..200 {
        c.execute(
            "INSERT INTO teams(normalized_name,country) VALUES(?1,'TR')",
            [format!("Phase8 Benchmark Team {i}")],
        )
        .unwrap();
    }
    for i in 0..100 {
        let match_id = 20 + i;
        let home = 51 + i * 2;
        c.execute("INSERT INTO matches(competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(2,'2026/27',?1,?2,?3,'scheduled','2026-09-02',1)", rusqlite::params![home, home + 1, format!("2026-09-03T{:02}:00:00Z", i % 24)]).unwrap();
        c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json,calculated_at) VALUES(?1,'fe_v1','2026-09-02T12:00:00Z','{}','{\"history_matches_home\":8,\"history_matches_away\":8}','2026-09-02T10:00:00Z')", [match_id]).unwrap();
        c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability,created_at) VALUES(?1,'TOTAL_GOALS','OVER',2.5,0.80,'bench','2026-09-02T12:00:00Z',1,0.80,0.80,'CALIBRATED_V1','cal_v1',25,'AVAILABLE','2026-09-02T11:00:00Z')", [match_id]).unwrap();
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(?1,'iddaa','bench','TOTAL_GOALS','OVER',1.30,'TOTAL_GOALS','OVER',2.5,'2026-09-02T11:00:00Z')", [match_id]).unwrap();
    }
    let started = std::time::Instant::now();
    let run = phase7_run(&c);
    let after_candidates = std::time::Instant::now();
    let mut timings = std::collections::BTreeMap::new();
    let mut coupons = Vec::new();
    for kind in [
        "DAILY_CORNERS",
        "DAILY_OVER_25",
        "DAILY_OVER_35",
        "DAILY_BTTS",
        "DAILY_HIGH_CONFIDENCE",
        "DAILY_SURPRISE_SYSTEM",
        "DAILY_COMPOUND",
    ] {
        let started = std::time::Instant::now();
        let mut generated = coupon_engine::generate_daily(
            &c,
            &coupon_engine::GenerateRequest {
                business_date: "2026-09-02".into(),
                candidate_run_id: Some(run.run_id),
                coupon_type: Some(kind.into()),
                unit_stake_cents: Some(1000),
            },
        )
        .unwrap();
        timings.insert(kind, started.elapsed().as_millis());
        coupons.append(&mut generated);
    }
    let elapsed = started.elapsed().as_millis();
    println!("PHASE 8 100-MATCH BENCHMARK\nmatches_processed={} potential_selections={} qualified_candidates={} drafts={} total_ms={} candidate_input_ms={} ordinary_ms={} high_confidence_ms={} system_ms={} compound_ms={} persistence_ms={} average_ms_per_match={:.3}", run.match_count_considered, run.candidates.len() + run.exclusions.len(), run.candidates.len(), coupons.len(), elapsed, after_candidates.duration_since(started).as_millis(), timings["DAILY_OVER_25"] + timings["DAILY_CORNERS"] + timings["DAILY_OVER_35"] + timings["DAILY_BTTS"], timings["DAILY_HIGH_CONFIDENCE"], timings["DAILY_SURPRISE_SYSTEM"], timings["DAILY_COMPOUND"], elapsed.saturating_sub(after_candidates.duration_since(started).as_millis()), elapsed as f64 / run.match_count_considered as f64);
    assert!(run.match_count_considered >= 100);
    assert!(!coupons.is_empty());
}

#[test]
fn phase8_daily_report_never_omits_unavailable_types() {
    let db = fixture_with_odd(1.00);
    let c = db.connection().unwrap();
    let run = phase7_run(&c);
    let report = coupon_engine::generate_daily_report(
        &c,
        &coupon_engine::GenerateRequest {
            business_date: "2026-09-02".into(),
            candidate_run_id: Some(run.run_id),
            coupon_type: None,
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap();
    assert_eq!(report.len(), 7);
    let surprise = report
        .iter()
        .find(|x| x.coupon_type == "DAILY_SURPRISE_SYSTEM")
        .unwrap();
    assert_eq!(surprise.generation_status, "UNAVAILABLE");
    assert_eq!(
        surprise.unavailable_reason.as_deref(),
        Some("INSUFFICIENT_QUALIFIED_SURPRISE_SELECTIONS")
    );
    let compound = report
        .iter()
        .find(|x| x.coupon_type == "DAILY_COMPOUND")
        .unwrap();
    assert_eq!(compound.generation_status, "UNAVAILABLE");
    assert_eq!(
        compound.unavailable_reason.as_deref(),
        Some("NO_QUALIFYING_COMPOUND_COUPON")
    );
    assert_eq!(
        report
            .iter()
            .filter(|x| x.display_name.starts_with("Günün"))
            .count(),
        7
    );
}

#[test]
fn phase8_katlama_seven_distinct_candidate_runs_and_history() {
    let db = fixture_with_odd(1.30);
    let c = db.connection().unwrap();
    let series = coupon_engine::start_series(&c, "2026-09-02", 10000).unwrap();
    let dates = [
        "2026-09-02",
        "2026-09-03",
        "2026-09-04",
        "2026-09-05",
        "2026-09-06",
        "2026-09-07",
        "2026-09-08",
    ];
    let mut run_ids = Vec::new();
    for (index, date) in dates.iter().enumerate() {
        if index > 0 {
            c.execute(
                "UPDATE matches SET scheduled_local_date=?1,kickoff_at=?2 WHERE id IN (1,2)",
                rusqlite::params![date, format!("{}T18:00:00Z", date)],
            )
            .unwrap();
            c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(1,'iddaa','katlama','TOTAL_GOALS','OVER',1.30,'TOTAL_GOALS','OVER',2.5,?1),(2,'iddaa','katlama','TOTAL_GOALS','OVER',1.30,'TOTAL_GOALS','OVER',2.5,?1)", [format!("{}T10:00:00Z", date)]).unwrap();
        }
        let run = candidate_engine::generate(
            &c,
            &candidate_engine::GenerateRequest {
                business_date: Some((*date).into()),
                generation_time: Some(format!("{}T12:00:00Z", date)),
                category: None,
                dry_run: Some(false),
            },
        )
        .unwrap();
        run_ids.push(run.run_id);
        let coupon = coupon_engine::generate_compound_step(&c, date, Some(run.run_id)).unwrap();
        assert_eq!(coupon.source_candidate_run_id, run.run_id);
        assert_eq!(coupon.business_date, *date);
        coupon_engine::finalize(&c, coupon.id).unwrap();
        let settled = coupon_engine::settle(
            &c,
            &coupon_engine::SettlementRequest {
                coupon_id: coupon.id,
                outcomes: coupon
                    .candidate_ids
                    .iter()
                    .map(|id| coupon_engine::SelectionOutcome {
                        candidate_id: *id,
                        result: "WON".into(),
                    })
                    .collect(),
                settled_at: format!("{}T13:00:00Z", date),
            },
        )
        .unwrap();
        assert_eq!(settled.status, "WON");
    }
    let final_state = coupon_engine::series(&c, series.id).unwrap();
    assert_eq!(final_state.status, "ACTIVE");
    assert_eq!(final_state.current_step, 1);
    assert_eq!(
        c.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM phase8_series_steps WHERE series_id=?1",
            [series.id],
            |x| x.get(0)
        )
        .unwrap(),
        7
    );
    let history_dates: Vec<String> = c
        .prepare(
            "SELECT business_date FROM phase8_series_steps WHERE series_id=?1 ORDER BY step_number",
        )
        .unwrap()
        .query_map([series.id], |x| x.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        history_dates,
        dates.iter().map(|x| x.to_string()).collect::<Vec<_>>()
    );
    assert_eq!(run_ids.len(), 7);
    assert!(coupon_engine::generate_compound_step(&c, "2026-09-09", Some(run_ids[6])).is_err());
}

#[test]
fn phase8_katlama_reset_preserves_history_and_creates_new_step_one() {
    let db = fixture_with_odd(1.30);
    let c = db.connection().unwrap();
    let series = coupon_engine::start_series(&c, "2026-09-02", 10000).unwrap();
    for (i, date) in ["2026-09-02", "2026-09-03", "2026-09-04", "2026-09-05"]
        .iter()
        .enumerate()
    {
        if i > 0 {
            c.execute(
                "UPDATE matches SET scheduled_local_date=?1,kickoff_at=?2 WHERE id IN (1,2)",
                rusqlite::params![date, format!("{}T18:00:00Z", date)],
            )
            .unwrap();
            c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(1,'iddaa','katlama','TOTAL_GOALS','OVER',1.30,'TOTAL_GOALS','OVER',2.5,?1),(2,'iddaa','katlama','TOTAL_GOALS','OVER',1.30,'TOTAL_GOALS','OVER',2.5,?1)", [format!("{}T10:00:00Z", date)]).unwrap();
        }
        let run = candidate_engine::generate(
            &c,
            &candidate_engine::GenerateRequest {
                business_date: Some((*date).into()),
                generation_time: Some(format!("{}T12:00:00Z", date)),
                category: None,
                dry_run: Some(false),
            },
        )
        .unwrap();
        let coupon = coupon_engine::generate_compound_step(&c, date, Some(run.run_id)).unwrap();
        coupon_engine::finalize(&c, coupon.id).unwrap();
        let result = if i == 2 { "LOST" } else { "WON" };
        let settled = coupon_engine::settle(
            &c,
            &coupon_engine::SettlementRequest {
                coupon_id: coupon.id,
                outcomes: coupon
                    .candidate_ids
                    .iter()
                    .map(|id| coupon_engine::SelectionOutcome {
                        candidate_id: *id,
                        result: result.into(),
                    })
                    .collect(),
                settled_at: format!("{}T13:00:00Z", date),
            },
        )
        .unwrap();
        assert_eq!(settled.status, result);
    }
    let s = coupon_engine::series(&c, series.id).unwrap();
    assert_eq!(s.current_step, 2);
    assert!(s.current_stake_cents > 10000);
    assert_eq!(s.reset_count, 1);
    let steps: Vec<i64> = c
        .prepare("SELECT step_number FROM phase8_series_steps WHERE series_id=?1 ORDER BY id")
        .unwrap()
        .query_map([series.id], |x| x.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(steps, vec![1, 2, 3, 1]);
}

#[test]
fn product_minimums_and_corner_maximum_are_enforced_at_publication() {
    for (kind, cat, market, selection, line) in [
        (
            "DAILY_CORNERS",
            "CORNERS",
            "FULL_TIME_TOTAL_CORNERS",
            "OVER",
            Some(8.5),
        ),
        ("DAILY_OVER_25", "OVER_25", "TOTAL_GOALS", "OVER", Some(2.5)),
        ("DAILY_OVER_35", "OVER_35", "TOTAL_GOALS", "OVER", Some(3.5)),
        ("DAILY_BTTS", "BTTS_YES", "BTTS", "YES", None),
        (
            "DAILY_HIGH_CONFIDENCE",
            "HIGH_CONFIDENCE",
            "TOTAL_GOALS",
            "OVER",
            Some(2.5),
        ),
    ] {
        for n in [2, 3, 4, 5, 7, 8] {
            let goal = matches!(kind, "DAILY_OVER_25" | "DAILY_OVER_35");
            let minimum = if goal { 3 } else { 5 };
            let db = fixture();
            let c = db.connection().unwrap();
            // Test inputs are populated before immutable snapshots are generated.
            c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json,calculated_at) SELECT match_id,'fe_v1','2026-09-02T11:01:00Z','{}',json_set(data_quality_json,'$.corners_coverage',1.0),'2026-09-02T11:01:00Z' FROM feature_sets",[]).unwrap();
            c.execute(
                "INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability,created_at) SELECT match_id,?1,?2,?3,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability,'2026-09-02T11:02:00Z' FROM predictions",
                params![market, selection, line],
            )
            .unwrap();
            c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) SELECT id,'iddaa','rules',?1,?2,2.6,?1,?2,?3,'2026-09-02T11:00:00Z' FROM matches WHERE id<=?4",params![market,selection,line,n]).unwrap();
            c.execute("UPDATE matches SET status='cancelled' WHERE id>?1", [n])
                .unwrap();
            let run = phase7_run(&c);
            let ids: Vec<i64> = run
                .candidates
                .iter()
                .filter(|x| x.category == cat)
                .map(|x| x.id)
                .collect();
            assert_eq!(ids.len(), n as usize, "{kind}");
            let generated = coupon_engine::generate_daily(
                &c,
                &coupon_engine::GenerateRequest {
                    business_date: run.business_date.clone(),
                    candidate_run_id: Some(run.run_id),
                    coupon_type: Some(kind.into()),
                    unit_stake_cents: None,
                },
            )
            .unwrap();
            if n < minimum {
                assert!(generated.is_empty(), "{kind}");
                let draft = coupon_engine::create_draft(
                    &c,
                    &coupon_engine::DraftRequest {
                        business_date: run.business_date,
                        candidate_run_id: run.run_id,
                        coupon_type: kind.into(),
                        candidate_ids: ids,
                        unit_stake_cents: None,
                    },
                )
                .unwrap();
                assert_eq!(draft.publication_status, "INSUFFICIENT");
                assert!(coupon_engine::finalize(&c, draft.id).is_err());
            } else {
                assert_eq!(generated.len(), 1, "{kind}");
                assert_eq!(generated[0].publication_status, "READY");
                assert!(generated[0].selections.len() >= minimum as usize);
                if goal {
                    assert_eq!(generated[0].selections.len(), (n as usize).min(5));
                }
                if matches!(kind, "DAILY_CORNERS" | "DAILY_BTTS") {
                    assert_eq!(generated[0].selections.len(), (n as usize).min(7));
                }
            }
        }
    }
}

#[test]
fn product_series_survives_reopen_and_pending_refresh() {
    let source = fixture_with_odd(1.30);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("persist.sqlite3");
    {
        let c = source.connection().unwrap();
        c.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
            .unwrap();
    }
    let (series_id, coupon_id);
    {
        let db = Database::open(path.clone()).unwrap();
        let c = db.connection().unwrap();
        let run = phase7_run(&c);
        let series = coupon_engine::start_series(&c, "2026-09-02", 10000).unwrap();
        series_id = series.id;
        let coupon =
            coupon_engine::generate_compound_step(&c, "2026-09-02", Some(run.run_id)).unwrap();
        coupon_id = coupon.id;
        assert!(coupon_engine::settle_series(&c, series_id, true, 100000).is_err());
        assert_eq!(
            coupon_engine::generate_compound_step(&c, "2026-09-03", None)
                .unwrap()
                .id,
            coupon_id
        );
    }
    {
        let db = Database::open(path).unwrap();
        let c = db.connection().unwrap();
        let series = coupon_engine::series(&c, series_id).unwrap();
        assert_eq!(series.current_step, 1);
        assert_eq!(series.latest_coupon_id, Some(coupon_id));
        assert_eq!(series.history[0].result, "UNSETTLED");
        let empty = candidate_engine::generate(
            &c,
            &candidate_engine::GenerateRequest {
                business_date: Some("2026-09-03".into()),
                generation_time: Some("2026-09-03T12:00:00Z".into()),
                category: None,
                dry_run: Some(false),
            },
        )
        .unwrap();
        assert!(empty.candidates.is_empty());
        assert_eq!(coupon_engine::series(&c, series_id).unwrap(), series);
        // An attached pending step is independent of a later publication.
        // Simulate publication replacement without modifying immutable coupon inputs.
        c.execute("INSERT INTO daily_output_publications(business_date,candidate_run_id) VALUES('2026-09-02',?1) ON CONFLICT(business_date) DO UPDATE SET candidate_run_id=excluded.candidate_run_id",[empty.run_id]).unwrap();
        let visible = coupon_engine::get_daily(&c, "2026-09-02", Some("DAILY_COMPOUND")).unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, coupon_id);
        assert_eq!(visible[0].publication_status, "READY");
        assert_eq!(
            coupon_engine::generate_compound_step(&c, "2026-09-03", None)
                .unwrap()
                .id,
            coupon_id
        );
    }
}
