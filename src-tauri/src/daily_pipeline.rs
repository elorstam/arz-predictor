use crate::{
    database::Database,
    repositories::{current_flow, iddaa, incremental_resolution},
};
use rusqlite::OptionalExtension;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::Emitter;
pub static PRODUCTION_LOCK: Mutex<()> = Mutex::new(());
static REFRESHING: AtomicBool = AtomicBool::new(false);
pub struct RefreshLease;
impl RefreshLease {
    pub fn acquire() -> Result<Self, String> {
        REFRESHING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| "DAILY_REFRESH_ALREADY_RUNNING".into())
    }
}
impl Drop for RefreshLease {
    fn drop(&mut self) {
        REFRESHING.store(false, Ordering::Release)
    }
}

pub fn after_refresh(
    path: PathBuf,
    import: i64,
    ingestion: u128,
) -> Result<serde_json::Value, String> {
    let db = Database::open(path).map_err(|e| e.to_string())?;
    let c = db.connection()?;
    let day = chrono::Utc::now()
        .with_timezone(&chrono_tz::Europe::Istanbul)
        .date_naive()
        .to_string();
    c.execute("INSERT INTO daily_refresh_audit(import_run_id,business_date,started_at,status,ingestion_ms) VALUES(?1,?2,strftime('%Y-%m-%dT%H:%M:%fZ','now'),'RUNNING',?3)",rusqlite::params![import,day,ingestion as i64]).map_err(|e|e.to_string())?;
    let audit = c.last_insert_rowid();
    let result = (|| -> Result<serde_json::Value, String> {
        let resolution = {
            let _guard = PRODUCTION_LOCK
                .lock()
                .map_err(|_| "PIPELINE_LOCK_POISONED")?;
            incremental_resolution::process(&c, 128).map_err(|e| e.to_string())?
        };
        let production_start = std::time::Instant::now();
        let production = current_flow::run(&c)?;
        let production_ms = production_start.elapsed().as_millis();
        let readiness_start = std::time::Instant::now();
        let counts = iddaa::bulletin_status(&c).map_err(|e| e.to_string())?;
        let readiness_ms = readiness_start.elapsed().as_millis();
        let report = serde_json::json!({"business_date":day,"resolution":resolution,"production":production,"readiness_counts":counts,"ingestion_ms":ingestion,"resolution_ms":resolution.elapsed_ms,"production_ms":production_ms,"readiness_ms":readiness_ms,"global_resolver_rebuild":false});
        c.execute("UPDATE daily_refresh_audit SET finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),status='COMPLETED',resolution_ms=?2,production_ms=?3,readiness_ms=?4,report_json=?5 WHERE id=?1",rusqlite::params![audit,resolution.elapsed_ms as i64,production_ms as i64,readiness_ms as i64,report.to_string()]).map_err(|e|e.to_string())?;
        Ok(report)
    })();
    if let Err(ref error) = result {
        let _=c.execute("UPDATE daily_refresh_audit SET status='FAILED',error=?2,finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",rusqlite::params![audit,error]);
    }
    result
}
pub fn start_scheduler(path: PathBuf, app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        if !REFRESHING.load(Ordering::Acquire) {
            let attempt = (|| -> Result<(), String> {
                let db = Database::open(path.clone()).map_err(|e| e.to_string())?;
                let c = db.connection()?;
                let batch = {
                    let Ok(_guard) = PRODUCTION_LOCK.try_lock() else {
                        return Ok(());
                    };
                    incremental_resolution::process(&c, 64).map_err(|e| e.to_string())?
                };
                let failed:Option<(i64,i64)>=c.query_row("SELECT import_run_id,ingestion_ms FROM daily_refresh_audit WHERE id=(SELECT max(id) FROM daily_refresh_audit) AND status IN ('FAILED','RUNNING') AND COALESCE(finished_at,started_at)<strftime('%Y-%m-%dT%H:%M:%fZ','now','-5 minutes')",[],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
                if let Some((import, ingestion)) = failed {
                    drop(c);
                    after_refresh(path.clone(), import, ingestion as u128)?;
                    let _ = app.emit("daily-publication-updated", ());
                } else if batch.resolved > 0 {
                    current_flow::run(&c)?;
                    let _ = app.emit("daily-publication-updated", ());
                }
                Ok(())
            })();
            if let Err(e) = attempt {
                eprintln!("Incremental daily queue: {e}");
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(60));
    });
}
