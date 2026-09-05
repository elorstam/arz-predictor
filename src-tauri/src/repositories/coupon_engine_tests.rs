use super::{candidate_engine, candidate_engine_acceptance_tests, coupon_engine};
use crate::database::Database;
use rusqlite::params;

fn fixture() -> Database {
    fixture_with_odd(2.60)
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
        c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json) VALUES(?1,'fe_v1','2026-09-02T11:00:00Z','{}',?2)",params![id, r#"{"history_matches_home":10,"history_matches_away":10}"#]).unwrap();
        c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability) VALUES(?1,'TOTAL_GOALS','OVER',2.5,0.80,'p8','2026-09-02T18:00:00Z',1,0.80,0.80,'CALIBRATED_V1','cal8',25,'AVAILABLE')",[id]).unwrap();
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
    c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,calibration_bucket,bucket_sample_size,availability) VALUES(?1,?2,?3,?4,0.80,'phase8-conflict','2026-09-02T12:00:00Z',1,0.80,0.80,'CALIBRATED_V1','cal_v1','0.8',25,'AVAILABLE')", rusqlite::params![match_id, market, selection, line]).unwrap();
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
    assert_eq!(s.current_step, 1);
    let s = coupon_engine::settle_series(&c, s.id, true, 16000).unwrap();
    assert_eq!(s.current_step, 2);
    let s = coupon_engine::settle_series(&c, s.id, false, 0).unwrap();
    assert_eq!(s.current_step, 1);
    assert_eq!(s.current_stake_cents, 10000);
    assert_eq!(s.reset_count, 1);
    coupon_engine::cancel_series(&c, s.id).unwrap();
    let final_series = coupon_engine::start_series(&c, "2026-09-02", 10000).unwrap();
    for _ in 0..6 {
        coupon_engine::settle_series(&c, final_series.id, true, 16000).unwrap();
    }
    let completed = coupon_engine::settle_series(&c, final_series.id, true, 16000).unwrap();
    assert_eq!(completed.status, "COMPLETED");
    assert_eq!(completed.current_step, 7);
    assert!(coupon_engine::generate_compound_step(&c, "2026-09-02", Some(run.run_id)).is_err());
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
    let one = source
        .candidates
        .iter()
        .find(|x| x.category == "OVER_25")
        .unwrap()
        .id;
    let draft = coupon_engine::create_draft(
        &c,
        &coupon_engine::DraftRequest {
            business_date: "2026-09-02".into(),
            candidate_run_id: run.run_id,
            coupon_type: "DAILY_OVER_25".into(),
            candidate_ids: vec![one],
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap();
    coupon_engine::finalize(&c, draft.id).unwrap();
    let won = coupon_engine::settle(
        &c,
        &coupon_engine::SettlementRequest {
            coupon_id: draft.id,
            outcomes: vec![coupon_engine::SelectionOutcome {
                candidate_id: one,
                result: "WON".into(),
            }],
            settled_at: "2026-09-02T13:00:00Z".into(),
        },
    )
    .unwrap();
    assert_eq!(won.status, "WON");
    assert_eq!(won.gross_return_cents, 2600);
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
    let high = source
        .candidates
        .iter()
        .find(|x| x.category == "HIGH_CONFIDENCE")
        .unwrap()
        .id;
    let void_draft = coupon_engine::create_draft(
        &c,
        &coupon_engine::DraftRequest {
            business_date: "2026-09-02".into(),
            candidate_run_id: run.run_id,
            coupon_type: "DAILY_HIGH_CONFIDENCE".into(),
            candidate_ids: vec![high],
            unit_stake_cents: Some(1000),
        },
    )
    .unwrap();
    coupon_engine::finalize(&c, void_draft.id).unwrap();
    let voided = coupon_engine::settle(
        &c,
        &coupon_engine::SettlementRequest {
            coupon_id: void_draft.id,
            outcomes: vec![coupon_engine::SelectionOutcome {
                candidate_id: high,
                result: "VOID".into(),
            }],
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
                .take(if kind == "DAILY_COMPOUND" { 2 } else { 1 })
                .map(|x| x.id)
                .collect();
            if ids.len() < if kind == "DAILY_COMPOUND" { 2 } else { 1 } {
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
    let after_void = coupon_engine::settle_series_result(&c, series.id, "VOID", 0).unwrap();
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
        c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json) VALUES(?1,'fe_v1','2026-09-02T12:00:00Z','{}','{\"history_matches_home\":8,\"history_matches_away\":8}')", [id]).unwrap();
        c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability) VALUES(?1,'TOTAL_GOALS','OVER',2.5,0.80,'p8','2026-09-02T12:00:00Z',1,0.80,0.80,'CALIBRATED_V1','cal_v1',25,'AVAILABLE')", [id]).unwrap();
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
    assert_eq!(coupons.len(), 7);
    for kind in [
        "DAILY_CORNERS",
        "DAILY_OVER_25",
        "DAILY_OVER_35",
        "DAILY_BTTS",
        "DAILY_HIGH_CONFIDENCE",
        "DAILY_SURPRISE_SYSTEM",
        "DAILY_COMPOUND",
    ] {
        assert!(
            coupons
                .iter()
                .any(|x| x.coupon_type == kind && !x.candidate_ids.is_empty()),
            "missing {kind}"
        );
    }
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
        c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json) VALUES(?1,'fe_v1','2026-09-02T12:00:00Z','{}','{\"history_matches_home\":8,\"history_matches_away\":8}')", [match_id]).unwrap();
        c.execute("INSERT INTO predictions(match_id,market,selection,line_value,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,calibration_version,bucket_sample_size,availability) VALUES(?1,'TOTAL_GOALS','OVER',2.5,0.80,'bench','2026-09-02T12:00:00Z',1,0.80,0.80,'CALIBRATED_V1','cal_v1',25,'AVAILABLE')", [match_id]).unwrap();
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
    assert_eq!(final_state.status, "COMPLETED");
    assert_eq!(final_state.current_step, 7);
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
