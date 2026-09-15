use crate::{
    database::Database,
    providers::{football_data, iddaa},
    repositories::{
        calibration, current_flow, data_center, features, incremental_resolution, prediction_engine,
    },
};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

pub const BOOTSTRAP_VERSION: &str = "bootstrap-v3:arz-base-1.0.2";
pub const MODEL_VERSION: &str = "arz-base-1.0.2";
pub const CALIBRATION_VERSION: &str = "arz-base-1.0.2-cal1";
const REQUIRED_SEASONS: &[&str] = &["2425", "2526"];
use crate::repositories::production_scope::{self, MODEL_LEAGUES};
const STAGES: &[(&str, &str)] = &[
    ("database", "Veritabanı"),
    ("history", "Geçmiş veri"),
    ("model", "BASE model"),
    ("calibration", "Kalibrasyon"),
    ("features", "Özellikler"),
    ("iddaa", "Canlı İddaa"),
    ("resolution", "Eşleştirme"),
    ("predictions", "Tahminler"),
    ("candidates", "Adaylar"),
    ("coupons", "Kuponlar"),
    ("popularity", "Popülerler"),
    ("logos", "Logolar"),
    ("readiness", "Hazırlık"),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StageStatus {
    pub id: String,
    pub title: String,
    pub state: String,
    pub completed: usize,
    pub total: usize,
    pub duration_ms: Option<u128>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Status {
    pub state: String,
    pub version: String,
    pub current_stage: Option<String>,
    pub completed_stages: usize,
    pub total_stages: usize,
    pub stages: Vec<StageStatus>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub next_retry_at: Option<String>,
    pub attempts: u32,
    pub error: Option<String>,
    pub report: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Manifest {
    version: String,
    completed: BTreeSet<String>,
    reports: BTreeMap<String, Value>,
    durations_ms: BTreeMap<String, u128>,
    started_at: Option<String>,
    completed_at: Option<String>,
    attempts: u32,
    error: Option<String>,
}

static STATUS: OnceLock<Mutex<Status>> = OnceLock::new();

fn blank_status() -> Status {
    Status {
        state: "PENDING".into(),
        version: BOOTSTRAP_VERSION.into(),
        current_stage: None,
        completed_stages: 0,
        total_stages: STAGES.len(),
        stages: STAGES
            .iter()
            .map(|(id, title)| StageStatus {
                id: (*id).into(),
                title: (*title).into(),
                state: "PENDING".into(),
                completed: 0,
                total: if *id == "history" {
                    required_datasets().len()
                } else {
                    1
                },
                duration_ms: None,
                message: "Bekliyor".into(),
            })
            .collect(),
        started_at: None,
        completed_at: None,
        next_retry_at: None,
        attempts: 0,
        error: None,
        report: BTreeMap::new(),
    }
}

fn required_datasets() -> Vec<&'static football_data::DatasetDefinition> {
    football_data::all_datasets()
        .iter()
        .filter(|dataset| {
            REQUIRED_SEASONS.contains(&dataset.season_code)
                && MODEL_LEAGUES.contains(&dataset.league_code)
        })
        .collect()
}

fn historical_runtime(database: &Database) -> Result<data_center::RuntimeReadiness, String> {
    let datasets = required_datasets();
    let mut imported = 0;
    let mut cached = 0;
    let mut bytes = 0;
    for dataset in &datasets {
        let cache = database
            .path()
            .parent()
            .unwrap()
            .join("data")
            .join("football-data")
            .join(dataset.season_code)
            .join(format!("{}.csv", dataset.league_code));
        if cache.is_file() {
            cached += 1;
            bytes += fs::metadata(&cache).map(|value| value.len()).unwrap_or(0);
        }
        if dataset_complete(database, dataset)? {
            imported += 1;
        }
    }
    Ok(data_center::RuntimeReadiness {
        historical_dataset_count: datasets.len(),
        historical_cached_count: cached,
        historical_imported_count: imported,
        historical_missing_count: datasets.len().saturating_sub(imported),
        historical_failed_count: 0,
        historical_cached_bytes: bytes,
        asset_state: "BACKGROUND".into(),
        ..data_center::RuntimeReadiness::default()
    })
}

fn manifest_path(database_path: &Path) -> PathBuf {
    database_path.with_file_name("first-run-bootstrap.json")
}

fn read_manifest(path: &Path) -> Manifest {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .filter(|manifest: &Manifest| manifest.version == BOOTSTRAP_VERSION)
        .unwrap_or_else(|| Manifest {
            version: BOOTSTRAP_VERSION.into(),
            ..Manifest::default()
        })
}

fn write_manifest(path: &Path, manifest: &Manifest) -> Result<(), String> {
    let temporary = path.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn project(manifest: &Manifest, current: Option<&str>, retry_at: Option<String>) -> Status {
    let mut status = blank_status();
    status.state = if manifest.completed_at.is_some() {
        "COMPLETED"
    } else if retry_at.is_some() {
        "RETRY_WAIT"
    } else {
        "RUNNING"
    }
    .into();
    status.current_stage = current.map(str::to_owned);
    status.completed_stages = manifest.completed.len();
    status.started_at = manifest.started_at.clone();
    status.completed_at = manifest.completed_at.clone();
    status.next_retry_at = retry_at.clone();
    status.attempts = manifest.attempts;
    status.error = manifest.error.clone();
    status.report = manifest.reports.clone();
    for stage in &mut status.stages {
        if manifest.completed.contains(&stage.id) {
            stage.state = "READY".into();
            stage.completed = stage.total;
            stage.message = "Hazır".into();
            stage.duration_ms = manifest.durations_ms.get(&stage.id).copied();
        } else if current == Some(stage.id.as_str()) {
            stage.state = "RUNNING".into();
            stage.message = "Hazırlanıyor".into();
        } else if manifest.error.is_some() && retry_at.is_some() {
            stage.message = "Yeniden denenecek".into();
        }
    }
    if let Some(progress) = manifest.reports.get("history") {
        if let Some(stage) = status.stages.iter_mut().find(|stage| stage.id == "history") {
            stage.completed = progress["completed"].as_u64().unwrap_or(0) as usize;
            stage.total = progress["total"].as_u64().unwrap_or(stage.total as u64) as usize;
        }
    }
    status
}

fn publish(manifest: &Manifest, current: Option<&str>, retry_at: Option<String>) {
    let value = project(manifest, current, retry_at);
    if let Some(control) = STATUS.get() {
        if let Ok(mut status) = control.lock() {
            *status = value;
        }
    }
}

pub fn status() -> Status {
    STATUS
        .get()
        .and_then(|status| status.lock().ok().map(|status| status.clone()))
        .unwrap_or_else(blank_status)
}

pub fn daily_refresh_allowed() -> bool {
    status().state == "COMPLETED"
}

fn dataset_complete(
    database: &Database,
    dataset: &football_data::DatasetDefinition,
) -> Result<bool, String> {
    let connection = database.connection()?;
    let imported: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM data_import_runs WHERE provider='football-data.co.uk' AND dataset_key=?1 AND status='completed' AND rows_seen>0)",
            [dataset.key()],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let cache = database
        .path()
        .parent()
        .unwrap()
        .join("data")
        .join("football-data")
        .join(dataset.season_code)
        .join(format!("{}.csv", dataset.league_code));
    let owned: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM matches m JOIN provider_competition_mappings p ON p.competition_id=m.competition_id AND p.provider='football-data.co.uk' WHERE p.external_competition_id=?1 AND m.season=?2 AND m.status='finished' AND EXISTS(SELECT 1 FROM provider_match_mappings h WHERE h.match_id=m.id AND h.provider='football-data.co.uk'))", rusqlite::params![dataset.league_code,dataset.season], |r| r.get(0)).map_err(|e| e.to_string())?;
    Ok(imported && cache.is_file() && owned)
}

fn copy_validated(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination.parent().ok_or("ARTIFACT_DESTINATION_INVALID")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = destination.with_extension("json.tmp");
    fs::copy(source, &temporary).map_err(|error| error.to_string())?;
    if destination.exists() {
        fs::remove_file(destination).map_err(|error| error.to_string())?;
    }
    fs::rename(temporary, destination).map_err(|error| error.to_string())
}

fn install_artifacts(database: &Database, resources: &Path) -> Result<Value, String> {
    let source_model = resources.join("base-model.json");
    let source_calibration = resources.join("calibration.json");
    let source_model_artifact = prediction_engine::load_artifact(&source_model)?;
    let source_calibration_artifact = calibration::load_calibration(&source_calibration)?;
    if source_model_artifact.bundle.model_version != MODEL_VERSION
        || source_calibration_artifact.calibration_version != CALIBRATION_VERSION
    {
        return Err("RELEASE_ARTIFACT_VERSION_MISMATCH".into());
    }
    let destination = database
        .path()
        .parent()
        .unwrap()
        .join("production-artifacts")
        .join(MODEL_VERSION);
    let model_path = destination.join("base-model.json");
    let calibration_path = destination.join("calibration.json");
    let needs_model_copy = match prediction_engine::load_artifact(&model_path) {
        Ok(artifact) => artifact.artifact_sha256 != source_model_artifact.artifact_sha256,
        Err(_) => true,
    };
    if needs_model_copy {
        copy_validated(&source_model, &model_path)?;
    }
    let needs_calibration_copy = match calibration::load_calibration(&calibration_path) {
        Ok(artifact) => artifact.artifact_sha256 != source_calibration_artifact.artifact_sha256,
        Err(_) => true,
    };
    if needs_calibration_copy {
        copy_validated(&source_calibration, &calibration_path)?;
    }
    let connection = database.connection()?;
    let existing: Option<(String, String)> = connection
        .query_row(
            "SELECT artifact_path,COALESCE(artifact_sha256,'') FROM model_versions WHERE version_identifier=?1",
            [MODEL_VERSION],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some((_, hash)) = existing {
        if hash != source_model_artifact.artifact_sha256 {
            return Err("MODEL_VERSION_PAYLOAD_CONFLICT".into());
        }
        connection
            .execute(
                "UPDATE model_versions SET artifact_path=?2 WHERE version_identifier=?1",
                rusqlite::params![MODEL_VERSION, model_path.to_string_lossy()],
            )
            .map_err(|error| error.to_string())?;
    } else {
        prediction_engine::register(&connection, &model_path)?;
    }
    prediction_engine::activate(&connection, MODEL_VERSION)?;
    calibration::register_artifact(&connection, &calibration_path, &source_model_artifact)?;
    calibration::activate_calibration(&connection, CALIBRATION_VERSION)?;
    Ok(json!({
        "source": "BUNDLED_INSTALLER_RESOURCE",
        "model_version": MODEL_VERSION,
        "model_sha256": source_model_artifact.artifact_sha256,
        "calibration_version": CALIBRATION_VERSION,
        "calibration_sha256": source_calibration_artifact.artifact_sha256,
        "model_reused": !needs_model_copy,
        "calibration_reused": !needs_calibration_copy,
    }))
}

fn current_fixture_features(database: &Database) -> Result<Value, String> {
    let connection = database.connection()?;
    let now = crate::business_clock::now().to_rfc3339();
    let mut query = connection.prepare("SELECT id FROM matches WHERE status='scheduled' AND kickoff_at>?1 AND EXISTS(SELECT 1 FROM provider_match_mappings p WHERE p.match_id=matches.id AND p.provider='iddaa') AND EXISTS(SELECT 1 FROM provider_competition_mappings p WHERE p.competition_id=matches.competition_id AND p.provider='football-data.co.uk') ORDER BY kickoff_at").map_err(|error| error.to_string())?;
    let ids = query
        .query_map([now], |row| row.get::<_, i64>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(query);
    let mut generated = 0;
    for id in ids {
        let snapshot = features::generate(&connection, id)?;
        generated += usize::from(features::persist(&connection, &snapshot)?);
    }
    Ok(json!({"generated": generated}))
}

fn stage(database: &Database, resources: &Path, id: &str) -> Result<Value, String> {
    match id {
        "database" => {
            database
                .connection()?
                .query_row("SELECT 1", [], |_| Ok(()))
                .map_err(|error| error.to_string())?;
            Ok(json!({"migrations": "current"}))
        }
        "history" => unreachable!("history has per-dataset persistence"),
        "model" | "calibration" => install_artifacts(database, resources),
        "features" => current_fixture_features(database),
        "iddaa" => tauri::async_runtime::block_on(iddaa::refresh_bulletin(database))
            .map(|summary| json!(summary)),
        "resolution" => {
            let connection = database.connection()?;
            incremental_resolution::process(&connection, 128)
                .map(|summary| json!(summary))
                .map_err(|error| error.to_string())
        }
        "predictions" => {
            let connection = database.connection()?;
            current_flow::run_with_feature_refresh(&connection, true).map(|summary| json!(summary))
        }
        "candidates" => {
            let connection = database.connection()?;
            let count: i64 = connection.query_row("SELECT COUNT(*) FROM candidate_engine_candidates WHERE run_id=(SELECT MAX(id) FROM candidate_engine_runs)", [], |row| row.get(0)).unwrap_or(0);
            Ok(json!({"count": count}))
        }
        "coupons" => {
            let connection = database.connection()?;
            let count: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM phase8_coupons WHERE business_date=?1",
                    [crate::business_clock::date()],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            Ok(json!({"count": count}))
        }
        "popularity" => tauri::async_runtime::block_on(iddaa::refresh_popularity(database))
            .map(|summary| json!(summary)),
        "logos" => Ok(json!({"state": "BACKGROUND_QUEUE_STARTED"})),
        "readiness" => {
            let runtime = historical_runtime(database)?;
            let connection = database.connection()?;
            let counts: (i64, i64, i64) = connection.query_row("SELECT (SELECT COUNT(*) FROM predictions),(SELECT COUNT(*) FROM candidate_engine_candidates),(SELECT COUNT(*) FROM phase8_coupons)", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).map_err(|error| error.to_string())?;
            let readiness = data_center::status(&connection, &runtime, chrono::Utc::now())?;
            if readiness.core_ready_count != readiness.core_check_count {
                let failures = readiness
                    .core_checks
                    .iter()
                    .filter(|check| check.status != data_center::ReadinessState::Ready)
                    .map(|check| format!("{}: {}", check.id, check.reasons.join(", ")))
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(format!(
                    "READINESS_NOT_COMPLETE: {}/{} ({failures})",
                    readiness.core_ready_count, readiness.core_check_count
                ));
            }
            Ok(
                json!({"predictions": counts.0, "candidates": counts.1, "coupons": counts.2, "core_ready": readiness.core_ready_count, "core_total": readiness.core_check_count}),
            )
        }
        _ => Err(format!("UNKNOWN_BOOTSTRAP_STAGE: {id}")),
    }
}

fn run_attempt(
    database: &Database,
    resources: &Path,
    manifest_path: &Path,
    manifest: &mut Manifest,
) -> Result<(), String> {
    {
        let c = database.connection()?;
        production_scope::repair(&c).map_err(|e| e.to_string())?;
    }
    if manifest.started_at.is_none() {
        manifest.started_at = Some(chrono::Utc::now().to_rfc3339());
    }
    manifest.attempts += 1;
    manifest.error = None;
    write_manifest(manifest_path, manifest)?;
    for (id, _) in STAGES {
        if manifest.completed.contains(*id) {
            continue;
        }
        publish(manifest, Some(id), None);
        let started = Instant::now();
        let report = if *id == "history" {
            let datasets = required_datasets();
            let mut completed = 0;
            let mut rows = 0;
            for dataset in &datasets {
                if dataset_complete(database, dataset)? {
                    completed += 1;
                    continue;
                }
                let imported = match football_data::import_cached_dataset(database, dataset) {
                    Ok(imported) => imported,
                    Err(_) => tauri::async_runtime::block_on(football_data::refresh_results(
                        database, dataset,
                    ))?,
                };
                completed += 1;
                rows += imported.rows_seen;
                manifest.reports.insert("history".into(), json!({"completed": completed, "total": datasets.len(), "rows_seen_this_attempt": rows}));
                write_manifest(manifest_path, manifest)?;
                publish(manifest, Some(id), None);
            }
            json!({"completed": completed, "total": datasets.len(), "rows_seen_this_attempt": rows})
        } else {
            stage(database, resources, id)?
        };
        manifest.reports.insert((*id).into(), report);
        manifest
            .durations_ms
            .insert((*id).into(), started.elapsed().as_millis());
        manifest.completed.insert((*id).into());
        write_manifest(manifest_path, manifest)?;
    }
    manifest.completed_at = Some(chrono::Utc::now().to_rfc3339());
    manifest.error = None;
    write_manifest(manifest_path, manifest)?;
    publish(manifest, None, None);
    Ok(())
}

pub fn start(database_path: PathBuf, resources: PathBuf, app: AppHandle) {
    let path = manifest_path(&database_path);
    let mut initial = read_manifest(&path);
    let scope_ready = Database::open(database_path.clone())
        .ok()
        .and_then(|db| {
            db.connection()
                .ok()
                .and_then(|c| production_scope::status(&c).ok())
        })
        .is_some_and(|scope| scope.ready());
    if !scope_ready {
        initial.completed_at = None;
        initial
            .completed
            .retain(|id| matches!(id.as_str(), "database" | "model" | "calibration"));
    }
    let _ = STATUS.set(Mutex::new(project(&initial, None, None)));
    if initial.completed_at.is_some() {
        return;
    }
    std::thread::spawn(move || {
        let mut manifest = initial;
        let mut failures = 0_u32;
        loop {
            let database = match Database::open(database_path.clone()) {
                Ok(database) => database,
                Err(error) => {
                    manifest.error = Some(error.to_string());
                    publish(&manifest, None, None);
                    return;
                }
            };
            match run_attempt(&database, &resources, &path, &mut manifest) {
                Ok(()) => {
                    app.state::<crate::assets::AssetSyncManager>().start();
                    let _ = crate::automatic_refresh::request(false);
                    let _ = app.emit("first-run-bootstrap-updated", ());
                    return;
                }
                Err(error) => {
                    failures += 1;
                    manifest.error = Some(error);
                    let delay = 5_u64.saturating_mul(2_u64.pow(failures.min(6))).min(300);
                    let retry_at = chrono::Utc::now() + chrono::Duration::seconds(delay as i64);
                    let _ = write_manifest(&path, &manifest);
                    publish(&manifest, None, Some(retry_at.to_rfc3339()));
                    let _ = app.emit("first-run-bootstrap-updated", ());
                    std::thread::sleep(Duration::from_secs(delay));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_app_data_is_detected_as_pending() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = read_manifest(&dir.path().join("missing.json"));
        assert!(manifest.completed.is_empty());
        assert_eq!(project(&manifest, None, None).state, "RUNNING");
    }

    #[test]
    fn state_machine_resumes_completed_stages_and_completes_once() {
        let mut manifest = Manifest {
            version: BOOTSTRAP_VERSION.into(),
            ..Manifest::default()
        };
        manifest.completed.insert("database".into());
        manifest.completed.insert("history".into());
        let resumed = project(&manifest, Some("model"), None);
        assert_eq!(resumed.completed_stages, 2);
        assert_eq!(resumed.current_stage.as_deref(), Some("model"));
        manifest
            .completed
            .extend(STAGES.iter().map(|stage| stage.0.to_string()));
        manifest.completed_at = Some("2026-09-14T00:00:00Z".into());
        assert_eq!(project(&manifest, None, None).state, "COMPLETED");
        assert_eq!(manifest.completed.len(), STAGES.len());
    }

    #[test]
    fn version_change_starts_an_incremental_upgrade_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        fs::write(&path, br#"{"version":"old","completed":["database"],"reports":{},"durations_ms":{},"attempts":1}"#).unwrap();
        assert!(read_manifest(&path).completed.is_empty());
    }

    #[test]
    fn required_history_is_stable_and_excludes_unpublished_catalog_items() {
        let datasets = required_datasets();
        assert_eq!(datasets.len(), 20);
        assert!(datasets
            .iter()
            .all(|dataset| REQUIRED_SEASONS.contains(&dataset.season_code)));
    }

    #[test]
    fn bundled_artifacts_install_validate_and_reuse_on_restart() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("fresh.sqlite3")).unwrap();
        let resources = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/production");
        let first = install_artifacts(&db, &resources).unwrap();
        let second = install_artifacts(&db, &resources).unwrap();
        assert_eq!(first["model_version"], MODEL_VERSION);
        assert_eq!(first["calibration_version"], CALIBRATION_VERSION);
        assert_eq!(second["model_reused"], true);
        assert_eq!(second["calibration_reused"], true);
        assert_eq!(
            prediction_engine::active(&db.connection().unwrap())
                .unwrap()
                .1
                .bundle
                .model_version,
            MODEL_VERSION
        );
    }

    #[test]
    #[ignore = "desktop acceptance audit; set ARZ_ACCEPTANCE_APP_DATA_DIR"]
    fn desktop_acceptance_audit() {
        let app_data = std::env::var("ARZ_ACCEPTANCE_APP_DATA_DIR").unwrap();
        let database =
            Database::open(Path::new(&app_data).join("football-predictor.sqlite3")).unwrap();
        let runtime = historical_runtime(&database).unwrap();
        let connection = database.connection().unwrap();
        let mut query = connection
            .prepare("SELECT h.normalized_name,a.normalized_name,q.evidence_json FROM daily_resolution_queue q JOIN matches m ON m.id=q.match_id JOIN teams h ON h.id=m.home_team_id JOIN teams a ON a.id=m.away_team_id WHERE q.status IN ('NEW','AUTO_RESOLVING','RETRY_LATER','AMBIGUOUS','MANUAL_REVIEW') AND m.status='scheduled' ORDER BY m.kickoff_at")
            .unwrap();
        let unresolved = query
            .query_map([], |row| {
                Ok(json!({
                    "home": row.get::<_, String>(0)?,
                    "away": row.get::<_, String>(1)?,
                    "evidence": row.get::<_, String>(2)?,
                }))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        drop(query);
        let readiness = data_center::status(&connection, &runtime, chrono::Utc::now()).unwrap();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "unresolved": unresolved,
                "core_ready": readiness.core_ready_count,
                "core_total": readiness.core_check_count,
                "checks": readiness.core_checks,
            }))
            .unwrap()
        );
    }
}
