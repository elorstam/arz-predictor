use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use image::{GenericImageView, ImageFormat};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::database::Database;

pub const MAX_DOWNLOAD_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_IMAGE_DIMENSION: u32 = 4096;
pub const REQUEST_TIMEOUT_SECONDS: u64 = 12;
pub const MAX_FAILURES: i64 = 3;

#[derive(Debug, Clone)]
pub struct EntityIdentity {
    pub entity_type: String,
    pub entity_id: i64,
    pub name: String,
    pub country: Option<String>,
}

pub trait EntityMetadataProvider: Send + Sync {
    fn provider_name(&self) -> &'static str;
    fn discovery_url(&self, entity: &EntityIdentity) -> String;
    fn parse_logo_url(&self, entity: &EntityIdentity, bytes: &[u8]) -> Option<String>;
}

/// Conservative exact-title Wikipedia provider. Search/fuzzy results are never
/// accepted: the returned page title must normalize exactly and its Wikidata
/// description must identify football subject matter.
pub struct WikimediaProvider;

impl EntityMetadataProvider for WikimediaProvider {
    fn provider_name(&self) -> &'static str {
        "wikimedia-pageimages-v2"
    }

    fn discovery_url(&self, entity: &EntityIdentity) -> String {
        let mut url =
            reqwest::Url::parse("https://en.wikipedia.org/api/rest_v1/page/summary/").unwrap();
        url.path_segments_mut()
            .unwrap()
            .pop_if_empty()
            .push(&entity.name);
        url.to_string()
    }

    fn parse_logo_url(&self, entity: &EntityIdentity, bytes: &[u8]) -> Option<String> {
        let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let title = value["title"].as_str()?;
        if crate::repositories::resolution::normalize_team_name(
            &title.replace("F.C.", "FC").replace("A.F.C.", "AFC"),
        ) != crate::repositories::resolution::normalize_team_name(
            &entity.name.replace("F.C.", "FC").replace("A.F.C.", "AFC"),
        ) {
            return None;
        }
        let description = value["description"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        if !["football", "soccer", "league", "competition"]
            .iter()
            .any(|v| description.contains(v))
        {
            return None;
        }
        if let Some(country) = entity.country.as_deref() {
            let country = country.to_lowercase();
            let demonym = match country.as_str() {
                "england" => "english",
                "turkey" | "türkiye" => "turkish",
                "france" => "french",
                "germany" => "german",
                "spain" => "spanish",
                "italy" => "italian",
                "netherlands" => "dutch",
                "belgium" => "belgian",
                "portugal" => "portuguese",
                "scotland" => "scottish",
                "greece" => "greek",
                _ => country.as_str(),
            };
            if !description.contains(&country) && !description.contains(demonym) {
                return None;
            }
        }
        let url = value["thumbnail"]["source"]
            .as_str()
            .or_else(|| value["thumbnail"]["url"].as_str())?;
        if !url.starts_with("https://upload.wikimedia.org/")
            && !url.starts_with("https://thumb.wikimedia.org/")
        {
            return None;
        }
        Some(url.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AssetScanSummary {
    pub teams_total: i64,
    pub competitions_total: i64,
    pub team_logos_ready: i64,
    pub competition_logos_ready: i64,
    pub team_logos_missing: i64,
    pub competition_logos_missing: i64,
    pub queued: i64,
    pub failed: i64,
    pub unavailable: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetSyncStatus {
    pub state: String,
    pub queued: i64,
    pub active: i64,
    pub completed_this_run: i64,
    pub failed_this_run: i64,
    pub bytes_downloaded_this_run: i64,
}

impl Default for AssetSyncStatus {
    fn default() -> Self {
        Self {
            state: "IDLE".into(),
            queued: 0,
            active: 0,
            completed_this_run: 0,
            failed_this_run: 0,
            bytes_downloaded_this_run: 0,
        }
    }
}

#[derive(Clone)]
pub struct AssetSyncManager {
    database_path: PathBuf,
    asset_root: PathBuf,
    status: Arc<Mutex<AssetSyncStatus>>,
    scheduler_started: Arc<AtomicBool>,
}

impl AssetSyncManager {
    pub fn new(database_path: PathBuf, app_data_root: PathBuf) -> Self {
        Self {
            database_path,
            asset_root: asset_root(&app_data_root),
            status: Arc::new(Mutex::new(AssetSyncStatus::default())),
            scheduler_started: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start_scheduler(&self) {
        if self.scheduler_started.swap(true, Ordering::SeqCst) {
            return;
        }
        // A single timer survives page navigation; persisted jobs survive the process.
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Ok(db) = Database::open(manager.database_path.clone()) {
                if let Ok(c) = db.connection() {
                    let _ = c.execute("UPDATE logo_sources SET state='RETRY',next_retry_at=NULL WHERE state='IN_PROGRESS'",[]);
                    let _ = c.execute(
                        "UPDATE entity_assets SET status='QUEUED' WHERE status='DOWNLOADING'",
                        [],
                    );
                }
            }
            loop {
                manager.start();
                tauri::async_runtime::spawn_blocking(|| {
                    std::thread::sleep(Duration::from_secs(30))
                })
                .await
                .ok();
            }
        });
    }

    pub fn scan(&self) -> Result<AssetScanSummary, String> {
        let db = Database::open(self.database_path.clone()).map_err(|e| e.to_string())?;
        let c = db.connection()?;
        scan(&c, &self.asset_root)
    }

    pub fn status(&self) -> AssetSyncStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn retry_failed(&self) -> Result<AssetScanSummary, String> {
        let db = Database::open(self.database_path.clone()).map_err(|e| e.to_string())?;
        {
            let c = db.connection()?;
            queue_background(&c)?;
        }
        let summary = self.scan()?;
        self.start();
        Ok(summary)
    }

    pub fn start(&self) -> bool {
        {
            let mut status = self.status.lock().unwrap();
            if status.state != "IDLE" {
                return false;
            }
            *status = AssetSyncStatus {
                state: "SCANNING".into(),
                ..Default::default()
            };
        }
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            manager.run_background().await;
        });
        true
    }

    async fn run_background(&self) {
        let result = self.process_missing().await;
        let mut status = self.status.lock().unwrap();
        if result.is_err() {
            status.failed_this_run += 1;
        }
        status.state = "IDLE".into();
        status.active = 0;
        status.queued = 0;
    }

    async fn process_missing(&self) -> Result<(), String> {
        let db = Database::open(self.database_path.clone()).map_err(|e| e.to_string())?;
        let items = {
            let c = db.connection()?;
            queue_background(&c)?;
            let rows = queued_items(&c)?;
            // A bounded local discovery batch runs off the UI connection. Network work
            // remains one request at a time; catalog misses are recorded separately.
            for item in &rows {
                crate::logo_discovery::seed(&c, item)?;
                crate::logo_discovery::settle(&c, item)?;
            }
            let mut s = self.status.lock().unwrap();
            s.state = "DOWNLOADING".into();
            s.queued = rows.len() as i64;
            rows
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
            .user_agent("ARZ-Predictor/0.1")
            .build()
            .map_err(|e| e.to_string())?;
        let provider = WikimediaProvider;
        for _ in 0..items.len().max(1) {
            let item = {
                let c = db.connection()?;
                let Some(item) = queued_items(&c)?.into_iter().next() else {
                    break;
                };
                item
            };
            {
                let mut s = self.status.lock().unwrap();
                s.active = 1;
                s.queued = s.queued.saturating_sub(1);
            }
            let outcome = process_one(&db, &self.asset_root, &client, &provider, &item).await;
            {
                let mut s = self.status.lock().unwrap();
                match outcome {
                    Ok(bytes) => {
                        s.completed_this_run += 1;
                        s.bytes_downloaded_this_run += bytes as i64
                    }
                    Err(error) => {
                        eprintln!("Logo {} {}: {}", item.entity_id, item.name, error);
                        s.failed_this_run += 1;
                        if error.starts_with("HTTP_429:") || error.starts_with("HTTP_503:") {
                            // Provider backoff is persisted by process_one and checked on
                            // every future batch. Do not rewrite unrelated provider jobs.
                            break;
                        }
                    }
                }
                drop(s);
            }
            tauri::async_runtime::spawn_blocking(|| {
                std::thread::sleep(Duration::from_millis(2100))
            })
            .await
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

pub fn asset_root(app_data_root: &Path) -> PathBuf {
    app_data_root.join("assets")
}
pub fn canonical_relative_path(entity_type: &str, entity_id: i64) -> Result<PathBuf, String> {
    if entity_id <= 0 {
        return Err("entity id must be positive".into());
    }
    match entity_type {
        "TEAM" => Ok(PathBuf::from("teams").join(format!("{entity_id}.png"))),
        "COMPETITION" => Ok(PathBuf::from("competitions").join(format!("{entity_id}.png"))),
        _ => Err("unsupported entity type".into()),
    }
}
pub fn safe_asset_path(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.is_absolute()
        || relative.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err("asset path escapes root".into());
    }
    Ok(root.join(relative))
}

pub struct NormalizedImage {
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
}
pub fn normalize_image(bytes: &[u8]) -> Result<NormalizedImage, String> {
    if bytes.is_empty() {
        return Err("empty image".into());
    }
    if bytes.len() > MAX_DOWNLOAD_BYTES {
        return Err("image payload too large".into());
    }
    let image =
        image::load_from_memory(bytes).map_err(|_| "unsupported or corrupt image".to_string())?;
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 || width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return Err("invalid image dimensions".into());
    }
    let mut cursor = Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let normalized = cursor.into_inner();
    let sha256 = format!("{:x}", Sha256::digest(&normalized));
    Ok(NormalizedImage {
        bytes: normalized,
        sha256,
        width,
        height,
    })
}

pub fn is_cached_asset_valid(root: &Path, relative: &str, expected_sha: Option<&str>) -> bool {
    let Ok(path) = safe_asset_path(root, Path::new(relative)) else {
        return false;
    };
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let Ok(image) = normalize_image(&bytes) else {
        return false;
    };
    expected_sha.map(|v| v == image.sha256).unwrap_or(true)
}

pub fn scan(c: &Connection, root: &Path) -> Result<AssetScanSummary, String> {
    fs::create_dir_all(root.join("teams")).map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join("competitions")).map_err(|e| e.to_string())?;
    queue_background(c)?;
    totals(c)
}

fn queue_background(c: &Connection) -> Result<(), String> {
    c.execute("UPDATE entity_assets SET priority=1 WHERE priority>20", [])
        .map_err(|e| e.to_string())?;
    c.execute("UPDATE entity_assets SET status='QUEUED',next_retry_at=NULL WHERE discovery_version<3 AND status='UNAVAILABLE'",[]).map_err(|e|e.to_string())?;
    c.execute("UPDATE entity_assets SET status='QUEUED',discovery_state='SEARCHED' WHERE status='DOWNLOADING' AND julianday(last_attempt_at)<julianday('now','-2 minutes')",[]).map_err(|e|e.to_string())?;
    c.execute("UPDATE logo_sources SET state='RETRY',next_retry_at=NULL WHERE state='IN_PROGRESS' AND julianday(last_attempt_at)<julianday('now','-2 minutes')",[]).map_err(|e|e.to_string())?;
    c.execute("INSERT OR IGNORE INTO entity_assets(entity_type,entity_id,local_relative_path,status,priority) SELECT 'TEAM',id,'teams/'||id||'.png','QUEUED',1 FROM teams",[]).map_err(|e|e.to_string())?;
    c.execute("UPDATE entity_assets SET priority=20 WHERE entity_type='TEAM' AND priority<20 AND entity_id IN (SELECT home_team_id FROM matches m JOIN provider_competition_mappings cp ON cp.competition_id=m.competition_id AND cp.provider='football-data.co.uk' UNION SELECT away_team_id FROM matches m JOIN provider_competition_mappings cp ON cp.competition_id=m.competition_id AND cp.provider='football-data.co.uk')",[]).map_err(|e|e.to_string())?;
    c.execute("UPDATE entity_assets SET status='QUEUED' WHERE status='MISSING' AND (next_retry_at IS NULL OR julianday(next_retry_at)<=julianday('now'))",[]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn request_asset(c: &Connection, kind: &str, id: i64, priority: i64) -> Result<(), String> {
    let relative = canonical_relative_path(kind, id)?
        .to_string_lossy()
        .replace('\\', "/");
    c.execute("INSERT OR IGNORE INTO entity_assets(entity_type,entity_id,local_relative_path,status,priority) VALUES(?1,?2,?3,'QUEUED',1)",params![kind,id,relative]).map_err(|e|e.to_string())?;
    // Visibility is a lease; an old page must not permanently outrank today's viewport.
    c.execute("UPDATE entity_assets SET visible_until=CASE WHEN ?3>=100 THEN datetime('now','+5 minutes') ELSE visible_until END,priority=CASE WHEN ?3<100 THEN max(priority,?3) ELSE priority END,status=CASE WHEN status='MISSING' AND (next_retry_at IS NULL OR julianday(next_retry_at)<=julianday('now')) THEN 'QUEUED' ELSE status END WHERE entity_type=?1 AND entity_id=?2",params![kind,id,priority]).map_err(|e|e.to_string())?;
    Ok(())
}
fn totals(c: &Connection) -> Result<AssetScanSummary, String> {
    let count = |kind: &str, status: &str| {
        c.query_row(
            "SELECT COUNT(*) FROM entity_assets WHERE entity_type=?1 AND status=?2",
            params![kind, status],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())
    };
    Ok(AssetScanSummary {
        teams_total: c
            .query_row("SELECT COUNT(*) FROM teams", [], |r| r.get(0))
            .map_err(|e| e.to_string())?,
        competitions_total: c
            .query_row("SELECT COUNT(*) FROM competitions", [], |r| r.get(0))
            .map_err(|e| e.to_string())?,
        team_logos_ready: count("TEAM", "READY")?,
        competition_logos_ready: count("COMPETITION", "READY")?,
        team_logos_missing: count("TEAM", "MISSING")?
            + count("TEAM", "QUEUED")?
            + count("TEAM", "FAILED")?,
        competition_logos_missing: count("COMPETITION", "MISSING")?
            + count("COMPETITION", "QUEUED")?
            + count("COMPETITION", "FAILED")?,
        queued: count("TEAM", "QUEUED")? + count("COMPETITION", "QUEUED")?,
        failed: count("TEAM", "FAILED")? + count("COMPETITION", "FAILED")?,
        unavailable: count("TEAM", "UNAVAILABLE")? + count("COMPETITION", "UNAVAILABLE")?,
    })
}

fn queued_items(c: &Connection) -> Result<Vec<EntityIdentity>, String> {
    let mut s=c.prepare("SELECT a.entity_type,a.entity_id,CASE a.entity_type WHEN 'TEAM' THEN (SELECT normalized_name FROM teams WHERE id=a.entity_id) ELSE (SELECT name FROM competitions WHERE id=a.entity_id) END,CASE a.entity_type WHEN 'TEAM' THEN (SELECT country FROM teams WHERE id=a.entity_id) ELSE (SELECT country FROM competitions WHERE id=a.entity_id) END FROM entity_assets a WHERE a.status='QUEUED' AND (a.next_retry_at IS NULL OR julianday(a.next_retry_at)<=julianday('now')) ORDER BY COALESCE(julianday(a.visible_until)>julianday('now'),0) DESC,min(a.priority,20) DESC,(EXISTS(SELECT 1 FROM logo_sources ls WHERE ls.entity_type=a.entity_type AND ls.entity_id=a.entity_id AND ls.kind='IMAGE' AND ls.state='PENDING')) DESC,COALESCE(a.last_attempt_at,'') ASC,a.entity_id LIMIT 24").map_err(|e|e.to_string())?;
    let rows = s
        .query_map([], |r| {
            Ok(EntityIdentity {
                entity_type: r.get(0)?,
                entity_id: r.get(1)?,
                name: r.get(2)?,
                country: r.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

fn http_failure(r: &reqwest::Response) -> String {
    let seconds = r
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.parse::<i64>().ok().or_else(|| {
                chrono::DateTime::parse_from_rfc2822(v)
                    .ok()
                    .map(|t| (t.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_seconds())
            })
        })
        .unwrap_or(1800)
        .max(60);
    format!("HTTP_{}:{}", r.status().as_u16(), seconds)
}
async fn process_one(
    db: &Database,
    root: &Path,
    client: &reqwest::Client,
    _provider: &dyn EntityMetadataProvider,
    item: &EntityIdentity,
) -> Result<usize, String> {
    use crate::logo_discovery as discovery;
    let source = {
        let c = db.connection()?;
        discovery::seed(&c, item)?;
        let source = discovery::next(&c, item)?;
        let Some(source) = source else {
            discovery::settle(&c, item)?;
            return Ok(0);
        };
        let claimed=c.execute("UPDATE entity_assets SET status='DOWNLOADING',discovery_state=?3,last_attempt_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE entity_type=?1 AND entity_id=?2 AND status='QUEUED'",params![item.entity_type,item.entity_id,if source.kind=="IMAGE"{"DOWNLOADING"}else{"DISCOVERING_SOURCE"}]).map_err(|e|e.to_string())?;
        if claimed == 0 {
            return Ok(0);
        }
        c.execute("UPDATE logo_sources SET state='IN_PROGRESS',attempts=attempts+1,last_attempt_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",[source.id]).map_err(|e|e.to_string())?;
        source
    };
    let mut http_status = None;
    let result: Result<Vec<u8>, String> = async {
        let mut response = client.get(&source.url).send().await.map_err(|e| {
            if e.is_timeout() {
                "TIMEOUT".to_string()
            } else {
                format!("DOWNLOAD_FAILED: {e}")
            }
        })?;
        http_status = Some(response.status().as_u16());
        if !response.status().is_success() {
            return Err(http_failure(&response));
        }
        if source.kind == "IMAGE"
            && response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| !v.starts_with("image/"))
        {
            return Err("INVALID_IMAGE: MIME".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| {
            if e.is_timeout() {
                "TIMEOUT".to_string()
            } else {
                e.to_string()
            }
        })? {
            if bytes.len() + chunk.len() > MAX_DOWNLOAD_BYTES {
                return Err("INVALID_IMAGE: payload limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
    .await;
    let mut saved = 0;
    let processed=result.and_then(|bytes|{
        if source.kind=="IMAGE" {
            let image=normalize_image(&bytes).map_err(|e|format!("INVALID_IMAGE: {e}"))?;
            let rel=canonical_relative_path(&item.entity_type,item.entity_id)?;let path=safe_asset_path(root,&rel)?;
            fs::create_dir_all(path.parent().ok_or("asset parent")?).map_err(|e|e.to_string())?;
            let tmp=path.with_extension("png.tmp");fs::write(&tmp,&image.bytes).map_err(|e|e.to_string())?;
            if path.exists(){fs::remove_file(&path).map_err(|e|e.to_string())?}fs::rename(&tmp,path).map_err(|e|e.to_string())?;
            let c=db.connection()?;
            c.execute("UPDATE entity_assets SET status='READY',discovery_state='READY',source_provider=?3,source_url=?4,local_relative_path=?5,content_sha256=?6,mime_type='image/png',byte_size=?7,width=?8,height=?9,failure_count=0,failure_kind=NULL,last_error=NULL,retry_state=NULL,next_retry_at=NULL,last_http_status=200,last_success_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE entity_type=?1 AND entity_id=?2",params![item.entity_type,item.entity_id,source.provider,source.url,rel.to_string_lossy().replace('\\',"/"),image.sha256,image.bytes.len() as i64,image.width,image.height]).map_err(|e|e.to_string())?;
            saved=image.bytes.len();
        }else {
            let found=if source.kind=="SPORTSDB"{discovery::sports_badge(&source,&bytes)}else{
                let q=EntityIdentity{name:source.key.clone(),country:Some(source.context.clone()),..item.clone()};
                WikimediaProvider.parse_logo_url(&q,&bytes).map(|url|(url,String::new()))
            }.ok_or("SOURCE_NOT_DISCOVERED: no exact country-qualified badge")?;
            let c=db.connection()?;
            let identity_key=if found.1.is_empty(){source.key.clone()}else{format!("idTeam:{} | {}",found.1,source.key)};
            discovery::add_source(&c,item,&source.provider,&found.0,"IMAGE",&identity_key,&source.context,-1)?;
            c.execute("UPDATE logo_sources SET resolved_url=?2 WHERE id=?1",params![source.id,found.0]).map_err(|e|e.to_string())?;
            // The image source retains the discovered provider ID without mutating
            // the prediction resolver's provider registry.
        }
        Ok(())
    });
    let c = db.connection()?;
    let (state, kind, detail, seconds) = match &processed {
        Ok(()) => ("SUCCEEDED", "SUCCESS", None, 0),
        Err(e) => {
            let kind = if e.starts_with("HTTP_429:") {
                "RATE_LIMITED"
            } else if e.starts_with("HTTP_404:") {
                "SOURCE_404"
            } else if e.starts_with("HTTP_4") {
                "UNSUPPORTED_SOURCE"
            } else if e.starts_with("TIMEOUT") {
                "TIMEOUT"
            } else if e.starts_with("INVALID_IMAGE") {
                "INVALID_IMAGE"
            } else if e.starts_with("SOURCE_NOT_DISCOVERED") {
                "SOURCE_NOT_DISCOVERED"
            } else {
                "DOWNLOAD_FAILED"
            };
            let transient = matches!(kind, "RATE_LIMITED" | "TIMEOUT" | "DOWNLOAD_FAILED");
            let seconds = if kind == "RATE_LIMITED" || e.starts_with("HTTP_503:") {
                e.split(':')
                    .nth(1)
                    .and_then(|x| x.parse::<i64>().ok())
                    .unwrap_or(1800)
            } else {
                60 * 5_i64.pow((source.attempts as u32).min(3))
            };
            (
                if transient { "RETRY" } else { "TERMINAL" },
                kind,
                Some(e.as_str()),
                seconds,
            )
        }
    };
    c.execute("UPDATE logo_sources SET state=?2,http_status=?3,error_kind=?4,next_retry_at=CASE WHEN ?2='RETRY' THEN datetime('now','+'||?5||' seconds') ELSE NULL END WHERE id=?1",params![source.id,state,http_status,if kind=="SUCCESS"{None}else{Some(kind)},seconds]).map_err(|e|e.to_string())?;
    c.execute("INSERT INTO logo_attempts(source_id,entity_type,entity_id,provider,url,stage,http_status,outcome,detail) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![source.id,item.entity_type,item.entity_id,source.provider,source.url,source.kind,http_status,kind,detail]).map_err(|e|e.to_string())?;
    if kind != "SUCCESS" {
        c.execute("UPDATE entity_assets SET failure_kind=?3,last_error=?4,last_http_status=?5,failure_count=failure_count+1,retry_state=CASE WHEN ?3='RATE_LIMITED' THEN 'RATE_LIMITED' ELSE NULL END WHERE entity_type=?1 AND entity_id=?2",params![item.entity_type,item.entity_id,kind,detail,http_status]).map_err(|e|e.to_string())?;
        if kind == "RATE_LIMITED" || http_status == Some(503) {
            c.execute("INSERT INTO logo_provider_backoff(provider,next_retry_at) VALUES(?1,datetime('now','+'||?2||' seconds')) ON CONFLICT(provider) DO UPDATE SET next_retry_at=excluded.next_retry_at",params![source.provider,seconds]).map_err(|e|e.to_string())?;
        }
    }
    discovery::settle(&c, item)?;
    // A permanent source failure advances this team's fallback; it does not kill the worker.
    Ok(saved)
}

pub fn logo_path(
    c: &Connection,
    root: &Path,
    kind: &str,
    id: i64,
) -> Result<Option<String>, String> {
    let row:Option<(String,Option<String>)>=c.query_row("SELECT status,local_relative_path FROM entity_assets WHERE entity_type=?1 AND entity_id=?2",params![kind,id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
    let Some((status, path)) = row else {
        return Ok(None);
    };
    if status != "READY" {
        return Ok(None);
    }
    if let Some(relative) = path.filter(|v| is_cached_asset_valid(root, v, None)) {
        return Ok(Some(
            safe_asset_path(root, Path::new(&relative))?
                .to_string_lossy()
                .into_owned(),
        ));
    }
    // A lost local file does not invalidate its proven remote source. Re-fetch
    // that successful URL once; terminal remote URLs remain terminal.
    c.execute("UPDATE logo_sources SET state='PENDING',next_retry_at=NULL WHERE entity_type=?1 AND entity_id=?2 AND kind='IMAGE' AND state='SUCCEEDED' AND url=(SELECT source_url FROM entity_assets WHERE entity_type=?1 AND entity_id=?2)",params![kind,id]).map_err(|e|e.to_string())?;
    c.execute("UPDATE entity_assets SET status='MISSING',retry_state='INVALID',failure_kind='INVALID_IMAGE',failure_count=0,next_retry_at=NULL,last_error='cached file missing or invalid' WHERE entity_type=?1 AND entity_id=?2",params![kind,id]).map_err(|e|e.to_string())?;
    Ok(None)
}

pub fn reconcile_team_assets(
    tx: &rusqlite::Transaction<'_>,
    duplicate: i64,
    canonical: i64,
) -> rusqlite::Result<()> {
    let canonical_ready:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM entity_assets WHERE entity_type='TEAM' AND entity_id=?1 AND status='READY')",[canonical],|r|r.get(0))?;
    if !canonical_ready {
        tx.execute(
            "DELETE FROM entity_assets WHERE entity_type='TEAM' AND entity_id=?1",
            [canonical],
        )?;
        tx.execute(
            "UPDATE entity_assets SET entity_id=?2 WHERE entity_type='TEAM' AND entity_id=?1",
            params![duplicate, canonical],
        )?;
    } else {
        tx.execute(
            "DELETE FROM entity_assets WHERE entity_type='TEAM' AND entity_id=?1",
            [duplicate],
        )?;
    }
    tx.execute("INSERT INTO team_metadata(team_id,display_name,short_name,city,venue_name,website,founded_year,source_provider,source_metadata_json) SELECT ?2,display_name,short_name,city,venue_name,website,founded_year,source_provider,source_metadata_json FROM team_metadata WHERE team_id=?1 ON CONFLICT(team_id) DO NOTHING",params![duplicate,canonical])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::teams;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn discovery_url_has_one_title_segment() {
        let entity = EntityIdentity {
            entity_type: "TEAM".into(),
            entity_id: 1,
            name: "A/B FC".into(),
            country: None,
        };
        assert_eq!(
            WikimediaProvider.discovery_url(&entity),
            "https://en.wikipedia.org/api/rest_v1/page/summary/A%2FB%20FC"
        );
    }

    fn png(color: [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(8, 6, image::Rgba(color));
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }
    fn ready(c: &Connection, root: &Path, kind: &str, id: i64, bytes: &[u8]) {
        let image = normalize_image(bytes).unwrap();
        let rel = canonical_relative_path(kind, id).unwrap();
        let path = safe_asset_path(root, &rel).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &image.bytes).unwrap();
        c.execute("UPDATE entity_assets SET status='READY',local_relative_path=?3,content_sha256=?4,mime_type='image/png',byte_size=?5,width=?6,height=?7 WHERE entity_type=?1 AND entity_id=?2",params![kind,id,rel.to_string_lossy().replace('\\',"/"),image.sha256,image.bytes.len() as i64,image.width,image.height]).unwrap();
    }
    fn server(body: Vec<u8>, requests: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for stream in listener.incoming().take(requests) {
                let mut stream = stream.unwrap();
                let mut buf = [0; 1024];
                let _ = stream.read(&mut buf);
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        format!("http://{addr}/logo.png")
    }

    #[test]
    fn provider_specific_id_precedes_name_discovery() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        let id = teams::insert(&c, "Arsenal", Some("England")).unwrap();
        c.execute("INSERT INTO provider_team_mappings(team_id,provider,external_team_id,external_team_name) VALUES(?1,'football-data.org','57','Arsenal')",[id]).unwrap();
        request_asset(&c, "TEAM", id, 100).unwrap();
        let item = EntityIdentity {
            entity_type: "TEAM".into(),
            entity_id: id,
            name: "Arsenal".into(),
            country: Some("England".into()),
        };
        crate::logo_discovery::seed(&c, &item).unwrap();
        assert_eq!(
            crate::logo_discovery::next(&c, &item).unwrap().unwrap().url,
            "https://crests.football-data.org/57.png"
        );
    }
    #[test]
    fn bounded_cache_first_assets_and_persistent_backoff() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("db.sqlite3")).unwrap();
        let c = db.connection().unwrap();
        let root = asset_root(dir.path());
        for i in 0..3000 {
            teams::insert(&c, &format!("Unrequested {i}"), Some("England")).unwrap();
        }
        let summary = scan(&c, &root).unwrap();
        assert_eq!(summary.queued, 3000);
        assert_eq!(
            c.query_row::<i64, _, _>("SELECT count(*) FROM entity_assets", [], |r| r.get(0))
                .unwrap(),
            3000
        );
        request_asset(&c, "TEAM", 1, 10).unwrap();
        request_asset(&c, "TEAM", 1, 10).unwrap();
        assert_eq!(
            c.query_row::<i64, _, _>("SELECT count(*) FROM entity_assets", [], |r| r.get(0))
                .unwrap(),
            3000
        );
        assert!(queued_items(&c).unwrap().len() <= 24);
        request_asset(&c, "TEAM", 3000, 100).unwrap();
        assert_eq!(queued_items(&c).unwrap()[0].entity_id, 3000);
        let bytes = png([0, 100, 200, 255]);
        ready(&c, &root, "TEAM", 1, &bytes);
        assert!(logo_path(&c, &root, "TEAM", 1).unwrap().is_some());
        request_asset(&c, "TEAM", 1, 10).unwrap();
        assert_eq!(
            c.query_row::<String, _, _>(
                "SELECT status FROM entity_assets WHERE entity_id=1",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "READY"
        );
        fs::write(root.join("teams/1.png"), b"not an image").unwrap();
        assert!(logo_path(&c, &root, "TEAM", 1).unwrap().is_none());
        request_asset(&c, "TEAM", 1, 10).unwrap();
        assert_eq!(
            c.query_row::<String, _, _>(
                "SELECT status FROM entity_assets WHERE entity_id=1",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "QUEUED"
        );
        c.execute("UPDATE entity_assets SET status='MISSING',retry_state='RATE_LIMITED',next_retry_at=datetime('now','+1 hour') WHERE entity_id=1",[]).unwrap();
        request_asset(&c, "TEAM", 1, 10).unwrap();
        assert_eq!(
            c.query_row::<String, _, _>(
                "SELECT status FROM entity_assets WHERE entity_id=1",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "MISSING"
        );
        assert!(normalize_image(b"<html>error</html>").is_err());
        assert!(safe_asset_path(&root, Path::new("../escape.png")).is_err());
        drop(c);
        drop(db);
        let reopened = Database::open(dir.path().join("db.sqlite3")).unwrap();
        let c = reopened.connection().unwrap();
        assert_eq!(
            c.query_row::<String, _, _>(
                "SELECT retry_state FROM entity_assets WHERE entity_id=1",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "RATE_LIMITED"
        );
        assert!(!queued_items(&c).unwrap().is_empty()); // One provider cooldown cannot block unrelated discovery.
                                                        // Scheduler wakeup, without any page visit, makes an expired retry runnable.
        c.execute(
            "UPDATE entity_assets SET next_retry_at=datetime('now','-1 minute') WHERE entity_id=1",
            [],
        )
        .unwrap();
        queue_background(&c).unwrap();
        assert_eq!(
            c.query_row::<String, _, _>(
                "SELECT status FROM entity_assets WHERE entity_id=1",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "QUEUED"
        );
        assert!(!queued_items(&c).unwrap().is_empty());
        c.execute("UPDATE entity_assets SET status='UNAVAILABLE',discovery_version=3,discovery_state='SOURCE_NOT_FOUND',last_error='all configured sources exhausted' WHERE entity_id=2",[]).unwrap();
        queue_background(&c).unwrap();
        request_asset(&c, "TEAM", 2, 100).unwrap();
        assert_eq!(
            c.query_row::<String, _, _>(
                "SELECT status FROM entity_assets WHERE entity_id=2",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "UNAVAILABLE"
        );
    }
    #[test]
    fn retry_after_and_real_http_image_validation() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0; 1024];
            let _ = stream.read(&mut buf);
            stream.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 120\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let failure = tauri::async_runtime::block_on(async {
            let response = reqwest::Client::new()
                .get(format!("http://{address}"))
                .send()
                .await
                .unwrap();
            http_failure(&response)
        });
        assert_eq!(failure, "HTTP_429:120");
        let bytes = reqwest::blocking::Client::new()
            .get(server(png([3, 5, 7, 255]), 1))
            .send()
            .unwrap()
            .bytes()
            .unwrap();
        assert!(normalize_image(&bytes).is_ok());
    }

    #[test]
    fn dead_url_falls_back_once_and_ready_is_not_downloaded_twice() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("test.sqlite3")).unwrap();
        let root = asset_root(dir.path());
        let item = EntityIdentity {
            entity_type: "TEAM".into(),
            entity_id: 1,
            name: "Test".into(),
            country: Some("England".into()),
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let bad = format!("http://{}/missing", listener.local_addr().unwrap());
        thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0; 1024];
            let _ = s.read(&mut buf);
            s.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        });
        let good = server(png([20, 40, 60, 255]), 1);
        {
            let c = db.connection().unwrap();
            teams::insert(&c, "Test", Some("England")).unwrap();
            request_asset(&c, "TEAM", 1, 100).unwrap();
            c.execute("UPDATE entity_assets SET discovery_version=3", [])
                .unwrap();
            crate::logo_discovery::add_source(
                &c, &item, "dead", &bad, "IMAGE", "Test", "england", 0,
            )
            .unwrap();
            crate::logo_discovery::add_source(
                &c,
                &item,
                "alternate",
                &good,
                "IMAGE",
                "Test",
                "england",
                1,
            )
            .unwrap();
        }
        let client = reqwest::Client::new();
        tauri::async_runtime::block_on(async {
            assert_eq!(
                process_one(&db, &root, &client, &WikimediaProvider, &item)
                    .await
                    .unwrap(),
                0
            );
            assert!(
                process_one(&db, &root, &client, &WikimediaProvider, &item)
                    .await
                    .unwrap()
                    > 0
            );
            assert_eq!(
                process_one(&db, &root, &client, &WikimediaProvider, &item)
                    .await
                    .unwrap(),
                0
            );
        });
        let c = db.connection().unwrap();
        assert!(logo_path(&c, &root, "TEAM", 1).unwrap().is_some());
        assert_eq!(
            c.query_row::<i64, _, _>(
                "SELECT count(*) FROM logo_attempts WHERE stage='IMAGE'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            2
        );
        assert_eq!(
            c.query_row::<String, _, _>(
                "SELECT error_kind FROM logo_sources WHERE provider='dead'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "SOURCE_404"
        );
        fs::remove_file(root.join("teams/1.png")).unwrap();
        assert!(logo_path(&c, &root, "TEAM", 1).unwrap().is_none());
        assert_eq!(
            c.query_row::<String, _, _>(
                "SELECT state FROM logo_sources WHERE provider='alternate'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "PENDING"
        );
        assert_eq!(
            c.query_row::<String, _, _>(
                "SELECT state FROM logo_sources WHERE provider='dead'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "TERMINAL"
        );
    }

    #[test]
    fn provider_cooldown_keeps_alternate_runnable_and_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite3");
        let db = Database::open(path.clone()).unwrap();
        let item = EntityIdentity {
            entity_type: "TEAM".into(),
            entity_id: 1,
            name: "Test".into(),
            country: Some("England".into()),
        };
        {
            let c = db.connection().unwrap();
            teams::insert(&c, "Test", Some("England")).unwrap();
            request_asset(&c, "TEAM", 1, 100).unwrap();
            crate::logo_discovery::add_source(
                &c,
                &item,
                "limited",
                "https://limited.test/logo",
                "IMAGE",
                "Test",
                "england",
                0,
            )
            .unwrap();
            crate::logo_discovery::add_source(
                &c,
                &item,
                "alternate",
                "https://alternate.test/logo",
                "IMAGE",
                "Test",
                "england",
                1,
            )
            .unwrap();
            c.execute(
                "INSERT INTO logo_provider_backoff VALUES('limited',datetime('now','+1 hour'))",
                [],
            )
            .unwrap();
        }
        drop(db);
        let db = Database::open(path).unwrap();
        let c = db.connection().unwrap();
        assert_eq!(
            crate::logo_discovery::next(&c, &item)
                .unwrap()
                .unwrap()
                .provider,
            "alternate"
        );
        c.execute(
            "UPDATE logo_provider_backoff SET next_retry_at=datetime('now','-1 second')",
            [],
        )
        .unwrap();
        assert_eq!(
            crate::logo_discovery::next(&c, &item)
                .unwrap()
                .unwrap()
                .provider,
            "limited"
        );
    }
}
