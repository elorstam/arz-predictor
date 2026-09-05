use chrono::Datelike;
use rusqlite::OptionalExtension;

use super::orchestrator::import_fixture;
use crate::repositories::popularity as popularity_repository;
use crate::{database::Database, repositories::iddaa};

const BULLETIN: &str = include_str!("fixtures/bulletin.json");
const POPULAR_BETS: &str = include_str!("fixtures/popular_bets.json");

fn seed_popular_match(database: &Database) -> i64 {
    let connection = database.connection().unwrap();
    connection
        .execute(
            "INSERT INTO competitions (name, country, current_season)
             VALUES ('Arjantin Premier Ligi', 'AR', '2026')",
            [],
        )
        .unwrap();
    let competition_id = connection.last_insert_rowid();
    connection
        .execute(
            "INSERT INTO teams (normalized_name, country) VALUES ('Estudiantes', 'AR')",
            [],
        )
        .unwrap();
    let home_id = connection.last_insert_rowid();
    connection
        .execute(
            "INSERT INTO teams (normalized_name, country) VALUES ('New Old Boys', 'AR')",
            [],
        )
        .unwrap();
    let away_id = connection.last_insert_rowid();
    connection
        .execute(
            "INSERT INTO matches (
                competition_id, season, home_team_id, away_team_id, kickoff_at, status
             ) VALUES (?1, '2026', ?2, ?3, '2026-08-31T22:00:00Z', 'scheduled')",
            rusqlite::params![competition_id, home_id, away_id],
        )
        .unwrap();
    let match_id = connection.last_insert_rowid();
    connection
        .execute(
            "INSERT INTO provider_match_mappings (match_id, provider, external_match_id)
             VALUES (?1, 'iddaa', '3109962')",
            [match_id],
        )
        .unwrap();
    for (market_id, code, selection_code, selection, odd, wodd) in [
        ("79405428", "1", "1", "1", 1.52, 1.46),
        ("79405480", "101", "2", "Üst", 2.29, 2.21),
    ] {
        connection
            .execute(
                "INSERT INTO odds_snapshots (
                    match_id, provider, market_code, market_name, line_value, selection,
                    odd, alternative_odd, captured_at, provider_market_id,
                    normalized_market_type, provider_selection_code, provider_line,
                    normalized_selection
                 ) VALUES (?1, 'iddaa', ?2, 'TEST', NULL, ?3, ?4, ?5,
                    '2026-09-01T00:00:00Z', ?6, 'TEST', ?7, NULL, NULL)",
                rusqlite::params![
                    match_id,
                    code,
                    selection,
                    odd,
                    wodd,
                    market_id,
                    selection_code
                ],
            )
            .unwrap();
    }
    match_id
}

#[test]
fn real_popularity_payload_preserves_verified_count_and_rank_only() {
    let database = Database::open_in_memory().unwrap();
    let match_id = seed_popular_match(&database);
    let summary = super::popularity::import_fixture(&database, POPULAR_BETS.as_bytes()).unwrap();
    assert_eq!(summary.selections_seen, 10);
    assert_eq!(summary.selections_imported, 10);
    assert_eq!(summary.snapshots_inserted, 20);
    assert_eq!((summary.matched_events, summary.unmatched_events), (2, 8));

    let connection = database.connection().unwrap();
    let metric_types: Vec<String> = connection
        .prepare("SELECT DISTINCT metric_type FROM popularity_snapshots ORDER BY metric_type")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(metric_types, vec!["COUNT", "RANK"]);
    let first: (f64, Option<String>, i64, Option<i64>) = connection
        .query_row(
            "SELECT metric_value, provider_raw_value, match_id,
                (SELECT rank_value FROM popularity_snapshots rank
                 WHERE rank.provider_event_id = count.provider_event_id
                   AND rank.provider_market_id = count.provider_market_id
                   AND rank.provider_selection_code = count.provider_selection_code
                   AND rank.metric_type = 'RANK')
             FROM popularity_snapshots count
             WHERE count.metric_type = 'COUNT' AND count.provider_market_id = '79405428'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        first,
        (3775.0, Some("3700+".to_string()), match_id, Some(1))
    );
    let forbidden_ratio_rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM popularity_snapshots
             WHERE raw_metric_name = 'playedRatio' OR metric_type IN ('SHARE', 'PERCENTAGE')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(forbidden_ratio_rows, 0);
}

#[test]
fn popularity_snapshots_deduplicate_and_preserve_count_changes() {
    let database = Database::open_in_memory().unwrap();
    super::popularity::import_fixture(&database, POPULAR_BETS.as_bytes()).unwrap();
    let repeated = super::popularity::import_fixture(&database, POPULAR_BETS.as_bytes()).unwrap();
    assert_eq!(repeated.snapshots_inserted, 0);
    assert_eq!(repeated.unchanged_snapshots_skipped, 20);

    let changed = POPULAR_BETS
        .replacen("\"totalPlayed\":3775", "\"totalPlayed\":3801", 1)
        .replacen(
            "\"totalPlayedRoundStr\":\"3700+\"",
            "\"totalPlayedRoundStr\":\"3800+\"",
            1,
        );
    let changed_summary = super::popularity::import_fixture(&database, changed.as_bytes()).unwrap();
    assert_eq!(changed_summary.snapshots_inserted, 1);
    assert_eq!(changed_summary.unchanged_snapshots_skipped, 19);
    let connection = database.connection().unwrap();
    let history: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM popularity_snapshots
             WHERE provider_market_id = '79405428' AND metric_type = 'COUNT'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(history, 2);
}

#[test]
fn latest_popularity_query_joins_normalized_match_and_latest_odds() {
    let database = Database::open_in_memory().unwrap();
    seed_popular_match(&database);
    super::popularity::import_fixture(&database, POPULAR_BETS.as_bytes()).unwrap();
    let connection = database.connection().unwrap();
    let latest = popularity_repository::latest_popular_selections(&connection).unwrap();
    assert_eq!(latest.len(), 2);
    assert_eq!((latest[0].rank, latest[0].play_count), (1, 3775));
    assert_eq!((latest[0].odd, latest[0].wodd), (Some(1.52), Some(1.46)));
    assert_eq!(latest[1].rank, 3);
    let status = popularity_repository::status(&connection).unwrap();
    assert_eq!(status.latest_selection_count, 10);
    assert_eq!(
        (
            status.matched_selection_count,
            status.unmatched_selection_count
        ),
        (2, 8)
    );
}

#[test]
fn popularity_snapshots_are_immutable() {
    let database = Database::open_in_memory().unwrap();
    super::popularity::import_fixture(&database, POPULAR_BETS.as_bytes()).unwrap();
    let connection = database.connection().unwrap();
    assert!(connection
        .execute("UPDATE popularity_snapshots SET metric_value = 1", [])
        .is_err());
    assert!(connection
        .execute("DELETE FROM popularity_snapshots", [])
        .is_err());
}

#[test]
fn observed_timestamp_is_unix_seconds() {
    let timestamp = super::orchestrator::normalize_timestamp(1_788_343_200)
        .expect("observed timestamp should normalize");
    assert_eq!(timestamp.timestamp(), 1_788_343_200);
    assert_eq!(timestamp.year(), 2026);
    let milliseconds = super::orchestrator::normalize_timestamp(1_788_343_200_000)
        .expect("millisecond timestamps are safely recognized");
    assert_eq!(milliseconds, timestamp);
}

#[test]
fn fixture_imports_matches_mappings_and_all_observed_market_shapes() {
    let database = Database::open_in_memory().expect("database should migrate");
    let summary = import_fixture(&database, BULLETIN.as_bytes()).expect("fixture should import");
    assert_eq!(summary.events_seen, 6);
    assert_eq!(summary.failed_events, 1);
    assert_eq!(summary.competitions_resolved, 2);
    assert_eq!(summary.competitions_unresolved, 0);
    assert_eq!(summary.teams_created, 10);
    assert_eq!(summary.matches_inserted, 5);
    assert_eq!(summary.odds_snapshots_inserted, 33);
    assert_eq!(summary.selections_seen, 33);

    let connection = database.connection().unwrap();
    for (table, expected) in [
        ("provider_competition_mappings", 2),
        ("provider_team_mappings", 10),
        ("provider_match_mappings", 5),
        ("matches", 5),
        ("odds_snapshots", 33),
    ] {
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, expected, "unexpected {table} count");
    }
    let unknown: (String, Option<String>, String, Option<String>) = connection
        .query_row(
            "SELECT normalized_market_type, provider_line, selection, normalized_selection
             FROM odds_snapshots WHERE provider = 'iddaa' AND market_code = '777'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        unknown,
        (
            "UNKNOWN".to_string(),
            Some("special".to_string()),
            "Kaynak Seçimi".to_string(),
            None
        )
    );
    let cautious_48: String = connection
        .query_row(
            "SELECT normalized_market_type FROM odds_snapshots WHERE market_code = '48' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(cautious_48, "UNKNOWN");
}

#[test]
fn repeat_import_is_idempotent_and_changed_prices_create_history() {
    let database = Database::open_in_memory().expect("database should migrate");
    let first = import_fixture(&database, BULLETIN.as_bytes()).unwrap();
    let second = import_fixture(&database, BULLETIN.as_bytes()).unwrap();
    assert_eq!(second.matches_inserted, 0);
    assert_eq!(second.matches_updated, 5);
    assert_eq!(second.odds_snapshots_inserted, 0);
    assert_eq!(second.unchanged_odds_skipped, first.odds_snapshots_inserted);

    let odd_changed = BULLETIN.replacen("\"odd\": 1.70", "\"odd\": 1.62", 1);
    let third = import_fixture(&database, odd_changed.as_bytes()).unwrap();
    assert_eq!(third.odds_snapshots_inserted, 1);

    let wodd_changed = odd_changed.replacen("\"wodd\": 1.66", "\"wodd\": 1.59", 1);
    let fourth = import_fixture(&database, wodd_changed.as_bytes()).unwrap();
    assert_eq!(fourth.odds_snapshots_inserted, 1);
    let connection = database.connection().unwrap();
    let history: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM odds_snapshots WHERE provider = 'iddaa'
             AND market_code = '101' AND provider_market_id = '80017358'
             AND provider_line = '2.5' AND selection = 'Üst'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(history, 3);
}

#[test]
fn stable_provider_event_id_allows_kickoff_update_without_duplicate() {
    let database = Database::open_in_memory().expect("database should migrate");
    import_fixture(&database, BULLETIN.as_bytes()).unwrap();
    let updated = BULLETIN.replacen("1788343200", "1788346800", 1);
    import_fixture(&database, updated.as_bytes()).unwrap();
    let connection = database.connection().unwrap();
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM provider_match_mappings
             WHERE provider = 'iddaa' AND external_match_id = '3096780'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let kickoff: String = connection
        .query_row(
            "SELECT m.kickoff_at FROM matches m JOIN provider_match_mappings pm ON pm.match_id = m.id
             WHERE pm.provider = 'iddaa' AND pm.external_match_id = '3096780'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        kickoff,
        super::orchestrator::normalize_timestamp(1_788_346_800)
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    );
}

#[test]
fn local_replay_uses_same_orchestrator_and_status_queries() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bulletin.json");
    std::fs::write(&path, BULLETIN).unwrap();
    let database = Database::open(directory.path().join("database.sqlite3")).unwrap();
    let summary = super::import_local_bulletin(&database, &path).unwrap();
    assert_eq!(summary.matches_inserted, 5);
    let connection = database.connection().unwrap();
    let status = iddaa::bulletin_status(&connection).unwrap();
    assert_eq!(status.upcoming_match_count, 5);
    assert_eq!(status.competition_count, 2);
    assert_eq!(status.team_count, 10);
    assert_eq!(status.odds_snapshot_count, 33);
    assert_eq!(status.unresolved_competition_count, 0);
    assert!(status.last_successful_refresh.is_some());
    let upcoming = iddaa::upcoming_matches(&connection).unwrap();
    assert_eq!(upcoming.len(), 5);
    assert!(upcoming
        .iter()
        .any(|item| item.provider_event_id == "3096780" && item.available_markets == 11));
}

#[test]
fn unresolved_competition_is_preserved_without_fabricated_name() {
    let payload =
        r#"{"events":[{"i":1,"hn":"A","an":"B","sid":1,"ci":9999,"d":1788343200,"m":[]}]}"#;
    let database = Database::open_in_memory().unwrap();
    let summary = import_fixture(&database, payload.as_bytes()).unwrap();
    assert_eq!(summary.competitions_unresolved, 1);
    assert_eq!(summary.matches_inserted, 0);
    let connection = database.connection().unwrap();
    let metadata: (Option<String>, String) = connection
        .query_row(
            "SELECT external_name, resolution_status FROM provider_competition_metadata
             WHERE provider = 'iddaa' AND external_competition_id = '9999'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(metadata, (None, "unresolved".to_string()));
    let fabricated: Option<String> = connection
        .query_row(
            "SELECT name FROM competitions WHERE name LIKE 'Competition %'",
            [],
            |row| row.get(0),
        )
        .optional()
        .unwrap();
    assert_eq!(fabricated, None);
}

#[test]
fn latest_odds_query_returns_only_latest_selection_state() {
    let database = Database::open_in_memory().unwrap();
    import_fixture(&database, BULLETIN.as_bytes()).unwrap();
    let changed = BULLETIN.replacen("\"odd\": 1.70", "\"odd\": 1.62", 1);
    import_fixture(&database, changed.as_bytes()).unwrap();
    let connection = database.connection().unwrap();
    let match_id: i64 = connection
        .query_row(
            "SELECT match_id FROM provider_match_mappings WHERE provider = 'iddaa'
             AND external_match_id = '3096780'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let latest = iddaa::latest_odds(&connection, match_id).unwrap();
    let over = latest
        .iter()
        .find(|odd| {
            odd.normalized_market_type == "TOTAL_GOALS"
                && odd.line == Some(2.5)
                && odd.normalized_selection.as_deref() == Some("OVER")
        })
        .unwrap();
    assert_eq!(over.odd, 1.62);
    assert_eq!(over.wodd, Some(1.66));
    assert_eq!(latest.len(), 25);
}

#[test]
fn iddaa_odds_snapshots_remain_immutable_and_foreign_keys_hold() {
    let database = Database::open_in_memory().unwrap();
    import_fixture(&database, BULLETIN.as_bytes()).unwrap();
    let connection = database.connection().unwrap();
    assert!(connection
        .execute(
            "UPDATE odds_snapshots SET odd = 9.99 WHERE provider = 'iddaa'",
            []
        )
        .is_err());
    assert!(connection
        .execute(
            "INSERT INTO odds_snapshots (
                match_id, provider, market_code, market_name, selection, odd, captured_at
             ) VALUES (999999, 'iddaa', '1', 'MATCH_RESULT', '1', 2.0, '2026-01-01T00:00:00Z')",
            [],
        )
        .is_err());
}

#[test]
#[ignore = "controlled local Iddaa production-path verification"]
fn controlled_local_iddaa_verification() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("iddaa-bulletin.json");
    std::fs::write(&fixture, BULLETIN).unwrap();
    let database = Database::open(directory.path().join("football-predictor.sqlite3")).unwrap();
    let summary = super::import_local_bulletin(&database, &fixture).unwrap();
    let connection = database.connection().unwrap();
    let status = iddaa::bulletin_status(&connection).unwrap();
    let upcoming = iddaa::upcoming_matches(&connection).unwrap();
    let detailed = upcoming
        .iter()
        .find(|item| item.provider_event_id == "3096780")
        .unwrap();
    let latest = iddaa::latest_odds(&connection, detailed.match_id).unwrap();
    let totals = crate::repositories::stats::database_stats(&connection).unwrap();
    println!(
        "IDDAA_LOCAL_SUMMARY={}",
        serde_json::to_string(&summary).unwrap()
    );
    println!(
        "IDDAA_LOCAL_STATUS={}",
        serde_json::to_string(&status).unwrap()
    );
    println!(
        "IDDAA_LOCAL_TOTALS={}",
        serde_json::to_string(&totals).unwrap()
    );
    println!(
        "IDDAA_LOCAL_MATCHES={}",
        serde_json::to_string(&upcoming).unwrap()
    );
    println!(
        "IDDAA_LOCAL_LATEST_ODDS={}",
        serde_json::to_string(&latest).unwrap()
    );
}

#[test]
#[ignore = "controlled live refresh; never part of offline suite"]
fn controlled_live_iddaa_refresh() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("football-predictor.sqlite3")).unwrap();
    let result = tauri::async_runtime::block_on(super::refresh_bulletin(&database));
    println!(
        "IDDAA_LIVE_RESULT={}",
        serde_json::to_string(&result).unwrap()
    );
}

#[test]
#[ignore = "controlled live popularity verification; never part of offline suite"]
fn controlled_live_iddaa_popularity() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("football-predictor.sqlite3")).unwrap();
    let bulletin = tauri::async_runtime::block_on(super::refresh_bulletin(&database)).unwrap();
    let popularity = tauri::async_runtime::block_on(super::refresh_popularity(&database)).unwrap();
    let connection = database.connection().unwrap();
    let status = popularity_repository::status(&connection).unwrap();
    let examples = popularity_repository::latest_popular_selections(&connection).unwrap();
    println!(
        "POPULAR_LIVE_BULLETIN={}",
        serde_json::to_string(&bulletin).unwrap()
    );
    println!(
        "POPULAR_LIVE_REFRESH={}",
        serde_json::to_string(&popularity).unwrap()
    );
    println!(
        "POPULAR_LIVE_STATUS={}",
        serde_json::to_string(&status).unwrap()
    );
    println!(
        "POPULAR_LIVE_EXAMPLES={}",
        serde_json::to_string(&examples.into_iter().take(10).collect::<Vec<_>>()).unwrap()
    );
}
