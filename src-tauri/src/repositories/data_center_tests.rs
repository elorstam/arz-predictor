use chrono::{TimeZone, Utc};

use super::{candidate_engine_acceptance_tests, data_center};
use crate::database::Database;

fn now(day: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, day, 12, 0, 0).unwrap()
}

#[test]
fn failed_daily_production_is_visible_until_automatic_retry_completes() {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    c.execute("INSERT INTO daily_refresh_audit(business_date,started_at,status,error) VALUES('2026-09-04','2026-09-04T10:00:00Z','FAILED','TRANSIENT_DB_BUSY')",[]).unwrap();
    let failed = data_center::status(&c, &Default::default(), now(4)).unwrap();
    assert!(!failed.can_generate_candidates.ready);
    assert!(!failed.can_generate_coupons.ready);
    assert!(failed
        .can_generate_candidates
        .reasons
        .iter()
        .any(|x| x.contains("TRANSIENT_DB_BUSY")));
    c.execute(
        "UPDATE daily_refresh_audit SET status='COMPLETED',error=NULL",
        [],
    )
    .unwrap();
    let recovered = data_center::status(&c, &Default::default(), now(4)).unwrap();
    assert!(!recovered
        .can_generate_candidates
        .reasons
        .iter()
        .any(|x| x.contains("TRANSIENT_DB_BUSY")));
}

#[test]
fn phase12_empty_database_is_durable_first_run_and_read_only() {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    let before = c.total_changes();
    let first = data_center::status(&c, &data_center::RuntimeReadiness::default(), now(4)).unwrap();
    let again = data_center::status(&c, &data_center::RuntimeReadiness::default(), now(4)).unwrap();
    assert!(first.first_run);
    assert_eq!(
        first.overall_status,
        data_center::ReadinessState::ActionRequired
    );
    assert_eq!(first.database.status, data_center::ReadinessState::Ready);
    assert_eq!(
        first.historical_data.status,
        data_center::ReadinessState::ActionRequired
    );
    assert_eq!(
        first.iddaa.status,
        data_center::ReadinessState::ActionRequired
    );
    assert_eq!(
        first.odds.status,
        data_center::ReadinessState::ActionRequired
    );
    assert_eq!(
        first.popularity.status,
        data_center::ReadinessState::Unavailable
    );
    assert_eq!(
        first.prediction_model.status,
        data_center::ReadinessState::ActionRequired
    );
    assert_eq!(
        first.calibration.status,
        data_center::ReadinessState::ActionRequired
    );
    assert_eq!(
        first.lineup_model.status,
        data_center::ReadinessState::Optional
    );
    assert_eq!(
        first.lineup_provider.status,
        data_center::ReadinessState::Optional
    );
    assert!(
        !first.can_generate_predictions.ready
            && !first.can_generate_candidates.ready
            && !first.can_generate_coupons.ready
    );
    assert_eq!(first, again);
    assert_eq!(before, c.total_changes());
}

#[test]
fn phase12_freshness_optional_dependencies_and_compatibility_are_explicit() {
    let db = candidate_engine_acceptance_tests::db();
    let c = db.connection().unwrap();
    let runtime = data_center::RuntimeReadiness {
        historical_dataset_count: 3,
        historical_cached_count: 3,
        historical_imported_count: 3,
        historical_cached_bytes: 123_456,
        asset_state: "IDLE".into(),
        ..Default::default()
    };
    // The fixture includes an intentionally future 13:00 quote.
    let future_quote = data_center::status(&c, &runtime, now(2)).unwrap();
    assert_ne!(future_quote.odds.status, data_center::ReadinessState::Ready);
    let fresh = data_center::status(&c, &runtime, now(2) + chrono::Duration::hours(1)).unwrap();
    assert!(!fresh.first_run);
    assert_eq!(fresh.odds.status, data_center::ReadinessState::Ready);
    assert!(fresh.iddaa.metrics.contains_key("competition_count"));
    assert_eq!(fresh.historical_data.metrics["cached_datasets"], 3);
    assert_eq!(
        fresh.popularity.status,
        data_center::ReadinessState::Unavailable
    );
    assert_eq!(
        fresh.team_logos.status,
        data_center::ReadinessState::Optional
    );
    assert_eq!(
        fresh.lineup_provider.status,
        data_center::ReadinessState::Optional
    );
    assert_eq!(
        fresh.calibration.status,
        data_center::ReadinessState::ActionRequired
    );
    assert!(fresh
        .can_generate_predictions
        .reasons
        .iter()
        .all(|x| !x.contains("Popüler") && !x.contains("logo") && !x.contains("11")));
    let stale = data_center::status(&c, &runtime, now(4)).unwrap();
    assert_eq!(stale.odds.status, data_center::ReadinessState::Partial);
    assert_eq!(stale.odds.technical_reason.as_deref(), Some("STALE_ODDS"));
}

#[test]
fn phase12_updating_progress_failures_and_json_contract_are_honest() {
    let db = candidate_engine_acceptance_tests::db();
    let c = db.connection().unwrap();
    let runtime = data_center::RuntimeReadiness {
        historical_dataset_count: 6,
        historical_cached_count: 3,
        historical_imported_count: 2,
        historical_missing_count: 3,
        historical_failed_count: 1,
        historical_cached_bytes: 2048,
        asset_state: "DOWNLOADING".into(),
        asset_bytes_this_run: 512,
        lineup_provider_configured: false,
    };
    let report = data_center::status(&c, &runtime, now(4)).unwrap();
    assert_eq!(report.historical_data.progress, None);
    assert_eq!(report.historical_data.downloaded_bytes, Some(2048));
    assert_eq!(
        report.historical_data.technical_reason.as_deref(),
        Some("HISTORICAL_IMPORT_FAILED")
    );
    assert_eq!(
        report.team_logos.status,
        data_center::ReadinessState::Background
    );
    assert_eq!(
        report.overall_status,
        data_center::ReadinessState::ActionRequired
    );
    let json = serde_json::to_value(&report).unwrap();
    assert!(json.get("overall_status").is_some() && json.get("can_generate_predictions").is_some());
    assert_eq!(
        json["lineup_provider"]["technical_reason"],
        "LIVE_LINEUP_PROVIDER_NOT_CONFIGURED"
    );
}

#[test]
fn phase12_offline_refresh_failure_is_isolated_and_retryable() {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    c.execute("INSERT INTO data_import_runs(provider,dataset_key,season,source_url,completed_at,status,error_message) VALUES('iddaa','iddaa:football-bulletin','2026/27','remote','2026-09-04T11:00:00Z','failed','NETWORK_UNAVAILABLE')",[]).unwrap();
    let before = c.total_changes();
    let report =
        data_center::status(&c, &data_center::RuntimeReadiness::default(), now(4)).unwrap();
    assert_eq!(report.iddaa.status, data_center::ReadinessState::Error);
    assert!(report.iddaa.retryable);
    assert_eq!(
        report.iddaa.technical_reason.as_deref(),
        Some("NETWORK_UNAVAILABLE")
    );
    assert_eq!(report.database.status, data_center::ReadinessState::Ready);
    assert_eq!(
        report.popularity.status,
        data_center::ReadinessState::Unavailable
    );
    assert_eq!(before, c.total_changes());
}

#[test]
fn historical_unresolved_backlog_does_not_gate_resolved_current_match() {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    c.execute(
        "INSERT INTO competitions(id,name,current_season) VALUES(1,'Current','2026/27')",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO teams(id,normalized_name) VALUES(1,'Home'),(2,'Away')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(1,1,'2026/27',1,2,'2026-09-04T15:00:00Z','scheduled','2026-09-04',1)", []).unwrap();
    c.execute("INSERT INTO provider_match_mappings(match_id,provider,external_match_id) VALUES(1,'iddaa','current')", []).unwrap();
    c.execute("INSERT INTO provider_team_mappings(team_id,provider,external_team_id,external_team_name) VALUES(1,'iddaa','home','Home'),(1,'football-data.co.uk','fd-home','Home'),(2,'iddaa','away','Away'),(2,'football-data.co.uk','fd-away','Away')", []).unwrap();
    c.execute("INSERT INTO provider_competition_metadata(provider,external_competition_id,external_name,resolution_status) VALUES('old-provider','old-league','Old League','unresolved')", []).unwrap();
    c.execute("INSERT INTO provider_competition_mappings(provider,external_competition_id,competition_id) VALUES('football-data.co.uk','TEST',1)",[]).unwrap();

    let report =
        data_center::status(&c, &data_center::RuntimeReadiness::default(), now(4)).unwrap();
    assert_eq!(
        report.entity_resolution.status,
        data_center::ReadinessState::Ready
    );
    assert_eq!(report.entity_resolution.metrics["current_match_count"], 1);
    assert_eq!(
        report.entity_resolution.metrics["current_resolved_match_count"],
        1
    );
    assert_eq!(
        report.entity_resolution.metrics["current_unresolved_match_count"],
        0
    );
    assert!(!report
        .can_generate_predictions
        .reasons
        .iter()
        .any(|reason| reason.contains("eşleştirme")));
}

#[test]
fn core_score_is_ten_required_checks_and_every_blocker_drops_ready() {
    use data_center::{CoreCheck, ReadinessState as S};
    let mut checks: Vec<_> = [
        "database",
        "live_iddaa",
        "history",
        "base_model",
        "calibration",
        "features",
        "resolution",
        "candidates",
        "coupons",
        "popularity",
    ]
    .into_iter()
    .map(|id| CoreCheck {
        id: id.into(),
        title: id.into(),
        status: S::Ready,
        reasons: vec![],
    })
    .collect();
    assert_eq!(data_center::core_overall(&checks), S::Ready);
    for i in 0..checks.len() {
        checks[i].status = S::ActionRequired;
        assert_ne!(data_center::core_overall(&checks), S::Ready);
        checks[i].status = S::Ready;
    }
    assert_ne!(data_center::core_overall(&checks[..9]), S::Ready);
}

#[test]
fn resolution_blocks_future_supported_only_and_optional_work_is_independent() {
    use data_center::ReadinessState as S;
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    c.execute("INSERT INTO competitions(id,name,country) VALUES(1,'Supported','Test'),(2,'Outside','Test')",[]).unwrap();
    c.execute(
        "INSERT INTO teams(id,normalized_name) VALUES(1,'Home'),(2,'Away')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO provider_competition_mappings(provider,external_competition_id,competition_id) VALUES('football-data.co.uk','TEST',1)",[]).unwrap();
    for (id, competition, day, status) in [
        (1, 1, "2026-09-03", "scheduled"),
        (2, 1, "2026-09-05", "cancelled"),
        (3, 2, "2026-09-05", "scheduled"),
        (4, 1, "2026-09-05", "scheduled"),
    ] {
        c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(?1,?2,'2026/27',1,2,?3,?4,?5,1)",rusqlite::params![id,competition,format!("{day}T15:0{id}:00Z"),status,day]).unwrap();
        c.execute("INSERT INTO provider_match_mappings(match_id,provider,external_match_id) VALUES(?1,'iddaa',?2)",rusqlite::params![id,id.to_string()]).unwrap();
    }
    let a = data_center::status(&c, &Default::default(), now(4)).unwrap();
    assert_eq!(a.entity_resolution.metrics["unresolved_count"], 3);
    assert_eq!(a.entity_resolution.metrics["blocking_unresolved"], 1);
    assert_eq!(a.entity_resolution.status, S::ActionRequired);
    c.execute("UPDATE matches SET status='cancelled' WHERE id=4", [])
        .unwrap();
    let historical_only = data_center::status(&c, &Default::default(), now(4)).unwrap();
    assert_eq!(
        historical_only.entity_resolution.metrics["unresolved_count"],
        3
    );
    assert_eq!(
        historical_only.entity_resolution.metrics["blocking_unresolved"],
        0
    );
    assert_eq!(historical_only.entity_resolution.status, S::Ready);
    c.execute("INSERT INTO provider_team_mappings(team_id,provider,external_team_id,external_team_name) VALUES(1,'football-data.co.uk','home','Home'),(2,'football-data.co.uk','away','Away')",[]).unwrap();
    let b = data_center::status(
        &c,
        &data_center::RuntimeReadiness {
            asset_state: "DOWNLOADING".into(),
            ..Default::default()
        },
        now(4),
    )
    .unwrap();
    assert_eq!(b.entity_resolution.status, S::Ready);
    assert_eq!(b.entity_resolution.metrics["blocking_unresolved"], 0);
    assert_eq!(b.lineup_model.status, S::Optional);
    assert_eq!(b.team_logos.status, S::Background);
    assert_eq!(b.core_check_count, 10);
    assert!(b
        .core_checks
        .iter()
        .all(|x| !x.id.contains("lineup") && !x.id.contains("logo")));
    assert!(b.core_ready_count < 10); // Missing history, artifacts and outputs remain real blockers.
}
