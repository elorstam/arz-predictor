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
    Optional,
    Background,
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
    pub core_checks: Vec<CoreCheck>,
    pub core_ready_count: usize,
    pub core_check_count: usize,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoreCheck {
    pub id: String,
    pub title: String,
    pub status: ReadinessState,
    pub reasons: Vec<String>,
}

fn core_check(id: &str, title: &str, status: ReadinessState, reason: &str) -> CoreCheck {
    CoreCheck {
        id: id.into(),
        title: title.into(),
        status,
        reasons: if status == ReadinessState::Ready {
            vec![]
        } else {
            vec![reason.into()]
        },
    }
}

pub(crate) fn core_overall(checks: &[CoreCheck]) -> ReadinessState {
    if checks.len() == 10 && checks.iter().all(|x| x.status == ReadinessState::Ready) {
        ReadinessState::Ready
    } else if checks.iter().any(|x| x.status == ReadinessState::Error) {
        ReadinessState::Error
    } else if checks
        .iter()
        .any(|x| x.status == ReadinessState::Ready && x.id == "base_model")
    {
        ReadinessState::Partial
    } else {
        ReadinessState::ActionRequired
    }
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
    let scope = super::production_scope::status(c).map_err(|e| e.to_string())?;
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
    let lineup_status = lineup::engine_status(c)?;
    let (matches, teams): (i64, i64) = c
        .query_row(
            "SELECT (SELECT COUNT(*) FROM matches),(SELECT COUNT(*) FROM teams)",
            [],
            |x| Ok((x.get(0)?, x.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let business_date = now.with_timezone(&Istanbul).date_naive().to_string();
    let finished:i64=c.query_row("SELECT COUNT(*) FROM matches WHERE status='finished' AND final_home_goals IS NOT NULL AND final_away_goals IS NOT NULL",[],|x|x.get(0)).map_err(|e|e.to_string())?;
    let latest_import:Option<String>=c.query_row("SELECT MAX(completed_at) FROM data_import_runs WHERE provider='football-data.co.uk' AND status='completed'",[],|x|x.get(0)).map_err(|e|e.to_string())?;
    let latest_iddaa_error:Option<String>=c.query_row("SELECT error_message FROM data_import_runs WHERE provider='iddaa' AND status='failed' ORDER BY completed_at DESC,id DESC LIMIT 1",[],|x|x.get(0)).optional().map_err(|e|e.to_string())?.flatten();
    let pending_resolution_diagnostics:i64=c.query_row("SELECT (SELECT COUNT(*) FROM provider_competition_metadata WHERE resolution_status='unresolved')+(SELECT COUNT(*) FROM team_resolution_candidates WHERE status='pending' AND confidence_level IN ('REVIEW','UNRESOLVED'))",[],|x|x.get(0)).map_err(|e|e.to_string())?;
    let (resolved_team_mappings, unresolved_team_mappings): (i64, i64) = c.query_row(
        "SELECT SUM(CASE WHEN EXISTS(SELECT 1 FROM provider_team_mappings canonical WHERE canonical.team_id=source.team_id AND canonical.provider='football-data.co.uk') THEN 1 ELSE 0 END),
                SUM(CASE WHEN NOT EXISTS(SELECT 1 FROM provider_team_mappings canonical WHERE canonical.team_id=source.team_id AND canonical.provider='football-data.co.uk') THEN 1 ELSE 0 END)
         FROM provider_team_mappings source WHERE source.provider='iddaa'",
        [], |x| Ok((x.get::<_,Option<i64>>(0)?.unwrap_or(0),x.get::<_,Option<i64>>(1)?.unwrap_or(0)))
    ).map_err(|e|e.to_string())?;
    let all_provider_matches: i64 = c
        .query_row(
            "SELECT COUNT(DISTINCT match_id) FROM provider_match_mappings WHERE provider='iddaa'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let (supported_matches, supported_resolved, supported_current, supported_current_resolved): (i64,i64,i64,i64) = c.query_row(
        "WITH scoped AS (
          SELECT m.*,EXISTS(SELECT 1 FROM provider_team_mappings h WHERE h.team_id=m.home_team_id AND h.provider='football-data.co.uk') AND EXISTS(SELECT 1 FROM provider_team_mappings a WHERE a.team_id=m.away_team_id AND a.provider='football-data.co.uk') resolved
          FROM matches m WHERE EXISTS(SELECT 1 FROM provider_match_mappings pm WHERE pm.match_id=m.id AND pm.provider='iddaa')
          AND EXISTS(SELECT 1 FROM provider_competition_mappings cp WHERE cp.competition_id=m.competition_id AND cp.provider='football-data.co.uk'))
         SELECT COUNT(*),COALESCE(SUM(resolved),0),COALESCE(SUM(status='scheduled' AND scheduled_local_date=?1),0),COALESCE(SUM(status='scheduled' AND scheduled_local_date=?1 AND resolved),0) FROM scoped",
        [&business_date], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))
    ).map_err(|e|e.to_string())?;
    let (global_matches, global_resolved, current_matches, current_resolved) = (
        supported_matches,
        supported_resolved,
        supported_current,
        supported_current_resolved,
    );
    let global_unresolved = global_matches - global_resolved;
    let current_unresolved = current_matches - current_resolved;
    // A mapped competition is only model-supported when both canonical team
    // identities and the minimum model history are available. Catalog/live
    // events outside that universe remain diagnostic and never block readiness.
    let production_ids = super::current_flow::production_matches(c, &now.to_rfc3339())?;
    let production_events = production_ids.len() as i64;
    let history_missing = 0;
    let mut features_missing = 0_i64;
    for id in &production_ids {
        if !features::current_snapshot_ready(c, *id)? {
            features_missing += 1;
        }
    }
    let (logos_ready,_logos_failed,logo_bytes,logo_last):(i64,i64,i64,Option<String>)=c.query_row("SELECT SUM(CASE WHEN entity_type='TEAM' AND status='READY' THEN 1 ELSE 0 END),SUM(CASE WHEN entity_type='TEAM' AND status='FAILED' THEN 1 ELSE 0 END),SUM(CASE WHEN entity_type='TEAM' AND status='READY' THEN COALESCE(byte_size,0) ELSE 0 END),MAX(CASE WHEN entity_type='TEAM' THEN updated_at END) FROM entity_assets",[],|x|Ok((x.get::<_,Option<i64>>(0)?.unwrap_or(0),x.get::<_,Option<i64>>(1)?.unwrap_or(0),x.get::<_,Option<i64>>(2)?.unwrap_or(0),x.get(3)?))).map_err(|e|e.to_string())?;
    let active_model:Option<(String,Option<String>,Option<String>)>=c.query_row("SELECT version_identifier,artifact_sha256,feature_engine_version FROM model_versions WHERE is_active=1",[],|x|Ok((x.get(0)?,x.get(1)?,x.get(2)?))).optional().map_err(|e|e.to_string())?;
    let calibration_row:Option<(String,String,String,String)>=c.query_row("SELECT calibration_version,parent_model_version,parent_artifact_sha256,artifact_path FROM calibration_models WHERE is_active=1",[],|x|Ok((x.get(0)?,x.get(1)?,x.get(2)?,x.get(3)?))).optional().map_err(|e|e.to_string())?;
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
    let historical_ok = scope.ready()
        && finished >= prediction_engine::MIN_TRAINING_SAMPLES as i64
        && runtime.historical_imported_count > 0
        && history_missing == 0;
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
    // Catalog coverage is diagnostic, never the production readiness denominator.
    historical_data.progress = None;
    historical_data
        .metrics
        .insert("supported_competitions".into(), scope.registered as i64);
    historical_data.metrics.insert(
        "required_supported_competitions".into(),
        scope.required as i64,
    );
    historical_data.metrics.insert(
        "production_history_matches".into(),
        scope.historical_matches,
    );
    if !scope.ready() {
        historical_data.status = ReadinessState::ActionRequired;
        historical_data.technical_reason = Some("PRODUCTION_SCOPE_BOOTSTRAP_INCOMPLETE".into());
        historical_data.message = "Üretim kapsamı hazırlanıyor; desteklenen lig kayıtları ve geçmiş bağlantıları henüz tamamlanmadı.".into();
    }
    historical_data
        .metrics
        .insert("production_events".into(), production_events);
    historical_data
        .metrics
        .insert("production_history_missing".into(), history_missing);
    if history_missing > 0 {
        historical_data.message =
            format!("{history_missing} gelecek desteklenen maç için gerekli geçmiş eksik.");
        historical_data.technical_reason = Some("PRODUCTION_HISTORY_MISSING".into());
    }
    let odds_age =
        parse_time(bulletin.latest_odds_snapshot_time.as_deref()).map(|x| (now - x).num_hours());
    let odds_fresh = bulletin.odds_snapshot_count > 0
        && odds_age.is_some_and(|x| (0..=candidate_engine::MAX_DAILY_ODDS_AGE_HOURS).contains(&x));
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
        if teams>0 && logos_ready==teams {ReadinessState::Ready}
        else if runtime.asset_state!="IDLE" {ReadinessState::Background}
        else {ReadinessState::Optional},
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
        Some("Kaynak bazında denemeler: logo_attempts; kalıcı eşlemeler ve yeniden deneme: logo_sources.".into()),
    );
    for (key,sql) in [
        ("queued_logos","SELECT count(*) FROM entity_assets WHERE entity_type='TEAM' AND (status='QUEUED')"),
        ("discovering_logos","SELECT count(*) FROM entity_assets WHERE entity_type='TEAM' AND (status='DOWNLOADING' AND discovery_state='DISCOVERING_SOURCE')"),
        ("downloading_logos","SELECT count(*) FROM entity_assets WHERE entity_type='TEAM' AND (status='DOWNLOADING' AND discovery_state<>'DISCOVERING_SOURCE')"),
        ("retry_later_logos","SELECT count(*) FROM entity_assets WHERE entity_type='TEAM' AND (status='MISSING' AND COALESCE(retry_state,'')<>'RATE_LIMITED')"),
        ("rate_limited_logos","SELECT count(*) FROM entity_assets WHERE entity_type='TEAM' AND (status='MISSING' AND retry_state='RATE_LIMITED')"),
        ("source_not_found_logos","SELECT count(*) FROM entity_assets WHERE entity_type='TEAM' AND (status='UNAVAILABLE' AND discovery_state='SOURCE_NOT_FOUND')"),
        ("unavailable_logos","SELECT count(*) FROM entity_assets WHERE entity_type='TEAM' AND (status='UNAVAILABLE' AND discovery_state NOT IN ('SOURCE_NOT_FOUND','INVALID'))"),
        ("invalid_logos","SELECT count(*) FROM entity_assets WHERE entity_type='TEAM' AND ((status='UNAVAILABLE' AND discovery_state='INVALID') OR status='FAILED')"),
    ] {team_logos.metrics.insert(key.into(),c.query_row(sql,[],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?);}
    if team_logos.metrics["queued_logos"]
        + team_logos.metrics["discovering_logos"]
        + team_logos.metrics["downloading_logos"]
        + team_logos.metrics["retry_later_logos"]
        + team_logos.metrics["rate_limited_logos"]
        > 0
    {
        team_logos.status = ReadinessState::Background;
        team_logos.message =
            "Kaynak keşfi ve logo indirmeleri arka planda sürüyor; üretim hazırlığını etkilemez."
                .into();
    }
    team_logos.downloaded_bytes = Some(logo_bytes.max(0) as u64);
    team_logos.metrics.extend(BTreeMap::from([
        ("total_known_teams".into(), teams),
        ("cached_valid_logos".into(), logos_ready),
    ]));
    let has_resolution_backlog = current_unresolved > 0;
    let mut entity_resolution = item(
        if !has_resolution_backlog {
            ReadinessState::Ready
        } else if global_resolved > 0 {
            ReadinessState::Partial
        } else {
            ReadinessState::ActionRequired
        },
        "Veri eşleştirme",
        if !has_resolution_backlog {
            "Kanonik eşleştirmeler hazır."
        } else {
            "Bazı kayıtlar güvenli eşleştirme veya inceleme bekliyor."
        },
        None,
        Some(global_resolved),
        None,
        false,
        has_resolution_backlog.then(|| format!("{global_unresolved} UNRESOLVED_MATCHES; {unresolved_team_mappings} UNRESOLVED_TEAMS; {pending_resolution_diagnostics} PENDING_DIAGNOSTICS")),
    );
    entity_resolution.metrics = BTreeMap::from([
        ("resolved_mapping_count".into(), global_resolved),
        ("unresolved_count".into(), global_unresolved),
        ("global_match_count".into(), global_matches),
        ("all_provider_match_count".into(), all_provider_matches),
        (
            "unsupported_match_count".into(),
            all_provider_matches - global_matches,
        ),
        (
            "supported_resolution_percent".into(),
            if global_matches > 0 {
                global_resolved * 100 / global_matches
            } else {
                0
            },
        ),
        ("resolved_team_mapping_count".into(), resolved_team_mappings),
        ("current_match_count".into(), current_matches),
        ("current_resolved_match_count".into(), current_resolved),
        ("current_unresolved_match_count".into(), current_unresolved),
    ]);
    let upcoming_model_ready:i64=c.query_row("SELECT COUNT(DISTINCT m.id) FROM matches m WHERE m.status='scheduled' AND julianday(m.kickoff_at)>julianday(?1) AND EXISTS(SELECT 1 FROM provider_match_mappings pm WHERE pm.match_id=m.id AND pm.provider='iddaa') AND EXISTS(SELECT 1 FROM predictions p JOIN model_versions mv ON mv.id=p.model_version_id WHERE p.match_id=m.id AND mv.is_active=1 AND p.public_probability IS NOT NULL)",[now.to_rfc3339()],|r|r.get(0)).map_err(|e|e.to_string())?;
    entity_resolution
        .metrics
        .insert("upcoming_model_ready".into(), upcoming_model_ready);
    let (upcoming_supported,upcoming_resolved):(i64,i64)=c.query_row("SELECT COUNT(*),COUNT(*) FROM matches m WHERE m.status='scheduled' AND julianday(m.kickoff_at)>julianday(?1) AND EXISTS(SELECT 1 FROM provider_match_mappings pm WHERE pm.match_id=m.id AND pm.provider='iddaa') AND EXISTS(SELECT 1 FROM provider_competition_mappings cp WHERE cp.competition_id=m.competition_id AND cp.provider='football-data.co.uk') AND EXISTS(SELECT 1 FROM provider_team_mappings h WHERE h.team_id=m.home_team_id AND h.provider='football-data.co.uk') AND EXISTS(SELECT 1 FROM provider_team_mappings a WHERE a.team_id=m.away_team_id AND a.provider='football-data.co.uk')",[now.to_rfc3339()],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?;
    entity_resolution
        .metrics
        .insert("upcoming_supported".into(), upcoming_supported);
    entity_resolution
        .metrics
        .insert("upcoming_resolved".into(), upcoming_resolved);
    entity_resolution.metrics.insert(
        "upcoming_unresolved".into(),
        upcoming_supported - upcoming_resolved,
    );
    entity_resolution.metrics.insert(
        "upcoming_resolution_percent".into(),
        if upcoming_supported > 0 {
            upcoming_resolved * 100 / upcoming_supported
        } else {
            0
        },
    );
    let blocking_unresolved = upcoming_supported - upcoming_resolved;
    entity_resolution.status = if scope.ready() && blocking_unresolved == 0 {
        ReadinessState::Ready
    } else {
        ReadinessState::ActionRequired
    };
    entity_resolution.message = if blocking_unresolved == 0 {
        "Güncel ve gelecek üretim kapsamındaki maçların kanonik eşleştirmeleri hazır.".into()
    } else {
        format!("{blocking_unresolved} gelecek desteklenen maç eşleştirme bekliyor.")
    };
    entity_resolution.technical_reason=Some(format!("{blocking_unresolved} PRODUCTION_UNRESOLVED; {global_unresolved} REGISTRY_UNRESOLVED; {current_unresolved} TODAY_UNRESOLVED; {pending_resolution_diagnostics} PENDING_DIAGNOSTICS"));
    if !scope.ready() {
        entity_resolution.message =
            "Desteklenen üretim kapsamı ve lig bağlantıları otomatik hazırlanıyor.".into();
        entity_resolution.technical_reason = Some("PRODUCTION_SCOPE_BOOTSTRAP_INCOMPLETE".into());
    }
    entity_resolution
        .metrics
        .insert("blocking_unresolved".into(), blocking_unresolved);
    entity_resolution.metrics.insert(
        "nonblocking_unresolved".into(),
        global_unresolved - blocking_unresolved,
    );
    let mut features = item(
        if scope.ready() && features_missing == 0 {
            ReadinessState::Ready
        } else if feature.features_generated > 0 {
            ReadinessState::Partial
        } else {
            ReadinessState::ActionRequired
        },
        "Özellik verisi",
        if production_events == 0 {
            "Hazırlanacak güncel desteklenen maç yok; özellik motoru hazır."
        } else if feature.features_generated > 0 {
            "Model özellik snapshot’ları mevcut."
        } else {
            "Henüz özellik snapshot’ı oluşturulmadı."
        },
        c.query_row("SELECT MAX(calculated_at) FROM feature_sets", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?,
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
    features
        .metrics
        .insert("production_features_missing".into(), features_missing);
    if features_missing > 0 {
        features.message =
            format!("{features_missing} gelecek desteklenen maç için özellik snapshot’ı eksik.");
        features.technical_reason = Some("PRODUCTION_FEATURES_MISSING".into());
    }
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
        (Some((version, hash, _)), Some((_, parent, parent_hash, path))) => {
            version == parent
                && hash.as_deref() == Some(parent_hash.as_str())
                && crate::repositories::calibration::load_calibration(std::path::Path::new(path))
                    .is_ok()
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
            ReadinessState::Optional
        },
        "11 modeli — isteğe bağlı",
        if lineup_model_valid {
            "Üretim 11 modeli doğrulandı ve BASE modelle uyumlu."
        } else if lineup_model_row.is_some() {
            "Aktif 11 modeli artefaktı geçersiz veya BASE modelle uyumsuz."
        } else if model_ok {
            "BASE tahmin modeli aktif. 11 verisi ve doğrulanmış 11 modeli mevcut olduğunda LINEUP_AWARE revizyon kullanılabilir. Eğitilmiş 11 artefaktı yok."
        } else {
            "11 modeli isteğe bağlıdır; eğitilmiş 11 artefaktı yok. BASE model hazırlığını ayrıca kontrol edin."
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
    lineup_model
        .metrics
        .insert("implementation_available".into(), 1);
    lineup_model.metrics.insert(
        "active_artifact_available".into(),
        i64::from(lineup_model_valid),
    );
    lineup_model.artifact_hash = lineup_model_row
        .as_ref()
        .map(|x| x.2.chars().take(12).collect());
    let provider_configured = runtime.lineup_provider_configured;
    let lineup_provider = item(
        if provider_configured && lineup_status.official_complete_lineups > 0 {
            ReadinessState::Ready
        } else {
            ReadinessState::Optional
        },
        "11 veri sağlayıcısı",
        if provider_configured && lineup_status.official_complete_lineups > 0 {
            "Canlı 11 veri sağlayıcısı hazır."
        } else if provider_configured {
            "11 sağlayıcısı yapılandırılmış; doğrulanmış tam kadro verisi henüz yok. BASE tahminler etkilenmez."
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
        (&iddaa, "Güncel bülten yok."),
        (&features, "Özellik verisi hazır değil."),
        (&prediction_model, "Aktif model hazır değil."),
        (&calibration, "Kalibrasyon hazır değil."),
    ]);
    let mut missing_predictions = 0;
    for id in &production_ids {
        let valid: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM predictions p JOIN model_versions mv ON mv.id=p.model_version_id AND mv.is_active=1 WHERE p.match_id=?1 AND p.public_probability IS NOT NULL)", [id], |r|r.get(0)).map_err(|e|e.to_string())?;
        if !valid {
            missing_predictions += 1;
        }
    }
    let can_generate_candidates = if can_generate_predictions.ready {
        let mut gate = capable(&[(&iddaa, "Güncel bülten yok."), (&odds, "Güncel oran yok.")]);
        if missing_predictions > 0 {
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
    let can_generate_coupons = if can_generate_candidates.ready {
        CapabilityReadiness {
            ready: true,
            reasons: vec![],
        }
    } else {
        CapabilityReadiness {
            ready: false,
            reasons: vec!["Önce aday hazırlığı tamamlanmalı.".into()],
        }
    };
    let publication = super::daily_selections::get(c, &business_date);
    let mut can_generate_candidates = can_generate_candidates;
    if let Err(ref error) = publication {
        can_generate_candidates.ready = false;
        can_generate_candidates
            .reasons
            .push(format!("Günlük aday yayını okunamıyor: {error}"));
    }
    let mut can_generate_coupons = can_generate_coupons;
    if !can_generate_candidates.ready || publication.is_err() {
        can_generate_coupons.ready = false;
        can_generate_coupons
            .reasons
            .push("Aday/kupon kaynağı doğrulanamadı.".into());
    }
    let pipeline_failure:Option<String>=c.query_row("SELECT error FROM daily_refresh_audit WHERE business_date=?1 AND status='FAILED' AND id=(SELECT max(id) FROM daily_refresh_audit WHERE business_date=?1)",[&business_date],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
    if let Some(error) = pipeline_failure.filter(|_| publication.is_err()) {
        can_generate_candidates.ready = false;
        can_generate_candidates
            .reasons
            .push(format!("Günlük üretim yeniden denenecek: {error}"));
        can_generate_coupons.ready = false;
        can_generate_coupons
            .reasons
            .push("Son günlük üretim tamamlanamadı; otomatik yeniden deneme bekleniyor.".into());
    }
    let core_checks = vec![
        core_check(
            "database",
            "Yerel veritabanı",
            database.status,
            &database.message,
        ),
        core_check(
            "live_iddaa",
            "Canlı İddaa ve oranlar",
            if iddaa.status == ReadinessState::Ready {
                odds.status
            } else {
                iddaa.status
            },
            "Güncel bülten ve geçerli oranlar gerekli.",
        ),
        core_check(
            "history",
            "Üretim geçmiş verisi",
            historical_data.status,
            &historical_data.message,
        ),
        core_check(
            "base_model",
            "BASE tahmin modeli",
            prediction_model.status,
            &prediction_model.message,
        ),
        core_check(
            "calibration",
            "Kalibrasyon",
            calibration.status,
            &calibration.message,
        ),
        core_check(
            "features",
            "Özellik verisi",
            features.status,
            &features.message,
        ),
        core_check(
            "resolution",
            "Güncel/gelecek eşleştirme",
            entity_resolution.status,
            &entity_resolution.message,
        ),
        core_check(
            "candidates",
            "Aday üretimi",
            if can_generate_candidates.ready {
                ReadinessState::Ready
            } else {
                ReadinessState::ActionRequired
            },
            &can_generate_candidates.reasons.join(" "),
        ),
        core_check(
            "coupons",
            "Kupon üretimi",
            if can_generate_coupons.ready {
                ReadinessState::Ready
            } else {
                ReadinessState::ActionRequired
            },
            &can_generate_coupons.reasons.join(" "),
        ),
        core_check(
            "popularity",
            "Popülerlik verisi",
            popularity.status,
            &popularity.message,
        ),
    ];
    let core_ready_count = core_checks
        .iter()
        .filter(|x| x.status == ReadinessState::Ready)
        .count();
    let core_check_count = core_checks.len();
    let overall_status = if !scope.ready() {
        ReadinessState::ActionRequired
    } else {
        core_overall(&core_checks)
    };
    Ok(DataCenterStatus {
        core_checks,
        core_ready_count,
        core_check_count,
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
