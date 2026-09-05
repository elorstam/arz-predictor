use rusqlite::{params, Connection};
use tempfile::tempdir;

use super::Database;
use crate::repositories::{
    competitions,
    coupon_candidates::{self, NewCandidate, NewCandidateRun},
    coupons::{self, NewCoupon, NewCouponSelection},
    matches, stats, teams,
};

struct PredictionFixture {
    model_version_id: i64,
    match_ids: Vec<i64>,
    prediction_ids: Vec<i64>,
}

fn seed_predictions(connection: &Connection, count: usize) -> PredictionFixture {
    connection
        .execute(
            "INSERT INTO model_versions (version_identifier, model_name, model_type)
             VALUES ('coupon-test-v1', 'Coupon test model', 'test')",
            [],
        )
        .expect("model version insert should succeed");
    let model_version_id = connection.last_insert_rowid();
    let competition_id = competitions::insert(
        connection,
        "Coupon Test League",
        Some("Test Country"),
        Some("2026-2027"),
    )
    .expect("competition insert should succeed");
    let home_team_id = teams::insert(connection, "Coupon Home FC", Some("Test Country"))
        .expect("home team insert should succeed");
    let away_team_id = teams::insert(connection, "Coupon Away FC", Some("Test Country"))
        .expect("away team insert should succeed");

    let mut match_ids = Vec::with_capacity(count);
    let mut prediction_ids = Vec::with_capacity(count);
    for index in 0..count {
        let kickoff_at = format!("2026-09-{:02}T18:00:00Z", index + 1);
        let match_id = matches::insert(
            connection,
            &matches::NewMatch {
                competition_id,
                season: "2026-2027",
                home_team_id,
                away_team_id,
                kickoff_at: &kickoff_at,
            },
        )
        .expect("match insert should succeed");
        connection
            .execute(
                "INSERT INTO predictions (
                    match_id, market, selection, line_value, model_probability,
                    confidence_bucket, iddaa_odd_at_prediction, kickoff_at, model_version_id
                 ) VALUES (?1, 'TOTAL_GOALS', 'OVER', 2.5, ?2, 'high', ?3, ?4, ?5)",
                params![
                    match_id,
                    0.70 + index as f64 * 0.01,
                    1.80 + index as f64 * 0.05,
                    kickoff_at,
                    model_version_id
                ],
            )
            .expect("prediction insert should succeed");
        match_ids.push(match_id);
        prediction_ids.push(connection.last_insert_rowid());
    }

    PredictionFixture {
        model_version_id,
        match_ids,
        prediction_ids,
    }
}

fn insert_candidate_run(connection: &Connection, model_version_id: i64) -> i64 {
    coupon_candidates::insert_run(
        connection,
        &NewCandidateRun {
            target_date: "2026-09-01",
            coupon_type: "OVER_25",
            generated_at: "2026-09-01T08:00:00Z",
            model_version_id,
            status: "completed",
            target_candidate_count: 10,
            qualified_candidate_count: 2,
            generation_config_json: Some(r#"{"minimumProbability":0.65}"#),
        },
    )
    .expect("candidate run insert should succeed")
}

fn insert_candidate(
    connection: &Connection,
    candidate_run_id: i64,
    prediction_id: i64,
    rank: i64,
) -> i64 {
    coupon_candidates::insert_candidate(
        connection,
        &NewCandidate {
            candidate_run_id,
            prediction_id,
            rank,
            ranking_score: 0.91 - rank as f64 * 0.01,
            model_probability_snapshot: 0.72,
            iddaa_odd_snapshot: Some(1.85),
            market_snapshot: "TOTAL_GOALS",
            selection_snapshot: "OVER",
            line_value_snapshot: Some(2.5),
            explanation_json: Some(r#"{"reason":"test"}"#),
        },
    )
    .expect("candidate insert should succeed")
}

fn insert_coupon_with_selection(
    connection: &Connection,
    fixture: &PredictionFixture,
    candidate_run_id: i64,
    candidate_id: i64,
    coupon_type: &str,
) -> (i64, i64) {
    let coupon_id = coupons::insert(
        connection,
        &NewCoupon {
            coupon_type,
            target_date: "2026-09-01",
            model_version_id: Some(fixture.model_version_id),
            source_candidate_run_id: Some(candidate_run_id),
            total_decimal_odd: Some(1.85),
            reference_stake: Some(100.0),
            notes: Some("test coupon"),
        },
    )
    .expect("coupon insert should succeed");
    let selection_id = coupons::insert_selection(
        connection,
        &NewCouponSelection {
            coupon_id,
            prediction_id: Some(fixture.prediction_ids[0]),
            source_candidate_id: Some(candidate_id),
            selection_order: 1,
            match_id: fixture.match_ids[0],
            market_snapshot: "TOTAL_GOALS",
            selection_snapshot: "OVER",
            line_value_snapshot: Some(2.5),
            model_probability_snapshot: 0.72,
            odd_snapshot: Some(1.85),
        },
    )
    .expect("coupon selection insert should succeed");
    (coupon_id, selection_id)
}

#[test]
fn fresh_database_applies_initial_migration() {
    let database = Database::open_in_memory().expect("fresh database should migrate");
    let connection = database
        .connection()
        .expect("database lock should be available");

    for table in [
        "competitions",
        "teams",
        "provider_team_mappings",
        "matches",
        "match_statistics",
        "provider_match_mappings",
        "odds_snapshots",
        "predictions",
        "model_versions",
        "coupon_candidate_runs",
        "coupon_candidates",
        "coupons",
        "coupon_selections",
        "coupon_system_sizes",
        "provider_competition_mappings",
        "data_import_runs",
        "provider_competition_metadata",
        "popularity_snapshots",
    ] {
        let exists: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("schema query should succeed");
        assert_eq!(exists, 1, "missing migrated table: {table}");
    }
}

#[test]
fn foreign_keys_are_enforced() {
    let database = Database::open_in_memory().expect("fresh database should migrate");
    let connection = database
        .connection()
        .expect("database lock should be available");

    let result = connection.execute(
        "INSERT INTO provider_team_mappings (team_id, provider, external_team_id)
         VALUES (?1, ?2, ?3)",
        params![999_999, "test-provider", "external-team"],
    );

    assert!(result.is_err(), "invalid foreign key insert must fail");
    let enabled: i64 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("foreign key pragma should be readable");
    assert_eq!(enabled, 1);
}

#[test]
fn migration_versions_are_tracked_and_not_reapplied() {
    let directory = tempdir().expect("temporary directory should be created");
    let path = directory.path().join("migration-test.sqlite3");

    {
        let database = Database::open(path.clone()).expect("first open should migrate");
        let connection = database
            .connection()
            .expect("database lock should be available");
        let versions: Vec<(i64, String)> = connection
            .prepare("SELECT version, name FROM schema_migrations ORDER BY version")
            .expect("migration query should prepare")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("migration query should run")
            .collect::<rusqlite::Result<_>>()
            .expect("migration rows should decode");
        assert_eq!(
            versions,
            vec![
                (1, "initial_normalized_schema".to_string()),
                (2, "coupon_tracking".to_string()),
                (3, "data_ingestion".to_string()),
                (4, "iddaa_ingestion".to_string()),
                (5, "iddaa_popularity".to_string()),
                (6, "entity_resolution".to_string()),
                (7, "entity_metadata_assets".to_string()),
                (8, "feature_engine".to_string()),
                (9, "prediction_engine".to_string()),
                (10, "backtest_calibration".to_string()),
                (11, "calibrated_backtest_results".to_string()),
                (12, "daily_candidate_engine".to_string()),
                (13, "candidate_exclusion_diagnostics".to_string()),
                (14, "final_coupon_engine".to_string()),
                (15, "coupon_settlement_progression".to_string()),
                (16, "lineup_foundation".to_string()),
                (17, "lineup_adjustment_model".to_string()),
                (18, "prediction_base_lambdas".to_string()),
                (19, "lineup_revision_core_payload".to_string()),
                (20, "candidate_lineup_revision_lineage".to_string())
            ]
        );
    }

    let reopened = Database::open(path).expect("second open should not reapply migration");
    let connection = reopened
        .connection()
        .expect("database lock should be available");
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("migration count should be readable");
    assert_eq!(count, 20);
}

#[test]
fn migration_0002_applies_after_existing_0001_database() {
    let mut connection = Connection::open_in_memory().expect("in-memory database should open");
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .expect("foreign keys should enable");
    let transaction = connection
        .transaction()
        .expect("baseline migration transaction should start");
    transaction
        .execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );",
        )
        .expect("migration table should be created");
    transaction
        .execute_batch(include_str!(
            "../../migrations/0001_initial_normalized_schema.sql"
        ))
        .expect("migration 0001 should apply");
    transaction
        .execute(
            "INSERT INTO schema_migrations (version, name) VALUES (1, 'initial_normalized_schema')",
            [],
        )
        .expect("migration 0001 should be tracked");
    transaction.commit().expect("migration 0001 should commit");

    super::migrations::apply_pending(&mut connection).expect("migration 0002 should apply");
    let versions: Vec<i64> = connection
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .expect("migration query should prepare")
        .query_map([], |row| row.get(0))
        .expect("migration query should run")
        .collect::<rusqlite::Result<_>>()
        .expect("migration versions should decode");
    assert_eq!(
        versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20]
    );
}

#[test]
fn migration_0003_applies_after_existing_0002_database() {
    let mut connection = Connection::open_in_memory().expect("in-memory database should open");
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .expect("foreign keys should enable");
    let transaction = connection
        .transaction()
        .expect("baseline migration transaction should start");
    transaction
        .execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );",
        )
        .expect("migration table should be created");
    transaction
        .execute_batch(include_str!(
            "../../migrations/0001_initial_normalized_schema.sql"
        ))
        .expect("migration 0001 should apply");
    transaction
        .execute_batch(include_str!("../../migrations/0002_coupon_tracking.sql"))
        .expect("migration 0002 should apply");
    transaction
        .execute(
            "INSERT INTO schema_migrations (version, name) VALUES
             (1, 'initial_normalized_schema'), (2, 'coupon_tracking')",
            [],
        )
        .expect("baseline migrations should be tracked");
    transaction.commit().expect("baseline should commit");

    super::migrations::apply_pending(&mut connection).expect("migration 0003 should apply");
    let versions: Vec<i64> = connection
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .expect("migration query should prepare")
        .query_map([], |row| row.get(0))
        .expect("migration query should run")
        .collect::<rusqlite::Result<_>>()
        .expect("migration versions should decode");
    assert_eq!(
        versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20]
    );

    super::migrations::apply_pending(&mut connection).expect("migration 0003 should not rerun");
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("migration count should load");
    assert_eq!(count, 20);
}

#[test]
fn migration_0004_applies_after_existing_0003_database() {
    let mut connection = Connection::open_in_memory().expect("database should open");
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );",
        )
        .unwrap();
    connection
        .execute_batch(include_str!(
            "../../migrations/0001_initial_normalized_schema.sql"
        ))
        .unwrap();
    connection
        .execute_batch(include_str!("../../migrations/0002_coupon_tracking.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../../migrations/0003_data_ingestion.sql"))
        .unwrap();
    connection
        .execute(
            "INSERT INTO schema_migrations (version, name) VALUES
             (1, 'initial_normalized_schema'), (2, 'coupon_tracking'),
             (3, 'data_ingestion')",
            [],
        )
        .unwrap();
    super::migrations::apply_pending(&mut connection).expect("migration 0004 should apply");
    let columns: Vec<String> = connection
        .prepare("PRAGMA table_info(odds_snapshots)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(columns.contains(&"provider_market_id".to_string()));
    assert!(columns.contains(&"normalized_selection".to_string()));
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 20);
    super::migrations::apply_pending(&mut connection).expect("migration should not rerun");
}

#[test]
fn migration_0005_applies_after_existing_0004_database() {
    let mut connection = Connection::open_in_memory().expect("database should open");
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );",
        )
        .unwrap();
    for sql in [
        include_str!("../../migrations/0001_initial_normalized_schema.sql"),
        include_str!("../../migrations/0002_coupon_tracking.sql"),
        include_str!("../../migrations/0003_data_ingestion.sql"),
        include_str!("../../migrations/0004_iddaa_ingestion.sql"),
    ] {
        connection.execute_batch(sql).unwrap();
    }
    connection
        .execute(
            "INSERT INTO schema_migrations (version, name) VALUES
             (1, 'initial_normalized_schema'), (2, 'coupon_tracking'),
             (3, 'data_ingestion'), (4, 'iddaa_ingestion')",
            [],
        )
        .unwrap();
    super::migrations::apply_pending(&mut connection).expect("migration 0005 should apply");
    let exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'popularity_snapshots'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(exists, 1);
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 20);
    super::migrations::apply_pending(&mut connection).expect("migration should not rerun");
}

#[test]
fn migration_0007_applies_after_existing_0006_database() {
    let mut connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,name TEXT NOT NULL,applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));").unwrap();
    for sql in [
        include_str!("../../migrations/0001_initial_normalized_schema.sql"),
        include_str!("../../migrations/0002_coupon_tracking.sql"),
        include_str!("../../migrations/0003_data_ingestion.sql"),
        include_str!("../../migrations/0004_iddaa_ingestion.sql"),
        include_str!("../../migrations/0005_iddaa_popularity.sql"),
        include_str!("../../migrations/0006_entity_resolution.sql"),
    ] {
        connection.execute_batch(sql).unwrap();
    }
    connection.execute("INSERT INTO schema_migrations(version,name) VALUES(1,'initial_normalized_schema'),(2,'coupon_tracking'),(3,'data_ingestion'),(4,'iddaa_ingestion'),(5,'iddaa_popularity'),(6,'entity_resolution')",[]).unwrap();
    super::migrations::apply_pending(&mut connection).unwrap();
    assert_eq!(
        connection
            .query_row::<i64, _, _>("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get(0))
            .unwrap(),
        20
    );
    for table in ["team_metadata", "competition_metadata", "entity_assets"] {
        assert_eq!(
            connection
                .query_row::<i64, _, _>(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0)
                )
                .unwrap(),
            1
        );
    }
}

#[test]
fn migration_0008_applies_after_existing_0007_database() {
    let mut connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,name TEXT NOT NULL,applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));").unwrap();
    for sql in [
        include_str!("../../migrations/0001_initial_normalized_schema.sql"),
        include_str!("../../migrations/0002_coupon_tracking.sql"),
        include_str!("../../migrations/0003_data_ingestion.sql"),
        include_str!("../../migrations/0004_iddaa_ingestion.sql"),
        include_str!("../../migrations/0005_iddaa_popularity.sql"),
        include_str!("../../migrations/0006_entity_resolution.sql"),
        include_str!("../../migrations/0007_entity_metadata_assets.sql"),
    ] {
        connection.execute_batch(sql).unwrap();
    }
    connection.execute("INSERT INTO schema_migrations(version,name) VALUES(1,'initial_normalized_schema'),(2,'coupon_tracking'),(3,'data_ingestion'),(4,'iddaa_ingestion'),(5,'iddaa_popularity'),(6,'entity_resolution'),(7,'entity_metadata_assets')",[]).unwrap();
    super::migrations::apply_pending(&mut connection).unwrap();
    assert_eq!(
        connection
            .query_row::<i64, _, _>("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get(0))
            .unwrap(),
        20
    );
    for table in ["feature_sets", "training_labels"] {
        assert_eq!(
            connection
                .query_row::<i64, _, _>(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0)
                )
                .unwrap(),
            1
        );
    }
}

#[test]
fn teams_and_matches_can_be_inserted_and_read() {
    let database = Database::open_in_memory().expect("fresh database should migrate");
    let connection = database
        .connection()
        .expect("database lock should be available");

    let competition_id = competitions::insert(
        &connection,
        "Test Premier League",
        Some("Test Country"),
        Some("2026-2027"),
    )
    .expect("competition insert should succeed");
    let home_team_id = teams::insert(&connection, "Home FC", Some("Test Country"))
        .expect("home team insert should succeed");
    let away_team_id = teams::insert(&connection, "Away FC", Some("Test Country"))
        .expect("away team insert should succeed");

    let match_id = matches::insert(
        &connection,
        &matches::NewMatch {
            competition_id,
            season: "2026-2027",
            home_team_id,
            away_team_id,
            kickoff_at: "2026-09-01T18:00:00Z",
        },
    )
    .expect("match insert should succeed");

    let home_team = teams::find_by_id(&connection, home_team_id)
        .expect("team read should succeed")
        .expect("team should exist");
    assert_eq!(home_team.normalized_name, "Home FC");
    assert!(home_team.created_at.ends_with('Z'));

    let saved_match = matches::find_by_id(&connection, match_id)
        .expect("match read should succeed")
        .expect("match should exist");
    assert_eq!(saved_match.competition_id, competition_id);
    assert_eq!(saved_match.home_team_id, home_team_id);
    assert_eq!(saved_match.away_team_id, away_team_id);
    assert_eq!(saved_match.kickoff_at, "2026-09-01T18:00:00Z");
    assert_eq!(saved_match.status, "scheduled");
}

#[test]
fn candidate_run_and_candidates_can_be_inserted_and_read() {
    let database = Database::open_in_memory().expect("fresh database should migrate");
    let connection = database
        .connection()
        .expect("database lock should be available");
    let fixture = seed_predictions(&connection, 2);
    let run_id = insert_candidate_run(&connection, fixture.model_version_id);
    insert_candidate(&connection, run_id, fixture.prediction_ids[0], 1);
    insert_candidate(&connection, run_id, fixture.prediction_ids[1], 2);

    let run = coupon_candidates::find_run_by_id(&connection, run_id)
        .expect("candidate run read should succeed")
        .expect("candidate run should exist");
    let candidates =
        coupon_candidates::list_for_run(&connection, run_id).expect("candidate list should load");

    assert_eq!(run.coupon_type, "OVER_25");
    assert_eq!(run.target_candidate_count, 10);
    assert_eq!(run.qualified_candidate_count, 2);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].rank, 1);
    assert_eq!(candidates[1].prediction_id, fixture.prediction_ids[1]);
}

#[test]
fn duplicate_prediction_in_candidate_run_is_rejected() {
    let database = Database::open_in_memory().expect("fresh database should migrate");
    let connection = database
        .connection()
        .expect("database lock should be available");
    let fixture = seed_predictions(&connection, 1);
    let run_id = insert_candidate_run(&connection, fixture.model_version_id);
    insert_candidate(&connection, run_id, fixture.prediction_ids[0], 1);

    let duplicate = coupon_candidates::insert_candidate(
        &connection,
        &NewCandidate {
            candidate_run_id: run_id,
            prediction_id: fixture.prediction_ids[0],
            rank: 2,
            ranking_score: 0.80,
            model_probability_snapshot: 0.72,
            iddaa_odd_snapshot: Some(1.85),
            market_snapshot: "TOTAL_GOALS",
            selection_snapshot: "OVER",
            line_value_snapshot: Some(2.5),
            explanation_json: None,
        },
    );
    assert!(duplicate.is_err());
}

#[test]
fn final_coupon_and_selections_can_be_inserted_and_read() {
    let database = Database::open_in_memory().expect("fresh database should migrate");
    let connection = database
        .connection()
        .expect("database lock should be available");
    let fixture = seed_predictions(&connection, 1);
    let run_id = insert_candidate_run(&connection, fixture.model_version_id);
    let candidate_id = insert_candidate(&connection, run_id, fixture.prediction_ids[0], 1);
    let (coupon_id, _) =
        insert_coupon_with_selection(&connection, &fixture, run_id, candidate_id, "SINGLE");

    let coupon = coupons::find_by_id(&connection, coupon_id)
        .expect("coupon read should succeed")
        .expect("coupon should exist");
    let selections =
        coupons::list_selections(&connection, coupon_id).expect("selection list should load");

    assert_eq!(coupon.status, "pending");
    assert_eq!(coupon.total_decimal_odd, Some(1.85));
    assert_eq!(selections.len(), 1);
    assert_eq!(selections[0].source_candidate_id, Some(candidate_id));
    assert_eq!(selections[0].market_snapshot, "TOTAL_GOALS");
    assert_eq!(selections[0].odd_snapshot, Some(1.85));
}

#[test]
fn system_sizes_five_and_six_can_be_stored() {
    let database = Database::open_in_memory().expect("fresh database should migrate");
    let connection = database
        .connection()
        .expect("database lock should be available");
    let fixture = seed_predictions(&connection, 1);
    let run_id = insert_candidate_run(&connection, fixture.model_version_id);
    let candidate_id = insert_candidate(&connection, run_id, fixture.prediction_ids[0], 1);
    let (coupon_id, _) =
        insert_coupon_with_selection(&connection, &fixture, run_id, candidate_id, "SYSTEM_5_6");

    coupons::add_system_size(&connection, coupon_id, 5).expect("size 5 should insert");
    coupons::add_system_size(&connection, coupon_id, 6).expect("size 6 should insert");
    assert_eq!(
        coupons::system_sizes(&connection, coupon_id).expect("system sizes should load"),
        vec![5, 6]
    );
}

#[test]
fn duplicate_system_size_is_rejected() {
    let database = Database::open_in_memory().expect("fresh database should migrate");
    let connection = database
        .connection()
        .expect("database lock should be available");
    let fixture = seed_predictions(&connection, 1);
    let coupon_id = coupons::insert(
        &connection,
        &NewCoupon {
            coupon_type: "SYSTEM_5_6",
            target_date: "2026-09-01",
            model_version_id: Some(fixture.model_version_id),
            source_candidate_run_id: None,
            total_decimal_odd: None,
            reference_stake: Some(100.0),
            notes: None,
        },
    )
    .expect("coupon insert should succeed");

    coupons::add_system_size(&connection, coupon_id, 5).expect("first size should insert");
    assert!(coupons::add_system_size(&connection, coupon_id, 5).is_err());
}

#[test]
fn coupon_snapshot_fields_cannot_be_modified() {
    let database = Database::open_in_memory().expect("fresh database should migrate");
    let connection = database
        .connection()
        .expect("database lock should be available");
    let fixture = seed_predictions(&connection, 1);
    let run_id = insert_candidate_run(&connection, fixture.model_version_id);
    let candidate_id = insert_candidate(&connection, run_id, fixture.prediction_ids[0], 1);
    let (coupon_id, selection_id) =
        insert_coupon_with_selection(&connection, &fixture, run_id, candidate_id, "SINGLE");

    assert!(connection
        .execute(
            "UPDATE coupon_candidates SET market_snapshot = 'CHANGED' WHERE id = ?1",
            [candidate_id]
        )
        .is_err());
    assert!(connection
        .execute(
            "UPDATE coupons SET target_date = '2026-09-02' WHERE id = ?1",
            [coupon_id]
        )
        .is_err());
    assert!(connection
        .execute(
            "UPDATE coupon_selections SET odd_snapshot = 9.99 WHERE id = ?1",
            [selection_id]
        )
        .is_err());
}

#[test]
fn coupon_and_selection_settlement_fields_can_transition() {
    let database = Database::open_in_memory().expect("fresh database should migrate");
    let connection = database
        .connection()
        .expect("database lock should be available");
    let fixture = seed_predictions(&connection, 1);
    let run_id = insert_candidate_run(&connection, fixture.model_version_id);
    let candidate_id = insert_candidate(&connection, run_id, fixture.prediction_ids[0], 1);
    let (coupon_id, selection_id) =
        insert_coupon_with_selection(&connection, &fixture, run_id, candidate_id, "SINGLE");

    assert!(
        coupons::settle_selection(&connection, selection_id, "won", "2026-09-01T20:00:00Z")
            .expect("selection settlement should succeed")
    );
    assert!(coupons::settle(
        &connection,
        coupon_id,
        "won",
        185.0,
        85.0,
        "2026-09-01T20:00:00Z"
    )
    .expect("coupon settlement should succeed"));

    let coupon = coupons::find_by_id(&connection, coupon_id)
        .expect("coupon read should succeed")
        .expect("coupon should exist");
    let selections =
        coupons::list_selections(&connection, coupon_id).expect("selection list should load");
    assert_eq!(coupon.status, "won");
    assert_eq!(coupon.settled_return, Some(185.0));
    assert_eq!(coupon.settled_profit, Some(85.0));
    assert_eq!(selections[0].status, "won");
    assert_eq!(
        selections[0].settled_at.as_deref(),
        Some("2026-09-01T20:00:00Z")
    );
}

#[test]
fn coupon_foreign_keys_are_enforced() {
    let database = Database::open_in_memory().expect("fresh database should migrate");
    let connection = database
        .connection()
        .expect("database lock should be available");
    let result = coupon_candidates::insert_run(
        &connection,
        &NewCandidateRun {
            target_date: "2026-09-01",
            coupon_type: "VALUE",
            generated_at: "2026-09-01T08:00:00Z",
            model_version_id: 999_999,
            status: "completed",
            target_candidate_count: 10,
            qualified_candidate_count: 0,
            generation_config_json: None,
        },
    );
    assert!(result.is_err());
}

#[test]
fn database_stats_include_coupon_tables() {
    let database = Database::open_in_memory().expect("fresh database should migrate");
    let connection = database
        .connection()
        .expect("database lock should be available");
    let fixture = seed_predictions(&connection, 1);
    let run_id = insert_candidate_run(&connection, fixture.model_version_id);
    let candidate_id = insert_candidate(&connection, run_id, fixture.prediction_ids[0], 1);
    let (coupon_id, _) =
        insert_coupon_with_selection(&connection, &fixture, run_id, candidate_id, "SYSTEM_5_6");
    coupons::add_system_size(&connection, coupon_id, 5).expect("system size should insert");
    coupons::add_system_size(&connection, coupon_id, 6).expect("system size should insert");

    let counts = stats::database_stats(&connection).expect("database stats should load");
    assert_eq!(counts.coupon_candidate_runs, 1);
    assert_eq!(counts.coupon_candidates, 1);
    assert_eq!(counts.coupons, 1);
    assert_eq!(counts.coupon_selections, 1);
    assert_eq!(counts.coupon_system_sizes, 2);
}
