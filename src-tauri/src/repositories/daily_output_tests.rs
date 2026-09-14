use super::{candidate_engine as candidates, coupon_engine as coupons};
use crate::database::Database;
use rusqlite::{params, params_from_iter, types::Value};
use serde_json::Value as Json;

fn fixture() -> Database {
    let data: Json = serde_json::from_str(include_str!(
        "../../tests/fixtures/daily_2026_09_12_14.json"
    ))
    .unwrap();
    let db = Database::open_in_memory().unwrap();
    {
        let c = db.connection().unwrap();
        c.execute("INSERT INTO model_versions(id,version_identifier,model_name,registry_status,is_active) VALUES(1,'daily-public-snapshot','daily regression','ACTIVE',1)",[]).unwrap();
        for m in data["matches"].as_array().unwrap() {
            let id = |k: &str| m[k].as_i64().unwrap();
            c.execute(
                "INSERT OR IGNORE INTO competitions(id,name,country) VALUES(?1,?2,'Fixture')",
                params![
                    id("competition_id"),
                    format!("Snapshot competition {}", id("competition_id"))
                ],
            )
            .unwrap();
            for team in [id("home_team_id"), id("away_team_id")] {
                c.execute(
                    "INSERT OR IGNORE INTO teams(id,normalized_name) VALUES(?1,?2)",
                    params![team, format!("Snapshot team {team}")],
                )
                .unwrap();
            }
            c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(?1,?2,'2026/27',?3,?4,?5,'scheduled',?6,1)",params![id("id"),id("competition_id"),id("home_team_id"),id("away_team_id"),m["kickoff_at"].as_str(),m["scheduled_local_date"].as_str()]).unwrap();
            c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,calculated_at,feature_json,data_quality_json) VALUES(?1,'fe_v1',?2,'2026-09-11T20:00:00Z','{}',?3)",params![id("id"),m["kickoff_at"].as_str(),m["data_quality_json"].as_str()]).unwrap();
        }
        let fields = [
            "id",
            "match_id",
            "market",
            "selection",
            "line_value",
            "model_probability",
            "raw_probability",
            "public_probability",
            "calibration_status",
            "calibration_version",
            "calibration_bucket",
            "bucket_observed_rate",
            "bucket_sample_size",
            "bucket_calibration_gap",
            "availability",
            "created_at",
        ];
        let sql=format!("INSERT INTO predictions({},confidence_bucket,kickoff_at,model_version_id) VALUES({},'snapshot',(SELECT kickoff_at FROM matches WHERE id=?2),1)",fields.join(","),(1..=fields.len()).map(|i|format!("?{i}")).collect::<Vec<_>>().join(","));
        for p in data["predictions"].as_array().unwrap() {
            let values = fields
                .iter()
                .map(|k| match &p[k] {
                    Json::Null => Value::Null,
                    Json::String(v) => Value::Text(v.clone()),
                    Json::Number(v) => v
                        .as_i64()
                        .map(Value::Integer)
                        .unwrap_or_else(|| Value::Real(v.as_f64().unwrap())),
                    _ => panic!(),
                })
                .collect::<Vec<_>>();
            c.execute(&sql, params_from_iter(values)).unwrap();
            let o = &p["odds"];
            if !o.is_null() {
                c.execute("INSERT OR IGNORE INTO odds_snapshots(id,match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,line_value,captured_at) VALUES(?1,?2,'iddaa','snapshot',?3,?4,?5,?3,?4,?6,?7)",params![o["id"].as_i64(),p["match_id"].as_i64(),p["market"].as_str(),p["selection"].as_str(),o["odd"].as_f64(),p["line_value"].as_f64(),o["captured_at"].as_str()]).unwrap();
            }
        }
    }
    db
}

fn request(date: &str) -> candidates::GenerateRequest {
    candidates::GenerateRequest {
        business_date: Some(date.into()),
        generation_time: Some("2026-09-11T20:36:00Z".into()),
        category: None,
        dry_run: Some(false),
    }
}

#[test]
fn production_three_date_replay_is_stable_and_coupon_backed() {
    let db = fixture();
    let c = db.connection().unwrap();
    for (date, count, coupon_count) in [
        ("2026-09-12", 13, 1),
        ("2026-09-13", 10, 2),
        ("2026-09-14", 3, 0),
    ] {
        let req = request(date);
        let run = candidates::generate_daily_output(&c, &req).unwrap();
        // The archived export contains zero history for its former goal picks.
        assert!(run
            .exclusions
            .iter()
            .any(|x| x.category == "OVER_25" && x.reason == "INSUFFICIENT_HISTORY"));
        assert_eq!(run.candidates.len(), count, "{date}");
        let repeat = candidates::generate_daily_output(&c, &req).unwrap();
        assert_eq!(run, repeat, "{date}");
        let result = coupons::get_daily(&c, date, None).unwrap();
        assert_eq!(result.len(), coupon_count, "{date}");
        for coupon in result {
            assert_eq!(coupon.source_candidate_run_id, run.run_id);
            assert!(!coupon.selections.is_empty());
            if let Some(odd) = coupon.combined_decimal_odd {
                assert!(
                    (odd - coupon.selections.iter().map(|s| s.odd).product::<f64>()).abs() < 1e-9
                );
            }
        }
        // Settlement of today's mutable fixture must not change pre-kickoff replay.
        c.execute(
            "UPDATE matches SET status='finished' WHERE scheduled_local_date=?1",
            [date],
        )
        .unwrap();
        assert_eq!(candidates::generate_daily_output(&c, &req).unwrap(), run);
        let mut empty = req.clone();
        empty.generation_time = Some(format!("{date}T23:59:00Z"));
        let late = candidates::generate_daily_output(&c, &empty).unwrap();
        assert!(late.candidates.is_empty());
        assert_eq!(
            candidates::selected_run_id(&c, date).unwrap(),
            Some(run.run_id)
        );
        assert_eq!(
            coupons::get_daily(&c, date, None).unwrap().len(),
            coupon_count
        );
        // Default coupon generation uses the publication, not the later replay id.
        let regenerated = coupons::generate_daily(
            &c,
            &coupons::GenerateRequest {
                business_date: date.into(),
                candidate_run_id: None,
                coupon_type: None,
                unit_stake_cents: None,
            },
        )
        .unwrap();
        assert!(regenerated
            .iter()
            .all(|x| x.source_candidate_run_id == run.run_id));
        assert_eq!(regenerated.len(), coupon_count);
    }
}

#[test]
fn category_request_does_not_poison_full_run_and_timezone_is_canonical() {
    let db = fixture();
    let c = db.connection().unwrap();
    let mut req = request("2026-09-12");
    // This older export has zero history for its goal selections. Current
    // ranking correctly excludes them; use the populated Surprise pool here.
    req.category = Some("SURPRISE".into());
    let filtered = candidates::generate(&c, &req).unwrap();
    assert!(!filtered.candidates.is_empty());
    req.category = None;
    req.generation_time = Some("2026-09-11T23:36:00+03:00".into());
    let full = candidates::generate(&c, &req).unwrap();
    assert_eq!(full.run_id, filtered.run_id);
    assert_eq!(full.candidates.len(), 13);
    assert_eq!(
        filtered.candidates,
        full.candidates
            .iter()
            .filter(|x| x.category == "SURPRISE")
            .cloned()
            .collect::<Vec<_>>()
    );
}

#[test]
fn zero_selection_draft_is_not_a_usable_coupon() {
    let db = fixture();
    let c = db.connection().unwrap();
    let run = candidates::generate_daily_output(&c, &request("2026-09-14")).unwrap();
    let result = coupons::create_draft(
        &c,
        &coupons::DraftRequest {
            business_date: run.business_date.clone(),
            candidate_run_id: run.run_id,
            coupon_type: "DAILY_CORNERS".into(),
            candidate_ids: vec![],
            unit_stake_cents: None,
        },
    );
    assert!(result.is_err());
    assert_eq!(
        coupons::get_daily(&c, &run.business_date, None)
            .unwrap()
            .len(),
        0
    );
}
