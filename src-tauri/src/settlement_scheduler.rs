use std::path::PathBuf;
use tauri::Emitter;

pub fn start(path: PathBuf, app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        let result = (|| -> Result<usize, String> {
            let db = crate::database::Database::open(path.clone()).map_err(|e| e.to_string())?;
            let c = db.connection()?;
            let changed = crate::repositories::coupon_settlement::run(&c)?;
            drop(c);
            refresh_one(&db, &app)?;
            Ok(changed)
        })();
        match result {
            Ok(n) if n > 0 => {
                let _ = app.emit("coupon-settlement-updated", n);
            }
            Err(e) => eprintln!("Automatic coupon settlement: {e}"),
            _ => {}
        }
        std::thread::sleep(std::time::Duration::from_secs(60));
    });
}

// One due league per minute; each league is retried at most once per 15 minutes.
// Fetch only seasons containing past, unsettled production selections.
pub(crate) fn refresh_one(
    db: &crate::database::Database,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    use rusqlite::{params, OptionalExtension};
    static RESULT_REFRESH: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = RESULT_REFRESH.lock().map_err(|_| "RESULT_REFRESH_LOCK")?;
    let job = {
        let c = db.connection()?;
        let job:Option<(String,String)>=c.query_row("WITH needed AS (SELECT DISTINCT cp.external_competition_id league,printf('%02d%02d',(CAST(strftime('%Y',m.kickoff_at) AS INTEGER)-CASE WHEN CAST(strftime('%m',m.kickoff_at) AS INTEGER)<7 THEN 1 ELSE 0 END)%100,(CAST(strftime('%Y',m.kickoff_at) AS INTEGER)+CASE WHEN CAST(strftime('%m',m.kickoff_at) AS INTEGER)>=7 THEN 1 ELSE 0 END)%100) season FROM phase8_coupon_selections s JOIN phase8_coupons p ON p.id=s.coupon_id JOIN matches m ON m.id=s.match_id JOIN provider_competition_mappings cp ON cp.competition_id=m.competition_id AND cp.provider='football-data.co.uk' WHERE s.status='PENDING' AND p.status<>'CANCELLED' AND NOT EXISTS(SELECT 1 FROM candidate_run_execution e WHERE e.run_id=p.source_candidate_run_id AND e.purpose='REPLAY') AND (p.status IN ('FINALIZED','SETTLED') OR json_extract(p.metadata_json,'$.published')=1 OR p.series_id IS NOT NULL) AND m.kickoff_at<strftime('%Y-%m-%dT%H:%M:%SZ','now','-3 hours')) SELECT n.league,n.season FROM needed n LEFT JOIN coupon_result_refresh r ON r.league_code=n.league AND r.season_code=n.season WHERE r.next_retry_at IS NULL OR r.next_retry_at<=strftime('%Y-%m-%dT%H:%M:%fZ','now') ORDER BY COALESCE(r.last_attempt_at,''),n.league LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
        if let Some((league, season)) = &job {
            c.execute("INSERT INTO coupon_result_refresh(league_code,season_code,last_attempt_at,next_retry_at,state) VALUES(?1,?2,strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now','+15 minutes'),'REFRESHING') ON CONFLICT(league_code,season_code) DO UPDATE SET last_attempt_at=excluded.last_attempt_at,next_retry_at=excluded.next_retry_at,state=excluded.state",params![league,season]).map_err(|e|e.to_string())?;
        }
        job
    };
    if let Some((league, season)) = job {
        let result = match crate::providers::football_data::find_dataset(&league, &season) {
            Some(dataset) => tauri::async_runtime::block_on(
                crate::providers::football_data::refresh_results(db, dataset),
            )
            .map(|s| {
                format!(
                    "{} rows; {} updated; {} failed",
                    s.rows_seen, s.matches_updated, s.rows_failed
                )
            }),
            None => Err("RESULT_DATASET_NOT_PUBLISHED".into()),
        };
        let (state, detail) = match result {
            Ok(detail) => ("REFRESHED", detail),
            Err(detail) => ("PENDING_DATA", detail),
        };
        db.connection()?.execute("UPDATE coupon_result_refresh SET state=?3,detail=?4 WHERE league_code=?1 AND season_code=?2",params![league,season,state,detail]).map_err(|e|e.to_string())?;
        if state == "REFRESHED" {
            // Result import can settle coupons inside its post-commit hook.
            let _ = app.emit("coupon-settlement-updated", ());
        }
    }
    Ok(())
}
