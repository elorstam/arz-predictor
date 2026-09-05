use serde::Deserialize;
use tauri::{AppHandle, Manager, State};

use crate::{
    database::Database,
    licensing::{
        device_fingerprint_hash, verifying_key_from_spki_b64, DesktopLicenseState, LicenseConfig,
        LicenseManager, LicenseSnapshot, SupabaseLicenseProvider, PUBLIC_VERIFYING_KEY_B64,
    },
    models::{DatabaseHealth, DatabaseStats},
    repositories,
};

#[derive(Debug, serde::Serialize)]
pub struct LicenseStatusDto {
    pub state: String,
    pub plan: Option<String>,
    pub license_id: Option<String>,
    pub activated_at: Option<String>,
    pub expires_at: Option<String>,
    pub last_verified_at: Option<String>,
    pub device_bound: bool,
    pub offline_grace_until: Option<String>,
    pub message: String,
}

fn license_file(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(license_root(app)?.join("activation.json"))
}

fn license_root(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|error| error.to_string())
}

fn license_manager() -> Result<LicenseManager<SupabaseLicenseProvider>, String> {
    let key = verifying_key_from_spki_b64(PUBLIC_VERIFYING_KEY_B64)
        .map_err(|_| "Lisans servisi public yapılandırması geçersiz.".to_owned())?;
    Ok(LicenseManager::new(
        SupabaseLicenseProvider::production(),
        key,
        LicenseConfig::default(),
    ))
}

fn snapshot_to_dto(snapshot: LicenseSnapshot) -> LicenseStatusDto {
    let claims = snapshot.claims;
    LicenseStatusDto {
        state: format!("{:?}", snapshot.state),
        plan: claims.as_ref().map(|value| format!("{:?}", value.plan)),
        license_id: claims.as_ref().map(|value| value.license_id.clone()),
        activated_at: snapshot.activated_at.map(|value| value.to_rfc3339()),
        expires_at: claims
            .as_ref()
            .and_then(|value| value.license_expires_at)
            .map(|value| value.to_rfc3339()),
        last_verified_at: snapshot.last_verified_at.map(|value| value.to_rfc3339()),
        device_bound: claims.is_some(),
        offline_grace_until: claims
            .as_ref()
            .map(|value| value.offline_grace_until.to_rfc3339()),
        message: snapshot.message,
    }
}

fn configuration_status(message: impl Into<String>) -> LicenseStatusDto {
    LicenseStatusDto {
        state: format!("{:?}", DesktopLicenseState::SERVER_UNAVAILABLE),
        plan: None,
        license_id: None,
        activated_at: None,
        expires_at: None,
        last_verified_at: None,
        device_bound: false,
        offline_grace_until: None,
        message: message.into(),
    }
}

#[tauri::command]
pub fn license_status(app: AppHandle) -> Result<LicenseStatusDto, String> {
    let path = license_file(&app)?;
    if !path.exists() {
        return Ok(LicenseStatusDto {
            state: "UNLICENSED".into(),
            plan: None,
            license_id: None,
            activated_at: None,
            expires_at: None,
            last_verified_at: None,
            device_bound: false,
            offline_grace_until: None,
            message: "Bir lisans anahtarıyla aktivasyon yapın.".into(),
        });
    }
    if PUBLIC_VERIFYING_KEY_B64.is_empty() {
        return Ok(configuration_status(
            "Lisans doğrulama anahtarı yapılandırılmadı.",
        ));
    }
    let device_hash = device_fingerprint_hash(&license_root(&app)?).map_err(|e| e.to_string())?;
    let snapshot = license_manager()?.status(&path, &device_hash, chrono::Utc::now(), false);
    Ok(snapshot_to_dto(snapshot))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseActivateRequest {
    pub license_key: String,
}

#[tauri::command]
pub fn license_activate(
    app: AppHandle,
    request: LicenseActivateRequest,
) -> Result<LicenseStatusDto, String> {
    let root = license_root(&app)?;
    let path = license_file(&app)?;
    let device_hash = device_fingerprint_hash(&root).map_err(|error| error.to_string())?;
    let snapshot = license_manager()?
        .activate(
            &path,
            &request.license_key,
            &device_hash,
            chrono::Utc::now(),
        )
        .map_err(|error| error.to_string())?;
    Ok(snapshot_to_dto(snapshot))
}

#[tauri::command]
pub fn license_verify(app: AppHandle) -> Result<LicenseStatusDto, String> {
    let path = license_file(&app)?;
    if !path.exists() {
        return license_status(app);
    }
    if PUBLIC_VERIFYING_KEY_B64.is_empty() {
        return Ok(configuration_status(
            "Lisans doğrulama anahtarı yapılandırılmadı.",
        ));
    }
    let device_hash = device_fingerprint_hash(&license_root(&app)?).map_err(|e| e.to_string())?;
    let snapshot = license_manager()?.status(&path, &device_hash, chrono::Utc::now(), true);
    Ok(snapshot_to_dto(snapshot))
}

#[tauri::command]
pub fn license_clear_local_token(app: AppHandle) -> Result<(), String> {
    let path = license_file(&app)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FootballDataDatasetRequest {
    league_code: String,
    season_code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FootballDataLocalDatasetRequest {
    league_code: String,
    season_code: String,
    file_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FootballDataBootstrapStatusRequest {
    preset: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IddaaLocalBulletinRequest {
    file_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IddaaLatestOddsRequest {
    match_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataCenterStatusRequest {
    as_of: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResolverCandidateRequest {
    id: i64,
}
#[derive(Debug, Deserialize)]
pub struct ResolverLinkRequest {
    mapping_id: i64,
    team_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityLogoPathRequest {
    entity_type: String,
    entity_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureMatchRequest {
    match_id: i64,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureDatasetRequest {
    competition_id: Option<i64>,
    season: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PredictionTrainRequest {
    model_version: String,
    output_artifact_directory: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PredictionArtifactRequest {
    artifact_path: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PredictionActivateRequest {
    model_version: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PredictionUpcomingRequest {
    competition_id: Option<i64>,
    date_from: Option<String>,
    date_to: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestRequest {
    model_version: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationFitRequest {
    model_artifact_path: String,
    output_calibration_path: String,
    calibration_version: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationActivateRequest {
    calibration_version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceRequest {
    #[serde(flatten)]
    filter: crate::repositories::performance::PerformanceFilter,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateGenerateRequest {
    pub business_date: Option<String>,
    pub generation_time: Option<String>,
    pub category: Option<String>,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateGetRequest {
    pub business_date: String,
    pub run_id: Option<i64>,
    pub category: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateLineupRevisionRequest {
    pub base_run_id: i64,
    pub business_date: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CouponLineupImpactRequest {
    pub coupon_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CouponGenerateRequest {
    pub business_date: String,
    pub candidate_run_id: Option<i64>,
    pub coupon_type: Option<String>,
    pub unit_stake_cents: Option<i64>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CouponDraftRequest {
    pub business_date: String,
    pub candidate_run_id: i64,
    pub coupon_type: String,
    pub candidate_ids: Vec<i64>,
    pub unit_stake_cents: Option<i64>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CouponUpdateRequest {
    pub coupon_id: i64,
    pub candidate_ids: Vec<i64>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CouponIdRequest {
    pub coupon_id: i64,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPreviewRequest {
    pub candidate_ids: Vec<i64>,
    pub system_sizes: Vec<usize>,
    pub unit_stake_cents: i64,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CouponDailyRequest {
    pub business_date: String,
    pub coupon_type: Option<String>,
    pub candidate_run_id: Option<i64>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesStartRequest {
    pub business_date: String,
    pub starting_stake_cents: i64,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesSettleRequest {
    pub series_id: i64,
    pub won: bool,
    pub gross_return_cents: i64,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CouponSettlementRequest {
    pub coupon_id: i64,
    pub outcomes: Vec<crate::repositories::coupon_engine::SelectionOutcome>,
    pub settled_at: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesIdRequest {
    pub series_id: i64,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineupMatchRequest {
    pub match_id: i64,
    pub now: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineupImportRequest {
    pub file_path: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineupFeatureRequest {
    pub match_id: i64,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineupModelTrainRequest {
    pub dataset_path: String,
    pub output_artifact_path: String,
    pub model_version: String,
    pub parent_model_version: Option<String>,
    pub parent_model_hash: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineupModelArtifactRequest {
    pub artifact_path: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineupModelActivateRequest {
    pub model_version: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineupRevisionRequest {
    pub prediction_id: i64,
    pub feature_set_id: i64,
}

#[tauri::command]
pub fn prediction_backtest_run(
    database: State<'_, Database>,
    request: BacktestRequest,
) -> Result<crate::repositories::calibration::BacktestReport, String> {
    let c = database.connection()?;
    let mut report = crate::repositories::calibration::walk_forward(&c, &request.model_version)?;
    report.run_id = Some(crate::repositories::calibration::persist_backtest(
        &c, &report,
    )?);
    Ok(report)
}

#[tauri::command]
pub fn prediction_backtest_summary(
    database: State<'_, Database>,
    request: BacktestRequest,
) -> Result<crate::repositories::calibration::BacktestReport, String> {
    let c = database.connection()?;
    crate::repositories::calibration::walk_forward(&c, &request.model_version)
}

#[tauri::command]
pub fn prediction_backtest_status(
    database: State<'_, Database>,
) -> Result<Vec<(i64, String, String, String)>, String> {
    let c = database.connection()?;
    let mut q = c.prepare("SELECT id,model_family_version,status,COALESCE(result_hash,'') FROM backtest_runs ORDER BY id DESC").map_err(|e| e.to_string())?;
    let mapped = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(mapped)
}

#[tauri::command]
pub fn prediction_calibration_fit(
    database: State<'_, Database>,
    request: CalibrationFitRequest,
) -> Result<crate::repositories::calibration::CalibrationReport, String> {
    let c = database.connection()?;
    let base = crate::repositories::prediction_engine::load_artifact(std::path::Path::new(
        &request.model_artifact_path,
    ))?;
    let report = crate::repositories::calibration::walk_forward(&c, &base.bundle.model_version)?;
    let run_id: i64 = c.query_row(
        "SELECT id FROM backtest_runs WHERE model_family_version=?1 AND result_hash=?2 ORDER BY id DESC LIMIT 1",
        rusqlite::params![base.bundle.model_version, report.result_hash],
        |r| r.get(0),
    ).or_else(|_| crate::repositories::calibration::persist_backtest(&c, &report)).map_err(|e| e.to_string())?;
    let fitted = crate::repositories::calibration::fit_calibration_version(
        &base,
        &report,
        std::path::Path::new(&request.output_calibration_path),
        request.calibration_version.as_deref(),
    )?;
    crate::repositories::calibration::persist_calibration_for_run(&c, &fitted, &base, run_id)?;
    Ok(fitted)
}

#[tauri::command]
pub fn prediction_calibration_validate(request: PredictionArtifactRequest) -> Result<bool, String> {
    crate::repositories::calibration::load_calibration(std::path::Path::new(&request.artifact_path))
        .map(|_| true)
}

#[tauri::command]
pub fn prediction_calibration_activate(
    database: State<'_, Database>,
    request: CalibrationActivateRequest,
) -> Result<(), String> {
    let c = database.connection()?;
    crate::repositories::calibration::activate_calibration(&c, &request.calibration_version)
}

#[tauri::command]
pub fn prediction_calibration_buckets(
    database: State<'_, Database>,
    request: PerformanceRequest,
) -> Result<crate::repositories::performance::PerformanceReport, String> {
    let c = database.connection()?;
    Ok(crate::repositories::performance::prediction_model_performance(&c, &request.filter)?)
}

#[tauri::command]
pub fn prediction_model_performance(
    database: State<'_, Database>,
    request: PerformanceRequest,
) -> Result<crate::repositories::performance::PerformanceReport, String> {
    let c = database.connection()?;
    crate::repositories::performance::prediction_model_performance(&c, &request.filter)
}

#[tauri::command]
pub fn prediction_odds_band_performance(
    database: State<'_, Database>,
    request: PerformanceRequest,
) -> Result<crate::repositories::performance::OddsBandReport, String> {
    let c = database.connection()?;
    crate::repositories::performance::prediction_odds_band_performance(&c, &request.filter)
}

#[tauri::command]
pub fn candidate_engine_generate_daily(
    database: State<'_, Database>,
    request: CandidateGenerateRequest,
) -> Result<crate::repositories::candidate_engine::DailyRun, String> {
    let c = database.connection()?;
    crate::repositories::candidate_engine::generate(
        &c,
        &crate::repositories::candidate_engine::GenerateRequest {
            business_date: request.business_date,
            generation_time: request.generation_time,
            category: request.category,
            dry_run: request.dry_run,
        },
    )
}

#[tauri::command]
pub fn candidate_engine_get_daily(
    database: State<'_, Database>,
    request: CandidateGetRequest,
) -> Result<crate::repositories::candidate_engine::DailyRun, String> {
    let c = database.connection()?;
    crate::repositories::candidate_engine::get(
        &c,
        &crate::repositories::candidate_engine::GetRequest {
            business_date: request.business_date,
            run_id: request.run_id,
            category: request.category,
        },
    )
}

#[tauri::command]
pub fn candidate_engine_generate_lineup_revision(
    database: State<'_, Database>,
    request: CandidateLineupRevisionRequest,
) -> Result<crate::repositories::candidate_engine::DailyRun, String> {
    let c = database.connection()?;
    crate::repositories::candidate_engine::generate_lineup_revision(
        &c,
        &crate::repositories::candidate_engine::LineupRevisionGenerateRequest {
            base_run_id: request.base_run_id,
            business_date: request.business_date,
        },
    )
}

#[tauri::command]
pub fn candidate_engine_get_lineup_revision(
    database: State<'_, Database>,
    request: CandidateGetRequest,
) -> Result<crate::repositories::candidate_engine::DailyRun, String> {
    candidate_engine_get_daily(database, request)
}

#[tauri::command]
pub fn candidate_engine_status(
    database: State<'_, Database>,
) -> Result<crate::repositories::candidate_engine::Status, String> {
    let c = database.connection()?;
    crate::repositories::candidate_engine::status(&c)
}

#[tauri::command]
pub fn candidate_engine_policy() -> crate::repositories::candidate_engine::CandidatePolicy {
    crate::repositories::candidate_engine::policy()
}

#[tauri::command]
pub fn model_supported_populars_get(
    database: State<'_, Database>,
    request: crate::repositories::model_supported_populars::GetRequest,
) -> Result<crate::repositories::model_supported_populars::ModelSupportedPopularResponse, String> {
    let connection = database.connection()?;
    crate::repositories::model_supported_populars::get(&connection, &request)
}

#[tauri::command]
pub fn model_performance_get(
    database: State<'_, Database>,
    request: crate::repositories::model_performance::ModelPerformanceRequest,
) -> Result<crate::repositories::model_performance::ModelPerformanceResponse, String> {
    let connection = database.connection()?;
    crate::repositories::model_performance::get(&connection, &request)
}

#[tauri::command]
pub fn coupon_performance_get(
    database: State<'_, Database>,
    request: crate::repositories::coupon_performance::CouponPerformanceRequest,
) -> Result<crate::repositories::coupon_performance::CouponPerformanceResponse, String> {
    let connection = database.connection()?;
    crate::repositories::coupon_performance::get(&connection, &request)
}

#[tauri::command]
pub fn coupon_engine_generate_daily(
    database: State<'_, Database>,
    request: CouponGenerateRequest,
) -> Result<Vec<crate::repositories::coupon_engine::DailyCouponResult>, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::generate_daily_report(
        &c,
        &crate::repositories::coupon_engine::GenerateRequest {
            business_date: request.business_date,
            candidate_run_id: request.candidate_run_id,
            coupon_type: request.coupon_type,
            unit_stake_cents: request.unit_stake_cents,
        },
    )
}
#[tauri::command]
pub fn coupon_engine_create_draft(
    database: State<'_, Database>,
    request: CouponDraftRequest,
) -> Result<crate::repositories::coupon_engine::Coupon, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::create_draft(
        &c,
        &crate::repositories::coupon_engine::DraftRequest {
            business_date: request.business_date,
            candidate_run_id: request.candidate_run_id,
            coupon_type: request.coupon_type,
            candidate_ids: request.candidate_ids,
            unit_stake_cents: request.unit_stake_cents,
        },
    )
}
#[tauri::command]
pub fn coupon_engine_update_draft_selections(
    database: State<'_, Database>,
    request: CouponUpdateRequest,
) -> Result<crate::repositories::coupon_engine::Coupon, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::update_draft(
        &c,
        &crate::repositories::coupon_engine::UpdateRequest {
            coupon_id: request.coupon_id,
            candidate_ids: request.candidate_ids,
        },
    )
}
#[tauri::command]
pub fn coupon_engine_finalize(
    database: State<'_, Database>,
    request: CouponIdRequest,
) -> Result<crate::repositories::coupon_engine::Coupon, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::finalize(&c, request.coupon_id)
}
#[tauri::command]
pub fn coupon_engine_get_coupon(
    database: State<'_, Database>,
    request: CouponIdRequest,
) -> Result<crate::repositories::coupon_engine::Coupon, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::get_coupon(&c, request.coupon_id)
}

#[tauri::command]
pub fn coupon_engine_lineup_revision_impact(
    database: State<'_, Database>,
    request: CouponLineupImpactRequest,
) -> Result<crate::repositories::coupon_engine::LineupRevisionImpact, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::lineup_revision_impact(
        &c,
        &crate::repositories::coupon_engine::LineupRevisionImpactRequest {
            coupon_id: request.coupon_id,
        },
    )
}
#[tauri::command]
pub fn coupon_engine_system_preview(
    request: SystemPreviewRequest,
    database: State<'_, Database>,
) -> Result<crate::repositories::coupon_engine::SystemPreview, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::system_preview(
        &c,
        &crate::repositories::coupon_engine::SystemPreviewRequest {
            candidate_ids: request.candidate_ids,
            system_sizes: request.system_sizes,
            unit_stake_cents: request.unit_stake_cents,
        },
    )
}
#[tauri::command]
pub fn coupon_engine_get_daily(
    database: State<'_, Database>,
    request: CouponDailyRequest,
) -> Result<Vec<crate::repositories::coupon_engine::Coupon>, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::get_daily(
        &c,
        &request.business_date,
        request.coupon_type.as_deref(),
    )
}
#[tauri::command]
pub fn compound_series_start(
    database: State<'_, Database>,
    request: SeriesStartRequest,
) -> Result<crate::repositories::coupon_engine::CompoundSeries, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::start_series(
        &c,
        &request.business_date,
        request.starting_stake_cents,
    )
}
#[tauri::command]
pub fn compound_series_status(
    database: State<'_, Database>,
) -> Result<Option<crate::repositories::coupon_engine::CompoundSeries>, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::series_status(&c)
}
#[tauri::command]
pub fn compound_series_settle_step(
    database: State<'_, Database>,
    request: SeriesSettleRequest,
) -> Result<crate::repositories::coupon_engine::CompoundSeries, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::settle_series(
        &c,
        request.series_id,
        request.won,
        request.gross_return_cents,
    )
}
#[tauri::command]
pub fn coupon_engine_settle(
    database: State<'_, Database>,
    request: CouponSettlementRequest,
) -> Result<crate::repositories::coupon_engine::SettlementResult, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::settle(
        &c,
        &crate::repositories::coupon_engine::SettlementRequest {
            coupon_id: request.coupon_id,
            outcomes: request.outcomes,
            settled_at: request.settled_at,
        },
    )
}
#[tauri::command]
pub fn compound_series_generate_step(
    database: State<'_, Database>,
    request: CouponDailyRequest,
) -> Result<crate::repositories::coupon_engine::Coupon, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::generate_compound_step(
        &c,
        &request.business_date,
        request.candidate_run_id,
    )
}
#[tauri::command]
pub fn compound_series_cancel(
    database: State<'_, Database>,
    request: SeriesIdRequest,
) -> Result<crate::repositories::coupon_engine::CompoundSeries, String> {
    let c = database.connection()?;
    crate::repositories::coupon_engine::cancel_series(&c, request.series_id)
}

#[tauri::command]
pub fn lineup_import_local(
    database: State<'_, Database>,
    request: LineupImportRequest,
) -> Result<crate::repositories::lineup::LineupImportResult, String> {
    let json = std::fs::read_to_string(&request.file_path).map_err(|e| e.to_string())?;
    let payload: crate::repositories::lineup::LocalLineupPayload =
        serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let c = database.connection()?;
    crate::repositories::lineup::import_local(&c, &payload)
}

#[tauri::command]
pub fn lineup_refresh_match(
    database: State<'_, Database>,
    request: LineupMatchRequest,
) -> Result<crate::repositories::lineup::LineupImportResult, String> {
    let c = database.connection()?;
    crate::repositories::lineup::refresh_match(&c, request.match_id)
}

#[tauri::command]
pub fn lineup_get_latest(
    database: State<'_, Database>,
    request: LineupMatchRequest,
) -> Result<Option<crate::repositories::lineup::LineupSnapshot>, String> {
    let c = database.connection()?;
    crate::repositories::lineup::latest(&c, request.match_id)
}

#[tauri::command]
pub fn lineup_get_history(
    database: State<'_, Database>,
    request: LineupMatchRequest,
) -> Result<Vec<crate::repositories::lineup::LineupSnapshot>, String> {
    let c = database.connection()?;
    crate::repositories::lineup::history(&c, request.match_id)
}

#[tauri::command]
pub fn lineup_status_for_match(
    database: State<'_, Database>,
    request: LineupMatchRequest,
) -> Result<crate::repositories::lineup::LineupStatus, String> {
    let c = database.connection()?;
    let now = request
        .now
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    crate::repositories::lineup::status_for_match(&c, request.match_id, &now)
}

#[tauri::command]
pub fn lineup_feature_generate_for_match(
    database: State<'_, Database>,
    request: LineupFeatureRequest,
) -> Result<crate::repositories::lineup::LineupFeatureSet, String> {
    let c = database.connection()?;
    crate::repositories::lineup::generate_features(&c, request.match_id)
}

#[tauri::command]
pub fn lineup_feature_quality(
    database: State<'_, Database>,
    request: LineupFeatureRequest,
) -> Result<crate::repositories::lineup::LineupFeatureSet, String> {
    let c = database.connection()?;
    crate::repositories::lineup::generate_features(&c, request.match_id)
}

#[tauri::command]
pub fn lineup_refresh_due_matches(
    database: State<'_, Database>,
    request: LineupMatchRequest,
) -> Result<Vec<i64>, String> {
    let c = database.connection()?;
    let now = request
        .now
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    crate::repositories::lineup::due_matches(&c, &now)
}

#[tauri::command]
pub fn lineup_engine_status(
    database: State<'_, Database>,
) -> Result<crate::repositories::lineup::LineupEngineStatus, String> {
    let c = database.connection()?;
    crate::repositories::lineup::engine_status(&c)
}

#[tauri::command]
pub fn lineup_model_train(
    database: State<'_, Database>,
    request: LineupModelTrainRequest,
) -> Result<crate::repositories::lineup_model::TrainReport, String> {
    let c = database.connection()?;
    let report = crate::repositories::lineup_model::train(
        &c,
        &request.model_version,
        std::path::Path::new(&request.output_artifact_path),
        request.parent_model_version,
        request.parent_model_hash,
    )?;
    let artifact =
        crate::repositories::lineup_model::load(std::path::Path::new(&report.artifact_path))?;
    crate::repositories::lineup_model::register(&c, &report, &artifact)?;
    Ok(report)
}

#[tauri::command]
pub fn lineup_model_validate_artifact(request: LineupModelArtifactRequest) -> Result<bool, String> {
    crate::repositories::lineup_model::load(std::path::Path::new(&request.artifact_path))
        .map(|_| true)
}

#[tauri::command]
pub fn lineup_model_activate(
    database: State<'_, Database>,
    request: LineupModelActivateRequest,
) -> Result<(), String> {
    let c = database.connection()?;
    crate::repositories::lineup_model::activate(&c, &request.model_version)
}

#[tauri::command]
pub fn lineup_model_status(database: State<'_, Database>) -> Result<serde_json::Value, String> {
    let c = database.connection()?;
    crate::repositories::lineup_model::status(&c)
}

#[tauri::command]
pub fn lineup_prediction_generate_revision(
    database: State<'_, Database>,
    request: LineupRevisionRequest,
) -> Result<crate::repositories::lineup_model::RevisionResult, String> {
    let c = database.connection()?;
    crate::repositories::lineup_model::generate_revision(
        &c,
        request.prediction_id,
        request.feature_set_id,
    )
}

#[tauri::command]
pub fn prediction_model_train(
    database: State<'_, Database>,
    request: PredictionTrainRequest,
) -> Result<crate::repositories::prediction_engine::TrainReport, String> {
    let c = database.connection()?;
    let report = crate::repositories::prediction_engine::train(
        &c,
        &request.model_version,
        std::path::Path::new(&request.output_artifact_directory),
    )?;
    crate::repositories::prediction_engine::register(
        &c,
        std::path::Path::new(&report.artifact_path),
    )?;
    Ok(report)
}

#[tauri::command]
pub fn prediction_model_validate_artifact(
    request: PredictionArtifactRequest,
) -> crate::repositories::prediction_engine::ArtifactValidation {
    crate::repositories::prediction_engine::validate_artifact(std::path::Path::new(
        &request.artifact_path,
    ))
}

#[tauri::command]
pub fn prediction_model_activate(
    database: State<'_, Database>,
    request: PredictionActivateRequest,
) -> Result<(), String> {
    let c = database.connection()?;
    crate::repositories::prediction_engine::activate(&c, &request.model_version)
}

fn predict_one(
    c: &rusqlite::Connection,
    match_id: i64,
) -> Result<crate::repositories::prediction_engine::PredictionResult, String> {
    let snapshot = crate::repositories::features::generate(c, match_id)?;
    crate::repositories::features::persist(c, &snapshot)?;
    let (model_id, artifact) = crate::repositories::prediction_engine::active(c)?;
    let mut result = crate::repositories::prediction_engine::infer(&artifact, &snapshot)?;
    if let Some(calibration) = crate::repositories::calibration::active_calibration(c)? {
        crate::repositories::calibration::apply(&mut result, &calibration);
    }
    let kickoff = c
        .query_row(
            "SELECT kickoff_at FROM matches WHERE id=?1",
            [match_id],
            |r| r.get::<_, String>(0),
        )
        .map_err(|e| e.to_string())?;
    crate::repositories::prediction_engine::persist_predictions(c, &result, model_id, &kickoff)?;
    Ok(result)
}

#[tauri::command]
pub fn prediction_generate_for_match(
    database: State<'_, Database>,
    request: FeatureMatchRequest,
) -> Result<crate::repositories::prediction_engine::PredictionResult, String> {
    let c = database.connection()?;
    predict_one(&c, request.match_id)
}

#[tauri::command]
pub fn prediction_generate_upcoming(
    database: State<'_, Database>,
    request: PredictionUpcomingRequest,
) -> Result<Vec<crate::repositories::prediction_engine::PredictionResult>, String> {
    let c = database.connection()?;
    let mut statement = c
        .prepare(
            "SELECT id FROM matches
             WHERE status='scheduled'
               AND (?1 IS NULL OR competition_id=?1)
               AND (?2 IS NULL OR scheduled_local_date>=?2)
               AND (?3 IS NULL OR scheduled_local_date<=?3)
             ORDER BY kickoff_at,id",
        )
        .map_err(|e| e.to_string())?;
    let ids = statement
        .query_map(
            rusqlite::params![request.competition_id, request.date_from, request.date_to],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(statement);
    ids.into_iter().map(|id| predict_one(&c, id)).collect()
}

#[tauri::command]
pub fn prediction_engine_status(
    database: State<'_, Database>,
) -> Result<crate::repositories::prediction_engine::EngineStatus, String> {
    let c = database.connection()?;
    crate::repositories::prediction_engine::status(&c)
}

#[tauri::command]
pub fn feature_engine_generate_for_match(
    database: State<'_, Database>,
    request: FeatureMatchRequest,
) -> Result<crate::repositories::features::FeatureSnapshot, String> {
    let c = database.connection()?;
    let snapshot = crate::repositories::features::generate(&c, request.match_id)?;
    crate::repositories::features::persist(&c, &snapshot)?;
    Ok(snapshot)
}
#[tauri::command]
pub fn feature_engine_generate_dataset(
    database: State<'_, Database>,
    request: FeatureDatasetRequest,
) -> Result<crate::repositories::features::DatasetSummary, String> {
    let c = database.connection()?;
    crate::repositories::features::generate_dataset(
        &c,
        request.competition_id,
        request.season.as_deref(),
    )
}
#[tauri::command]
pub fn feature_engine_market_readiness(
    database: State<'_, Database>,
) -> Result<Vec<crate::repositories::features::Readiness>, String> {
    let c = database.connection()?;
    crate::repositories::features::market_readiness(&c)
}
#[tauri::command]
pub fn feature_engine_quality_summary(
    database: State<'_, Database>,
) -> Result<crate::repositories::features::QualitySummary, String> {
    let c = database.connection()?;
    crate::repositories::features::quality_summary(&c)
}

fn trigger_asset_discovery(manager: &crate::assets::AssetSyncManager) {
    let _ = manager.scan();
    manager.start();
}

#[tauri::command]
pub fn asset_sync_scan(
    manager: State<'_, crate::assets::AssetSyncManager>,
) -> Result<crate::assets::AssetScanSummary, String> {
    manager.scan()
}
#[tauri::command]
pub fn asset_sync_start(manager: State<'_, crate::assets::AssetSyncManager>) -> bool {
    manager.start()
}
#[tauri::command]
pub fn asset_sync_status(
    manager: State<'_, crate::assets::AssetSyncManager>,
) -> crate::assets::AssetSyncStatus {
    manager.status()
}
#[tauri::command]
pub fn asset_sync_retry_failed(
    manager: State<'_, crate::assets::AssetSyncManager>,
) -> Result<crate::assets::AssetScanSummary, String> {
    manager.retry_failed()
}
#[tauri::command]
pub fn entity_logo_path(
    database: State<'_, Database>,
    request: EntityLogoPathRequest,
) -> Result<Option<String>, String> {
    let c = database.connection()?;
    let root = crate::assets::asset_root(
        database
            .path()
            .parent()
            .ok_or("database path has no parent")?,
    );
    crate::assets::logo_path(
        &c,
        &root,
        &request.entity_type.trim().to_ascii_uppercase(),
        request.entity_id,
    )
}

#[tauri::command]
pub fn database_health(database: State<'_, Database>) -> Result<DatabaseHealth, String> {
    let connection = database.connection()?;
    connection
        .query_row("SELECT 1", [], |_| Ok(()))
        .map_err(|error| error.to_string())?;
    let foreign_keys_enabled = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
        .map_err(|error| error.to_string())?
        == 1;
    let applied_migrations = connection
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;

    Ok(DatabaseHealth {
        status: "ok",
        database_path: database.path().to_string_lossy().into_owned(),
        foreign_keys_enabled,
        applied_migrations,
    })
}

#[tauri::command]
pub fn database_stats(database: State<'_, Database>) -> Result<DatabaseStats, String> {
    let connection = database.connection()?;
    repositories::stats::database_stats(&connection).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn data_center_status(
    database: State<'_, Database>,
    asset_manager: State<'_, crate::assets::AssetSyncManager>,
    request: DataCenterStatusRequest,
) -> Result<crate::repositories::data_center::DataCenterStatus, String> {
    let historical = crate::providers::football_data::status(
        database.inner(),
        crate::providers::football_data::CORE_RECENT_3_SEASONS,
    )?;
    let assets = asset_manager.status();
    let runtime = crate::repositories::data_center::RuntimeReadiness {
        historical_dataset_count: historical.preset_dataset_count,
        historical_cached_count: historical.datasets_cached,
        historical_imported_count: historical.datasets_imported_successfully,
        historical_missing_count: historical.datasets_missing,
        historical_failed_count: historical.datasets_with_failed_last_import,
        historical_cached_bytes: historical.total_cached_bytes,
        asset_state: assets.state,
        asset_bytes_this_run: assets.bytes_downloaded_this_run,
        lineup_provider_configured: false,
    };
    let now = match request.as_of {
        Some(value) => chrono::DateTime::parse_from_rfc3339(&value)
            .map_err(|_| "INVALID_AS_OF".to_string())?
            .with_timezone(&chrono::Utc),
        None => chrono::Utc::now(),
    };
    let connection = database.connection()?;
    crate::repositories::data_center::status(&connection, &runtime, now)
}

#[tauri::command]
pub fn resolver_scan(
    database: State<'_, Database>,
) -> Result<crate::repositories::resolution::ScanSummary, String> {
    let c = database.connection()?;
    crate::repositories::resolution::scan(&c).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resolver_apply_safe_matches(
    database: State<'_, Database>,
) -> Result<crate::repositories::resolution::ApplySummary, String> {
    let mut c = database.connection()?;
    crate::repositories::resolution::apply_safe(&mut c).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resolver_get_review_queue(
    database: State<'_, Database>,
) -> Result<Vec<crate::repositories::resolution::ResolutionCandidate>, String> {
    let c = database.connection()?;
    crate::repositories::resolution::review_queue(&c).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resolver_accept_team_match(
    database: State<'_, Database>,
    request: ResolverCandidateRequest,
) -> Result<(), String> {
    let c = database.connection()?;
    crate::repositories::resolution::accept(&c, request.id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resolver_reject_team_match(
    database: State<'_, Database>,
    request: ResolverCandidateRequest,
) -> Result<(), String> {
    let c = database.connection()?;
    crate::repositories::resolution::reject(&c, request.id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resolver_link_team_mapping(
    database: State<'_, Database>,
    request: ResolverLinkRequest,
) -> Result<(), String> {
    let c = database.connection()?;
    crate::repositories::resolution::link_mapping(&c, request.mapping_id, request.team_id)
        .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
pub struct ResolverMergeRequest {
    duplicate_id: i64,
    canonical_id: i64,
}

#[tauri::command]
pub fn merge_normalized_teams(
    database: State<'_, Database>,
    request: ResolverMergeRequest,
) -> Result<(), String> {
    let mut c = database.connection()?;
    crate::repositories::resolution::merge_team(&mut c, request.duplicate_id, request.canonical_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn merge_normalized_matches(
    database: State<'_, Database>,
    request: ResolverMergeRequest,
) -> Result<(), String> {
    let mut c = database.connection()?;
    crate::repositories::resolution::merge_match(&mut c, request.duplicate_id, request.canonical_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn football_data_supported_datasets() -> Vec<crate::providers::football_data::SupportedDataset>
{
    crate::providers::football_data::supported_datasets()
}

#[tauri::command]
pub async fn football_data_import_dataset(
    database: State<'_, Database>,
    asset_manager: State<'_, crate::assets::AssetSyncManager>,
    request: FootballDataDatasetRequest,
) -> Result<crate::providers::football_data::ImportSummary, String> {
    let dataset = crate::providers::football_data::find_dataset(
        request.league_code.trim(),
        request.season_code.trim(),
    )
    .ok_or_else(|| {
        format!(
            "unsupported football-data.co.uk dataset: {} {}",
            request.league_code, request.season_code
        )
    })?;
    let result = crate::providers::football_data::import_dataset(database.inner(), dataset).await;
    if result.is_ok() {
        trigger_asset_discovery(asset_manager.inner())
    }
    result
}

#[tauri::command]
pub async fn football_data_network_diagnostic() -> crate::providers::football_data::NetworkDiagnostic
{
    let dataset = crate::providers::football_data::find_dataset("E0", "2627")
        .expect("the diagnostic dataset is registered");
    crate::providers::football_data::diagnose_dataset(dataset).await
}

#[tauri::command]
pub fn football_data_import_local_dataset(
    database: State<'_, Database>,
    asset_manager: State<'_, crate::assets::AssetSyncManager>,
    request: FootballDataLocalDatasetRequest,
) -> Result<crate::providers::football_data::ImportSummary, String> {
    let dataset = crate::providers::football_data::find_dataset(
        request.league_code.trim(),
        request.season_code.trim(),
    )
    .ok_or_else(|| {
        format!(
            "unsupported football-data.co.uk dataset: {} {}",
            request.league_code, request.season_code
        )
    })?;
    let result = crate::providers::football_data::import_local_dataset(
        database.inner(),
        dataset,
        std::path::Path::new(&request.file_path),
    );
    if result.is_ok() {
        trigger_asset_discovery(asset_manager.inner())
    }
    result
}

#[tauri::command]
pub fn football_data_cache_local_file(
    database: State<'_, Database>,
    request: FootballDataLocalDatasetRequest,
) -> Result<crate::providers::football_data::CacheMetadata, String> {
    let dataset = crate::providers::football_data::find_dataset(
        request.league_code.trim(),
        request.season_code.trim(),
    )
    .ok_or_else(|| {
        format!(
            "unsupported football-data.co.uk dataset: {} {}",
            request.league_code, request.season_code
        )
    })?;
    crate::providers::football_data::cache_local_file(
        database.inner(),
        dataset,
        std::path::Path::new(&request.file_path),
    )
}

#[tauri::command]
pub fn football_data_import_cached_dataset(
    database: State<'_, Database>,
    asset_manager: State<'_, crate::assets::AssetSyncManager>,
    request: FootballDataDatasetRequest,
) -> Result<crate::providers::football_data::ImportSummary, String> {
    let dataset = crate::providers::football_data::find_dataset(
        request.league_code.trim(),
        request.season_code.trim(),
    )
    .ok_or_else(|| {
        format!(
            "unsupported football-data.co.uk dataset: {} {}",
            request.league_code, request.season_code
        )
    })?;
    let result = crate::providers::football_data::import_cached_dataset(database.inner(), dataset);
    if result.is_ok() {
        trigger_asset_discovery(asset_manager.inner())
    }
    result
}

#[tauri::command]
pub async fn football_data_bootstrap_cached(
    database: State<'_, Database>,
    asset_manager: State<'_, crate::assets::AssetSyncManager>,
    request: crate::providers::football_data::CachedBootstrapRequest,
) -> Result<crate::providers::football_data::CachedBootstrapSummary, String> {
    let result =
        crate::providers::football_data::bootstrap_cached(database.inner(), &request).await;
    if result.is_ok() {
        trigger_asset_discovery(asset_manager.inner())
    }
    result
}

#[tauri::command]
pub fn football_data_bootstrap_status(
    database: State<'_, Database>,
    request: FootballDataBootstrapStatusRequest,
) -> Result<crate::providers::football_data::BootstrapStatus, String> {
    crate::providers::football_data::status(database.inner(), request.preset.trim())
}

#[tauri::command]
pub fn football_data_cache_stats(
    database: State<'_, Database>,
) -> Result<crate::providers::football_data::CacheStats, String> {
    crate::providers::football_data::cache_stats(database.inner())
}

#[tauri::command]
pub async fn football_data_bootstrap(
    database: State<'_, Database>,
    asset_manager: State<'_, crate::assets::AssetSyncManager>,
    request: crate::providers::football_data::BootstrapRequest,
) -> Result<crate::providers::football_data::BootstrapSummary, String> {
    let result = crate::providers::football_data::bootstrap(database.inner(), &request).await;
    if result.is_ok() {
        trigger_asset_discovery(asset_manager.inner())
    }
    result
}

#[tauri::command]
pub fn football_data_quality_summary(
    database: State<'_, Database>,
) -> Result<Vec<crate::models::FootballDataQualitySummary>, String> {
    let connection = database.connection()?;
    crate::repositories::quality::football_data_summary(
        &connection,
        crate::providers::football_data::PROVIDER_ID,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn iddaa_refresh_bulletin(
    database: State<'_, Database>,
    asset_manager: State<'_, crate::assets::AssetSyncManager>,
) -> Result<crate::providers::iddaa::RefreshSummary, String> {
    let result = crate::providers::iddaa::refresh_bulletin(database.inner()).await;
    if result.is_ok() {
        trigger_asset_discovery(asset_manager.inner())
    }
    result
}

#[tauri::command]
pub fn iddaa_import_local_bulletin(
    database: State<'_, Database>,
    asset_manager: State<'_, crate::assets::AssetSyncManager>,
    request: IddaaLocalBulletinRequest,
) -> Result<crate::providers::iddaa::RefreshSummary, String> {
    let result = crate::providers::iddaa::import_local_bulletin(
        database.inner(),
        std::path::Path::new(&request.file_path),
    );
    if result.is_ok() {
        trigger_asset_discovery(asset_manager.inner())
    }
    result
}

#[tauri::command]
pub fn iddaa_bulletin_status(
    database: State<'_, Database>,
) -> Result<crate::repositories::iddaa::BulletinStatus, String> {
    let connection = database.connection()?;
    crate::repositories::iddaa::bulletin_status(&connection).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn iddaa_get_upcoming_matches(
    database: State<'_, Database>,
) -> Result<Vec<crate::repositories::iddaa::UpcomingMatch>, String> {
    let connection = database.connection()?;
    crate::repositories::iddaa::upcoming_matches(&connection).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn iddaa_get_latest_odds(
    database: State<'_, Database>,
    request: IddaaLatestOddsRequest,
) -> Result<Vec<crate::repositories::iddaa::LatestOdd>, String> {
    let connection = database.connection()?;
    crate::repositories::iddaa::latest_odds(&connection, request.match_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn iddaa_refresh_popularity(
    database: State<'_, Database>,
) -> Result<crate::providers::iddaa::PopularityRefreshSummary, String> {
    crate::providers::iddaa::refresh_popularity(database.inner()).await
}

#[tauri::command]
pub fn iddaa_popularity_status(
    database: State<'_, Database>,
) -> Result<crate::repositories::popularity::PopularityStatus, String> {
    let connection = database.connection()?;
    crate::repositories::popularity::status(&connection).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn iddaa_get_latest_popular_selections(
    database: State<'_, Database>,
) -> Result<Vec<crate::repositories::popularity::LatestPopularSelection>, String> {
    let connection = database.connection()?;
    crate::repositories::popularity::latest_popular_selections(&connection)
        .map_err(|error| error.to_string())
}
