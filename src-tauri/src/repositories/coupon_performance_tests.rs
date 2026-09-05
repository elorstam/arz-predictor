use super::{
    candidate_engine, candidate_engine_acceptance_tests, coupon_engine::SettlementResult,
    coupon_performance::*,
};
use rusqlite::{params, Connection};

fn add_coupon(
    c: &Connection,
    id: i64,
    run: i64,
    kind: &str,
    date: &str,
    status: &str,
    odds: f64,
    stake: i64,
    result: Option<SettlementResult>,
) {
    let metadata = result
        .map(|x| serde_json::json!({"settlement":x}))
        .unwrap_or_else(|| serde_json::json!({}));
    c.execute("INSERT INTO phase8_coupons(id,coupon_type,business_date,source_candidate_run_id,policy_version,model_version,calibration_version,generation_cutoff,status,combined_decimal_odd,system_sizes_json,unit_stake_cents,total_stake_cents,metadata_json,settled_at,settlement_result) VALUES(?1,?2,?3,?4,?5,'model_v1','cal_v1',?6,?7,?8,CASE WHEN ?2='DAILY_SURPRISE_SYSTEM' THEN '[5,6]' END,1000,?9,?10,CASE WHEN ?7='SETTLED' THEN ?6 END,json_extract(?10,'$.settlement.status'))",
        params![id,kind,date,run,format!("p{id}"),format!("{date}T13:00:00Z"),status,odds,stake,metadata.to_string()]).unwrap();
}
fn result(
    id: i64,
    status: &str,
    columns: (usize, usize, usize, usize),
    stake: i64,
    gross: i64,
) -> SettlementResult {
    SettlementResult {
        coupon_id: id,
        status: status.into(),
        winning_columns: columns.0,
        losing_columns: columns.1,
        void_columns: columns.2,
        unsettled_columns: 0,
        total_columns: columns.3,
        total_stake_cents: stake,
        gross_return_cents: gross,
        profit_loss_cents: gross - stake,
    }
}
fn fixture() -> crate::database::Database {
    let db = candidate_engine_acceptance_tests::db();
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
    .unwrap()
    .run_id;
    add_coupon(
        &c,
        1,
        run,
        "DAILY_HIGH_CONFIDENCE",
        "2026-09-01",
        "SETTLED",
        1.60,
        10000,
        Some(result(1, "WON", (1, 0, 0, 1), 10000, 16000)),
    );
    add_coupon(
        &c,
        2,
        run,
        "DAILY_OVER_25",
        "2026-08-20",
        "SETTLED",
        2.50,
        10000,
        Some(result(2, "LOST", (0, 1, 0, 1), 10000, 0)),
    );
    add_coupon(
        &c,
        3,
        run,
        "DAILY_OVER_25",
        "2026-09-02",
        "SETTLED",
        1.40,
        5000,
        Some(result(3, "VOID", (0, 0, 1, 1), 5000, 5000)),
    );
    add_coupon(
        &c,
        4,
        run,
        "DAILY_HIGH_CONFIDENCE",
        "2026-09-02",
        "FINALIZED",
        1.80,
        12000,
        None,
    );
    add_coupon(
        &c,
        5,
        run,
        "DAILY_BTTS",
        "2026-09-02",
        "DRAFT",
        3.20,
        10000,
        None,
    );
    add_coupon(
        &c,
        6,
        run,
        "DAILY_SURPRISE_SYSTEM",
        "2026-09-03",
        "SETTLED",
        6.00,
        7000,
        Some(result(6, "WON", (1, 6, 0, 7), 7000, 9000)),
    );
    add_coupon(
        &c,
        7,
        run,
        "DAILY_SURPRISE_SYSTEM",
        "2026-08-25",
        "SETTLED",
        12.00,
        7000,
        Some(result(7, "VOID", (0, 7, 0, 7), 7000, 0)),
    );
    add_coupon(
        &c,
        8,
        run,
        "DAILY_CORNERS",
        "2026-06-01",
        "SETTLED",
        1.20,
        0,
        Some(result(8, "VOID", (0, 0, 1, 1), 0, 0)),
    );
    c.execute("INSERT INTO phase8_compound_series(id,business_date,status,current_step,starting_stake_cents,current_stake_cents,completed_steps,reset_count,metadata_json) VALUES(1,'2026-08-20','ACTIVE',3,10000,25600,2,1,'{}'),(2,'2026-08-21','COMPLETED',7,10000,26800,7,0,'{}')",[]).unwrap();
    let mut id = 20;
    let mut add_step = |series: i64, step: i64, date: &str, stake: i64, state: &str, gross: i64| {
        let coupon_status = if state == "UNSETTLED" {
            "FINALIZED"
        } else {
            "SETTLED"
        };
        let settlement = (state != "UNSETTLED").then(|| {
            result(
                id,
                state,
                (
                    usize::from(state == "WON"),
                    usize::from(state == "LOST"),
                    usize::from(state == "VOID"),
                    1,
                ),
                stake,
                gross,
            )
        });
        add_coupon(
            &c,
            id,
            run,
            "DAILY_COMPOUND",
            date,
            coupon_status,
            1.60,
            stake,
            settlement,
        );
        c.execute(
            "UPDATE phase8_coupons SET series_id=?2,step_number=?3 WHERE id=?1",
            params![id, series, step],
        )
        .unwrap();
        c.execute("INSERT INTO phase8_series_steps(series_id,step_number,coupon_id,business_date,stake_cents,combined_odd,potential_return_cents,result,settled_at) VALUES(?1,?2,?3,?4,?5,1.6,?6,?7,CASE WHEN ?7='UNSETTLED' THEN NULL ELSE ?4||'T13:00:00Z' END)",params![series,step,id,date,stake,(stake as f64*1.6).round() as i64,state]).unwrap();
        id += 1;
    };
    add_step(1, 1, "2026-08-20", 10000, "LOST", 0);
    add_step(1, 1, "2026-08-22", 10000, "WON", 16000);
    add_step(1, 2, "2026-08-23", 16000, "WON", 25600);
    add_step(1, 3, "2026-08-24", 25600, "UNSETTLED", 0);
    let mut stake = 10000;
    for step in 1..=7 {
        let gross = if step == 7 {
            26800
        } else {
            (stake as f64 * 1.1).round() as i64
        };
        add_step(
            2,
            step,
            &format!("2026-08-{}", 24 + step),
            stake,
            "WON",
            gross,
        );
        stake = gross;
    }
    drop(c);
    db
}
fn request(window: super::model_performance::PerformanceWindow) -> CouponPerformanceRequest {
    CouponPerformanceRequest {
        window,
        coupon_type: None,
        as_of: Some("2026-09-04".into()),
        include_system: None,
        include_katlama: None,
    }
}

#[test]
fn phase10_1_ordinary_economics_windows_types_and_odds_bands() {
    let db = fixture();
    let c = db.connection().unwrap();
    let all = get(
        &c,
        &request(super::model_performance::PerformanceWindow::AllTime),
    )
    .unwrap();
    assert_eq!(all.financial_mode, FinancialMode::Realized);
    assert_eq!(all.overall.coupon_count, 7);
    assert_eq!(all.overall.settled_count, 6);
    assert_eq!(
        (
            all.overall.won_count,
            all.overall.lost_count,
            all.overall.void_count
        ),
        (2, 2, 2)
    );
    assert_eq!(all.overall.hit_rate, Some(0.5));
    assert_eq!(
        (
            all.overall.total_stake_cents,
            all.overall.total_gross_return_cents,
            all.overall.total_net_pnl_cents
        ),
        (39000, 30000, -9000)
    );
    assert_eq!(all.overall.roi, Some(-9000.0 / 39000.0));
    assert_eq!(all.overall.average_stake_cents, Some(6500.0));
    assert!(all
        .by_coupon_type
        .iter()
        .any(|x| x.coupon_type == "DAILY_OVER_25" && x.metrics.coupon_count == 2));
    assert!(all
        .by_coupon_type
        .iter()
        .any(|x| { x.coupon_type == "DAILY_SURPRISE_SYSTEM" && x.metrics.hit_rate == Some(0.5) }));
    assert!(all
        .by_odds_band
        .iter()
        .any(|x| x.odds_band == "10.00+" && x.metrics.roi == Some(-1.0)));
    let d7 = get(
        &c,
        &request(super::model_performance::PerformanceWindow::Last7Days),
    )
    .unwrap();
    assert_eq!(d7.from_date.as_deref(), Some("2026-08-29"));
    assert_eq!(d7.overall.coupon_count, 4);
    let d30 = get(
        &c,
        &request(super::model_performance::PerformanceWindow::Last30Days),
    )
    .unwrap();
    assert_eq!(d30.overall.coupon_count, 6);
    let season = get(
        &c,
        &request(super::model_performance::PerformanceWindow::Season),
    )
    .unwrap();
    assert_eq!(season.from_date.as_deref(), Some("2026-07-01"));
    assert_eq!(season.overall.coupon_count, 6);
}

#[test]
fn phase10_1_system_uses_persisted_columns_and_total_stake_once() {
    let db = fixture();
    let c = db.connection().unwrap();
    let report = get(
        &c,
        &request(super::model_performance::PerformanceWindow::AllTime),
    )
    .unwrap();
    let s = report.system_summary.unwrap();
    assert_eq!(
        (s.system_coupon_count, s.settled_system_coupon_count),
        (2, 2)
    );
    assert_eq!(
        (
            s.total_columns,
            s.winning_columns,
            s.losing_columns,
            s.void_columns
        ),
        (14, 1, 13, 0)
    );
    assert_eq!(
        (
            s.total_stake_cents,
            s.total_gross_return_cents,
            s.net_pnl_cents
        ),
        (14000, 9000, -5000)
    );
    assert_eq!(s.roi, Some(-5000.0 / 14000.0));
}

#[test]
fn phase10_1_katlama_rollover_counts_only_step_one_external_stake() {
    let db = fixture();
    let c = db.connection().unwrap();
    let k = get(
        &c,
        &request(super::model_performance::PerformanceWindow::AllTime),
    )
    .unwrap()
    .katlama_summary
    .unwrap();
    assert_eq!(
        (
            k.sequences_started,
            k.active_sequences,
            k.failed_sequences,
            k.completed_sequences
        ),
        (3, 1, 1, 1)
    );
    assert_eq!(k.highest_reached_step, Some(7));
    assert!((k.average_reached_step.unwrap() - 11.0 / 3.0).abs() < 1e-12);
    assert_eq!(
        (
            k.total_initial_stake_cents,
            k.realized_initial_stake_cents,
            k.total_realized_return_cents,
            k.net_pnl_cents
        ),
        (30000, 20000, 26800, 6800)
    );
    assert_eq!(k.roi, Some(6800.0 / 20000.0));
    assert_eq!(
        (
            k.by_step[0].attempts,
            k.by_step[0].wins,
            k.by_step[0].losses
        ),
        (3, 2, 1)
    );
    assert_eq!(k.by_step[6].attempts, 1);
}

#[test]
fn phase10_1_filters_serialization_zero_safety_and_read_only() {
    let db = fixture();
    let c = db.connection().unwrap();
    let before = c.total_changes();
    let mut r = request(super::model_performance::PerformanceWindow::AllTime);
    r.coupon_type = Some("DAILY_OVER_25".into());
    r.include_system = Some(false);
    r.include_katlama = Some(false);
    let report = get(&c, &r).unwrap();
    assert_eq!(report.overall.coupon_count, 2);
    assert!(report.system_summary.is_none() && report.katlama_summary.is_none());
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["financial_mode"], "REALIZED");
    let text = json.to_string();
    assert!(!text.contains("brier") && !text.contains("log_loss") && !text.contains("ece"));
    assert_eq!(before, c.total_changes());
    let mut zero = request(super::model_performance::PerformanceWindow::AllTime);
    zero.coupon_type = Some("DAILY_CORNERS".into());
    zero.include_katlama = Some(false);
    let zero = get(&c, &zero).unwrap();
    assert_eq!(zero.overall.total_stake_cents, 0);
    assert_eq!(zero.overall.roi, None);
    assert_eq!(zero.overall.hit_rate, None);
}
