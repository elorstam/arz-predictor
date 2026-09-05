use crate::{
    database::Database,
    repositories::{competitions, features::*, teams},
};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

fn add(
    c: &Connection,
    comp: i64,
    home: i64,
    away: i64,
    date: &str,
    hg: i64,
    ag: i64,
    stats: bool,
    known: bool,
) -> i64 {
    let kick = format!("{date}T15:00:00Z");
    c.execute("INSERT INTO matches(competition_id,season,home_team_id,away_team_id,kickoff_at,status,final_home_goals,final_away_goals,scheduled_local_date,kickoff_time_known) VALUES(?1,'2026/27',?2,?3,?4,'finished',?5,?6,?7,?8)",params![comp,home,away,kick,hg,ag,date,known]).unwrap();
    let id = c.last_insert_rowid();
    if stats {
        c.execute("INSERT INTO match_statistics(match_id,home_shots,away_shots,home_shots_on_target,away_shots_on_target,home_corners,away_corners,home_fouls,away_fouls,home_yellow_cards,away_yellow_cards,home_red_cards,away_red_cards) VALUES(?1,10,8,5,3,6,4,12,10,2,3,0,1)",[id]).unwrap();
    }
    id
}
fn universe() -> (Database, i64, i64, i64, i64, i64, Vec<i64>) {
    let db = Database::open_in_memory().unwrap();
    let c = db.connection().unwrap();
    assert_eq!(
        c.query_row::<i64, _, _>("SELECT MAX(version) FROM schema_migrations", [], |r| r
            .get(0))
            .unwrap(),
        20
    );
    let comp =
        competitions::insert(&c, "Premier League", Some("England"), Some("2026/27")).unwrap();
    let a = teams::insert(&c, "Arsenal", Some("England")).unwrap();
    let b = teams::insert(&c, "Chelsea", Some("England")).unwrap();
    let x = teams::insert(&c, "Everton", Some("England")).unwrap();
    let y = teams::insert(&c, "Liverpool", Some("England")).unwrap();
    let mut hist = Vec::new();
    for i in 1..=10 {
        let date = format!("2026-08-{i:02}");
        hist.push(add(&c, comp, a, x, &date, i % 4, (i + 1) % 3, true, true));
        hist.push(add(&c, comp, y, b, &date, (i + 2) % 3, i % 3, true, true));
    }
    for i in 11..=13 {
        hist.push(add(
            &c,
            comp,
            a,
            b,
            &format!("2026-08-{i:02}"),
            2,
            1,
            true,
            true,
        ));
    }
    let target = add(&c, comp, a, b, "2026-08-20", 1, 1, true, true);
    let f1 = add(&c, comp, a, x, "2026-08-21", 9, 0, true, true);
    let f2 = add(&c, comp, y, b, "2026-08-22", 0, 9, true, true);
    drop(c);
    hist.push(f2);
    (db, comp, a, b, target, f1, hist)
}

#[test]
fn phase_5_feature_engine_and_leakage_acceptance() {
    let (db, comp, _a, _b, target, future, ids) = universe();
    let c = db.connection().unwrap();
    let before = generate(&c, target).unwrap();
    assert_eq!(before.feature_engine_version, "fe_v1");
    assert_eq!(before.home.overall_last10.sample_size, 10);
    assert_eq!(before.away.overall_last10.sample_size, 10);
    assert_eq!(before.home.venue_last5.sample_size, 5);
    assert_eq!(before.away.venue_last5.sample_size, 5);
    assert!(before.home.overall_last5.goals_for_avg.is_some());
    assert_eq!(before.home.overall_last5.goals_for_avg, Some(1.8));
    assert_eq!(before.home.overall_last5.goals_against_avg, Some(1.2));
    assert_eq!(before.away.overall_last5.goals_for_avg, Some(0.8));
    assert_eq!(before.away.overall_last5.goals_against_avg, Some(1.6));
    assert!(before.home.overall_last5.btts_rate.is_some());
    assert_eq!(
        before.home.overall_last5.wins
            + before.home.overall_last5.draws
            + before.home.overall_last5.losses,
        5
    );
    assert!(before.home.overall_last5.points_per_match.is_some());
    assert!(before.home.overall_last5.over_1_5_rate.is_some());
    assert!(before.home.overall_last5.over_2_5_rate.is_some());
    assert!(before.home.overall_last5.over_3_5_rate.is_some());
    assert_eq!(before.home.shooting_last5.shots_for_avg, Some(10.0));
    assert_eq!(before.home.shooting_last5.shots_against_avg, Some(8.0));
    assert_eq!(
        before.home.shooting_last5.shots_on_target_for_avg,
        Some(5.0)
    );
    assert_eq!(before.home.corners_last5.corners_for_avg, Some(6.0));
    assert_eq!(before.home.corners_last5.corners_against_avg, Some(4.0));
    assert!(before.home.corners_last5.over_9_5_rate.is_some());
    assert_eq!(before.home.cards_last5.yellow_for_avg, Some(2.0));
    assert_eq!(before.home.fouls_last5.fouls_committed_avg, Some(12.0));
    assert!(before.home.days_since_last_match.unwrap() > 0.0);
    assert!(before.home.matches_in_last_7_days > 0);
    assert!(before.home.matches_in_last_14_days >= before.home.matches_in_last_7_days);
    assert!(before.home_elo != ELO_INITIAL);
    assert_ne!(generate(&c, future).unwrap().home_elo, before.home_elo);
    assert!(before.home.average_opponent_elo_last5.is_some());
    assert_eq!(before.h2h.sample_size, 3);
    assert!(before.league.sample_size >= 20);
    assert_eq!(before.data_quality.shots_coverage, 1.0);
    assert!(persist(&c, &before).unwrap());
    assert!(!persist(&c, &before).unwrap());
    let frozen_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&before).unwrap()));
    c.execute(
        "UPDATE matches SET final_home_goals=7,final_away_goals=6 WHERE id=?1",
        [target],
    )
    .unwrap();
    c.execute("UPDATE match_statistics SET home_shots=99,away_shots=98,home_corners=30,away_corners=29,home_yellow_cards=9,away_yellow_cards=8 WHERE match_id=?1",[target]).unwrap();
    c.execute(
        "UPDATE matches SET final_home_goals=8,final_away_goals=8 WHERE id=?1",
        [future],
    )
    .unwrap();
    c.execute("UPDATE match_statistics SET home_shots=88,home_corners=28,home_yellow_cards=7 WHERE match_id=?1",[future]).unwrap();
    c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,captured_at) VALUES(?1,'iddaa','1','MATCH_RESULT','1',1.1,'2026-08-19T12:00:00Z')",[target]).unwrap();
    c.execute("INSERT INTO popularity_snapshots(provider,match_id,provider_event_id,metric_type,metric_value,raw_metric_name,captured_at) VALUES('iddaa',?1,'target','COUNT',99999,'totalPlayed','2026-08-19T12:00:00Z')",[target]).unwrap();
    let after = generate(&c, target).unwrap();
    assert_eq!(before, after);
    assert_eq!(
        frozen_hash,
        format!("{:x}", Sha256::digest(serde_json::to_vec(&after).unwrap()))
    );
    let m3 = ids[2];
    c.execute("UPDATE matches SET final_home_goals=6 WHERE id=?1", [m3])
        .unwrap();
    assert_ne!(before, generate(&c, target).unwrap());
    let label = label(&c, target).unwrap().unwrap();
    assert_eq!(label.total_goals, 13);
    assert_eq!(label.total_corners, Some(59));
    assert!(label.corner_thresholds.unwrap()["over_11_5"]);
    assert!(label.card_thresholds.unwrap()["over_6_5"]);
    let readiness = market_readiness(&c).unwrap();
    assert_eq!(
        readiness
            .iter()
            .find(|r| r.market == "FIRST_HALF_TOTAL_CORNERS")
            .unwrap()
            .state,
        "SOURCE_NOT_AVAILABLE"
    );
    assert_eq!(
        readiness
            .iter()
            .find(|r| r.market == "FIRST_HALF_TOTAL_CARDS")
            .unwrap()
            .state,
        "SOURCE_NOT_AVAILABLE"
    );
    assert_eq!(
        readiness
            .iter()
            .find(|r| r.market == "FULL_TIME_TOTAL_CORNERS")
            .unwrap()
            .state,
        "SUPPORTED"
    );
    assert_eq!(
        readiness
            .iter()
            .find(|r| r.market == "FULL_TIME_TOTAL_CARDS")
            .unwrap()
            .state,
        "SUPPORTED"
    );
    let summary = generate_dataset(&c, Some(comp), Some("2026/27")).unwrap();
    assert!(summary.generated > 0);
    assert_eq!(
        c.query_row::<i64, _, _>("SELECT COUNT(*) FROM training_labels", [], |r| r.get(0))
            .unwrap(),
        summary.eligible_matches as i64
    );
    assert_eq!(quality_summary(&c).unwrap().feature_engine_version, "fe_v1");
    println!("FEATURE_INTEGRATION competitions=1 teams=4 finished=26 date_range=2026-08-01..2026-08-22 eligible={} generated={} insufficient={} avg_history={:.2} shots={:.2} corners={:.2} cards={:.2} fouls={:.2} LEAKAGE=PASS DETERMINISM=PASS",summary.eligible_matches,summary.generated,summary.skipped_insufficient_history,summary.average_historical_sample_size,summary.average_shots_coverage,summary.average_corners_coverage,summary.average_cards_coverage,summary.average_fouls_coverage);
}

#[test]
fn conservative_cutoff_missing_stats_and_unfinished_are_safe() {
    let (db, comp, a, b, target, _, _) = universe();
    let c = db.connection().unwrap();
    let baseline = generate(&c, target).unwrap();
    let same = add(&c, comp, b, a, "2026-08-20", 9, 9, true, true);
    assert_eq!(baseline, generate(&c, target).unwrap());
    c.execute("UPDATE matches SET status='scheduled' WHERE id=?1", [same])
        .unwrap();
    assert_eq!(baseline, generate(&c, target).unwrap());
    let unknown_target = add(&c, comp, a, b, "2026-08-15", 0, 0, false, false);
    let conservative = generate(&c, unknown_target).unwrap();
    assert_eq!(
        conservative.data_quality.kickoff_quality,
        "DATE_ONLY_CONSERVATIVE"
    );
    assert!(conservative.home.overall_last10.sample_size <= 10);
    let missing = add(&c, comp, a, b, "2026-08-16", 1, 0, false, true);
    let snap = generate(&c, target).unwrap();
    assert!(snap.home.shooting_last10.sample_size < snap.home.overall_last10.sample_size);
    assert!(label(&c, missing).unwrap().unwrap().total_corners.is_none());
    assert!(label(&c, missing)
        .unwrap()
        .unwrap()
        .total_yellow_cards
        .is_none());
}
