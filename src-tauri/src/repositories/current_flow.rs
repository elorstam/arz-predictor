use super::{
    calibration, candidate_engine, coupon_engine, features, model_coverage, prediction_engine,
};
use rusqlite::Connection;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct MatchOutcome {
    pub match_id: i64,
    pub state: String,
    pub reason: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct FlowReport {
    pub upcoming_days: Vec<serde_json::Value>,
    pub matches: Vec<MatchOutcome>,
    pub candidate_run_id: i64,
    pub candidates: usize,
    pub usable_coupons: usize,
}

/// Same production universe as inference; unsupported/insufficient-history
/// fixtures remain exclusions rather than blocking the entire installation.
pub fn production_matches(c: &Connection, now: &str) -> Result<Vec<i64>, String> {
    let mut q = c.prepare("SELECT m.id FROM matches m WHERE m.status='scheduled' AND julianday(m.kickoff_at)>julianday(?1) AND EXISTS(SELECT 1 FROM provider_match_mappings p WHERE p.match_id=m.id AND p.provider='iddaa') ORDER BY m.kickoff_at,m.id").map_err(|e|e.to_string())?;
    let ids = q
        .query_map([now], |r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    ids.into_iter()
        .filter_map(|id| match model_coverage::classify(c, id) {
            Ok(
                model_coverage::CoverageState::ModelSupported
                | model_coverage::CoverageState::ModelReady,
            ) => Some(Ok(id)),
            Ok(_) => None,
            Err(e) => Some(Err(e.to_string())),
        })
        .collect()
}

/// Resume a recovered bootstrap from live dependencies, not completed-stage flags.
pub fn repair_readiness_dependencies(c: &Connection) -> Result<(), String> {
    let ids = production_matches(c, &crate::business_clock::now().to_rfc3339())?;
    let model: i64 = c
        .query_row("SELECT id FROM model_versions WHERE is_active=1", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    let mut missing_predictions = false;
    for id in ids {
        features::ensure_current_snapshot(c, id)?;
        missing_predictions |= !base_prediction_exists(c, id, model).map_err(|e| e.to_string())?;
    }
    if missing_predictions
        || super::daily_selections::get(c, &crate::business_clock::date()).is_err()
    {
        run(c)?;
    }
    Ok(())
}

/// Iddaa owns live fixtures; history comes from safely resolved canonical teams.
pub fn run(c: &Connection) -> Result<FlowReport, String> {
    run_with_feature_refresh(c, false)
}
pub fn run_with_feature_refresh(
    c: &Connection,
    history_changed: bool,
) -> Result<FlowReport, String> {
    let _guard = crate::daily_pipeline::PRODUCTION_LOCK
        .lock()
        .map_err(|_| "PIPELINE_LOCK_POISONED")?;
    super::incremental_resolution::process(c, 128).map_err(|e| e.to_string())?;
    let now = crate::business_clock::now();
    let date = crate::business_clock::date();
    let mut q=c.prepare("SELECT m.id FROM matches m WHERE m.status='scheduled' AND m.kickoff_at>?1 AND EXISTS(SELECT 1 FROM provider_competition_mappings cp WHERE cp.competition_id=m.competition_id AND cp.provider='football-data.co.uk') AND EXISTS(SELECT 1 FROM provider_match_mappings p WHERE p.match_id=m.id AND p.provider='iddaa') ORDER BY m.kickoff_at,m.id").map_err(|e|e.to_string())?;
    let ids = q
        .query_map([now.to_rfc3339()], |r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(q);
    let mut matches = Vec::new();
    let (model_id, artifact) = prediction_engine::active(c)?;
    let cal = calibration::active_calibration(c)?;
    for id in ids {
        let coverage = model_coverage::classify(c, id).map_err(|e| e.to_string())?;
        if !matches!(
            coverage,
            model_coverage::CoverageState::ModelSupported
                | model_coverage::CoverageState::ModelReady
        ) {
            matches.push(MatchOutcome {
                match_id: id,
                state: serde_json::to_value(coverage)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .into(),
                reason: None,
            });
            continue;
        }
        features::ensure_current_snapshot(c, id)?;
        let already = base_prediction_exists(c, id, model_id).map_err(|e| e.to_string())?;
        if already
            && !history_changed
            && matches!(coverage, model_coverage::CoverageState::ModelReady)
        {
            matches.push(MatchOutcome {
                match_id: id,
                state: "MODEL_READY_REUSED".into(),
                reason: None,
            });
            continue;
        }
        let result = (|| -> Result<(), String> {
            let snapshot = features::generate(c, id)?;
            features::persist(c, &snapshot)?;
            let mut result = prediction_engine::infer(&artifact, &snapshot)?;
            if let Some(ref cal) = cal {
                calibration::apply(&mut result, cal);
            }
            let kickoff: String = c
                .query_row("SELECT kickoff_at FROM matches WHERE id=?1", [id], |r| {
                    r.get(0)
                })
                .map_err(|e| e.to_string())?;
            prediction_engine::persist_predictions(c, &result, model_id, &kickoff)?;
            Ok(())
        })();
        matches.push(MatchOutcome {
            match_id: id,
            state: if result.is_ok() {
                "MODEL_READY"
            } else {
                "MODEL_INPUT_MISSING"
            }
            .into(),
            reason: result.err(),
        });
    }
    let run = candidate_engine::generate_daily_output(
        c,
        &candidate_engine::GenerateRequest {
            business_date: Some(date.clone()),
            generation_time: None,
            category: None,
            dry_run: Some(false),
        },
    )?;
    let coupons = coupon_engine::get_daily(c, &date, None)?;
    let mut upcoming_days = Vec::new();
    let mut q=c.prepare("SELECT DISTINCT scheduled_local_date FROM matches m WHERE status='scheduled' AND scheduled_local_date>?1 AND EXISTS(SELECT 1 FROM predictions p WHERE p.match_id=m.id) ORDER BY scheduled_local_date LIMIT 7").map_err(|e|e.to_string())?;
    let dates = q
        .query_map([&date], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for day in dates {
        let next = candidate_engine::generate_daily_output(
            c,
            &candidate_engine::GenerateRequest {
                business_date: Some(day.clone()),
                generation_time: None,
                category: None,
                dry_run: Some(false),
            },
        )?;
        let coupons = coupon_engine::get_daily(c, &day, None)?;
        upcoming_days.push(serde_json::json!({"business_date":day,"run_id":next.run_id,"considered":next.match_count_considered,"candidates":next.candidates.len(),"usable_coupons":coupons.len()}));
    }
    Ok(FlowReport {
        upcoming_days,
        matches,
        candidate_run_id: run.run_id,
        candidates: run.candidates.len(),
        usable_coupons: coupons.iter().filter(|x| !x.selections.is_empty()).count(),
    })
}

fn base_prediction_exists(c: &Connection, id: i64, model: i64) -> rusqlite::Result<bool> {
    // BASE is stored in predictions; lineup revisions have their own table.
    c.query_row("SELECT EXISTS(SELECT 1 FROM predictions WHERE match_id=?1 AND model_version_id=?2 AND public_probability IS NOT NULL)",rusqlite::params![id,model],|r|r.get(0))
}
#[cfg(test)]
mod tests {
    #[test]
    fn prediction_reuse_query_uses_real_base_schema() {
        let db = crate::database::Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        assert!(!super::base_prediction_exists(&c, 1, 1).unwrap());
    }
}
