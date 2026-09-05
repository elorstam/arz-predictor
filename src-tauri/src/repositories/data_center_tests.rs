use chrono::{TimeZone, Utc};

use super::{candidate_engine_acceptance_tests, data_center};
use crate::database::Database;

fn now(day: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, day, 12, 0, 0).unwrap()
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
        data_center::ReadinessState::Unavailable
    );
    assert_eq!(
        first.lineup_provider.status,
        data_center::ReadinessState::Unavailable
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
    let fresh = data_center::status(&c, &runtime, now(2)).unwrap();
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
        data_center::ReadinessState::Unavailable
    );
    assert_eq!(
        fresh.lineup_provider.status,
        data_center::ReadinessState::Unavailable
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
    assert_eq!(report.historical_data.progress, Some(0.5));
    assert_eq!(report.historical_data.downloaded_bytes, Some(2048));
    assert_eq!(
        report.historical_data.technical_reason.as_deref(),
        Some("HISTORICAL_IMPORT_FAILED")
    );
    assert_eq!(
        report.team_logos.status,
        data_center::ReadinessState::Updating
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
