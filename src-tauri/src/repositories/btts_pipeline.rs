//! BTTS-only recovery, immutable audit and publication. Other coupons stay frozen.
use super::{
    calibration, candidate_engine as ce, coupon_engine as coupons, features, iddaa,
    prediction_engine as pe,
};
use crate::providers::iddaa::{
    dto::parse_bulletin,
    markets::{normalize_selection, NormalizedMarketType},
};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Audit {
    pub business_date: String,
    pub as_of: String,
    pub policy_version: String,
    pub stages: BTreeMap<String, usize>,
    pub primary_exclusions: BTreeMap<String, usize>,
    pub matches: Vec<Value>,
    pub mapping_rows: Vec<Value>,
    pub selected: Vec<ce::Candidate>,
    pub candidate_run_id: Option<i64>,
    pub coupon_id: Option<i64>,
    pub combined_odds: Option<f64>,
    pub refresh: Option<Value>,
}

fn time(s: &str) -> Result<DateTime<chrono::FixedOffset>, String> {
    DateTime::parse_from_rfc3339(s).map_err(|e| e.to_string())
}

fn decision(c: &Connection, day: &str, as_of: Option<&str>) -> Result<String, String> {
    if let Some(value) = as_of {
        time(value)?;
        return Ok(value.into());
    }
    let now = Utc::now();
    let today = now
        .with_timezone(&chrono_tz::Europe::Istanbul)
        .date_naive()
        .to_string();
    if day < today.as_str() {
        return c.query_row("SELECT r.generated_at FROM daily_output_publications d JOIN candidate_engine_runs r ON r.id=d.candidate_run_id WHERE d.business_date=?1", [day], |r|r.get(0)).map_err(|e|e.to_string());
    }
    Ok(now.to_rfc3339())
}

/// Ready means a known kickoff, resolved canonical identity and >=5 games/team
/// in an available snapshot. Started games remain in the trace, excluded later.
fn ready_rows(c: &Connection, day: &str, as_of: &str) -> Result<Vec<Value>, String> {
    let mut q=c.prepare("SELECT m.id,h.normalized_name,a.normalized_name,m.kickoff_at,m.kickoff_time_known,m.status,fs.feature_json,fs.data_quality_json, NOT EXISTS(SELECT 1 FROM provider_match_mappings pm WHERE pm.match_id=m.id AND pm.provider='iddaa') OR (EXISTS(SELECT 1 FROM provider_competition_mappings pc WHERE pc.competition_id=m.competition_id AND pc.provider='football-data.co.uk') AND EXISTS(SELECT 1 FROM provider_team_mappings pt WHERE pt.team_id=m.home_team_id AND pt.provider='football-data.co.uk') AND EXISTS(SELECT 1 FROM provider_team_mappings pt WHERE pt.team_id=m.away_team_id AND pt.provider='football-data.co.uk')) FROM matches m JOIN teams h ON h.id=m.home_team_id JOIN teams a ON a.id=m.away_team_id LEFT JOIN feature_sets fs ON fs.id=(SELECT id FROM feature_sets WHERE match_id=m.id AND julianday(calculated_at)<=julianday(?2) ORDER BY julianday(calculated_at) DESC,id DESC LIMIT 1) WHERE m.scheduled_local_date=?1 ORDER BY m.kickoff_at,m.id").map_err(|e|e.to_string())?;
    let rows = q
        .query_map(params![day, as_of], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, bool>(8)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|(id,home,away,kickoff,known,status,f,q,resolved)|{
        let f:Value=serde_json::from_str(f.as_deref().unwrap_or("{}")).unwrap_or_default();
        let q:Value=serde_json::from_str(q.as_deref().unwrap_or("{}")).unwrap_or_default();
        let hn=f.pointer("/home/overall_last10/sample_size").or_else(||q.get("history_matches_home")).and_then(Value::as_u64).unwrap_or(0);
        let an=f.pointer("/away/overall_last10/sample_size").or_else(||q.get("history_matches_away")).and_then(Value::as_u64).unwrap_or(0);
        json!({"match_id":id,"home":home,"away":away,"kickoff":kickoff,"status":status,"kickoff_known":known,"resolved":resolved,"history_home":hn,"history_away":an,"prediction_ready":known&&resolved&&hn>=5&&an>=5})
    }).collect())
}

pub fn audit(c: &Connection, day: &str, as_of: Option<&str>) -> Result<Audit, String> {
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|e| e.to_string())?;
    let as_of = decision(c, day, as_of)?;
    let cutoff = time(&as_of)?;
    let run = ce::generate(
        c,
        &ce::GenerateRequest {
            business_date: Some(day.into()),
            generation_time: Some(as_of.clone()),
            category: Some("BTTS_YES".into()),
            dry_run: Some(true),
        },
    )?;
    let by_match: BTreeMap<_, _> = run.candidates.iter().map(|p| (p.match_id, p)).collect();
    let mut stages: BTreeMap<String, usize> = [
        "fixtures",
        "prediction_ready",
        "btts_probability",
        "calibrated_btts_probability",
        "public_btts_fallback",
        "btts_odds",
        "valid_yes_mapping",
        "invalid_input_survivors",
        "ranking_pool",
        "selected",
        "pair_sum_failures",
        "current_matches_with_yes_odds",
        "mapping_failed_matches",
    ]
    .into_iter()
    .map(|s| (s.into(), 0))
    .collect();
    let mut exclusions = BTreeMap::new();
    let mut matches = ready_rows(c, day, &as_of)?;
    let mut mapping_rows = Vec::new();
    for row in &mut matches {
        stages.entry("fixtures".into()).and_modify(|v| *v += 1);
        let id = row["match_id"].as_i64().unwrap();
        let mut ps=c.prepare("SELECT p.id,p.selection,COALESCE(p.public_probability,p.model_probability),p.raw_probability,p.calibration_status,p.calibration_version,p.created_at,p.prediction_run_id,p.availability FROM predictions p WHERE p.match_id=?1 AND p.market='BTTS' AND p.model_version_id=(SELECT id FROM model_versions WHERE is_active=1) AND julianday(p.created_at)<=julianday(?2) ORDER BY julianday(p.created_at) DESC,p.id DESC").map_err(|e|e.to_string())?;
        let predictions=ps.query_map(params![id,as_of],|r|Ok((r.get::<_,String>(1)?,json!({"prediction_id":r.get::<_,i64>(0)?,"probability":r.get::<_,Option<f64>>(2)?,"raw_probability":r.get::<_,Option<f64>>(3)?,"calibration_status":r.get::<_,Option<String>>(4)?,"calibration_version":r.get::<_,Option<String>>(5)?,"created_at":r.get::<_,String>(6)?,"prediction_run_id":r.get::<_,Option<i64>>(7)?,"availability":r.get::<_,Option<String>>(8)?})))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        let yes = predictions
            .iter()
            .find(|p| p.0 == "YES")
            .map(|p| p.1.clone());
        let no = predictions
            .iter()
            .find(|p| p.0 == "NO")
            .map(|p| p.1.clone());
        let yp = yes.as_ref().and_then(|v| v["probability"].as_f64());
        let np = no.as_ref().and_then(|v| v["probability"].as_f64());
        let pair_ok = yp.zip(np).is_some_and(|(y, n)| {
            y.is_finite()
                && n.is_finite()
                && (0.0..=1.0).contains(&y)
                && (0.0..=1.0).contains(&n)
                && (y + n - 1.0).abs() < 1e-8
        }) && yes.as_ref().zip(no.as_ref()).is_some_and(|(y, n)| {
            y["prediction_run_id"] == n["prediction_run_id"]
                && (!y["prediction_run_id"].is_null() || y["created_at"] == n["created_at"])
        });
        row["yes"] = json!(yes);
        row["no"] = json!(no);
        row["pair_sum_valid"] = json!(pair_ok);
        let mut oq=c.prepare("SELECT o.id,o.market_code,o.market_name,o.selection,o.normalized_market_type,o.normalized_selection,o.odd,o.captured_at,o.provider_market_id,o.provider_selection_code,(SELECT external_match_id FROM provider_match_mappings WHERE match_id=o.match_id AND provider='iddaa' LIMIT 1),o.line_value FROM odds_snapshots o WHERE o.match_id=?1 AND o.provider='iddaa' AND julianday(o.captured_at)<=julianday(?2) AND (o.market_code='89' OR o.normalized_market_type IN ('BTTS','BOTH_TEAMS_TO_SCORE') OR lower(o.market_name) LIKE '%btts%' OR lower(o.market_name) LIKE '%kg %' OR lower(o.market_name) LIKE '%karşılıklı%') ORDER BY julianday(o.captured_at) DESC,o.id DESC").map_err(|e|e.to_string())?;
        let odds=oq.query_map(params![id,as_of],|r|Ok(json!({"match_id":id,"snapshot_id":r.get::<_,i64>(0)?,"raw_market_code":r.get::<_,String>(1)?,"raw_market_name":r.get::<_,String>(2)?,"raw_selection":r.get::<_,String>(3)?,"normalized_market":r.get::<_,Option<String>>(4)?,"normalized_outcome":r.get::<_,Option<String>>(5)?,"odds":r.get::<_,f64>(6)?,"captured_at":r.get::<_,String>(7)?,"provider_market_id":r.get::<_,Option<String>>(8)?,"provider_selection_code":r.get::<_,Option<String>>(9)?,"event_id":r.get::<_,Option<String>>(10)?,"line":r.get::<_,Option<f64>>(11)?}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        let latest_yes = odds.iter().find(|q| {
            q["normalized_outcome"] == "YES"
                && matches!(
                    q["normalized_market"].as_str(),
                    Some("BTTS" | "BOTH_TEAMS_TO_SCORE")
                )
                && q["line"].is_null()
        });
        let raw_yes = odds.iter().find(|q| {
            normalize_selection(
                NormalizedMarketType::BothTeamsToScore,
                q["raw_selection"].as_str().unwrap_or(""),
            ) == Some("YES")
        });
        let mapping_failed = raw_yes.is_some() && latest_yes.is_none();
        if latest_yes.is_some() {
            *stages.get_mut("current_matches_with_yes_odds").unwrap() += 1;
        }
        if mapping_failed {
            *stages.get_mut("mapping_failed_matches").unwrap() += 1;
        }
        let mut seen = BTreeSet::new();
        for q in &odds {
            if seen.insert((
                q["raw_market_code"].to_string(),
                q["raw_selection"].to_string(),
            )) {
                mapping_rows.push(q.clone());
            }
        }
        row["odds"] = json!(latest_yes);
        row["mapping_failed"] = json!(mapping_failed);
        if row["prediction_ready"] != true {
            row["primary_exclusion"] = json!(if row["resolved"] != true {
                "UNRESOLVED"
            } else if row["kickoff_known"] != true {
                "OTHER"
            } else {
                "INSUFFICIENT_HISTORY"
            });
            continue;
        }
        *stages.get_mut("prediction_ready").unwrap() += 1;
        if yp.is_some() {
            *stages.get_mut("btts_probability").unwrap() += 1;
        }
        if yp.is_some()
            && yes
                .as_ref()
                .is_some_and(|y| y["calibration_status"] == "CALIBRATED_V1")
        {
            *stages.get_mut("calibrated_btts_probability").unwrap() += 1;
        } else if yp.is_some() {
            *stages.get_mut("public_btts_fallback").unwrap() += 1;
        }
        if !pair_ok {
            *stages.get_mut("pair_sum_failures").unwrap() += 1;
        }
        if yp.is_some() && (raw_yes.is_some() || latest_yes.is_some()) {
            *stages.get_mut("btts_odds").unwrap() += 1;
        }
        if yp.is_some() && latest_yes.is_some() {
            *stages.get_mut("valid_yes_mapping").unwrap() += 1;
        }
        let reason = if yp.is_none() || np.is_none() {
            Some("NO_BTTS_PREDICTION")
        } else if !pair_ok {
            Some("POOR_MODEL_QUALITY")
        } else if latest_yes.is_none() {
            Some("NO_BTTS_ODDS")
        } else if latest_yes.unwrap()["odds"]
            .as_f64()
            .is_none_or(|v| !v.is_finite() || v <= 1.0)
        {
            Some("INVALID_ODDS")
        } else if (cutoff - time(latest_yes.unwrap()["captured_at"].as_str().unwrap())?)
            .num_seconds()
            > 24 * 3600
        {
            Some("STALE_ODDS")
        } else if !by_match.contains_key(&id) {
            let rejected = run.exclusions.iter().find(|e| {
                e.match_id == Some(id)
                    && e.market.as_deref() == Some("BTTS")
                    && e.selection.as_deref() == Some("YES")
            });
            Some(match rejected.map(|e| e.reason.as_str()) {
                Some("INSUFFICIENT_HISTORY" | "LOW_DATA_QUALITY") => "INSUFFICIENT_HISTORY",
                Some("RESOLUTION_FAILED") => "UNRESOLVED",
                Some("NEGATIVE_MODEL_QUALITY" | "INVALID_PROBABILITY" | "MODEL_UNAVAILABLE") => {
                    "POOR_MODEL_QUALITY"
                }
                Some("CORRELATION_REDUCTION") => "DUPLICATE",
                _ => "OTHER",
            })
        } else {
            None
        };
        row["primary_exclusion"] = json!(reason);
        if let Some(reason) = reason {
            *exclusions.entry(reason.into()).or_insert(0) += 1;
            row["detail"] = json!(if reason == "OTHER" {
                "Match started, cancelled, unknown kickoff, or unavailable at this cutoff"
            } else if mapping_failed {
                "Raw YES alias exists but normalized mapping is missing"
            } else {
                reason
            });
        } else {
            *stages.get_mut("invalid_input_survivors").unwrap() += 1;
        }
    }
    let survivors: BTreeSet<_> = matches
        .iter()
        .filter(|r| r["prediction_ready"] == true && r["primary_exclusion"].is_null())
        .filter_map(|r| r["match_id"].as_i64())
        .collect();
    let pool: Vec<_> = run
        .candidates
        .into_iter()
        .filter(|p| survivors.contains(&p.match_id))
        .collect();
    stages.insert("ranking_pool".into(), pool.len());
    let selected = if pool.len() >= 5 {
        pool.into_iter().take(7).collect::<Vec<_>>()
    } else {
        vec![]
    };
    stages.insert("selected".into(), selected.len());
    let combined_odds =
        (!selected.is_empty()).then(|| selected.iter().map(|p| p.iddaa_odd).product());
    Ok(Audit {
        business_date: day.into(),
        as_of,
        policy_version: ce::POLICY_VERSION.into(),
        stages,
        primary_exclusions: exclusions,
        matches,
        mapping_rows,
        selected,
        candidate_run_id: None,
        coupon_id: None,
        combined_odds,
        refresh: None,
    })
}

/// Import only current provider BTTS observations for already linked event ids.
pub fn import_prices(c: &Connection, bytes: &[u8], captured: &str) -> Result<Value, String> {
    let parsed = parse_bulletin(bytes)?;
    let (mut linked, mut inserted, mut unknown) = (0, 0, Vec::new());
    for event in parsed.events {
        let Some(event_id) = event.i else { continue };
        let id:Option<i64>=c.query_row("SELECT match_id FROM provider_match_mappings WHERE provider='iddaa' AND external_match_id=?1",[event_id.to_string()],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
        let Some(id) = id else { continue };
        linked += 1;
        for market in event.m.iter().filter(|m| m.st == Some(89)) {
            for selection in &market.o {
                let Some(raw) = selection.n.as_deref() else {
                    continue;
                };
                let outcome = normalize_selection(NormalizedMarketType::BothTeamsToScore, raw);
                if outcome.is_none() {
                    unknown.push(json!({"event_id":event_id,"market_code":89,"raw_selection":raw}));
                    continue;
                }
                let Some(odd) = selection.odd.filter(|v| v.is_finite() && *v > 0.0) else {
                    continue;
                };
                inserted += usize::from(
                    iddaa::insert_odds_if_changed(
                        c,
                        &iddaa::OddsInput {
                            match_id: id,
                            market_code: "89",
                            provider_market_id: market.i.map(|v| v.to_string()).as_deref(),
                            normalized_market_type: "BOTH_TEAMS_TO_SCORE",
                            provider_line: market.sov.as_deref(),
                            line_value: None,
                            provider_selection_code: selection.no.map(|v| v.to_string()).as_deref(),
                            selection: raw,
                            normalized_selection: outcome,
                            odd,
                            alternative_odd: selection.wodd,
                            captured_at: captured,
                        },
                    )
                    .map_err(|e| e.to_string())?,
                );
            }
        }
    }
    Ok(
        json!({"captured_at":captured,"linked_events":linked,"btts_snapshots_inserted":inserted,"unmapped_selections":unknown}),
    )
}

pub fn refresh_predictions(c: &Connection, day: &str, as_of: &str) -> Result<Value, String> {
    let (model_id, artifact) = pe::active(c)?;
    let cal = calibration::active_calibration(c)?;
    let mut generated = 0;
    let mut errors = Vec::new();
    for r in ready_rows(c, day, as_of)?
        .into_iter()
        .filter(|r| r["prediction_ready"] == true && r["status"] == "scheduled")
    {
        if time(r["kickoff"].as_str().unwrap())? <= time(as_of)? {
            continue;
        }
        let id = r["match_id"].as_i64().unwrap();
        let result = (|| -> Result<(), String> {
            let snapshot = features::generate(c, id)?;
            features::persist(c, &snapshot)?;
            let mut result = pe::infer(&artifact, &snapshot)?;
            if let Some(ref cal) = cal {
                calibration::apply(&mut result, cal);
            }
            result.markets.retain(|m| m.market_type == "BTTS");
            let sum: f64 = result
                .markets
                .iter()
                .filter_map(|m| m.public_probability)
                .sum();
            if result.markets.len() != 2 || (sum - 1.0).abs() > 1e-8 {
                return Err("BTTS_PAIR_INVALID".into());
            }
            pe::persist_predictions(c, &result, model_id, r["kickoff"].as_str().unwrap())?;
            Ok(())
        })();
        match result {
            Ok(()) => generated += 1,
            Err(e) => errors.push(json!({"match_id":id,"error":e})),
        }
    }
    Ok(json!({"btts_pairs_generated":generated,"errors":errors}))
}

pub fn publish(
    c: &Connection,
    day: &str,
    as_of: &str,
    refresh: Option<Value>,
) -> Result<Audit, String> {
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let mut report = audit(&tx, day, Some(as_of))?;
    report.refresh = refresh;
    let source = ce::generate(
        &tx,
        &ce::GenerateRequest {
            business_date: Some(day.into()),
            generation_time: Some(as_of.into()),
            category: Some("BTTS_YES".into()),
            dry_run: Some(true),
        },
    )?;
    let fingerprint = format!(
        "btts-only|{}|{}|{}",
        day,
        as_of,
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    tx.execute("INSERT INTO candidate_engine_runs(business_date,timezone,generated_at,model_version,calibration_version,candidate_policy_version,feature_engine_version,odds_cutoff_at,configuration_hash,input_fingerprint,status,match_count_considered,prediction_context) VALUES(?1,'Europe/Istanbul',?2,?3,?4,?5,'fe_v1',?2,?5,?6,'COMPLETED',?7,'BASE')",params![day,as_of,source.model_version,source.calibration_version,ce::POLICY_VERSION,fingerprint,report.stages["prediction_ready"]]).map_err(|e|e.to_string())?;
    let run_id = tx.last_insert_rowid();
    let valid: BTreeSet<_> = report
        .matches
        .iter()
        .filter(|r| r["prediction_ready"] == true && r["primary_exclusion"].is_null())
        .filter_map(|r| r["match_id"].as_i64())
        .collect();
    for candidate in &source.candidates {
        if valid.contains(&candidate.match_id) {
            ce::insert_candidate(&tx, run_id, candidate)?;
        }
    }
    for row in report
        .matches
        .iter()
        .filter(|r| r["prediction_ready"] == true && !r["primary_exclusion"].is_null())
    {
        tx.execute("INSERT INTO candidate_engine_exclusions(run_id,match_id,business_date,category,market,selection,reason,details_json,generated_at) VALUES(?1,?2,?3,'BTTS_YES','BTTS','YES',?4,?5,?6)",params![run_id,row["match_id"].as_i64(),day,row["primary_exclusion"].as_str(),row.to_string(),as_of]).map_err(|e|e.to_string())?;
    }
    let generated = coupons::generate_daily(
        &tx,
        &coupons::GenerateRequest {
            business_date: day.into(),
            candidate_run_id: Some(run_id),
            coupon_type: Some("DAILY_BTTS".into()),
            unit_stake_cents: None,
        },
    )?;
    report.candidate_run_id = Some(run_id);
    report.coupon_id = generated.first().map(|c| c.id);
    report.selected = ce::get(
        &tx,
        &ce::GetRequest {
            business_date: day.into(),
            run_id: Some(run_id),
            category: Some("BTTS_YES".into()),
        },
    )?
    .candidates
    .into_iter()
    .take(if report.stages["ranking_pool"] >= 5 {
        7
    } else {
        0
    })
    .collect();
    tx.execute("INSERT INTO btts_daily_runs(business_date,as_of,candidate_run_id,report_json) VALUES(?1,?2,?3,?4)",params![day,as_of,run_id,serde_json::to_string(&report).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(report)
}

pub fn latest(c: &Connection, day: &str) -> Result<Audit, String> {
    let stored:Option<String>=c.query_row("SELECT report_json FROM btts_daily_runs WHERE business_date=?1 ORDER BY id DESC LIMIT 1",[day],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
    match stored {
        Some(s) => serde_json::from_str(&s).map_err(|e| e.to_string()),
        None => audit(c, day, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;

    fn fixture() -> Database {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        c.execute("INSERT INTO model_versions(version_identifier,model_name,registry_status,is_active) VALUES('btts-test','test','ACTIVE',1)",[]).unwrap();
        c.execute(
            "INSERT INTO competitions(name,country,current_season) VALUES('Test','TR','2026/27')",
            [],
        )
        .unwrap();
        for id in 1..=10 {
            c.execute(
                "INSERT INTO teams(id,normalized_name) VALUES(?1,?2),(?3,?4)",
                params![id * 2 - 1, format!("H{id}"), id * 2, format!("A{id}")],
            )
            .unwrap();
            c.execute("INSERT INTO matches(id,competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(?1,1,'2026/27',?2,?3,'2026-09-13T18:00:00Z','scheduled','2026-09-13',1)",params![id,id*2-1,id*2]).unwrap();
            c.execute("INSERT INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json,calculated_at) VALUES(?1,'fe_v1','2026-09-13T10:00:00Z','{}','{\"history_matches_home\":10,\"history_matches_away\":10}','2026-09-13T10:00:00Z')",[id]).unwrap();
            if id != 9 {
                for (selection, p) in [("YES", 0.58), ("NO", 0.42)] {
                    c.execute("INSERT INTO predictions(match_id,market,selection,model_probability,confidence_bucket,kickoff_at,model_version_id,raw_probability,public_probability,calibration_status,availability,created_at) VALUES(?1,'BTTS',?2,?3,'test','2026-09-13T18:00:00Z',1,?3,?3,'UNCALIBRATED_V1','AVAILABLE','2026-09-13T11:00:00Z')",params![id,selection,p]).unwrap();
                }
            }
            if id != 10 {
                c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,normalized_market_type,normalized_selection,captured_at) VALUES(?1,'iddaa','89','BOTH_TEAMS_TO_SCORE','Var',1.5,'BOTH_TEAMS_TO_SCORE','YES','2026-09-13T11:00:00Z')",[id]).unwrap();
            }
        }
        drop(c);
        db
    }

    #[test]
    fn btts_trace_has_one_primary_reason_per_excluded_ready_match_and_real_top_seven() {
        let db = fixture();
        let c = db.connection().unwrap();
        let a = audit(&c, "2026-09-13", Some("2026-09-13T12:00:00Z")).unwrap();
        assert_eq!(a.stages["prediction_ready"], 10);
        assert_eq!(a.stages["btts_probability"], 9);
        assert_eq!(a.stages["ranking_pool"], 8);
        assert_eq!(a.stages["selected"], 7);
        assert_eq!(a.primary_exclusions.get("NO_BTTS_PREDICTION"), Some(&1));
        assert_eq!(a.primary_exclusions.get("NO_BTTS_ODDS"), Some(&1));
        assert_eq!(
            a.primary_exclusions.values().sum::<usize>() + a.stages["ranking_pool"],
            10
        );
        assert!(a
            .selected
            .iter()
            .all(|p| p.public_probability < 0.64 && p.expected_value < 0.0));
        let p = publish(&c, "2026-09-13", "2026-09-13T12:00:00Z", None).unwrap();
        let visible = coupons::get_daily(&c, "2026-09-13", Some("DAILY_BTTS")).unwrap();
        let output = super::super::daily_selections::get(&c, "2026-09-13").unwrap();
        assert_eq!(
            output.publication.category_run_ids["BTTS_YES"],
            p.candidate_run_id.unwrap()
        );
        assert_eq!(output.run.category_counts["BTTS_YES"], 8);
        for selection in &visible[0].selections {
            assert!(output
                .run
                .run
                .candidates
                .iter()
                .any(|x| x.id == selection.candidate_id));
            assert_eq!(
                output.run.selection_sources[&selection.candidate_id].selection_status,
                "DAILY_RANKED"
            );
            assert!(!output.run.selection_sources[&selection.candidate_id].strict_qualified);
        }
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, p.coupon_id.unwrap());
        assert_eq!(visible[0].selections.len(), 7);
        assert!((visible[0].combined_decimal_odd.unwrap() - 1.5f64.powi(7)).abs() < 1e-10);
        assert_eq!(
            c.query_row::<i64, _, _>(
                "SELECT count(*) FROM candidate_engine_exclusions WHERE run_id=?1",
                [p.candidate_run_id.unwrap()],
                |r| r.get(0)
            )
            .unwrap(),
            2
        );
        assert_eq!(
            c.query_row::<i64, _, _>("SELECT count(*) FROM daily_output_publications", [], |r| r
                .get(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn btts_stale_or_invalid_pairs_cannot_fill_coupon_and_hide_previous_ready_snapshot() {
        let db = fixture();
        let c = db.connection().unwrap();
        publish(&c, "2026-09-13", "2026-09-13T12:00:00Z", None).unwrap();
        // A new assessment with stale quotes must hide the older ready BTTS coupon.
        let a = publish(&c, "2026-09-13", "2026-09-14T12:00:00Z", None).unwrap();
        assert_eq!(a.stages["ranking_pool"], 0);
        assert_eq!(a.primary_exclusions.get("STALE_ODDS"), Some(&8));
        assert!(coupons::get_daily(&c, "2026-09-13", Some("DAILY_BTTS"))
            .unwrap()
            .is_empty());
        c.execute("INSERT INTO predictions(match_id,market,selection,model_probability,confidence_bucket,kickoff_at,model_version_id,public_probability,calibration_status,availability,created_at) VALUES(1,'BTTS','YES',0.9,'test','2026-09-13T18:00:00Z',1,0.9,'UNCALIBRATED_V1','AVAILABLE','2026-09-13T11:30:00Z')",[]).unwrap();
        let bad = audit(&c, "2026-09-13", Some("2026-09-13T12:00:00Z")).unwrap();
        assert_eq!(
            bad.matches.iter().find(|r| r["match_id"] == 1).unwrap()["primary_exclusion"],
            "POOR_MODEL_QUALITY"
        );
    }
}
