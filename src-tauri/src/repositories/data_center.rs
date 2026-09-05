//! Operational readiness projection. Read-only: it never starts refresh or training work.
use chrono::{DateTime, Utc};
use chrono_tz::Europe::Istanbul;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::{
    candidate_engine, features, iddaa, lineup, lineup_model, popularity, prediction_engine,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReadinessState {
    Ready,
    Partial,
    ActionRequired,
    Updating,
    Unavailable,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubsystemReadiness {
    pub status: ReadinessState,
    pub title: String,
    pub message: String,
    pub last_updated: Option<String>,
    pub progress: Option<f64>,
    pub item_count: Option<i64>,
    pub metrics: BTreeMap<String, i64>,
    pub downloaded_bytes: Option<u64>,
    pub source: Option<String>,
    pub retryable: bool,
    pub technical_reason: Option<String>,
    pub artifact_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityReadiness {
    pub ready: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataCenterStatus {
    pub overall_status: ReadinessState,
    pub first_run: bool,
    pub checked_at: String,
    pub database: SubsystemReadiness,
    pub historical_data: SubsystemReadiness,
    pub iddaa: SubsystemReadiness,
    pub odds: SubsystemReadiness,
    pub popularity: SubsystemReadiness,
    pub team_logos: SubsystemReadiness,
    pub entity_resolution: SubsystemReadiness,
    pub features: SubsystemReadiness,
    pub prediction_model: SubsystemReadiness,
    pub calibration: SubsystemReadiness,
    pub lineup_model: SubsystemReadiness,
    pub lineup_provider: SubsystemReadiness,
    pub can_generate_predictions: CapabilityReadiness,
    pub can_generate_candidates: CapabilityReadiness,
    pub can_generate_coupons: CapabilityReadiness,
}

#[derive(Debug, Clone)]
pub struct RuntimeReadiness {
    pub historical_dataset_count: usize,
    pub historical_cached_count: usize,
    pub historical_imported_count: usize,
    pub historical_missing_count: usize,
    pub historical_failed_count: usize,
    pub historical_cached_bytes: u64,
    pub asset_state: String,
    pub asset_bytes_this_run: i64,
    pub lineup_provider_configured: bool,
}
impl Default for RuntimeReadiness {
    fn default() -> Self {
        Self {
            historical_dataset_count: 0,
            historical_cached_count: 0,
            historical_imported_count: 0,
            historical_missing_count: 0,
            historical_failed_count: 0,
            historical_cached_bytes: 0,
            asset_state: "IDLE".into(),
            asset_bytes_this_run: 0,
            lineup_provider_configured: false,
        }
    }
}

fn item(
    status: ReadinessState,
    title: &str,
    message: &str,
    last: Option<String>,
    count: Option<i64>,
    source: Option<&str>,
    retryable: bool,
    reason: Option<String>,
) -> SubsystemReadiness {
    SubsystemReadiness {
        status,
        title: title.into(),
        message: message.into(),
        last_updated: last,
        progress: None,
        item_count: count,
        metrics: BTreeMap::new(),
        downloaded_bytes: None,
        source: source.map(str::to_string),
        retryable,
        technical_reason: reason,
        artifact_hash: None,
    }
}
fn parse_time(value: Option<&str>) -> Option<DateTime<Utc>> {
    value
        .and_then(|x| DateTime::parse_from_rfc3339(x).ok())
        .map(|x| x.with_timezone(&Utc))
}
fn capable(checks: &[(&SubsystemReadiness, &str)]) -> CapabilityReadiness {
    let reasons = checks
        .iter()
        .filter(|(x, _)| x.status != ReadinessState::Ready)
        .map(|(_, reason)| (*reason).to_string())
        .collect::<Vec<_>>();
    CapabilityReadiness {
        ready: reasons.is_empty(),
        reasons,
    }
}

pub fn status(
    c: &Connection,
    runtime: &RuntimeReadiness,
    now: DateTime<Utc>,
) -> Result<DataCenterStatus, String> {
    let bulletin = iddaa::bulletin_status(c).map_err(|e| e.to_string())?;
    let popular = popularity::status(c).map_err(|e| e.to_string())?;
    let feature = features::quality_summary(c)?;
    let engine = prediction_engine::status(c).unwrap_or(prediction_engine::EngineStatus {
        active_model_version: None,
        artifact_valid: false,
        feature_compatible: false,
        supported_markets: vec![],
        training_cutoff: None,
        calibration_status: "CALIBRATION_PENDING".into(),
    });
    let candidates = candidate_engine::status(c)?;
    let lineup_status = lineup::engine_status(c)?;
    let (matches, teams): (i64, i64) = c
        .query_row(
            "SELECT (SELECT COUNT(*) FROM matches),(SELECT COUNT(*) FROM teams)",
            [],
            |x| Ok((x.get(0)?, x.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let business_date = now.with_timezone(&Istanbul).date_naive().to_string();
    let predictions_today: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM predictions p JOIN matches m ON m.id=p.match_id WHERE m.scheduled_local_date=?1",
            [&business_date],
            |x| x.get(0),
        )
        .map_err(|e| e.to_string())?;
    let finished:i64=c.query_row("SELECT COUNT(*) FROM matches WHERE status='finished' AND final_home_goals IS NOT NULL AND final_away_goals IS NOT NULL",[],|x|x.get(0)).map_err(|e|e.to_string())?;
    let latest_import:Option<String>=c.query_row("SELECT MAX(completed_at) FROM data_import_runs WHERE provider='football-data.co.uk' AND status='completed'",[],|x|x.get(0)).map_err(|e|e.to_string())?;
    let latest_iddaa_error:Option<String>=c.query_row("SELECT error_message FROM data_import_runs WHERE provider='iddaa' AND status='failed' ORDER BY completed_at DESC,id DESC LIMIT 1",[],|x|x.get(0)).optional().map_err(|e|e.to_string())?.flatten();
    let unresolved:i64=c.query_row("SELECT (SELECT COUNT(*) FROM provider_competition_metadata WHERE resolution_status='unresolved')+(SELECT COUNT(*) FROM team_resolution_candidates WHERE status='pending' AND confidence_level IN ('REVIEW','UNRESOLVED'))",[],|x|x.get(0)).map_err(|e|e.to_string())?;
    let resolved: i64 = c
        .query_row("SELECT COUNT(*) FROM provider_team_mappings", [], |x| {
            x.get(0)
        })
        .map_err(|e| e.to_string())?;
    let (logos_ready,logos_failed,logo_bytes,logo_last):(i64,i64,i64,Option<String>)=c.query_row("SELECT SUM(CASE WHEN entity_type='TEAM' AND status='READY' THEN 1 ELSE 0 END),SUM(CASE WHEN entity_type='TEAM' AND status='FAILED' THEN 1 ELSE 0 END),SUM(CASE WHEN entity_type='TEAM' AND status='READY' THEN COALESCE(byte_size,0) ELSE 0 END),MAX(CASE WHEN entity_type='TEAM' THEN updated_at END) FROM entity_assets",[],|x|Ok((x.get::<_,Option<i64>>(0)?.unwrap_or(0),x.get::<_,Option<i64>>(1)?.unwrap_or(0),x.get::<_,Option<i64>>(2)?.unwrap_or(0),x.get(3)?))).map_err(|e|e.to_string())?;
    let active_model:Option<(String,Option<String>,Option<String>)>=c.query_row("SELECT version_identifier,artifact_sha256,feature_engine_version FROM model_versions WHERE is_active=1",[],|x|Ok((x.get(0)?,x.get(1)?,x.get(2)?))).optional().map_err(|e|e.to_string())?;
    let calibration_row:Option<(String,String,String)>=c.query_row("SELECT calibration_version,parent_model_version,parent_artifact_sha256 FROM calibration_models WHERE is_active=1",[],|x|Ok((x.get(0)?,x.get(1)?,x.get(2)?))).optional().map_err(|e|e.to_string())?;
    let lineup_model_row: Option<(String, String, String, String, String, String)> = c
        .query_row(
            "SELECT version_identifier,artifact_path,artifact_sha256,parent_model_version,parent_model_hash,feature_version FROM lineup_adjustment_models WHERE status='ACTIVE' ORDER BY id DESC LIMIT 1",
            [],
            |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?, x.get(3)?, x.get(4)?, x.get(5)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let database = item(
        ReadinessState::Ready,
        "Yerel veritabanı",
        "Yerel veri deposu erişilebilir.",
        None,
        Some(matches),
        Some("SQLite"),
        false,
        None,
    );
    let historical_ok = finished >= prediction_engine::MIN_TRAINING_SAMPLES as i64
        && runtime.historical_imported_count > 0;
    let historical_state = if historical_ok {
        ReadinessState::Ready
    } else if runtime.historical_failed_count > 0
        && finished == 0
        && runtime.historical_cached_count == 0
    {
        ReadinessState::Error
    } else if finished > 0 || runtime.historical_cached_count > 0 {
        ReadinessState::Partial
    } else {
        ReadinessState::ActionRequired
    };
    let mut historical_data = item(
        historical_state,
        "Geçmiş veri",
        if historical_ok {
            "Model için yeterli sonuçlanmış geçmiş mevcut."
        } else if finished > 0 {
            "Geçmiş veri var ancak model hazırlığı için eksik."
        } else {
            "Geçmiş futbol verisi henüz içe aktarılmadı."
        },
        latest_import,
        Some(finished),
        Some("football-data.co.uk"),
        true,
        (runtime.historical_failed_count > 0).then(|| "HISTORICAL_IMPORT_FAILED".into()),
    );
    historical_data.downloaded_bytes = Some(runtime.historical_cached_bytes);
    historical_data.metrics = BTreeMap::from([
        (
            "supported_datasets".into(),
            runtime.historical_dataset_count as i64,
        ),
        (
            "cached_datasets".into(),
            runtime.historical_cached_count as i64,
        ),
        (
            "imported_datasets".into(),
            runtime.historical_imported_count as i64,
        ),
        (
            "missing_datasets".into(),
            runtime.historical_missing_count as i64,
        ),
        (
            "failed_datasets".into(),
            runtime.historical_failed_count as i64,
        ),
        ("settled_matches".into(), finished),
    ]);
    historical_data.progress = (runtime.historical_dataset_count > 0).then_some(
        runtime.historical_cached_count as f64 / runtime.historical_dataset_count as f64,
    );
    let odds_age =
        parse_time(bulletin.latest_odds_snapshot_time.as_deref()).map(|x| (now - x).num_hours());
    let odds_fresh = bulletin.odds_snapshot_count > 0
        && odds_age.is_some_and(|x| x <= candidate_engine::MAX_DAILY_ODDS_AGE_HOURS);
    let iddaa_state = if bulletin.upcoming_match_count > 0 {
        if odds_fresh {
            ReadinessState::Ready
        } else {
            ReadinessState::Partial
        }
    } else if latest_iddaa_error.is_some() {
        ReadinessState::Error
    } else {
        ReadinessState::ActionRequired
    };
    let mut iddaa = item(
        iddaa_state,
        "Canlı İddaa verisi",
        if bulletin.upcoming_match_count > 0 {
            "Güncel bülten kullanılabilir."
        } else if latest_iddaa_error.is_some() {
            "Son canlı veri yenilemesi başarısız; yerel veriler korunuyor."
        } else {
            "Güncel İddaa bülteni bulunamadı."
        },
        bulletin.last_successful_refresh.clone(),
        Some(bulletin.upcoming_match_count),
        Some("İddaa"),
        true,
        latest_iddaa_error,
    );
    iddaa.metrics = BTreeMap::from([
        ("event_count".into(), bulletin.upcoming_match_count),
        ("competition_count".into(), bulletin.competition_count),
        ("odds_snapshot_count".into(), bulletin.odds_snapshot_count),
    ]);
    let odds = item(
        if odds_fresh {
            ReadinessState::Ready
        } else if bulletin.odds_snapshot_count > 0 {
            ReadinessState::Partial
        } else {
            ReadinessState::ActionRequired
        },
        "Oranlar",
        if odds_fresh {
            "Oran snapshot’ları güncel."
        } else if bulletin.odds_snapshot_count > 0 {
            "Oran snapshot’ları eskimiş."
        } else {
            "Oran verisi bulunamadı."
        },
        bulletin.latest_odds_snapshot_time.clone(),
        Some(bulletin.odds_snapshot_count),
        Some("İddaa"),
        true,
        (!odds_fresh && bulletin.odds_snapshot_count > 0).then(|| "STALE_ODDS".into()),
    );
    let mut popularity = item(
        if popular.latest_selection_count > 0 {
            if parse_time(popular.latest_snapshot_time.as_deref())
                .is_some_and(|x| (now - x).num_hours() <= 24)
            {
                ReadinessState::Ready
            } else {
                ReadinessState::Partial
            }
        } else {
            ReadinessState::Unavailable
        },
        "Popülerlik",
        if popular.latest_selection_count > 0 {
            "Popüler seçim snapshot’ı kullanılabilir."
        } else {
            "Popülerlik verisi yok; tahminler etkilenmez."
        },
        popular.latest_snapshot_time.clone(),
        Some(popular.latest_selection_count),
        Some("İddaa"),
        true,
        None,
    );
    popularity
        .metrics
        .insert("item_count".into(), popular.latest_selection_count);
    let mut team_logos = item(
        if runtime.asset_state != "IDLE" {
            ReadinessState::Updating
        } else if teams == 0 {
            ReadinessState::Unavailable
        } else if logos_ready == teams {
            ReadinessState::Ready
        } else if logos_ready > 0 {
            ReadinessState::Partial
        } else {
            ReadinessState::Unavailable
        },
        "Takım logoları",
        if runtime.asset_state != "IDLE" {
            "Eksik logolar arka planda kontrol ediliyor."
        } else if teams == 0 {
            "Henüz bilinen takım yok; tahmin hazırlığı etkilenmez."
        } else if logos_ready == teams {
            "Tüm bilinen takım logoları hazır."
        } else {
            "Bazı logolar eksik; tahminler etkilenmez."
        },
        logo_last,
        Some(logos_ready),
        Some("Yerel önbellek"),
        true,
        (logos_failed > 0).then(|| format!("{logos_failed} LOGO_FAILED")),
    );
    team_logos.downloaded_bytes = Some((logo_bytes + runtime.asset_bytes_this_run).max(0) as u64);
    team_logos.metrics = BTreeMap::from([
        ("total_known_teams".into(), teams),
        ("cached_valid_logos".into(), logos_ready),
        (
            "missing_logos".into(),
            (teams - logos_ready - logos_failed).max(0),
        ),
        ("failed_or_corrupt_logos".into(), logos_failed),
    ]);
    let mut entity_resolution = item(
        if unresolved == 0 {
            ReadinessState::Ready
        } else if resolved > 0 {
            ReadinessState::Partial
        } else {
            ReadinessState::ActionRequired
        },
        "Veri eşleştirme",
        if unresolved == 0 {
            "Kanonik eşleştirmeler hazır."
        } else {
            "Bazı kayıtlar güvenli eşleştirme veya inceleme bekliyor."
        },
        None,
        Some(resolved),
        None,
        false,
        (unresolved > 0).then(|| format!("{unresolved} UNRESOLVED")),
    );
    entity_resolution.metrics = BTreeMap::from([
        ("resolved_mapping_count".into(), resolved),
        ("unresolved_count".into(), unresolved),
    ]);
    let mut features = item(
        if feature.features_generated > 0
            && feature.insufficient_history < feature.features_generated
        {
            ReadinessState::Ready
        } else if feature.features_generated > 0 {
            ReadinessState::Partial
        } else {
            ReadinessState::ActionRequired
        },
        "Özellik verisi",
        if feature.features_generated > 0 {
            "Model özellik snapshot’ları mevcut."
        } else {
            "Henüz özellik snapshot’ı oluşturulmadı."
        },
        feature.latest_snapshot.clone(),
        Some(feature.features_generated),
        Some(&feature.feature_engine_version),
        false,
        (feature.insufficient_history > 0)
            .then(|| format!("{} INSUFFICIENT_HISTORY", feature.insufficient_history)),
    );
    features.metrics = BTreeMap::from([
        ("features_generated".into(), feature.features_generated),
        ("insufficient_history".into(), feature.insufficient_history),
    ]);
    let model_ok = active_model.is_some() && engine.artifact_valid && engine.feature_compatible;
    let mut prediction_model = item(
        if model_ok {
            ReadinessState::Ready
        } else {
            ReadinessState::ActionRequired
        },
        "Tahmin modeli",
        if model_ok {
            "Aktif model doğrulandı ve uyumlu."
        } else if active_model.is_some() {
            "Aktif model artefaktı geçersiz veya uyumsuz."
        } else {
            "Aktif tahmin modeli bulunamadı."
        },
        engine.training_cutoff.clone(),
        None,
        active_model.as_ref().map(|x| x.0.as_str()),
        false,
        (!model_ok).then(|| "MODEL_NOT_READY".into()),
    );
    prediction_model.artifact_hash = active_model
        .as_ref()
        .and_then(|x| x.1.as_ref())
        .map(|x| x.chars().take(12).collect());
    let calibration_ok = match (&active_model, &calibration_row) {
        (Some((version, hash, _)), Some((_, parent, parent_hash))) => {
            version == parent && hash.as_deref() == Some(parent_hash.as_str())
        }
        _ => false,
    };
    let mut calibration = item(
        if calibration_ok {
            ReadinessState::Ready
        } else {
            ReadinessState::ActionRequired
        },
        "Kalibrasyon",
        if calibration_ok {
            "Aktif kalibrasyon model artefaktıyla uyumlu."
        } else {
            "Aktif ve uyumlu kalibrasyon bulunamadı."
        },
        None,
        None,
        calibration_row.as_ref().map(|x| x.0.as_str()),
        false,
        (!calibration_ok).then(|| "CALIBRATION_INCOMPATIBLE_OR_MISSING".into()),
    );
    calibration.artifact_hash = calibration_row
        .as_ref()
        .map(|x| x.2.chars().take(12).collect());
    let lineup_model_valid = lineup_model_row.as_ref().is_some_and(|row| {
        lineup_model::load(std::path::Path::new(&row.1)).is_ok()
            && row.5 == lineup::LINEUP_FEATURE_VERSION
            && active_model
                .as_ref()
                .is_some_and(|model| row.3 == model.0 && model.1.as_deref() == Some(row.4.as_str()))
    });
    let mut lineup_model = item(
        if lineup_model_valid {
            ReadinessState::Ready
        } else if lineup_model_row.is_some() {
            ReadinessState::Error
        } else {
            ReadinessState::Unavailable
        },
        "11 modeli",
        if lineup_model_valid {
            "Üretim 11 modeli doğrulandı ve BASE modelle uyumlu."
        } else if lineup_model_row.is_some() {
            "Aktif 11 modeli artefaktı geçersiz veya BASE modelle uyumsuz."
        } else {
            "11 modeli üretim geçmişiyle eğitilmemiş; BASE model kullanılabilir."
        },
        None,
        None,
        lineup_model_row.as_ref().map(|x| x.0.as_str()),
        false,
        if lineup_model_row.is_none() {
            Some("LINEUP_MODEL_NOT_TRAINED_ON_PRODUCTION_DATA".into())
        } else if !lineup_model_valid {
            Some("LINEUP_MODEL_ARTIFACT_INVALID_OR_INCOMPATIBLE".into())
        } else {
            None
        },
    );
    lineup_model.artifact_hash = lineup_model_row
        .as_ref()
        .map(|x| x.2.chars().take(12).collect());
    let provider_configured = runtime.lineup_provider_configured;
    let lineup_provider = item(
        if provider_configured {
            ReadinessState::Ready
        } else {
            ReadinessState::Unavailable
        },
        "11 veri sağlayıcısı",
        if provider_configured {
            "Canlı 11 veri sağlayıcısı hazır."
        } else {
            "Canlı 11 sağlayıcısı yapılandırılmamış; BASE tahminler etkilenmez."
        },
        lineup_status.latest_refresh.clone(),
        Some(lineup_status.official_complete_lineups),
        None,
        true,
        (!provider_configured).then(|| lineup_status.provider_availability.clone()),
    );
    let can_generate_predictions = capable(&[
        (&database, "Veritabanı hazır değil."),
        (&historical_data, "Yeterli geçmiş veri yok."),
        (&entity_resolution, "Veri eşleştirme eksik."),
        (&features, "Özellik verisi hazır değil."),
        (&prediction_model, "Aktif model hazır değil."),
        (&calibration, "Kalibrasyon hazır değil."),
    ]);
    let can_generate_candidates = if can_generate_predictions.ready {
        let mut gate = capable(&[(&iddaa, "Güncel bülten yok."), (&odds, "Güncel oran yok.")]);
        if predictions_today == 0 {
            gate.ready = false;
            gate.reasons.push("Henüz kalıcı tahmin bulunmuyor.".into())
        }
        gate
    } else {
        CapabilityReadiness {
            ready: false,
            reasons: vec!["Önce tahmin hazırlığı tamamlanmalı.".into()],
        }
    };
    let qualified = if candidates.latest_business_date.as_deref() == Some(&business_date) {
        candidates.qualified_counts.values().sum::<usize>() as i64
    } else {
        0
    };
    let can_generate_coupons = if can_generate_candidates.ready && qualified > 0 {
        CapabilityReadiness {
            ready: true,
            reasons: vec![],
        }
    } else {
        CapabilityReadiness {
            ready: false,
            reasons: if !can_generate_candidates.ready {
                vec!["Önce aday hazırlığı tamamlanmalı.".into()]
            } else {
                vec!["Nitelikli aday bulunmuyor.".into()]
            },
        }
    };
    let all = [
        &database,
        &historical_data,
        &iddaa,
        &odds,
        &popularity,
        &team_logos,
        &entity_resolution,
        &features,
        &prediction_model,
        &calibration,
        &lineup_model,
        &lineup_provider,
    ];
    let overall_status = if all.iter().all(|x| x.status == ReadinessState::Ready) {
        ReadinessState::Ready
    } else if all.iter().any(|x| x.status == ReadinessState::Error) {
        ReadinessState::Error
    } else if !can_generate_predictions.ready {
        ReadinessState::ActionRequired
    } else if all.iter().any(|x| x.status == ReadinessState::Updating) {
        ReadinessState::Updating
    } else {
        ReadinessState::Partial
    };
    Ok(DataCenterStatus {
        overall_status,
        first_run: matches == 0 && teams == 0 && active_model.is_none(),
        checked_at: now.to_rfc3339(),
        database,
        historical_data,
        iddaa,
        odds,
        popularity,
        team_logos,
        entity_resolution,
        features,
        prediction_model,
        calibration,
        lineup_model,
        lineup_provider,
        can_generate_predictions,
        can_generate_candidates,
        can_generate_coupons,
    })
}
