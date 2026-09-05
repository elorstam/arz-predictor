use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
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
        "wikimedia-pageimages"
    }

    fn discovery_url(&self, entity: &EntityIdentity) -> String {
        let mut url = reqwest::Url::parse("https://en.wikipedia.org/w/rest.php/v1/page/").unwrap();
        url.path_segments_mut()
            .unwrap()
            .push(&entity.name)
            .push("bare");
        url.to_string()
    }

    fn parse_logo_url(&self, entity: &EntityIdentity, bytes: &[u8]) -> Option<String> {
        let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let title = value["title"].as_str()?;
        if crate::repositories::resolution::normalize_team_name(title)
            != crate::repositories::resolution::normalize_team_name(&entity.name)
        {
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
                _ => country.as_str(),
            };
            if !description.contains(&country) && !description.contains(demonym) {
                return None;
            }
        }
        let url = value["thumbnail"]["url"].as_str()?;
        if !url.starts_with("https://upload.wikimedia.org/") {
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
}

impl AssetSyncManager {
    pub fn new(database_path: PathBuf, app_data_root: PathBuf) -> Self {
        Self {
            database_path,
            asset_root: asset_root(&app_data_root),
            status: Arc::new(Mutex::new(AssetSyncStatus::default())),
        }
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
            c.execute("UPDATE entity_assets SET status='MISSING',failure_count=0,next_retry_at=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE status='FAILED'",[]).map_err(|e|e.to_string())?;
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
            let summary = scan(&c, &self.asset_root)?;
            let rows = queued_items(&c)?;
            let mut s = self.status.lock().unwrap();
            s.state = "DOWNLOADING".into();
            s.queued = summary.queued;
            rows
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
            .user_agent("ARZ-Predictor/0.1")
            .build()
            .map_err(|e| e.to_string())?;
        let provider = WikimediaProvider;
        for item in items {
            {
                let mut s = self.status.lock().unwrap();
                s.active = 1;
                s.queued = s.queued.saturating_sub(1);
            }
            let outcome = process_one(&db, &self.asset_root, &client, &provider, &item).await;
            let mut s = self.status.lock().unwrap();
            match outcome {
                Ok(bytes) => {
                    s.completed_this_run += 1;
                    s.bytes_downloaded_this_run += bytes as i64
                }
                Err(_) => s.failed_this_run += 1,
            }
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
    for (kind, table) in [("TEAM", "teams"), ("COMPETITION", "competitions")] {
        let sql = format!("SELECT id FROM {table}");
        let mut stmt = c.prepare(&sql).map_err(|e| e.to_string())?;
        let ids = stmt
            .query_map([], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for id in ids {
            let rel = canonical_relative_path(kind, id)?
                .to_string_lossy()
                .replace('\\', "/");
            c.execute("INSERT OR IGNORE INTO entity_assets(entity_type,entity_id,local_relative_path,status) VALUES(?1,?2,?3,'MISSING')",params![kind,id,rel]).map_err(|e|e.to_string())?;
        }
    }
    let mut stmt=c.prepare("SELECT id,entity_type,entity_id,status,local_relative_path,content_sha256 FROM entity_assets WHERE asset_type='LOGO'").map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (id, kind, entity_id, status, path, sha) in rows {
        if status == "READY" {
            let valid = path
                .as_deref()
                .is_some_and(|p| is_cached_asset_valid(root, p, sha.as_deref()));
            if !valid {
                c.execute("UPDATE entity_assets SET status='MISSING',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",[id]).map_err(|e|e.to_string())?;
            } else {
                let expected = canonical_relative_path(&kind, entity_id)?
                    .to_string_lossy()
                    .replace('\\', "/");
                if path.as_deref() != Some(expected.as_str()) {
                    let old = safe_asset_path(root, Path::new(path.as_deref().unwrap()))?;
                    let new = safe_asset_path(root, Path::new(&expected))?;
                    if let Some(parent) = new.parent() {
                        fs::create_dir_all(parent).map_err(|e| e.to_string())?
                    }
                    fs::copy(&old, &new).map_err(|e| e.to_string())?;
                    let _ = fs::remove_file(old);
                    c.execute("UPDATE entity_assets SET local_relative_path=?2,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",params![id,expected]).map_err(|e|e.to_string())?;
                }
            }
        }
    }
    c.execute("UPDATE entity_assets SET status='QUEUED',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE (status='MISSING' AND (failure_count=0 OR next_retry_at IS NULL OR next_retry_at<=strftime('%Y-%m-%dT%H:%M:%fZ','now'))) OR (status='FAILED' AND failure_count<?1 AND (next_retry_at IS NULL OR next_retry_at<=strftime('%Y-%m-%dT%H:%M:%fZ','now')))",[MAX_FAILURES]).map_err(|e|e.to_string())?;
    totals(c)
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
    let mut s=c.prepare("SELECT a.entity_type,a.entity_id,CASE a.entity_type WHEN 'TEAM' THEN (SELECT normalized_name FROM teams WHERE id=a.entity_id) ELSE (SELECT name FROM competitions WHERE id=a.entity_id) END,CASE a.entity_type WHEN 'TEAM' THEN (SELECT country FROM teams WHERE id=a.entity_id) ELSE (SELECT country FROM competitions WHERE id=a.entity_id) END FROM entity_assets a WHERE a.status='QUEUED' ORDER BY a.entity_type,a.entity_id").map_err(|e|e.to_string())?;
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

async fn process_one(
    db: &Database,
    root: &Path,
    client: &reqwest::Client,
    provider: &dyn EntityMetadataProvider,
    item: &EntityIdentity,
) -> Result<usize, String> {
    {
        let c = db.connection()?;
        c.execute("UPDATE entity_assets SET status='DOWNLOADING',last_attempt_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE entity_type=?1 AND entity_id=?2",params![item.entity_type,item.entity_id]).map_err(|e|e.to_string())?;
    }
    let source_url = {
        let c = db.connection()?;
        c.query_row(
            "SELECT source_url FROM entity_assets WHERE entity_type=?1 AND entity_id=?2",
            params![item.entity_type, item.entity_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .map_err(|e| e.to_string())?
    };
    let result: Result<(String, NormalizedImage), String> = async {
        let logo_url = if let Some(url) = source_url {
            url
        } else {
            let response = client
                .get(provider.discovery_url(item))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!("metadata HTTP {}", response.status()));
            }
            let bytes = response.bytes().await.map_err(|e| e.to_string())?;
            provider
                .parse_logo_url(item, &bytes)
                .ok_or_else(|| "no exact safe logo match".to_string())?
        };
        let response = client
            .get(&logo_url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("logo HTTP {}", response.status()));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_DOWNLOAD_BYTES as u64)
        {
            return Err("image payload too large".into());
        }
        if response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| !v.starts_with("image/"))
        {
            return Err("response is not an image".into());
        }
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        Ok((logo_url, normalize_image(&bytes)?))
    }
    .await;
    let c = db.connection()?;
    match result {
        Ok((url, image)) => {
            let rel = canonical_relative_path(&item.entity_type, item.entity_id)?;
            let path = safe_asset_path(root, &rel)?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?
            }
            let tmp = path.with_extension("png.tmp");
            fs::write(&tmp, &image.bytes).map_err(|e| e.to_string())?;
            if path.exists() {
                fs::remove_file(&path).map_err(|e| e.to_string())?
            }
            fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
            c.execute("UPDATE entity_assets SET source_provider=?3,source_url=?4,local_relative_path=?5,content_sha256=?6,mime_type='image/png',byte_size=?7,width=?8,height=?9,status='READY',failure_count=0,last_success_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),next_retry_at=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE entity_type=?1 AND entity_id=?2",params![item.entity_type,item.entity_id,provider.provider_name(),url,rel.to_string_lossy().replace('\\',"/"),image.sha256,image.bytes.len() as i64,image.width,image.height]).map_err(|e|e.to_string())?;
            Ok(image.bytes.len())
        }
        Err(error) => {
            if error == "no exact safe logo match" {
                c.execute("UPDATE entity_assets SET status='UNAVAILABLE',next_retry_at=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE entity_type=?1 AND entity_id=?2",params![item.entity_type,item.entity_id]).map_err(|e|e.to_string())?;
            } else {
                c.execute("UPDATE entity_assets SET failure_count=failure_count+1,status=CASE WHEN failure_count+1>=?3 THEN 'FAILED' ELSE 'MISSING' END,next_retry_at=datetime('now','+'||(CASE failure_count WHEN 0 THEN 5 WHEN 1 THEN 30 ELSE 120 END)||' minutes'),updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE entity_type=?1 AND entity_id=?2",params![item.entity_type,item.entity_id,MAX_FAILURES]).map_err(|e|e.to_string())?;
            }
            Err(error)
        }
    }
}

pub fn logo_path(
    c: &Connection,
    root: &Path,
    kind: &str,
    id: i64,
) -> Result<Option<String>, String> {
    let row:Option<(String,Option<String>)>=c.query_row("SELECT status,local_relative_path FROM entity_assets WHERE entity_type=?1 AND entity_id=?2",params![kind,id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
    Ok(row.and_then(|(s, p)| {
        if s == "READY"
            && p.as_deref()
                .is_some_and(|v| is_cached_asset_valid(root, v, None))
        {
            p.and_then(|v| safe_asset_path(root, Path::new(&v)).ok())
                .map(|v| v.to_string_lossy().into_owned())
        } else {
            None
        }
    }))
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
    use crate::repositories::{competitions, teams};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

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
    fn phase_4_1_asset_acceptance_and_integration() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(temp.path().join("test.sqlite3")).unwrap();
        let c = db.connection().unwrap();
        assert_eq!(
            c.query_row::<i64, _, _>("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get(0))
                .unwrap(),
            20
        );
        let comp1 =
            competitions::insert(&c, "Premier League", Some("England"), Some("2026/27")).unwrap();
        let comp2 =
            competitions::insert(&c, "Championship", Some("England"), Some("2026/27")).unwrap();
        let t1 = teams::insert(&c, "Arsenal", Some("England")).unwrap();
        let t2 = teams::insert(&c, "Chelsea", Some("England")).unwrap();
        let _t3 = teams::insert(&c, "Liverpool", Some("England")).unwrap();
        let root = asset_root(temp.path());
        assert_eq!(
            canonical_relative_path("TEAM", t1).unwrap(),
            PathBuf::from(format!("teams/{t1}.png"))
        );
        assert_eq!(
            canonical_relative_path("COMPETITION", comp1).unwrap(),
            PathBuf::from(format!("competitions/{comp1}.png"))
        );
        assert!(safe_asset_path(&root, Path::new("../escape.png")).is_err());
        assert!(normalize_image(&[]).is_err());
        assert!(normalize_image(b"not image").is_err());
        assert!(normalize_image(&vec![0; MAX_DOWNLOAD_BYTES + 1]).is_err());
        let valid = png([20, 40, 60, 128]);
        let normalized = normalize_image(&valid).unwrap();
        assert_eq!((normalized.width, normalized.height), (8, 6));
        assert_eq!(
            normalized.sha256,
            format!("{:x}", Sha256::digest(&normalized.bytes))
        );
        assert_eq!(&normalized.bytes[..8], b"\x89PNG\r\n\x1a\n");

        let initial = scan(&c, &root).unwrap();
        assert_eq!(
            (
                initial.teams_total,
                initial.competitions_total,
                initial.queued
            ),
            (3, 2, 5)
        );
        ready(&c, &root, "TEAM", t1, &valid);
        ready(&c, &root, "COMPETITION", comp1, &valid);
        c.execute("UPDATE entity_assets SET status='UNAVAILABLE' WHERE entity_type='COMPETITION' AND entity_id=?1",[comp2]).unwrap();
        let t2rel = canonical_relative_path("TEAM", t2).unwrap();
        fs::write(safe_asset_path(&root, &t2rel).unwrap(), b"corrupt").unwrap();
        c.execute("UPDATE entity_assets SET status='READY',local_relative_path=?2 WHERE entity_type='TEAM' AND entity_id=?1",params![t2,t2rel.to_string_lossy().replace('\\',"/")]).unwrap();
        let first = scan(&c, &root).unwrap();
        assert_eq!(first.team_logos_ready, 1);
        assert_eq!(first.competition_logos_ready, 1);
        assert_eq!(first.unavailable, 1);
        assert_eq!(first.queued, 2);
        assert!(is_cached_asset_valid(
            &root,
            &canonical_relative_path("TEAM", t1)
                .unwrap()
                .to_string_lossy(),
            None
        ));

        let url = server(valid.clone(), 2);
        c.execute(
            "UPDATE entity_assets SET source_url=?1 WHERE status='QUEUED'",
            [&url],
        )
        .unwrap();
        let items = queued_items(&c).unwrap();
        assert_eq!(items.len(), 2);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let provider = WikimediaProvider;
        drop(c);
        let mut bytes = 0;
        for item in &items {
            bytes +=
                tauri::async_runtime::block_on(process_one(&db, &root, &client, &provider, item))
                    .unwrap();
        }
        let c = db.connection().unwrap();
        let second = scan(&c, &root).unwrap();
        assert_eq!(second.queued, 0);
        assert_eq!(second.team_logos_ready, 3);
        assert_eq!(second.competition_logos_ready, 1);
        assert!(bytes > 0);
        let third = scan(&c, &root).unwrap();
        assert_eq!(third.queued, 0);
        let new_team = teams::insert(&c, "New Town", Some("England")).unwrap();
        let after_new = scan(&c, &root).unwrap();
        assert_eq!(after_new.queued, 1);
        assert_eq!(after_new.team_logos_ready, 3);
        assert_eq!(after_new.teams_total, 4);
        let new_comp =
            competitions::insert(&c, "League One", Some("England"), Some("2026/27")).unwrap();
        let after_comp = scan(&c, &root).unwrap();
        assert_eq!(after_comp.queued, 2);
        assert_eq!(after_comp.competitions_total, 3);
        assert_eq!(
            c.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM entity_assets WHERE entity_type='TEAM' AND entity_id=?1",
                [t1],
                |r| r.get(0)
            )
            .unwrap(),
            1
        );
        c.execute("INSERT INTO provider_team_mappings(team_id,provider,external_team_id,external_team_name) VALUES(?1,'one','a','Arsenal'),(?1,'two','b','Arsenal FC')",[t1]).unwrap();
        scan(&c, &root).unwrap();
        assert_eq!(
            c.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM entity_assets WHERE entity_type='TEAM' AND entity_id=?1",
                [t1],
                |r| r.get(0)
            )
            .unwrap(),
            1
        );
        assert!(logo_path(&c, &root, "TEAM", t1).unwrap().is_some());
        assert!(logo_path(&c, &root, "TEAM", new_team).unwrap().is_none());

        fs::remove_file(
            safe_asset_path(&root, &canonical_relative_path("TEAM", t1).unwrap()).unwrap(),
        )
        .unwrap();
        let recovery = scan(&c, &root).unwrap();
        assert_eq!(recovery.queued, 3);
        assert_eq!(
            c.query_row::<String, _, _>(
                "SELECT status FROM entity_assets WHERE entity_type='TEAM' AND entity_id=?1",
                [t1],
                |r| r.get(0)
            )
            .unwrap(),
            "QUEUED"
        );
        ready(&c, &root, "TEAM", t1, &valid);
        c.execute("UPDATE entity_assets SET status='QUEUED',source_url='http://127.0.0.1:1/nope' WHERE entity_type='TEAM' AND entity_id=?1",[new_team]).unwrap();
        let failed_item = queued_items(&c)
            .unwrap()
            .into_iter()
            .find(|i| i.entity_id == new_team)
            .unwrap();
        drop(c);
        for _ in 0..3 {
            let _ = tauri::async_runtime::block_on(process_one(
                &db,
                &root,
                &client,
                &provider,
                &failed_item,
            ));
        }
        let mut c = db.connection().unwrap();
        assert_eq!(c.query_row::<(String,i64),_,_>("SELECT status,failure_count FROM entity_assets WHERE entity_type='TEAM' AND entity_id=?1",[new_team],|r|Ok((r.get(0)?,r.get(1)?))).unwrap(),("FAILED".into(),3));
        let before_sha=c.query_row::<String,_,_>("SELECT content_sha256 FROM entity_assets WHERE entity_type='TEAM' AND entity_id=?1",[t1],|r|r.get(0)).unwrap();
        let _ = scan(&c, &root).unwrap();
        let after_sha=c.query_row::<String,_,_>("SELECT content_sha256 FROM entity_assets WHERE entity_type='TEAM' AND entity_id=?1",[t1],|r|r.get(0)).unwrap();
        assert_eq!(before_sha, after_sha);

        let duplicate = teams::insert(&c, "Arsenal Alias", Some("England")).unwrap();
        scan(&c, &root).unwrap();
        ready(&c, &root, "TEAM", duplicate, &valid);
        let tx = c.transaction().unwrap();
        reconcile_team_assets(&tx, duplicate, t1).unwrap();
        tx.execute("DELETE FROM teams WHERE id=?1", [duplicate])
            .unwrap();
        tx.commit().unwrap();
        assert_eq!(c.query_row::<i64,_,_>("SELECT COUNT(*) FROM entity_assets WHERE entity_type='TEAM' AND entity_id=?1 AND status='READY'",[t1],|r|r.get(0)).unwrap(),1);
        assert_eq!(c.query_row::<i64,_,_>("SELECT COUNT(*) FROM entity_assets WHERE entity_type='COMPETITION' AND entity_id=?1",[comp1],|r|r.get(0)).unwrap(),1);
        println!("ASSET_INTEGRATION INITIAL teams=3 competitions=2 ready=2 corrupt=1 unavailable=1 FIRST_SCAN queued=2 FIRST_DOWNLOAD successful=2 failed=0 bytes={bytes} SECOND_SCAN queued=0 AFTER_NEW_TEAM newly_missing=1 existing_requeued=0 CORRUPT_RECOVERY requeued=1");
        let _ = new_comp;
    }
}
