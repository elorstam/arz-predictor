use crate::{
    database::Database,
    repositories::{coupon_settlement, current_flow, iddaa, incremental_resolution},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub enabled: bool,
    pub interval_seconds: u64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 300,
        }
    }
}
#[derive(Clone, Serialize, Default)]
pub struct Status {
    pub settings: Settings,
    pub state: String,
    pub last_started: Option<i64>,
    pub last_success: Option<i64>,
    pub next_check: Option<i64>,
    pub business_date: String,
    pub cycles: u64,
    pub report: Option<Value>,
    pub error: Option<String>,
}
struct Control {
    status: Status,
    pending: bool,
    settings_path: PathBuf,
}
static CONTROL: OnceLock<Mutex<Control>> = OnceLock::new();
fn due(status: &Status, now: i64, date: &str, pending: bool) -> bool {
    if matches!(status.state.as_str(), "RUNNING" | "PENDING_RERUN") {
        return false;
    }
    pending
        || (status.settings.enabled
            && (status.next_check.is_none_or(|next| now >= next) || status.business_date != date))
}
fn scheduler_due(
    status: &Status,
    now: i64,
    date: &str,
    pending: bool,
    bootstrap_ready: bool,
) -> bool {
    bootstrap_ready && due(status, now, date, pending)
}
pub fn status() -> Result<Status, String> {
    Ok(CONTROL
        .get()
        .ok_or("SCHEDULER_NOT_STARTED")?
        .lock()
        .map_err(|_| "SCHEDULER_LOCK")?
        .status
        .clone())
}
pub fn request(stale_only: bool) -> Result<Status, String> {
    let mut c = CONTROL
        .get()
        .ok_or("SCHEDULER_NOT_STARTED")?
        .lock()
        .map_err(|_| "SCHEDULER_LOCK")?;
    let now = chrono::Utc::now().timestamp();
    if stale_only && matches!(c.status.state.as_str(), "RUNNING" | "PENDING_RERUN") {
        return Ok(c.status.clone());
    }
    if !stale_only
        || (c.status.settings.enabled
            && c.status
                .last_success
                .is_none_or(|last| now - last >= c.status.settings.interval_seconds as i64))
    {
        c.pending = true;
        if c.status.state == "RUNNING" {
            c.status.state = "PENDING_RERUN".into();
        }
    }
    Ok(c.status.clone())
}
pub fn publication_in_flight(status: &Status, date: &str) -> bool {
    status.settings.enabled && (status.business_date != date || status.state != "IDLE")
}
pub fn configure(settings: Settings) -> Result<Status, String> {
    if settings.interval_seconds < 300 {
        return Err("MINIMUM_REFRESH_INTERVAL_300_SECONDS".into());
    }
    let mut c = CONTROL
        .get()
        .ok_or("SCHEDULER_NOT_STARTED")?
        .lock()
        .map_err(|_| "SCHEDULER_LOCK")?;
    std::fs::write(
        &c.settings_path,
        serde_json::to_vec(&settings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    c.status.settings = settings;
    c.status.next_check =
        Some(chrono::Utc::now().timestamp() + c.status.settings.interval_seconds as i64);
    Ok(c.status.clone())
}
fn fingerprint(c: &rusqlite::Connection, date: &str) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(date);
    for sql in [
        "SELECT json_array(m.id,m.competition_id,m.home_team_id,m.away_team_id,m.kickoff_at,m.status) FROM matches m WHERE m.scheduled_local_date>=?1 AND m.status='scheduled' AND EXISTS(SELECT 1 FROM provider_match_mappings p WHERE p.match_id=m.id AND p.provider='iddaa') AND EXISTS(SELECT 1 FROM provider_competition_mappings cp WHERE cp.competition_id=m.competition_id AND cp.provider='football-data.co.uk') ORDER BY m.id",
        "SELECT json_array(MAX(o.id)) FROM odds_snapshots o JOIN matches m ON m.id=o.match_id WHERE m.scheduled_local_date>=?1 AND m.status='scheduled' AND EXISTS(SELECT 1 FROM provider_competition_mappings cp WHERE cp.competition_id=m.competition_id AND cp.provider='football-data.co.uk') AND o.normalized_market_type IN ('MATCH_RESULT','TOTAL_GOALS','BOTH_TEAMS_TO_SCORE','CORNERS_TOTAL')",
        "SELECT json_array(version) FROM production_history_epoch WHERE id=1 AND ?1 IS NOT NULL",
        "SELECT json_array(id,version_identifier,is_active) FROM model_versions WHERE is_active=1 AND ?1 IS NOT NULL ORDER BY id",
        "SELECT json_array(id,calibration_version,is_active) FROM calibration_models WHERE is_active=1 AND ?1 IS NOT NULL ORDER BY id",
        "SELECT json_array(s.id,s.status,s.current_step,s.reset_count,s.current_stake_cents,(SELECT MAX(t.id) FROM phase8_series_steps t WHERE t.series_id=s.id AND t.result<>'UNSETTLED')) FROM phase8_compound_series s WHERE ?1 IS NOT NULL ORDER BY s.id",
    ]{let mut q=c.prepare(sql).map_err(|e|e.to_string())?;let rows=q.query_map([date],|r|r.get::<_,String>(0)).map_err(|e|e.to_string())?;for row in rows{hash.update(row.map_err(|e|e.to_string())?);}}
    Ok(format!("{:x}", hash.finalize()))
}

fn maintenance(db: &Database) -> Result<Value, String> {
    let start = Instant::now();
    let c = db.connection()?;
    let mut missing = Vec::new();
    let mut retry = None;
    let mut current_sources = Vec::new();
    // Match the production readiness denominator: missing catalog history is
    // actionable only when a current/future supported team lacks five samples.
    let mut q=c.prepare("SELECT DISTINCT cp.external_competition_id FROM matches m JOIN provider_competition_mappings cp ON cp.competition_id=m.competition_id AND cp.provider='football-data.co.uk' WHERE m.status='scheduled' AND julianday(m.kickoff_at)>julianday('now') AND EXISTS(SELECT 1 FROM provider_match_mappings pm WHERE pm.match_id=m.id AND pm.provider='iddaa') AND ((SELECT COUNT(*) FROM matches h WHERE h.status='finished' AND h.kickoff_at<m.kickoff_at AND (h.home_team_id=m.home_team_id OR h.away_team_id=m.home_team_id))<5 OR (SELECT COUNT(*) FROM matches h WHERE h.status='finished' AND h.kickoff_at<m.kickoff_at AND (h.home_team_id=m.away_team_id OR h.away_team_id=m.away_team_id))<5)").map_err(|e|e.to_string())?;
    let needed = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(q);
    for dataset in crate::providers::football_data::all_datasets() {
        if dataset.is_current_season {
            let supported:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM provider_competition_mappings WHERE provider='football-data.co.uk' AND external_competition_id=?1)",[dataset.league_code],|r|r.get(0)).map_err(|e|e.to_string())?;
            if supported {
                current_sources.push(dataset);
            }
        }
        if !needed.iter().any(|league| league == dataset.league_code) {
            continue;
        }
        let available:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM data_import_runs WHERE provider='football-data.co.uk' AND dataset_key=?1 AND status='completed' AND rows_seen>0)",[dataset.key()],|r|r.get(0)).map_err(|e|e.to_string())?;
        if !available {
            missing.push(dataset.key());
            let recent:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM data_import_runs WHERE provider='football-data.co.uk' AND dataset_key=?1 AND started_at>strftime('%Y-%m-%dT%H:%M:%fZ','now','-30 minutes'))",[dataset.key()],|r|r.get(0)).map_err(|e|e.to_string())?;
            if !recent && retry.is_none() {
                retry = Some(dataset);
            }
        }
    }
    let mut artifact_paths = Vec::new();
    for sql in [
        "SELECT artifact_path FROM model_versions WHERE is_active=1",
        "SELECT artifact_path FROM calibration_models WHERE is_active=1",
    ] {
        let mut q = c.prepare(sql).map_err(|e| e.to_string())?;
        let paths = q
            .query_map([], |r| r.get::<_, Option<String>>(0))
            .map_err(|e| e.to_string())?;
        for path in paths {
            let path = path.map_err(|e| e.to_string())?;
            artifact_paths.push(json!({"path":path,"exists":path.as_ref().is_some_and(|p|std::path::Path::new(p).is_file())}));
        }
    }
    let check_ms = start.elapsed().as_millis();
    drop(c);
    // Check one current-season source at a time, oldest check first. HEAD never
    // downloads the historical corpus; a changed known ETag schedules one import.
    let metadata_path = db.path().with_file_name("historical-source-checks.json");
    let mut metadata: std::collections::BTreeMap<String, (String, i64)> =
        std::fs::read(&metadata_path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
    current_sources.sort_by_key(|dataset| metadata.get(&dataset.key()).map(|v| v.1).unwrap_or(0));
    let mut source_check = json!({"state":"NO_CURRENT_SOURCE"});
    if let Some(dataset) = current_sources.first() {
        let now = chrono::Utc::now().timestamp();
        if metadata
            .get(&dataset.key())
            .is_none_or(|v| now - v.1 >= 300)
        {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .map_err(|e| e.to_string())?;
            match client
                .head(dataset.mirror_url())
                .send()
                .and_then(|r| r.error_for_status())
            {
                Ok(response) => {
                    let tag = response
                        .headers()
                        .get(reqwest::header::ETAG)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    let changed = metadata
                        .get(&dataset.key())
                        .is_some_and(|old| !old.0.is_empty() && !tag.is_empty() && old.0 != tag);
                    source_check = json!({"dataset":dataset.key(),"state":if changed{"SOURCE_CHANGED"}else{"METADATA_CHECKED"},"etag":tag});
                    if changed && retry.is_none() {
                        retry = Some(*dataset);
                    } else if !changed {
                        metadata.insert(dataset.key(), (tag, now));
                    }
                }
                Err(error) => {
                    source_check = json!({"dataset":dataset.key(),"state":"RETRY_LATER","error":error.to_string()})
                }
            }
            std::fs::write(
                &metadata_path,
                serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        } else {
            source_check = json!({"state":"FRESH_METADATA"});
        }
    }
    let repaired = retry.map(|dataset| {
        match tauri::async_runtime::block_on(crate::providers::football_data::refresh_results(
            db, dataset,
        )) {
            Ok(r) => json!({"dataset":dataset.key(),"state":"IMPORTED","rows":r.rows_seen}),
            Err(e) => json!({"dataset":dataset.key(),"state":"RETRY_LATER","error":e}),
        }
    });
    if source_check["state"] == "SOURCE_CHANGED"
        && repaired.as_ref().is_some_and(|repair| {
            repair["state"] == "IMPORTED" && repair["dataset"] == source_check["dataset"]
        })
    {
        metadata.insert(
            source_check["dataset"].as_str().unwrap().into(),
            (
                source_check["etag"].as_str().unwrap().into(),
                chrono::Utc::now().timestamp(),
            ),
        );
        std::fs::write(
            &metadata_path,
            serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(
        json!({"metadata_check_ms":check_ms,"required_missing":missing,"artifacts":artifact_paths,"repair":repaired,"source_check":source_check,"current_season_updates":"BOUNDED_RESULT_SCHEDULER_AND_ETAG_CHECK","logo_queue":"EXISTING_BOUNDED_WORKER"}),
    )
}
fn cycle(path: PathBuf, app: &tauri::AppHandle) -> Result<Value, String> {
    let started = Instant::now();
    let _lease = crate::daily_pipeline::RefreshLease::acquire()?;
    let db = Database::open(path.clone()).map_err(|e| e.to_string())?;
    let date = crate::business_clock::date();
    let mut errors = Vec::new();
    let mut timings = serde_json::Map::new();
    timings.insert("freshness_ms".into(), json!(started.elapsed().as_millis()));
    let t = Instant::now();
    let bulletin = tauri::async_runtime::block_on(crate::providers::iddaa::refresh_bulletin(&db));
    timings.insert("iddaa_ms".into(), json!(t.elapsed().as_millis()));
    let live_ok = match &bulletin {
        Ok(s) if s.events_seen > 0 && s.failed_events == 0 => true,
        Ok(s) => {
            errors.push(format!(
                "BULLETIN_PARTIAL: {} events; {} failures",
                s.events_seen, s.failed_events
            ));
            false
        }
        Err(e) => {
            errors.push(e.clone());
            false
        }
    };
    let t = Instant::now();
    let popularity =
        tauri::async_runtime::block_on(crate::providers::iddaa::refresh_popularity(&db));
    if let Err(e) = &popularity {
        errors.push(e.clone());
    }
    timings.insert("popularity_ms".into(), json!(t.elapsed().as_millis()));
    // Fetch due final results before deciding whether today's publication changed.
    // The bounded result worker shares its persisted retry schedule with this cycle.
    let t = Instant::now();
    if let Err(error) = crate::settlement_scheduler::refresh_one(&db, app) {
        errors.push(error);
    }
    timings.insert("result_ingestion_ms".into(), json!(t.elapsed().as_millis()));
    let c = db.connection()?;
    let t = Instant::now();
    let settled = coupon_settlement::run(&c)?;
    timings.insert("settlement_ms".into(), json!(t.elapsed().as_millis()));
    let t = Instant::now();
    let resolution = incremental_resolution::process(&c, 128).map_err(|e| e.to_string())?;
    timings.insert("resolution_ms".into(), json!(t.elapsed().as_millis()));
    let fingerprint_path = path.with_file_name("automatic-publication-inputs.json");
    let input = fingerprint(&c, &date)?;
    let prior: Value = std::fs::read(&fingerprint_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null);
    let history_epoch: i64 = c
        .query_row(
            "SELECT version FROM production_history_epoch WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let history_changed = prior["history_epoch"]
        .as_i64()
        .is_some_and(|epoch| epoch != history_epoch);
    let t = Instant::now();
    let production = if live_ok
        && (prior["input"].as_str() != Some(input.as_str()) || resolution.resolved > 0)
    {
        match current_flow::run_with_feature_refresh(&c, history_changed) {
            Ok(report) => {
                std::fs::write(
                    &fingerprint_path,
                    json!({"input":fingerprint(&c,&date)?,"history_epoch":history_epoch})
                        .to_string(),
                )
                .map_err(|e| e.to_string())?;
                json!({"state":"UPDATED","report":report})
            }
            Err(e) => {
                errors.push(e.clone());
                json!({"state":"FAILED","error":e})
            }
        }
    } else {
        json!({"state":"SKIPPED","reason":if live_ok{"UNCHANGED_INPUTS"}else{"UPSTREAM_FAILED"}})
    };
    timings.insert(
        "prediction_candidate_coupon_ms".into(),
        json!(t.elapsed().as_millis()),
    );
    let t = Instant::now();
    let readiness = iddaa::bulletin_status(&c).map_err(|e| e.to_string())?;
    timings.insert("readiness_ms".into(), json!(t.elapsed().as_millis()));
    if let Ok(summary) = &bulletin {
        c.execute("INSERT INTO daily_refresh_audit(import_run_id,business_date,started_at,finished_at,status,ingestion_ms,resolution_ms,production_ms,readiness_ms,error,report_json) VALUES(?1,?2,strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),?3,?4,?5,?6,?7,?8,?9)",rusqlite::params![summary.import_run_id,date,if errors.is_empty(){"COMPLETED"}else{"FAILED"},summary.elapsed_ms as i64,resolution.elapsed_ms as i64,timings["prediction_candidate_coupon_ms"].as_i64().unwrap_or(0),timings["readiness_ms"].as_i64().unwrap_or(0),(!errors.is_empty()).then(||errors.join("; ")),json!({"timings":timings,"production":production}).to_string()]).map_err(|e|e.to_string())?;
    }
    drop(c);
    let maintenance = maintenance(&db)?;
    // Existing bounded asset scheduler owns discovery; this only wakes its queue.
    app.state::<crate::assets::AssetSyncManager>().start();
    let _ = app.emit("daily-publication-updated", ());
    if settled > 0 {
        let _ = app.emit("coupon-settlement-updated", settled);
    }
    Ok(
        json!({"date":date,"duration_ms":started.elapsed().as_millis(),"timings":timings,"bulletin":bulletin.ok(),"popularity":popularity.ok(),"resolution":resolution,"production":production,"settled":settled,"readiness":readiness,"maintenance":maintenance,"errors":errors,"global_resolution":false,"historical_bulk_import":false}),
    )
}
pub fn start(path: PathBuf, app: tauri::AppHandle) {
    let settings_path = path.with_file_name("automatic-refresh-settings.json");
    let settings: Settings = std::fs::read(&settings_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let settings = Settings {
        interval_seconds: settings.interval_seconds.max(300),
        ..settings
    };
    if CONTROL
        .set(Mutex::new(Control {
            pending: settings.enabled,
            settings_path,
            status: Status {
                settings,
                state: "IDLE".into(),
                ..Status::default()
            },
        }))
        .is_err()
    {
        return;
    }
    std::thread::spawn(move || loop {
        let now = chrono::Utc::now().timestamp();
        let date = crate::business_clock::date();
        let run = {
            let mut c = CONTROL.get().unwrap().lock().unwrap();
            if scheduler_due(
                &c.status,
                now,
                &date,
                c.pending,
                crate::first_run::daily_refresh_allowed(),
            ) {
                c.pending = false;
                c.status.state = "RUNNING".into();
                c.status.last_started = Some(now);
                c.status.business_date = date;
                c.status.next_check = Some(now + c.status.settings.interval_seconds as i64);
                true
            } else {
                false
            }
        };
        if run {
            let result = cycle(path.clone(), &app);
            let mut c = CONTROL.get().unwrap().lock().unwrap();
            c.status.cycles += 1;
            match result {
                Ok(report) => {
                    let errors = report["errors"].as_array().unwrap();
                    c.status.error = (!errors.is_empty()).then(|| report["errors"].to_string());
                    if errors.is_empty() {
                        c.status.last_success = Some(chrono::Utc::now().timestamp());
                    }
                    c.status.report = Some(report);
                }
                Err(e) => c.status.error = Some(e),
            }
            // Pending is consumed once by the next iteration after this run exits.
            c.status.state = "IDLE".into();
            if let Ok(line) = serde_json::to_string(&c.status) {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(path.with_file_name("automatic-refresh.jsonl"))
                {
                    let _ = writeln!(f, "{line}");
                }
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn katlama_progression_invalidates_unchanged_live_inputs_once() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        c.execute("INSERT INTO phase8_compound_series(business_date,status,current_step,starting_stake_cents,current_stake_cents,metadata_json) VALUES('2026-09-14','ACTIVE',1,10000,10000,'{}')", []).unwrap();
        let before = fingerprint(&c, "2026-09-14").unwrap();
        c.execute(
            "UPDATE phase8_compound_series SET current_step=2,current_stake_cents=16900",
            [],
        )
        .unwrap();
        let won = fingerprint(&c, "2026-09-14").unwrap();
        assert_ne!(before, won);
        assert_eq!(won, fingerprint(&c, "2026-09-14").unwrap());
        c.execute("UPDATE phase8_compound_series SET current_step=1,current_stake_cents=10000,reset_count=1", []).unwrap();
        let lost = fingerprint(&c, "2026-09-14").unwrap();
        assert_ne!(
            before, lost,
            "a step-1 loss must still invalidate publication"
        );
        assert_eq!(lost, fingerprint(&c, "2026-09-14").unwrap());
    }
    #[test]
    fn frontend_ensure_waits_for_midnight_owner_without_requesting_another_run() {
        let mut s = Status {
            settings: Settings::default(),
            state: "IDLE".into(),
            business_date: "2026-09-14".into(),
            ..Status::default()
        };
        assert!(publication_in_flight(&s, "2026-09-15"));
        s.business_date = "2026-09-15".into();
        s.state = "RUNNING".into();
        assert!(publication_in_flight(&s, "2026-09-15"));
        s.state = "IDLE".into();
        assert!(!publication_in_flight(&s, "2026-09-15"));
        s.settings.enabled = false;
        assert!(!publication_in_flight(&s, "2026-09-16"));
    }
    #[test]
    fn startup_interval_resume_midnight_and_disabled_scheduler() {
        let mut status = Status {
            settings: Settings::default(),
            state: "IDLE".into(),
            ..Status::default()
        };
        assert_eq!(status.settings.interval_seconds, 300);
        assert!(due(&status, 1000, "2026-09-14", false));
        status.business_date = "2026-09-14".into();
        status.next_check = Some(1300);
        assert!(!due(&status, 1299, "2026-09-14", false));
        assert!(due(&status, 1300, "2026-09-14", false));
        assert!(due(&status, 10000, "2026-09-14", false));
        assert!(due(&status, 1200, "2026-09-15", false));
        status.settings.enabled = false;
        assert!(!due(&status, 10000, "2026-09-15", false));
        assert!(due(&status, 10000, "2026-09-15", true));
    }
    #[test]
    fn pending_requests_never_overlap_and_coalesce_to_one_run() {
        let mut status = Status {
            state: "RUNNING".into(),
            business_date: "2026-09-14".into(),
            next_check: Some(1300),
            ..Status::default()
        };
        let mut pending = false;
        for _ in 0..100 {
            pending = true;
            assert!(!due(&status, 1000, "2026-09-14", pending));
        }
        status.state = "IDLE".into();
        assert!(due(&status, 1000, "2026-09-14", pending));
        pending = false;
        status.state = "RUNNING".into();
        assert!(!due(&status, 1000, "2026-09-14", pending));
        status.state = "IDLE".into();
        assert!(!due(&status, 1000, "2026-09-14", pending));
    }

    #[test]
    fn bootstrap_gate_precedes_daily_scheduler() {
        assert!(!scheduler_due(
            &Status::default(),
            1000,
            "2026-09-14",
            true,
            false
        ));
        assert!(scheduler_due(
            &Status::default(),
            1000,
            "2026-09-14",
            true,
            true
        ));
    }
    #[test]
    fn unchanged_input_does_not_require_a_new_publication() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        let before = fingerprint(&c, "2026-09-14").unwrap();
        assert_eq!(before, fingerprint(&c, "2026-09-14").unwrap());
        assert_ne!(before, fingerprint(&c, "2026-09-15").unwrap());
        c.execute("INSERT INTO model_versions(version_identifier,model_name,is_active) VALUES('new','model',1)",[]).unwrap();
        assert_ne!(before, fingerprint(&c, "2026-09-14").unwrap());
    }

    #[test]
    fn repeated_bulletin_and_invalid_or_empty_input_preserve_last_good_state() {
        let db = Database::open_in_memory().unwrap();
        db.connection().unwrap().execute_batch("INSERT INTO competitions(id,name,country,current_season) VALUES(1,'Premier League','England','2026/27'); INSERT INTO provider_competition_mappings(provider,external_competition_id,competition_id) VALUES('football-data.co.uk','E0',1);").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("bulletin.json");
        std::fs::write(
            &file,
            include_bytes!("providers/iddaa/fixtures/bulletin.json"),
        )
        .unwrap();
        let first = crate::providers::iddaa::import_local_bulletin(&db, &file).unwrap();
        let before = {
            let c = db.connection().unwrap();
            fingerprint(&c, "2026-01-01").unwrap()
        };
        let second = crate::providers::iddaa::import_local_bulletin(&db, &file).unwrap();
        assert_eq!(first.failed_events, second.failed_events);
        assert_eq!(second.odds_snapshots_inserted, 0);
        {
            let c = db.connection().unwrap();
            assert_eq!(before, fingerprint(&c, "2026-01-01").unwrap());
        }
        std::fs::write(&file, b"invalid network payload").unwrap();
        assert!(crate::providers::iddaa::import_local_bulletin(&db, &file).is_err());
        {
            let c = db.connection().unwrap();
            assert_eq!(before, fingerprint(&c, "2026-01-01").unwrap());
        }
        std::fs::write(&file, b"{\"events\":[],\"competitions\":[]}").unwrap();
        let _ = crate::providers::iddaa::import_local_bulletin(&db, &file);
        {
            let c = db.connection().unwrap();
            assert_eq!(before, fingerprint(&c, "2026-01-01").unwrap());
        }
    }
}
