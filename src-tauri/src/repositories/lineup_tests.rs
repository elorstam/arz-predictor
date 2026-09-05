use crate::database::Database;
use crate::repositories::lineup::*;
use rusqlite::params;
use std::time::Instant;

fn db() -> Database {
    Database::open_in_memory().unwrap()
}

fn seed_match(db: &Database, kickoff: &str, known: bool) -> i64 {
    let c = db.connection().unwrap();
    c.execute("INSERT INTO competitions(id,name,country) VALUES(1,'Test League','TR'),(2,'Other League','TR')", []).unwrap();
    for id in 1..=4 {
        c.execute(
            "INSERT INTO teams(id,normalized_name,country) VALUES(?1,?2,'TR')",
            params![id, format!("team {id}")],
        )
        .unwrap();
    }
    c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(1,1,'2026/27',1,2,?1,'scheduled','2026-09-03',?2)",params![kickoff,known]).unwrap();
    1
}

fn payload(match_id: i64, captured: &str, official: bool, away_count: usize) -> LocalLineupPayload {
    let mut players = Vec::new();
    for (side, team, count) in [("HOME", 1, 11usize), ("AWAY", 2, away_count)] {
        for n in 0..count {
            players.push(LineupPlayerInput {
                team_id: team,
                side: side.into(),
                role: "STARTER".into(),
                provider_player_id: Some(format!("{side}-{n}")),
                provider_player_name: format!("{side} Player {n}"),
                position: Some(if n == 0 { "GK" } else { "DF" }.into()),
                shirt_number: Some(n as i64 + 1),
                formation_slot: None,
                captain: Some(n == 0),
                goalkeeper: Some(n == 0),
                date_of_birth: None,
                nationality: Some("TR".into()),
            });
        }
    }
    LocalLineupPayload {
        provider: "local-test".into(),
        provider_event_id: format!("event-{match_id}"),
        match_id,
        captured_at: captured.into(),
        lineup_status: if official {
            "OFFICIAL".into()
        } else {
            "PARTIAL".into()
        },
        is_official: official,
        players,
    }
}

#[test]
fn official_partial_duplicate_and_dedupe_are_safe() {
    let db = db();
    seed_match(&db, "2026-09-03T16:00:00Z", true);
    let c = db.connection().unwrap();
    let p = payload(1, "2026-09-03T14:00:00Z", true, 11);
    let first = import_local(&c, &p).unwrap();
    assert_eq!(first.completeness_status, "COMPLETE_OFFICIAL");
    assert_eq!(first.starter_count_home, 11);
    assert_eq!(first.starter_count_away, 11);
    assert_eq!(first.resolved_players, 22);
    let again = import_local(&c, &p).unwrap();
    assert!(again.deduplicated);
    assert_eq!(again.snapshot_id, first.snapshot_id);
    assert_eq!(history(&c, 1).unwrap().len(), 1);
    let corrected = LocalLineupPayload {
        players: {
            let mut x = p.players.clone();
            x[0].provider_player_name = "Corrected Player".into();
            x
        },
        captured_at: "2026-09-03T14:05:00Z".into(),
        ..p.clone()
    };
    let second = import_local(&c, &corrected).unwrap();
    assert_ne!(second.snapshot_id, first.snapshot_id);
    assert_eq!(history(&c, 1).unwrap().len(), 2);
    let partial = payload(1, "2026-09-03T14:10:00Z", true, 10);
    let result = import_local(&c, &partial).unwrap();
    assert_eq!(result.completeness_status, "PARTIAL");
    let duplicate = LocalLineupPayload {
        players: {
            let mut x = payload(1, "2026-09-03T14:20:00Z", true, 11).players;
            x.push(x[0].clone());
            x
        },
        ..payload(1, "2026-09-03T14:20:00Z", true, 11)
    };
    let result = import_local(&c, &duplicate).unwrap();
    assert_eq!(result.completeness_status, "PARTIAL");
}

#[test]
fn readiness_window_and_unknown_kickoff_are_conservative() {
    let k = "2026-09-03T16:00:00Z";
    assert_eq!(
        readiness(k, true, "2026-09-03T13:00:00Z", false),
        "NOT_EXPECTED_YET"
    );
    assert_eq!(
        readiness(k, true, "2026-09-03T14:50:00Z", false),
        "WAITING_FOR_LINEUP"
    );
    assert_eq!(
        readiness(k, true, "2026-09-03T16:10:00Z", false),
        "LINEUP_REVISION_UNAVAILABLE"
    );
    assert_eq!(
        readiness(k, false, "2026-09-03T14:50:00Z", false),
        "WAITING_FOR_LINEUP"
    );
    assert_eq!(
        readiness(k, true, "2026-09-03T14:50:00Z", true),
        "OFFICIAL_LINEUP_READY"
    );
}

#[test]
fn prior_only_features_and_revision_pending_do_not_change_probability() {
    let db = db();
    seed_match(&db, "2026-09-03T16:00:00Z", true);
    let c = db.connection().unwrap();
    let p = payload(1, "2026-09-03T14:00:00Z", true, 11);
    let imported = import_local(&c, &p).unwrap();
    assert!(generate_features(&c, 1).is_ok());
    let feature = generate_features(&c, 1).unwrap();
    assert_eq!(feature.feature_version, LINEUP_FEATURE_VERSION);
    assert!(feature.features["home"]["prior_match_sample"].as_i64() == Some(0));
    c.execute(
        "INSERT INTO model_versions(id,version_identifier,model_name) VALUES(1,'model_v1','test')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO prediction_runs(id,match_id,model_version_id,feature_engine_version,feature_schema_version,artifact_sha256) VALUES(1,1,1,'fe_v1','pred_features_v1','hash')",[]).unwrap();
    c.execute("INSERT INTO predictions(id,match_id,market,selection,model_probability,confidence_bucket,kickoff_at,model_version_id,prediction_run_id,raw_probability,public_probability,calibration_status,availability,sample_quality) VALUES(1,1,'TOTAL_GOALS','OVER',0.72,'MODEL_OUTPUT','2026-09-03T16:00:00Z',1,1,0.72,0.72,'UNCALIBRATED_V1','AVAILABLE','MODEL_TRAINED')",[]).unwrap();
    assert_eq!(
        pending_revision_for_match(&c, 1, imported.snapshot_id).unwrap(),
        1
    );
    let revision: (Option<f64>, String) = c
        .query_row(
            "SELECT revised_probability,revision_status FROM prediction_revisions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(revision, (None, "REVISION_PENDING_MODEL".into()));
}

#[test]
fn player_resolution_keeps_ambiguous_name_unresolved() {
    let db = db();
    seed_match(&db, "2026-09-03T16:00:00Z", true);
    let c = db.connection().unwrap();
    c.execute("INSERT INTO players(id,canonical_name,normalized_name,primary_team_id) VALUES(10,'Alex Smith','alex smith',3),(11,'Alex Smith','alex smith',4)",[]).unwrap();
    let mut p = payload(1, "2026-09-03T14:00:00Z", false, 10);
    p.players[0].provider_player_id = None;
    p.players[0].provider_player_name = "Alex Smith".into();
    let r = import_local(&c, &p).unwrap();
    assert!(r.unresolved_players >= 1);
}

#[test]
fn deterministic_100_snapshot_feature_benchmark_is_temp_only() {
    let db = db();
    let c = db.connection().unwrap();
    c.execute(
        "INSERT INTO competitions(id,name,country) VALUES(1,'League A','TR'),(2,'League B','TR')",
        [],
    )
    .unwrap();
    for id in 1..=8 {
        c.execute(
            "INSERT INTO teams(id,normalized_name,country) VALUES(?1,?2,'TR')",
            params![id, format!("team {id}")],
        )
        .unwrap();
    }
    let started = Instant::now();
    for i in 0..100i64 {
        let home = 1 + ((i % 4) * 2);
        let away = home + 1;
        let kickoff = format!("2025-{:02}-{:02}T12:00:00Z", 1 + i / 28, 1 + i % 28);
        c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(?1,?2,'2025/26',?3,?4,?5,'finished',substr(?5,1,10),1)",params![i+1,1+i%2,home,away,kickoff]).unwrap();
        let mut p = payload(
            i + 1,
            &format!("2025-{:02}-{:02}T10:00:00Z", 1 + i / 28, 1 + i % 28),
            true,
            11,
        );
        p.provider_event_id = format!("benchmark-{i}");
        for player in &mut p.players {
            if player.side == "HOME" {
                player.team_id = home;
            } else {
                player.team_id = away;
            }
        }
        import_local(&c, &p).unwrap();
    }
    c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(1000,1,'2026/27',1,2,'2026-09-03T16:00:00Z','scheduled','2026-09-03',1)",[]).unwrap();
    let mut target = payload(1000, "2026-09-03T14:00:00Z", true, 11);
    target.provider_event_id = "benchmark-target".into();
    import_local(&c, &target).unwrap();
    let parse_persist_ms = started.elapsed().as_secs_f64() * 1000.0;
    let feature_started = Instant::now();
    for id in 1..=100 {
        let _ = generate_features(&c, id);
    }
    let feature_ms = feature_started.elapsed().as_secs_f64() * 1000.0;
    println!("LINEUP_BENCHMARK snapshots=100 features=100 parse_persist_ms={parse_persist_ms:.3} feature_total_ms={feature_ms:.3} feature_avg_ms={:.3}",feature_ms/100.0);
    assert_eq!(
        c.query_row::<i64, _, _>("SELECT COUNT(*) FROM lineup_snapshots", [], |r| r.get(0))
            .unwrap(),
        101
    );
    assert_eq!(
        c.query_row::<i64, _, _>("SELECT COUNT(*) FROM player_match_participation", [], |r| r
            .get(0))
            .unwrap(),
        2222
    );
}
