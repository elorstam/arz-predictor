use rusqlite::{params, Connection};

use super::{candidate_engine, model_supported_populars as subject};
use crate::database::Database;

const DATE: &str = "2026-09-04";
const CUTOFF: &str = "2026-09-04T12:00:00Z";

fn add_popular(
    c: &Connection,
    event: &str,
    match_id: Option<i64>,
    market_id: &str,
    selection_code: &str,
    market: &str,
    selection: &str,
    line: Option<f64>,
    count: i64,
    display: &str,
    rank: i64,
    captured: &str,
) {
    c.execute(
        "INSERT INTO popularity_snapshots(provider,match_id,provider_event_id,
         provider_market_id,provider_selection_code,market_type,line_value,selection,
         metric_type,metric_value,raw_metric_name,provider_raw_value,captured_at)
         VALUES('iddaa',?1,?2,?3,?4,?5,?6,?7,'COUNT',?8,'totalPlayed',?9,?10)",
        params![
            match_id,
            event,
            market_id,
            selection_code,
            market,
            line,
            selection,
            count,
            display,
            captured
        ],
    )
    .unwrap();
    c.execute(
        "INSERT INTO popularity_snapshots(provider,match_id,provider_event_id,
         provider_market_id,provider_selection_code,market_type,line_value,selection,
         metric_type,rank_value,raw_metric_name,captured_at)
         VALUES('iddaa',?1,?2,?3,?4,?5,?6,?7,'RANK',?8,
                'football_list_position',?9)",
        params![
            match_id,
            event,
            market_id,
            selection_code,
            market,
            line,
            selection,
            rank,
            captured
        ],
    )
    .unwrap();
}

fn add_prediction(
    c: &Connection,
    id: i64,
    match_id: i64,
    market: &str,
    selection: &str,
    line: Option<f64>,
    probability: f64,
) {
    c.execute(
        "INSERT INTO predictions(id,match_id,market,selection,line_value,model_probability,
         confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,
         calibration_status,calibration_version,availability)
         VALUES(?1,?2,?3,?4,?5,?6,'MODEL_OUTPUT',
                (SELECT kickoff_at FROM matches WHERE id=?2),1,?6,?6,
                'CALIBRATED_V1','cal_v1','AVAILABLE')",
        params![id, match_id, market, selection, line, probability],
    )
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
fn add_candidate(
    c: &Connection,
    id: i64,
    prediction_id: i64,
    match_id: i64,
    category: &str,
    market: &str,
    selection: &str,
    line: Option<f64>,
    probability: f64,
    odds: f64,
    rank: i64,
    source: &str,
    revision_id: Option<i64>,
) {
    let implied = 1.0 / odds;
    let edge = probability - implied;
    let ev = probability * odds - 1.0;
    c.execute(
        "INSERT INTO candidate_engine_candidates(
         id,run_id,prediction_id,match_id,competition_id,category,market,selection,line_value,
         raw_probability,public_probability,calibration_status,calibration_version,
         iddaa_odd,odds_snapshot_id,odds_captured_at,implied_probability,probability_edge,
         expected_value,data_quality,score,score_components_json,rank,same_match_group,
         correlation_type,compound_eligible,qualification_state,prediction_source,
         lineup_revision_id,base_public_probability,final_public_probability,
         lineup_model_version)
         VALUES(?1,2,?2,?3,1,?4,?5,?6,?7,?8,?8,'CALIBRATED_V1','cal_v1',
                ?9,?10,?11,?12,?13,?14,'GOOD',?8,'{}',?15,CAST(?3 AS TEXT),
                'NONE',0,'QUALIFIED',?16,?17,
                (SELECT public_probability FROM predictions WHERE id=?2),?8,'lineup_v1')",
        params![
            id,
            prediction_id,
            match_id,
            category,
            market,
            selection,
            line,
            probability,
            odds,
            id + 1000,
            CUTOFF,
            implied,
            edge,
            ev,
            rank,
            source,
            revision_id
        ],
    )
    .unwrap();
}

fn fixture() -> Database {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    c.execute(
        "INSERT INTO model_versions(id,version_identifier,model_name,registry_status,is_active)
         VALUES(1,'model_v1','fixture','ACTIVE',1)",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO competitions(id,name,country,current_season)
         VALUES(1,'Test League','TR','2026/27')",
        [],
    )
    .unwrap();
    for id in 1..=14 {
        c.execute(
            "INSERT INTO teams(id,normalized_name) VALUES(?1,?2)",
            params![id, format!("Team {id}")],
        )
        .unwrap();
    }
    for id in 1..=7i64 {
        c.execute(
            "INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,
             status,scheduled_local_date,kickoff_time_known)
             VALUES(?1,1,'2026/27',?2,?3,?4,'scheduled',?5,1)",
            params![
                id,
                id * 2 - 1,
                id * 2,
                format!("2026-09-04T{:02}:00:00Z", 12 + id),
                DATE
            ],
        )
        .unwrap();
    }
    c.execute(
        "INSERT INTO candidate_engine_runs(id,business_date,timezone,generated_at,model_version,
         calibration_version,candidate_policy_version,feature_engine_version,odds_cutoff_at,
         configuration_hash,input_fingerprint,status,match_count_considered,prediction_context)
         VALUES(1,?1,'Europe/Istanbul',?2,'model_v1','cal_v1',?3,'fe_v1',?2,
                'base-config','base-fingerprint','COMPLETED',7,'BASE')",
        params![DATE, CUTOFF, candidate_engine::POLICY_VERSION],
    )
    .unwrap();
    c.execute(
        "INSERT INTO candidate_engine_runs(id,business_date,timezone,generated_at,model_version,
         calibration_version,candidate_policy_version,feature_engine_version,odds_cutoff_at,
         configuration_hash,input_fingerprint,status,match_count_considered,prediction_context,
         parent_candidate_run_id,lineup_model_version)
         VALUES(2,?1,'Europe/Istanbul',?2,'model_v1','cal_v1',?3,'fe_v1',?2,
                'lineup-config','lineup-fingerprint','COMPLETED',7,'LINEUP_AWARE',1,'lineup_v1')",
        params![DATE, CUTOFF, candidate_engine::POLICY_VERSION],
    )
    .unwrap();

    add_prediction(&c, 1, 1, "MATCH_RESULT", "HOME", None, 0.76);
    add_prediction(&c, 2, 2, "TOTAL_GOALS", "OVER", Some(2.5), 0.60);
    add_prediction(&c, 5, 5, "TOTAL_GOALS", "OVER", Some(3.5), 0.58);
    add_prediction(&c, 6, 6, "FULL_TIME_TOTAL_CORNERS", "OVER", Some(8.5), 0.61);
    add_prediction(&c, 7, 7, "TOTAL_GOALS", "UNDER", Some(2.5), 0.55);

    add_candidate(
        &c,
        101,
        1,
        1,
        "HIGH_CONFIDENCE",
        "MATCH_RESULT",
        "HOME",
        None,
        0.82,
        1.60,
        1,
        "LINEUP_AWARE",
        Some(901),
    );
    add_candidate(
        &c,
        105,
        5,
        5,
        "OVER_35",
        "TOTAL_GOALS",
        "OVER",
        Some(3.5),
        0.66,
        2.10,
        1,
        "LINEUP_AWARE",
        Some(905),
    );
    add_candidate(
        &c,
        106,
        6,
        6,
        "CORNERS",
        "FULL_TIME_TOTAL_CORNERS",
        "OVER",
        Some(8.5),
        0.61,
        1.90,
        1,
        "BASE_RETAINED",
        Some(906),
    );

    c.execute(
        "INSERT INTO odds_snapshots(id,match_id,provider,market_code,market_name,line_value,
         selection,odd,captured_at,normalized_market_type,normalized_selection)
         VALUES(2002,2,'iddaa','101','TOTAL_GOALS',2.5,'Üst',2.0,?1,
                'TOTAL_GOALS','OVER')",
        [CUTOFF],
    )
    .unwrap();
    c.execute(
        "INSERT INTO candidate_engine_exclusions(run_id,prediction_id,match_id,business_date,
         competition_id,category,market,selection,line_value,reason,generated_at)
         VALUES(2,2,2,?1,1,'OVER_25','TOTAL_GOALS','OVER',2.5,
                'LOW_PUBLIC_PROBABILITY',?2)",
        params![DATE, CUTOFF],
    )
    .unwrap();

    add_popular(
        &c,
        "e1",
        Some(1),
        "m1",
        "s1",
        "MATCH_RESULT",
        "1",
        None,
        4500,
        "4500+",
        2,
        "2026-09-04T09:00:00Z",
    );
    add_popular(
        &c,
        "e1",
        Some(1),
        "m1",
        "s1",
        "MATCH_RESULT",
        "1",
        None,
        5000,
        "5.000+",
        1,
        "2026-09-04T10:00:00Z",
    );
    add_popular(
        &c,
        "e2",
        Some(2),
        "m2",
        "s2",
        "TOTAL_GOALS",
        "Üst",
        Some(2.5),
        4012,
        "4.000+",
        2,
        "2026-09-04T10:00:00Z",
    );
    add_popular(
        &c,
        "e3",
        Some(3),
        "m3",
        "s3",
        "DOUBLE_CHANCE",
        "1 veya 0",
        None,
        3000,
        "3.000+",
        3,
        "2026-09-04T10:00:00Z",
    );
    add_popular(
        &c,
        "e4",
        None,
        "m4",
        "s4",
        "MATCH_RESULT",
        "1",
        None,
        2500,
        "2.500+",
        4,
        "2026-09-04T10:00:00Z",
    );
    add_popular(
        &c,
        "e5",
        Some(4),
        "m5",
        "s5",
        "BOTH_TEAMS_TO_SCORE",
        "Var",
        None,
        2000,
        "2.000+",
        5,
        "2026-09-04T10:00:00Z",
    );
    add_popular(
        &c,
        "e6",
        Some(5),
        "m6",
        "s6",
        "TOTAL_GOALS",
        "Üst",
        Some(3.5),
        1500,
        "1.500+",
        6,
        "2026-09-04T10:00:00Z",
    );
    add_popular(
        &c,
        "e7",
        Some(6),
        "m7",
        "s7",
        "CORNERS_TOTAL",
        "Üst",
        Some(8.5),
        1000,
        "1.000+",
        7,
        "2026-09-04T10:00:00Z",
    );
    add_popular(
        &c,
        "e8",
        Some(7),
        "m8",
        "s8",
        "TOTAL_GOALS",
        "Alt",
        Some(2.5),
        500,
        "500+",
        8,
        "2026-09-04T10:00:00Z",
    );
    drop(c);
    db
}

fn request(supported_only: bool) -> subject::GetRequest {
    subject::GetRequest {
        business_date: Some(DATE.into()),
        supported_only: Some(supported_only),
        limit: Some(100),
    }
}

#[test]
fn projection_preserves_popularity_and_exposes_all_typed_statuses() {
    let db = fixture();
    let c = db.connection().unwrap();
    let response = subject::get(&c, &request(false)).unwrap();
    assert_eq!(response.prediction_context.as_deref(), Some("LINEUP_AWARE"));
    assert_eq!(response.candidate_run_id, Some(2));
    assert_eq!(response.total_popular_items, 8);
    assert_eq!(response.model_supported_items, 3);
    assert_eq!(response.below_policy_items, 1);
    assert_eq!(response.unsupported_market_items, 1);
    assert_eq!(response.unresolved_items, 1);
    assert_eq!(response.data_unavailable_items, 1);
    assert_eq!(
        response
            .items
            .iter()
            .map(|i| i.popularity_rank)
            .collect::<Vec<_>>(),
        (1..=8).collect::<Vec<_>>()
    );

    let first = &response.items[0];
    assert_eq!(first.total_played, 5000);
    assert_eq!(first.total_played_round_str.as_deref(), Some("5.000+"));
    assert_eq!(first.support_status, subject::SupportStatus::ModelSupported);
    assert_eq!(first.model_probability, Some(0.82));
    assert_eq!(first.qualified, Some(true));
    assert_eq!(first.prediction_source.as_deref(), Some("LINEUP_AWARE"));
    assert_eq!(first.odds, Some(1.60));
    assert!((first.edge.unwrap() - (0.82 - 0.625)).abs() < 1e-12);
    assert!((first.expected_value.unwrap() - 0.312).abs() < 1e-12);

    let below = &response.items[1];
    assert_eq!(below.total_played, 4012);
    assert_eq!(
        below.support_status,
        subject::SupportStatus::ModelBelowPolicy
    );
    assert_eq!(below.reason.as_deref(), Some("LOW_PUBLIC_PROBABILITY"));
    assert_eq!(below.model_probability, Some(0.60));
    assert_eq!(below.qualified, Some(false));
    assert_eq!(below.odds, Some(2.0));
    assert_eq!(below.implied_probability, Some(0.5));
    assert!((below.edge.unwrap() - 0.1).abs() < 1e-12);
    assert!((below.expected_value.unwrap() - 0.2).abs() < 1e-12);

    assert_eq!(
        response.items[2].support_status,
        subject::SupportStatus::MarketNotSupported
    );
    assert_eq!(response.items[2].model_probability, None);
    assert_eq!(
        response.items[3].support_status,
        subject::SupportStatus::Unresolved
    );
    assert_eq!(
        response.items[3].resolution_status,
        subject::ResolutionStatus::Unresolved
    );
    assert_eq!(response.items[3].model_probability, None);
    assert_eq!(
        response.items[4].support_status,
        subject::SupportStatus::ModelDataUnavailable
    );
    assert_eq!(response.items[4].model_probability, None);
    assert_eq!(
        response.items[7].support_status,
        subject::SupportStatus::ModelNotSupported
    );
    assert_eq!(response.items[7].model_probability, Some(0.55));
    assert_ne!(response.items[1].line, response.items[5].line);
}

#[test]
fn lineup_aware_and_base_retained_candidates_roundtrip_exactly() {
    let db = fixture();
    let c = db.connection().unwrap();
    let response = subject::get(&c, &request(false)).unwrap();
    let lineup = response
        .items
        .iter()
        .find(|item| item.match_id == Some(5))
        .unwrap();
    assert_eq!(lineup.model_probability, Some(0.66));
    assert_eq!(lineup.prediction_source.as_deref(), Some("LINEUP_AWARE"));
    assert_eq!(lineup.lineup_revision_id, Some(905));
    assert_eq!(lineup.lineup_model_version.as_deref(), Some("lineup_v1"));
    let retained = response
        .items
        .iter()
        .find(|item| item.match_id == Some(6))
        .unwrap();
    assert_eq!(retained.model_probability, Some(0.61));
    assert_eq!(retained.prediction_source.as_deref(), Some("BASE_RETAINED"));
    assert_eq!(retained.lineup_revision_id, Some(906));
}

#[test]
fn supported_filter_keeps_rank_order_and_query_is_read_only() {
    let db = fixture();
    let c = db.connection().unwrap();
    let before: (i64, i64, i64) = c
        .query_row(
            "SELECT (SELECT COUNT(*) FROM popularity_snapshots),
                (SELECT COUNT(*) FROM candidate_engine_runs),
                (SELECT COUNT(*) FROM candidate_engine_candidates)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let response = subject::get(&c, &request(true)).unwrap();
    assert_eq!(
        response
            .items
            .iter()
            .map(|i| i.popularity_rank)
            .collect::<Vec<_>>(),
        vec![1, 6, 7]
    );
    assert!(response
        .items
        .iter()
        .all(|i| i.support_status == subject::SupportStatus::ModelSupported));
    let after: (i64, i64, i64) = c
        .query_row(
            "SELECT (SELECT COUNT(*) FROM popularity_snapshots),
                (SELECT COUNT(*) FROM candidate_engine_runs),
                (SELECT COUNT(*) FROM candidate_engine_candidates)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(before, after);
}

#[test]
fn popularity_changes_and_duplicate_snapshots_do_not_change_model_outputs() {
    let db = fixture();
    let c = db.connection().unwrap();
    let before = subject::get(&c, &request(false)).unwrap();
    let original = before
        .items
        .iter()
        .find(|item| item.match_id == Some(1))
        .unwrap()
        .clone();
    add_popular(
        &c,
        "e1",
        Some(1),
        "m1",
        "s1",
        "MATCH_RESULT",
        "1",
        None,
        100,
        "100+",
        20,
        "2026-09-04T11:00:00Z",
    );
    let after = subject::get(&c, &request(false)).unwrap();
    assert_eq!(after.total_popular_items, before.total_popular_items);
    let changed = after
        .items
        .iter()
        .find(|item| item.match_id == Some(1))
        .unwrap();
    assert_eq!((changed.popularity_rank, changed.total_played), (20, 100));
    assert_eq!(changed.model_probability, original.model_probability);
    assert_eq!(changed.qualified, original.qualified);
    assert_eq!(changed.edge, original.edge);
    assert_eq!(changed.expected_value, original.expected_value);
}

#[test]
fn base_run_is_used_when_no_lineup_run_exists() {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    c.execute("INSERT INTO model_versions(id,version_identifier,model_name) VALUES(1,'model_v1','fixture')", []).unwrap();
    c.execute("INSERT INTO competitions(id,name) VALUES(1,'League')", [])
        .unwrap();
    c.execute(
        "INSERT INTO teams(id,normalized_name) VALUES(1,'Home'),(2,'Away')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(1,1,'2026/27',1,2,'2026-09-04T18:00:00Z','scheduled',?1,1)", [DATE]).unwrap();
    c.execute("INSERT INTO candidate_engine_runs(id,business_date,timezone,generated_at,model_version,candidate_policy_version,feature_engine_version,odds_cutoff_at,configuration_hash,input_fingerprint,status,match_count_considered,prediction_context) VALUES(1,?1,'Europe/Istanbul',?2,'model_v1',?3,'fe_v1',?2,'cfg','fp','COMPLETED',1,'BASE')", params![DATE,CUTOFF,candidate_engine::POLICY_VERSION]).unwrap();
    add_prediction(&c, 1, 1, "MATCH_RESULT", "HOME", None, 0.8);
    c.execute("INSERT INTO candidate_engine_candidates(id,run_id,prediction_id,match_id,competition_id,category,market,selection,raw_probability,public_probability,calibration_status,iddaa_odd,odds_snapshot_id,odds_captured_at,implied_probability,probability_edge,expected_value,data_quality,score,score_components_json,rank,same_match_group,correlation_type,compound_eligible,qualification_state,prediction_source,base_public_probability,final_public_probability) VALUES(1,1,1,1,1,'HIGH_CONFIDENCE','MATCH_RESULT','HOME',.8,.8,'CALIBRATED_V1',1.5,1,?1,0.6666666666666666,0.1333333333333334,.2,'GOOD',.8,'{}',1,'1','NONE',0,'QUALIFIED','BASE',.8,.8)", [CUTOFF]).unwrap();
    add_popular(
        &c,
        "base",
        Some(1),
        "m",
        "s",
        "MATCH_RESULT",
        "1",
        None,
        42,
        "42",
        1,
        CUTOFF,
    );
    let response = subject::get(&c, &request(false)).unwrap();
    assert_eq!(response.prediction_context.as_deref(), Some("BASE"));
    assert_eq!(response.candidate_run_id, Some(1));
    assert_eq!(response.items[0].prediction_source.as_deref(), Some("BASE"));
    assert_eq!(response.items[0].model_probability, Some(0.8));
}

#[test]
fn request_and_response_serialization_are_explicit_without_popularity_percentage() {
    let parsed: subject::GetRequest = serde_json::from_value(serde_json::json!({
        "businessDate": DATE,
        "supportedOnly": true,
        "limit": 5
    }))
    .unwrap();
    assert_eq!(
        parsed,
        subject::GetRequest {
            business_date: Some(DATE.into()),
            supported_only: Some(true),
            limit: Some(5)
        }
    );
    let db = fixture();
    let response = subject::get(&db.connection().unwrap(), &request(false)).unwrap();
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"model_probability\""));
    assert!(json.contains("\"total_played\":4012"));
    assert!(!json.contains("popularity_percentage"));
    assert!(!json.contains("popularity_share"));
    assert_eq!(
        candidate_engine::policy().version,
        candidate_engine::POLICY_VERSION
    );
}
