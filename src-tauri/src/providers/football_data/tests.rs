use rusqlite::OptionalExtension;

use super::{
    acquisition::categorize_error_details,
    bootstrap::{bootstrap, validate_request, BootstrapRequest},
    catalog::{find_dataset, supported_datasets},
    orchestrator::{import_fixture, import_local_dataset},
};
use crate::{database::Database, repositories::stats};

const COMPLETE_CSV: &str = "Div,Date,Time,HomeTeam,AwayTeam,FTHG,FTAG,FTR,HTHG,HTAG,HTR,HS,AS,HST,AST,HF,AF,HC,AC,HY,AY,HR,AR\nE0,15/08/2026,15:00,Arsenal,Chelsea,2,1,H,1,0,H,14,10,6,4,9,12,7,3,1,2,0,0\n";
const MISSING_OPTIONAL_CSV: &str =
    "Div,Date,Time,HomeTeam,AwayTeam,FTHG,FTAG,FTR\nE0,16/08/2026,14:00,Liverpool,Everton,1,0,H\n";
const SCHEDULED_CSV: &str =
    "Div,Date,Time,HomeTeam,AwayTeam,FTHG,FTAG,HS,HST\nE0,17/08/2026,20:00,Leeds,Newcastle,,,,\n";
const FINISHED_CSV: &str = "Div,Date,Time,HomeTeam,AwayTeam,FTHG,FTAG,HS,HST\nE0,17/08/2026,20:00,Leeds,Newcastle,3,2,11,5\n";

fn local_request(files: std::collections::HashMap<String, String>) -> BootstrapRequest {
    BootstrapRequest {
        preset: None,
        league_codes: vec!["E0".to_string(), "E1".to_string()],
        season_codes: vec!["2627".to_string()],
        acquisition_mode: "local".to_string(),
        local_files: files,
    }
}

fn dataset() -> &'static super::DatasetDefinition {
    find_dataset("E0", "2627").expect("E0 2627 should be registered")
}

#[test]
fn parses_local_date_and_time_into_utc() {
    let database = Database::open_in_memory().expect("database should migrate");
    import_fixture(&database, dataset(), COMPLETE_CSV.as_bytes()).expect("import should succeed");
    let connection = database.connection().expect("database lock should work");
    let (kickoff, local_date, time_known): (String, String, bool) = connection
        .query_row(
            "SELECT kickoff_at, scheduled_local_date, kickoff_time_known FROM matches",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("match should be stored");
    assert_eq!(kickoff, "2026-08-15T14:00:00Z");
    assert_eq!(local_date, "2026-08-15");
    assert!(time_known);
}

#[test]
fn complete_row_creates_normalized_graph_and_statistics() {
    let database = Database::open_in_memory().expect("database should migrate");
    let summary = import_fixture(&database, dataset(), COMPLETE_CSV.as_bytes())
        .expect("import should succeed");
    assert_eq!(summary.competitions_created, 1);
    assert_eq!(summary.teams_created, 2);
    assert_eq!(summary.matches_inserted, 1);
    assert_eq!(summary.statistics_inserted, 1);
    assert_eq!(summary.rows_failed, 0);

    let connection = database.connection().expect("database lock should work");
    for (table, expected) in [
        ("competitions", 1),
        ("provider_competition_mappings", 1),
        ("teams", 2),
        ("provider_team_mappings", 2),
        ("matches", 1),
        ("provider_match_mappings", 1),
        ("match_statistics", 1),
    ] {
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count should load");
        assert_eq!(count, expected, "unexpected count for {table}");
    }
    let values: (i64, i64, i64, i64, i64, i64) = connection
        .query_row(
            "SELECT home_shots, away_shots, home_shots_on_target, away_shots_on_target,
                    home_corners, away_corners FROM match_statistics",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .expect("statistics should load");
    assert_eq!(values, (14, 10, 6, 4, 7, 3));
}

#[test]
fn missing_optional_columns_import_as_null() {
    let database = Database::open_in_memory().expect("database should migrate");
    let summary = import_fixture(&database, dataset(), MISSING_OPTIONAL_CSV.as_bytes())
        .expect("import should succeed");
    assert_eq!(summary.matches_inserted, 1);
    assert_eq!(summary.statistics_inserted, 0);
    assert_eq!(summary.rows_failed, 0);
    let connection = database.connection().expect("database lock should work");
    let halftime: (Option<i64>, Option<i64>) = connection
        .query_row(
            "SELECT halftime_home_goals, halftime_away_goals FROM matches",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("match should load");
    assert_eq!(halftime, (None, None));
}

#[test]
fn missing_kickoff_time_uses_flagged_deterministic_fallback() {
    let csv = "Div,Date,HomeTeam,AwayTeam,FTHG,FTAG\nE0,18/08/2026,Fulham,Burnley,,\n";
    let database = Database::open_in_memory().expect("database should migrate");
    import_fixture(&database, dataset(), csv.as_bytes()).expect("import should succeed");
    let connection = database.connection().expect("database lock should work");
    let (kickoff, known): (String, bool) = connection
        .query_row(
            "SELECT kickoff_at, kickoff_time_known FROM matches",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("match should load");
    assert_eq!(kickoff, "2026-08-18T11:00:00Z");
    assert!(!known);
}

#[test]
fn known_kickoff_time_enriches_date_only_match_without_duplication() {
    let without_time = "Div,Date,HomeTeam,AwayTeam,FTHG,FTAG\nE0,18/08/2026,Fulham,Burnley,,\n";
    let with_time =
        "Div,Date,Time,HomeTeam,AwayTeam,FTHG,FTAG\nE0,18/08/2026,19:45,Fulham,Burnley,2,0\n";
    let database = Database::open_in_memory().expect("database should migrate");
    import_fixture(&database, dataset(), without_time.as_bytes()).expect("import should succeed");
    let summary = import_fixture(&database, dataset(), with_time.as_bytes())
        .expect("enrichment should succeed");
    assert_eq!(summary.matches_inserted, 0);
    assert_eq!(summary.matches_updated, 1);
    let connection = database.connection().expect("database lock should work");
    let (count, kickoff, known): (i64, String, bool) = connection
        .query_row(
            "SELECT COUNT(*), kickoff_at, kickoff_time_known FROM matches",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("match should load");
    assert_eq!(count, 1);
    assert_eq!(kickoff, "2026-08-18T18:45:00Z");
    assert!(known);
}

#[test]
fn reimport_is_idempotent() {
    let database = Database::open_in_memory().expect("database should migrate");
    import_fixture(&database, dataset(), COMPLETE_CSV.as_bytes()).expect("import should succeed");
    let second = import_fixture(&database, dataset(), COMPLETE_CSV.as_bytes())
        .expect("reimport should succeed");
    assert_eq!(second.competitions_found, 1);
    assert_eq!(second.teams_found, 2);
    assert_eq!(second.matches_inserted, 0);
    assert_eq!(second.matches_updated, 1);
    assert_eq!(second.statistics_updated, 1);

    let connection = database.connection().expect("database lock should work");
    for table in [
        "competitions",
        "provider_competition_mappings",
        "matches",
        "provider_match_mappings",
        "match_statistics",
    ] {
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count should load");
        assert_eq!(count, 1, "reimport duplicated {table}");
    }
}

#[test]
fn reimport_updates_scheduled_match_and_fills_statistics() {
    let database = Database::open_in_memory().expect("database should migrate");
    import_fixture(&database, dataset(), SCHEDULED_CSV.as_bytes())
        .expect("scheduled import should succeed");
    let second = import_fixture(&database, dataset(), FINISHED_CSV.as_bytes())
        .expect("finished import should succeed");
    assert_eq!(second.matches_inserted, 0);
    assert_eq!(second.matches_updated, 1);
    assert_eq!(second.statistics_inserted, 1);

    let connection = database.connection().expect("database lock should work");
    let result: (String, i64, i64) = connection
        .query_row(
            "SELECT status, final_home_goals, final_away_goals FROM matches",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("match should load");
    assert_eq!(result, ("finished".to_string(), 3, 2));
    let stats_row: (i64, i64) = connection
        .query_row(
            "SELECT home_shots, home_shots_on_target FROM match_statistics",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("statistics should load");
    assert_eq!(stats_row, (11, 5));
}

#[test]
fn later_missing_statistics_do_not_erase_known_values() {
    let database = Database::open_in_memory().expect("database should migrate");
    import_fixture(&database, dataset(), COMPLETE_CSV.as_bytes()).expect("import should succeed");
    let without_stats =
        "Div,Date,Time,HomeTeam,AwayTeam,FTHG,FTAG\nE0,15/08/2026,15:00,Arsenal,Chelsea,2,1\n";
    import_fixture(&database, dataset(), without_stats.as_bytes())
        .expect("reimport should succeed");

    let connection = database.connection().expect("database lock should work");
    let home_shots: i64 = connection
        .query_row("SELECT home_shots FROM match_statistics", [], |row| {
            row.get(0)
        })
        .expect("known statistic should remain");
    assert_eq!(home_shots, 14);
}

#[test]
fn malformed_optional_number_is_reported_without_panicking() {
    let csv = "Div,Date,Time,HomeTeam,AwayTeam,FTHG,FTAG,HS\nE0,19/08/2026,15:00,West Ham,Brentford,1,1,not-a-number\n";
    let database = Database::open_in_memory().expect("database should migrate");
    let summary =
        import_fixture(&database, dataset(), csv.as_bytes()).expect("dataset import should finish");
    assert_eq!(summary.rows_seen, 1);
    assert_eq!(summary.rows_failed, 1);
    assert_eq!(summary.matches_inserted, 0);
    assert!(summary.issues[0].message.contains("column 'HS'"));
}

#[test]
fn duplicate_source_rows_do_not_duplicate_matches() {
    let csv = format!("{COMPLETE_CSV}{}", COMPLETE_CSV.lines().nth(1).unwrap()) + "\n";
    let database = Database::open_in_memory().expect("database should migrate");
    let summary =
        import_fixture(&database, dataset(), csv.as_bytes()).expect("import should succeed");
    assert_eq!(summary.rows_seen, 2);
    assert_eq!(summary.matches_inserted, 1);
    assert_eq!(summary.matches_updated, 1);
    let connection = database.connection().expect("database lock should work");
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM matches", [], |row| row.get(0))
        .expect("count should load");
    assert_eq!(count, 1);
}

#[test]
fn database_stats_include_ingestion_tables() {
    let database = Database::open_in_memory().expect("database should migrate");
    import_fixture(&database, dataset(), COMPLETE_CSV.as_bytes()).expect("import should succeed");
    let connection = database.connection().expect("database lock should work");
    let counts = stats::database_stats(&connection).expect("stats should load");
    assert_eq!(counts.provider_competition_mappings, 1);
    assert_eq!(counts.data_import_runs, 1);
    let failed_run: Option<String> = connection
        .query_row(
            "SELECT status FROM data_import_runs WHERE status = 'failed'",
            [],
            |row| row.get(0),
        )
        .optional()
        .expect("run query should work");
    assert_eq!(failed_run, None);
}

#[test]
fn transport_error_categories_are_specific() {
    assert_eq!(
        categorize_error_details(true, false, false, true, false, "timed out"),
        "timeout"
    );
    assert_eq!(
        categorize_error_details(false, true, false, false, false, "too many redirects"),
        "redirect"
    );
    assert_eq!(
        categorize_error_details(false, false, true, false, false, "HTTP 404"),
        "http_status"
    );
    assert_eq!(
        categorize_error_details(false, false, false, true, false, "DNS lookup failed"),
        "dns"
    );
    assert_eq!(
        categorize_error_details(false, false, false, true, false, "TLS certificate error"),
        "tls"
    );
    assert_eq!(
        categorize_error_details(false, false, false, true, false, "proxy connect failed"),
        "proxy"
    );
    assert_eq!(
        categorize_error_details(false, false, false, true, false, "connection refused"),
        "connection"
    );
}

#[test]
fn catalog_contains_only_verified_recent_datasets() {
    let datasets = supported_datasets();
    assert_eq!(datasets.len(), 59);
    assert!(find_dataset("E0", "2627").is_some());
    assert!(find_dataset("E1", "2122").is_some());
    assert!(find_dataset("T1", "2627").is_some());
    assert!(find_dataset("D1", "2627").is_none());
    assert_eq!(
        find_dataset("E0", "2627")
            .expect("current Premier League should exist")
            .is_current_season,
        true
    );
}

#[test]
fn local_file_uses_the_same_import_pipeline_as_acquired_bytes() {
    let directory = tempfile::tempdir().expect("temporary directory should exist");
    let path = directory.path().join("E0.csv");
    std::fs::write(&path, COMPLETE_CSV).expect("fixture should be written");
    let local_database = Database::open_in_memory().expect("database should migrate");
    let byte_database = Database::open_in_memory().expect("database should migrate");

    let local = import_local_dataset(&local_database, dataset(), &path)
        .expect("local import should succeed");
    let acquired = import_fixture(&byte_database, dataset(), COMPLETE_CSV.as_bytes())
        .expect("acquired-byte import should succeed");
    assert_eq!(local.rows_seen, acquired.rows_seen);
    assert_eq!(local.matches_inserted, acquired.matches_inserted);
    assert_eq!(local.statistics_inserted, acquired.statistics_inserted);

    let local_quality = crate::repositories::quality::football_data_summary(
        &local_database
            .connection()
            .expect("database lock should work"),
        super::PROVIDER_ID,
    )
    .expect("quality should load");
    let byte_quality = crate::repositories::quality::football_data_summary(
        &byte_database
            .connection()
            .expect("database lock should work"),
        super::PROVIDER_ID,
    )
    .expect("quality should load");
    assert_eq!(local_quality[0].match_count, byte_quality[0].match_count);
    assert_eq!(
        local_quality[0].matches_with_shots,
        byte_quality[0].matches_with_shots
    );
}

#[test]
fn bootstrap_imports_multiple_local_datasets_and_is_idempotent() {
    let directory = tempfile::tempdir().expect("temporary directory should exist");
    let e0_path = directory.path().join("E0.csv");
    let e1_path = directory.path().join("E1.csv");
    std::fs::write(&e0_path, COMPLETE_CSV).expect("fixture should be written");
    std::fs::write(
        &e1_path,
        "Div,Date,Time,HomeTeam,AwayTeam,FTHG,FTAG,HS,AS\nE1,15/08/2026,12:30,Oxford,Derby,1,1,8,9\n",
    )
    .expect("fixture should be written");
    let request = local_request(std::collections::HashMap::from([
        ("E0:2627".to_string(), e0_path.display().to_string()),
        ("E1:2627".to_string(), e1_path.display().to_string()),
    ]));
    let database = Database::open_in_memory().expect("database should migrate");
    let first = tauri::async_runtime::block_on(bootstrap(&database, &request))
        .expect("bootstrap should succeed");
    assert_eq!(first.datasets_requested, 2);
    assert_eq!(first.datasets_completed, 2);
    assert_eq!(first.datasets_failed, 0);
    assert_eq!(first.matches_inserted, 2);

    let second = tauri::async_runtime::block_on(bootstrap(&database, &request))
        .expect("repeated bootstrap should succeed");
    assert_eq!(second.matches_inserted, 0);
    assert_eq!(second.matches_updated, 2);
    let connection = database.connection().expect("database lock should work");
    let match_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM matches", [], |row| row.get(0))
        .expect("match count should load");
    assert_eq!(match_count, 2);
}

#[test]
fn bootstrap_dataset_failure_does_not_rollback_completed_dataset() {
    let directory = tempfile::tempdir().expect("temporary directory should exist");
    let e0_path = directory.path().join("E0.csv");
    let e1_path = directory.path().join("broken.csv");
    std::fs::write(&e0_path, COMPLETE_CSV).expect("fixture should be written");
    std::fs::write(&e1_path, "not,a,football,data,file\n1,2,3,4,5\n")
        .expect("broken fixture should be written");
    let request = local_request(std::collections::HashMap::from([
        ("E0:2627".to_string(), e0_path.display().to_string()),
        ("E1:2627".to_string(), e1_path.display().to_string()),
    ]));
    let database = Database::open_in_memory().expect("database should migrate");
    let summary = tauri::async_runtime::block_on(bootstrap(&database, &request))
        .expect("bootstrap operation should finish");
    assert_eq!(summary.datasets_completed, 1);
    assert_eq!(summary.datasets_failed, 1);
    assert_eq!(summary.matches_inserted, 1);
    let connection = database.connection().expect("database lock should work");
    let completed_runs: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM data_import_runs WHERE status = 'completed'",
            [],
            |row| row.get(0),
        )
        .expect("completed run count should load");
    let failed_runs: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM data_import_runs WHERE status = 'failed'",
            [],
            |row| row.get(0),
        )
        .expect("failed run count should load");
    assert_eq!((completed_runs, failed_runs), (1, 1));
}

#[test]
fn bootstrap_refresh_updates_current_season() {
    let directory = tempfile::tempdir().expect("temporary directory should exist");
    let path = directory.path().join("E0.csv");
    std::fs::write(&path, SCHEDULED_CSV).expect("fixture should be written");
    let request = BootstrapRequest {
        preset: None,
        league_codes: vec!["E0".to_string()],
        season_codes: vec!["2627".to_string()],
        acquisition_mode: "local".to_string(),
        local_files: std::collections::HashMap::from([(
            "E0:2627".to_string(),
            path.display().to_string(),
        )]),
    };
    let database = Database::open_in_memory().expect("database should migrate");
    tauri::async_runtime::block_on(bootstrap(&database, &request))
        .expect("scheduled bootstrap should succeed");
    std::fs::write(&path, FINISHED_CSV).expect("updated fixture should be written");
    let refreshed = tauri::async_runtime::block_on(bootstrap(&database, &request))
        .expect("refresh should succeed");
    assert_eq!(refreshed.matches_inserted, 0);
    assert_eq!(refreshed.matches_updated, 1);
    assert_eq!(refreshed.statistics_inserted, 1);
    let connection = database.connection().expect("database lock should work");
    let status: String = connection
        .query_row("SELECT status FROM matches", [], |row| row.get(0))
        .expect("status should load");
    assert_eq!(status, "finished");
}

#[test]
fn data_quality_summary_reports_coverage_and_dates() {
    let database = Database::open_in_memory().expect("database should migrate");
    import_fixture(&database, dataset(), COMPLETE_CSV.as_bytes()).expect("import should succeed");
    import_fixture(&database, dataset(), MISSING_OPTIONAL_CSV.as_bytes())
        .expect("second import should succeed");
    let connection = database.connection().expect("database lock should work");
    let quality =
        crate::repositories::quality::football_data_summary(&connection, super::PROVIDER_ID)
            .expect("quality should load");
    assert_eq!(quality.len(), 1);
    let row = &quality[0];
    assert_eq!(row.match_count, 2);
    assert_eq!(row.finished_match_count, 2);
    assert_eq!(row.scheduled_match_count, 0);
    assert_eq!(row.matches_with_shots, 1);
    assert_eq!(row.matches_with_shots_on_target, 1);
    assert_eq!(row.matches_with_corners, 1);
    assert_eq!(row.matches_with_cards, 1);
    assert_eq!(row.matches_with_halftime, 1);
    assert_eq!(row.earliest_match_date.as_deref(), Some("2026-08-15"));
    assert_eq!(row.latest_match_date.as_deref(), Some("2026-08-16"));
    assert!(row.last_successful_import_at.is_some());
}

#[test]
fn bootstrap_rejects_unregistered_datasets() {
    let request = BootstrapRequest {
        preset: None,
        league_codes: vec!["UNKNOWN".to_string()],
        season_codes: vec!["2627".to_string()],
        acquisition_mode: "http".to_string(),
        local_files: std::collections::HashMap::new(),
    };
    assert!(validate_request(&request)
        .expect_err("unknown dataset must be rejected")
        .contains("unsupported"));

    let unpublished = BootstrapRequest {
        preset: None,
        league_codes: vec!["D1".to_string()],
        season_codes: vec!["2627".to_string()],
        acquisition_mode: "http".to_string(),
        local_files: std::collections::HashMap::new(),
    };
    assert!(validate_request(&unpublished)
        .expect_err("unpublished dataset must be rejected")
        .contains("unregistered"));
}

#[test]
#[ignore = "controlled live verification; never runs in the fixture-only test suite"]
fn live_e0_2627_import_verification() {
    let diagnostic = tauri::async_runtime::block_on(super::diagnose_dataset(dataset()));
    println!(
        "LIVE_DIAGNOSTIC={}",
        serde_json::to_string(&diagnostic).expect("diagnostic should serialize")
    );
    if !diagnostic.success {
        return;
    }
    let app_data = std::env::var_os("APPDATA").expect("APPDATA must be available on Windows");
    let database_path = std::path::PathBuf::from(app_data)
        .join("com.footballpredictor.app")
        .join("football-predictor.sqlite3");
    let database = Database::open(database_path.clone()).expect("application database should open");
    let summary = tauri::async_runtime::block_on(super::import_dataset(&database, dataset()))
        .expect("live E0 2627 import should succeed");
    let connection = database.connection().expect("database lock should work");
    let totals = stats::database_stats(&connection).expect("database totals should load");
    let mut statement = connection
        .prepare(
            "SELECT m.scheduled_local_date, m.kickoff_at, home.normalized_name,
                    away.normalized_name, m.status, m.final_home_goals, m.final_away_goals
             FROM matches m
             JOIN teams home ON home.id = m.home_team_id
             JOIN teams away ON away.id = m.away_team_id
             JOIN competitions c ON c.id = m.competition_id
             WHERE c.name = 'Premier League' AND m.season = '2026/27'
             ORDER BY m.scheduled_local_date, m.kickoff_at, m.id
             LIMIT 5",
        )
        .expect("sample query should prepare");
    let samples: Vec<serde_json::Value> = statement
        .query_map([], |row| {
            Ok(serde_json::json!({
                "local_date": row.get::<_, Option<String>>(0)?,
                "kickoff_utc": row.get::<_, String>(1)?,
                "home": row.get::<_, String>(2)?,
                "away": row.get::<_, String>(3)?,
                "status": row.get::<_, String>(4)?,
                "home_goals": row.get::<_, Option<i64>>(5)?,
                "away_goals": row.get::<_, Option<i64>>(6)?,
            }))
        })
        .expect("sample query should run")
        .collect::<rusqlite::Result<_>>()
        .expect("sample rows should decode");

    println!("LIVE_DATABASE_PATH={}", database_path.display());
    println!(
        "LIVE_SUMMARY={}",
        serde_json::to_string(&summary).expect("summary should serialize")
    );
    println!(
        "LIVE_TOTALS={}",
        serde_json::to_string(&totals).expect("totals should serialize")
    );
    println!(
        "LIVE_SAMPLES={}",
        serde_json::to_string(&samples).expect("samples should serialize")
    );
}

#[test]
#[ignore = "controlled local fallback verification with five fixture matches"]
fn controlled_local_fallback_verification() {
    let csv = "Div,Date,Time,HomeTeam,AwayTeam,FTHG,FTAG,FTR,HTHG,HTAG,HTR,HS,AS,HST,AST,HF,AF,HC,AC,HY,AY,HR,AR\nE0,15/08/2026,15:00,Arsenal,Chelsea,2,1,H,1,0,H,14,10,6,4,9,12,7,3,1,2,0,0\nE0,15/08/2026,17:30,Liverpool,Everton,3,0,H,1,0,H,18,7,9,2,8,13,8,2,0,3,0,0\nE0,16/08/2026,14:00,Fulham,Burnley,1,1,D,0,1,A,11,9,4,3,10,11,5,4,2,2,0,0\nE0,16/08/2026,16:30,Leeds,Newcastle,0,2,A,0,1,A,8,15,2,7,14,9,3,6,3,1,1,0\nE0,17/08/2026,20:00,West Ham,Brentford,2,2,D,1,1,D,13,12,5,5,11,10,4,5,2,2,0,0\n";
    let directory = tempfile::tempdir().expect("temporary directory should exist");
    let csv_path = directory.path().join("E0.csv");
    std::fs::write(&csv_path, csv).expect("local fixture should be written");
    let database = Database::open(directory.path().join("local-verification.sqlite3"))
        .expect("temporary database should open");
    let summary = import_local_dataset(&database, dataset(), &csv_path)
        .expect("local production import path should succeed");
    let connection = database.connection().expect("database lock should work");
    let totals = stats::database_stats(&connection).expect("totals should load");
    let quality =
        crate::repositories::quality::football_data_summary(&connection, super::PROVIDER_ID)
            .expect("quality should load");
    let samples: Vec<serde_json::Value> = connection
        .prepare(
            "SELECT m.scheduled_local_date, m.kickoff_at, h.normalized_name, a.normalized_name,
                    m.status, m.final_home_goals, m.final_away_goals
             FROM matches m
             JOIN teams h ON h.id = m.home_team_id
             JOIN teams a ON a.id = m.away_team_id
             ORDER BY m.scheduled_local_date, m.kickoff_at LIMIT 5",
        )
        .expect("sample query should prepare")
        .query_map([], |row| {
            Ok(serde_json::json!({
                "date": row.get::<_, String>(0)?,
                "kickoff_utc": row.get::<_, String>(1)?,
                "home": row.get::<_, String>(2)?,
                "away": row.get::<_, String>(3)?,
                "status": row.get::<_, String>(4)?,
                "score": [row.get::<_, i64>(5)?, row.get::<_, i64>(6)?],
            }))
        })
        .expect("sample query should run")
        .collect::<rusqlite::Result<_>>()
        .expect("samples should decode");
    println!(
        "LOCAL_SUMMARY={}",
        serde_json::to_string(&summary).expect("summary should serialize")
    );
    println!(
        "LOCAL_TOTALS={}",
        serde_json::to_string(&totals).expect("totals should serialize")
    );
    println!(
        "LOCAL_QUALITY={}",
        serde_json::to_string(&quality).expect("quality should serialize")
    );
    println!(
        "LOCAL_SAMPLES={}",
        serde_json::to_string(&samples).expect("samples should serialize")
    );
}

#[test]
#[ignore = "reports the app-data database after a controlled live import attempt"]
fn live_database_report_after_attempt() {
    let app_data = std::env::var_os("APPDATA").expect("APPDATA must be available on Windows");
    let database_path = std::path::PathBuf::from(app_data)
        .join("com.footballpredictor.app")
        .join("football-predictor.sqlite3");
    let database = Database::open(database_path.clone()).expect("application database should open");
    let connection = database.connection().expect("database lock should work");
    let totals = stats::database_stats(&connection).expect("database totals should load");
    let import_runs: Vec<serde_json::Value> = connection
        .prepare(
            "SELECT id, dataset_key, status, rows_seen, rows_inserted, rows_updated,
                    rows_skipped, rows_failed, error_message
             FROM data_import_runs ORDER BY id",
        )
        .expect("run query should prepare")
        .query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "dataset": row.get::<_, String>(1)?,
                "status": row.get::<_, String>(2)?,
                "rows_seen": row.get::<_, i64>(3)?,
                "rows_inserted": row.get::<_, i64>(4)?,
                "rows_updated": row.get::<_, i64>(5)?,
                "rows_skipped": row.get::<_, i64>(6)?,
                "rows_failed": row.get::<_, i64>(7)?,
                "error": row.get::<_, Option<String>>(8)?,
            }))
        })
        .expect("run query should execute")
        .collect::<rusqlite::Result<_>>()
        .expect("runs should decode");
    println!("LIVE_DATABASE_PATH={}", database_path.display());
    println!(
        "LIVE_TOTALS={}",
        serde_json::to_string(&totals).expect("totals should serialize")
    );
    println!(
        "LIVE_IMPORT_RUNS={}",
        serde_json::to_string(&import_runs).expect("runs should serialize")
    );
}

fn file_database(directory: &tempfile::TempDir) -> Database {
    Database::open(directory.path().join("football-predictor.sqlite3"))
        .expect("temporary database should open")
}

#[test]
fn managed_cache_paths_are_deterministic_and_catalog_bounded() {
    let directory = tempfile::tempdir().expect("temporary directory should exist");
    let first =
        super::cache::dataset_path(directory.path(), dataset()).expect("path should resolve");
    let second =
        super::cache::dataset_path(directory.path(), dataset()).expect("path should resolve");
    assert_eq!(first, second);
    assert_eq!(
        first,
        directory.path().join("data/football-data/2627/E0.csv")
    );
    for unsafe_value in ["../E0", "..", "E0/other", "E0\\other", "C:"] {
        assert!(super::cache::validate_component_for_test(unsafe_value).is_err());
    }
}

#[test]
fn manual_file_is_validated_and_atomically_cached() {
    let directory = tempfile::tempdir().expect("temporary directory should exist");
    let database = file_database(&directory);
    let source = directory.path().join("manual.csv");
    std::fs::write(&source, COMPLETE_CSV).expect("fixture should be written");
    let metadata =
        super::cache_local_file(&database, dataset(), &source).expect("valid fixture should cache");
    assert!(metadata.exists && metadata.parseable);
    assert!(metadata.sha256.is_some());
    assert_eq!(metadata.size_bytes, Some(COMPLETE_CSV.len() as u64));

    let destination = std::path::PathBuf::from(&metadata.path);
    let original = std::fs::read(&destination).expect("cache should be readable");
    std::fs::write(&source, "not,a,football,csv\n1,2,3,4\n").expect("bad fixture should write");
    assert!(super::cache_local_file(&database, dataset(), &source).is_err());
    assert_eq!(
        std::fs::read(destination).expect("old cache should survive"),
        original
    );
}

#[test]
fn cache_replacement_and_cached_import_use_production_pipeline() {
    let directory = tempfile::tempdir().expect("temporary directory should exist");
    let database = file_database(&directory);
    let source = directory.path().join("manual.csv");
    std::fs::write(&source, SCHEDULED_CSV).expect("fixture should be written");
    super::cache_local_file(&database, dataset(), &source).expect("fixture should cache");
    let first = super::import_cached_dataset(&database, dataset()).expect("cache should import");
    assert_eq!((first.matches_inserted, first.statistics_inserted), (1, 0));

    std::fs::write(&source, FINISHED_CSV).expect("updated fixture should be written");
    super::cache_local_file(&database, dataset(), &source).expect("cache should replace");
    let second = super::import_cached_dataset(&database, dataset()).expect("cache should reimport");
    assert_eq!((second.matches_inserted, second.matches_updated), (0, 1));
    assert_eq!(second.statistics_inserted, 1);
}

#[test]
fn cached_bootstrap_presets_resolve_without_fabricating_bundesliga() {
    let six = super::cached_bootstrap::preset_datasets(super::CORE_6_SEASONS)
        .expect("six-season preset should resolve");
    let recent = super::cached_bootstrap::preset_datasets(super::CORE_RECENT_3_SEASONS)
        .expect("recent preset should resolve");
    assert_eq!(six.len(), 59);
    assert_eq!(recent.len(), 29);
    assert!(!six
        .iter()
        .any(|item| item.league_code == "D1" && item.season_code == "2627"));
    assert!(super::cached_bootstrap::preset_datasets("unknown").is_err());
}

#[test]
fn cache_only_and_cache_first_are_offline_and_idempotent() {
    let directory = tempfile::tempdir().expect("temporary directory should exist");
    let database = file_database(&directory);
    let app_data = directory.path();
    for (index, definition) in
        super::cached_bootstrap::preset_datasets(super::CORE_RECENT_3_SEASONS)
            .expect("preset should resolve")
            .into_iter()
            .enumerate()
    {
        let csv = format!(
            "Div,Date,Time,HomeTeam,AwayTeam,FTHG,FTAG,HS,AS\n{},15/08/2026,15:00,Home {},Away {},1,0,9,7\n",
            definition.league_code, index, index
        );
        super::cache::store_validated_bytes(app_data, definition, csv.as_bytes())
            .expect("fixture should cache");
    }

    let cache_only = super::CachedBootstrapRequest {
        preset: super::CORE_RECENT_3_SEASONS.to_string(),
        acquisition_mode: "cache_only".to_string(),
        allow_stale_cache_on_refresh_failure: true,
    };
    let first = tauri::async_runtime::block_on(super::bootstrap_cached(&database, &cache_only))
        .expect("bootstrap should finish");
    assert_eq!(first.datasets_requested, 29);
    assert_eq!(
        (first.datasets_available, first.datasets_imported),
        (29, 29)
    );
    assert_eq!(first.matches_inserted, 29);

    let cache_first = super::CachedBootstrapRequest {
        acquisition_mode: "cache_first".to_string(),
        ..cache_only
    };
    let second = tauri::async_runtime::block_on(super::bootstrap_cached(&database, &cache_first))
        .expect("bootstrap should finish without networking for cached E0");
    let e0 = second
        .datasets
        .iter()
        .find(|row| row.league_code == "E0" && row.season_code == "2627")
        .unwrap();
    assert_eq!(e0.acquisition_source.as_deref(), Some("cache"));
    assert!(!e0.downloaded);
    assert_eq!(e0.matches_inserted, 0);
    assert_eq!(second.datasets_imported, 29);
}

#[test]
fn cache_stats_and_bootstrap_status_are_filesystem_and_database_derived() {
    let directory = tempfile::tempdir().expect("temporary directory should exist");
    let database = file_database(&directory);
    let source = directory.path().join("manual.csv");
    std::fs::write(&source, COMPLETE_CSV).expect("fixture should be written");
    let cached =
        super::cache_local_file(&database, dataset(), &source).expect("fixture should cache");
    super::import_cached_dataset(&database, dataset()).expect("fixture should import");
    let stats = super::cache_stats(&database).expect("cache stats should load");
    assert_eq!((stats.file_count, stats.dataset_count), (1, 1));
    assert_eq!(stats.total_bytes, cached.size_bytes.unwrap());
    let status = super::status(&database, super::CORE_RECENT_3_SEASONS)
        .expect("bootstrap status should load");
    assert_eq!(status.preset_dataset_count, 29);
    assert_eq!(status.datasets_cached, 1);
    assert_eq!(status.datasets_imported_successfully, 1);
    assert_eq!(
        (status.total_normalized_matches, status.total_teams),
        (1, 2)
    );
    assert_eq!(
        status.earliest_normalized_match.as_deref(),
        Some("2026-08-15")
    );
}

#[test]
#[ignore = "controlled three-dataset cached-bootstrap verification"]
fn controlled_cached_bootstrap_verification() {
    let directory = tempfile::tempdir().expect("temporary directory should exist");
    let database = file_database(&directory);
    let fixtures = [
        ("E0", "2627", COMPLETE_CSV),
        ("E1", "2627", "Div,Date,Time,HomeTeam,AwayTeam,FTHG,FTAG,HS,AS\nE1,16/08/2026,15:00,Oxford,Derby,1,0,9,7\n"),
        ("SP1", "2526", "Div,Date,Time,HomeTeam,AwayTeam,FTHG,FTAG,HS,AS\nSP1,17/08/2025,20:00,Sevilla,Valencia,2,2,12,10\n"),
    ];
    for (league, season, csv) in fixtures {
        let source = directory.path().join(format!("{league}-{season}.csv"));
        std::fs::write(&source, csv).expect("fixture should be written");
        let definition = find_dataset(league, season).expect("fixture dataset should exist");
        super::cache_local_file(&database, definition, &source).expect("fixture should cache");
    }
    let request = super::CachedBootstrapRequest {
        preset: super::CORE_6_SEASONS.to_string(),
        acquisition_mode: "cache_only".to_string(),
        allow_stale_cache_on_refresh_failure: true,
    };
    let summary = tauri::async_runtime::block_on(super::bootstrap_cached(&database, &request))
        .expect("cached bootstrap should finish");
    let status = super::status(&database, super::CORE_6_SEASONS).expect("status should load");
    let cache_stats = super::cache_stats(&database).expect("cache stats should load");
    let totals = crate::repositories::stats::database_stats(
        &database.connection().expect("connection should open"),
    )
    .expect("totals should load");
    println!(
        "CACHED_BOOTSTRAP_SUMMARY={}",
        serde_json::to_string(&summary).unwrap()
    );
    println!(
        "CACHED_BOOTSTRAP_STATUS={}",
        serde_json::to_string(&status).unwrap()
    );
    println!(
        "CACHED_CACHE_STATS={}",
        serde_json::to_string(&cache_stats).unwrap()
    );
    println!(
        "CACHED_DATABASE_TOTALS={}",
        serde_json::to_string(&totals).unwrap()
    );
}
