//! Phase 8.1A lineup data foundation.
//!
//! This module deliberately stops at immutable lineup snapshots and prior-only
//! feature metadata.  It does not alter baseline predictions and it has no live
//! provider dependency.

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const LINEUP_FEATURE_VERSION: &str = "lineup_features_v1";
pub const ADJUSTMENT_MODEL_STATUS: &str = "PENDING_PHASE_8_1B";
pub const LINEUP_CHECK_START_MINUTES_BEFORE_KICKOFF: i64 = 75;
pub const LINEUP_EXPECTED_MINUTES_BEFORE_KICKOFF: i64 = 45;
pub const LINEUP_CHECK_STOP_MINUTES_AFTER_KICKOFF: i64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LineupPlayerInput {
    pub team_id: i64,
    pub side: String,
    pub role: String,
    pub provider_player_id: Option<String>,
    pub provider_player_name: String,
    pub position: Option<String>,
    pub shirt_number: Option<i64>,
    pub formation_slot: Option<String>,
    pub captain: Option<bool>,
    pub goalkeeper: Option<bool>,
    pub date_of_birth: Option<String>,
    pub nationality: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalLineupPayload {
    pub provider: String,
    pub provider_event_id: String,
    pub match_id: i64,
    pub captured_at: String,
    pub lineup_status: String,
    pub is_official: bool,
    pub players: Vec<LineupPlayerInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LineupImportResult {
    pub snapshot_id: i64,
    pub source_hash: String,
    pub deduplicated: bool,
    pub completeness_status: String,
    pub quality_status: String,
    pub resolved_players: usize,
    pub unresolved_players: usize,
    pub starter_count_home: usize,
    pub starter_count_away: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LineupSnapshot {
    pub id: i64,
    pub match_id: i64,
    pub provider: String,
    pub provider_event_id: String,
    pub captured_at: String,
    pub lineup_status: String,
    pub is_official: bool,
    pub source_hash: String,
    pub completeness_status: String,
    pub player_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LineupFeatureSet {
    pub id: i64,
    pub match_id: i64,
    pub lineup_snapshot_id: i64,
    pub feature_version: String,
    pub quality_status: String,
    pub features: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LineupStatus {
    pub match_id: i64,
    pub status: String,
    pub lineup_snapshot_id: Option<i64>,
    pub captured_at: Option<String>,
    pub revision_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LineupEngineStatus {
    pub providers: Vec<String>,
    pub provider_availability: String,
    pub matches_waiting_today: i64,
    pub matches_inside_refresh_window: i64,
    pub official_complete_lineups: i64,
    pub partial_lineups: i64,
    pub unresolved_lineup_matches: i64,
    pub player_resolution_coverage: f64,
    pub latest_refresh: Option<String>,
    pub historical_participation_rows: i64,
    pub lineup_feature_version: String,
    pub adjustment_model_status: String,
}

/// Provider adapters can implement this interface later without coupling
/// parsing and persistence to a specific website or network client.
pub trait LineupProvider {
    fn provider_name(&self) -> &str;
    fn fetch_lineup(&self, _provider_event_id: &str) -> Result<LocalLineupPayload, String> {
        Err("LIVE_LINEUP_PROVIDER_NOT_CONFIGURED".to_string())
    }
}

pub fn normalize_name(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn source_hash(payload: &LocalLineupPayload) -> Result<String, String> {
    let mut players = payload.players.clone();
    players.sort_by_key(|p| {
        (
            p.side.clone(),
            p.role.clone(),
            p.provider_player_id.clone().unwrap_or_default(),
            normalize_name(&p.provider_player_name),
            p.team_id,
        )
    });
    let canonical = serde_json::json!({
        "provider": payload.provider,
        "event": payload.provider_event_id,
        "match": payload.match_id,
        "status": payload.lineup_status,
        "official": payload.is_official,
        "players": players,
    });
    let bytes = serde_json::to_vec(&canonical).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    digest.update(bytes);
    Ok(format!("{:x}", digest.finalize()))
}

fn completeness(payload: &LocalLineupPayload, home: i64, away: i64) -> String {
    let mut h = BTreeSet::new();
    let mut a = BTreeSet::new();
    let mut duplicate = false;
    for player in payload.players.iter().filter(|p| p.role == "STARTER") {
        let key = player
            .provider_player_id
            .clone()
            .unwrap_or_else(|| normalize_name(&player.provider_player_name));
        let set = if player.team_id == home && player.side == "HOME" {
            &mut h
        } else if player.team_id == away && player.side == "AWAY" {
            &mut a
        } else {
            continue;
        };
        if !set.insert(key) {
            duplicate = true;
        }
    }
    if payload.is_official
        && payload.lineup_status == "OFFICIAL"
        && !duplicate
        && h.len() == 11
        && a.len() == 11
    {
        "COMPLETE_OFFICIAL".to_string()
    } else if h.len() <= 11 && a.len() <= 11 {
        "PARTIAL".to_string()
    } else {
        "INVALID".to_string()
    }
}

fn resolve_player(
    tx: &Connection,
    provider: &str,
    input: &LineupPlayerInput,
    captured_at: &str,
) -> rusqlite::Result<Option<i64>> {
    if let Some(provider_id) = input.provider_player_id.as_deref() {
        if let Some(id) = tx
            .query_row(
                "SELECT canonical_player_id FROM provider_player_mappings WHERE provider=?1 AND provider_player_id=?2",
                params![provider, provider_id],
                |row| row.get(0),
            )
            .optional()? {
            return Ok(Some(id));
        }
        tx.execute(
            "INSERT INTO players(canonical_name,normalized_name,primary_position,date_of_birth,nationality,primary_team_id) VALUES(?1,?2,?3,?4,?5,?6)",
            params![input.provider_player_name, normalize_name(&input.provider_player_name), input.position, input.date_of_birth, input.nationality, input.team_id],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO provider_player_mappings(provider,provider_player_id,canonical_player_id,provider_name,first_seen_at,last_seen_at) VALUES(?1,?2,?3,?4,?5,?5)",
            params![provider, provider_id, id, input.provider_player_name, captured_at],
        )?;
        return Ok(Some(id));
    }
    let normalized = normalize_name(&input.provider_player_name);
    let mut stmt = tx.prepare("SELECT id FROM players WHERE normalized_name=?1 AND (primary_team_id=?2 OR primary_team_id IS NULL)")?;
    let ids: Vec<i64> = stmt
        .query_map(params![normalized, input.team_id], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    if ids.len() == 1 {
        Ok(Some(ids[0]))
    } else {
        Ok(None)
    }
}

pub fn import_local(
    connection: &Connection,
    payload: &LocalLineupPayload,
) -> Result<LineupImportResult, String> {
    let (home, away): (i64, i64) = connection
        .query_row(
            "SELECT home_team_id,away_team_id FROM matches WHERE id=?1",
            [payload.match_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "MATCH_NOT_FOUND".to_string())?;
    if payload
        .players
        .iter()
        .any(|p| p.team_id != home && p.team_id != away)
    {
        return Err("WRONG_MATCH_MAPPING".to_string());
    }
    let hash = source_hash(payload)?;
    if let Some(existing) = connection.query_row("SELECT id,completeness_status FROM lineup_snapshots WHERE provider=?1 AND provider_event_id=?2 AND source_hash=?3", params![payload.provider,payload.provider_event_id,hash], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?))).optional().map_err(|e|e.to_string())? {
        let (resolved, unresolved) = resolution_counts(connection, existing.0).map_err(|e|e.to_string())?;
        return Ok(LineupImportResult { snapshot_id: existing.0, source_hash: hash, deduplicated: true, completeness_status: existing.1, quality_status: if unresolved == 0 {"COMPLETE".into()} else {"PARTIAL_PLAYER_RESOLUTION".into()}, resolved_players: resolved, unresolved_players: unresolved, starter_count_home: starter_count(connection, existing.0, "HOME").map_err(|e|e.to_string())?, starter_count_away: starter_count(connection, existing.0, "AWAY").map_err(|e|e.to_string())? });
    }
    let complete = completeness(payload, home, away);
    connection.execute("INSERT INTO lineup_snapshots(match_id,provider,provider_event_id,captured_at,lineup_status,home_team_id,away_team_id,is_official,source_hash,completeness_status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)", params![payload.match_id,payload.provider,payload.provider_event_id,payload.captured_at,payload.lineup_status,home,away,payload.is_official,hash,complete]).map_err(|e|e.to_string())?;
    let snapshot_id = connection.last_insert_rowid();
    let mut resolved = 0usize;
    for p in &payload.players {
        let player_id = resolve_player(connection, &payload.provider, p, &payload.captured_at)
            .map_err(|e| e.to_string())?;
        if player_id.is_some() {
            resolved += 1;
        }
        connection.execute("INSERT INTO lineup_players(lineup_snapshot_id,team_id,player_id,provider_player_id,provider_player_name,side,role,position,shirt_number,formation_slot,captain,goalkeeper) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![snapshot_id,p.team_id,player_id,p.provider_player_id,p.provider_player_name,p.side,p.role,p.position,p.shirt_number,p.formation_slot,p.captain.unwrap_or(false),p.goalkeeper.unwrap_or(false)]).map_err(|e|e.to_string())?;
        connection.execute("INSERT OR IGNORE INTO player_match_participation(match_id,player_id,team_id,started,substitute,minutes_played,position,lineup_snapshot_id,provider_player_id) VALUES(?1,?2,?3,?4,?5,NULL,?6,?7,?8)", params![payload.match_id,player_id,p.team_id,p.role=="STARTER",p.role=="SUBSTITUTE",p.position,snapshot_id,p.provider_player_id]).map_err(|e|e.to_string())?;
    }
    let unresolved = payload.players.len() - resolved;
    Ok(LineupImportResult {
        snapshot_id,
        source_hash: hash,
        deduplicated: false,
        completeness_status: complete,
        quality_status: if unresolved == 0 {
            "COMPLETE".into()
        } else {
            "PARTIAL_PLAYER_RESOLUTION".into()
        },
        resolved_players: resolved,
        unresolved_players: unresolved,
        starter_count_home: payload
            .players
            .iter()
            .filter(|p| p.side == "HOME" && p.role == "STARTER")
            .count(),
        starter_count_away: payload
            .players
            .iter()
            .filter(|p| p.side == "AWAY" && p.role == "STARTER")
            .count(),
    })
}

fn starter_count(c: &Connection, snapshot: i64, side: &str) -> rusqlite::Result<usize> {
    c.query_row("SELECT COUNT(*) FROM lineup_players WHERE lineup_snapshot_id=?1 AND side=?2 AND role='STARTER'",params![snapshot,side],|r|r.get::<_,i64>(0)).map(|v|v as usize)
}
fn resolution_counts(c: &Connection, snapshot: i64) -> rusqlite::Result<(usize, usize)> {
    c.query_row("SELECT SUM(player_id IS NOT NULL),SUM(player_id IS NULL) FROM lineup_players WHERE lineup_snapshot_id=?1",[snapshot],|r|Ok((r.get::<_,Option<i64>>(0)?.unwrap_or(0) as usize,r.get::<_,Option<i64>>(1)?.unwrap_or(0) as usize)))
}

pub fn latest(connection: &Connection, match_id: i64) -> Result<Option<LineupSnapshot>, String> {
    connection.query_row("SELECT s.id,s.match_id,s.provider,s.provider_event_id,s.captured_at,s.lineup_status,s.is_official,s.source_hash,s.completeness_status,(SELECT COUNT(*) FROM lineup_players p WHERE p.lineup_snapshot_id=s.id) FROM lineup_snapshots s WHERE s.match_id=?1 ORDER BY s.captured_at DESC,s.id DESC LIMIT 1",[match_id],|r|Ok(LineupSnapshot{id:r.get(0)?,match_id:r.get(1)?,provider:r.get(2)?,provider_event_id:r.get(3)?,captured_at:r.get(4)?,lineup_status:r.get(5)?,is_official:r.get(6)?,source_hash:r.get(7)?,completeness_status:r.get(8)?,player_count:r.get::<_,i64>(9)? as usize})).optional().map_err(|e|e.to_string())
}

pub fn history(connection: &Connection, match_id: i64) -> Result<Vec<LineupSnapshot>, String> {
    let mut stmt = connection.prepare("SELECT s.id,s.match_id,s.provider,s.provider_event_id,s.captured_at,s.lineup_status,s.is_official,s.source_hash,s.completeness_status,(SELECT COUNT(*) FROM lineup_players p WHERE p.lineup_snapshot_id=s.id) FROM lineup_snapshots s WHERE s.match_id=?1 ORDER BY s.captured_at,s.id").map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map([match_id], |r| {
            Ok(LineupSnapshot {
                id: r.get(0)?,
                match_id: r.get(1)?,
                provider: r.get(2)?,
                provider_event_id: r.get(3)?,
                captured_at: r.get(4)?,
                lineup_status: r.get(5)?,
                is_official: r.get(6)?,
                source_hash: r.get(7)?,
                completeness_status: r.get(8)?,
                player_count: r.get::<_, i64>(9)? as usize,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

pub fn status_for_match(
    connection: &Connection,
    match_id: i64,
    now: &str,
) -> Result<LineupStatus, String> {
    let (kickoff, known): (String, bool) = connection
        .query_row(
            "SELECT kickoff_at,kickoff_time_known FROM matches WHERE id=?1",
            [match_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "MATCH_NOT_FOUND".to_string())?;
    let latest = latest(connection, match_id)?;
    let complete = latest
        .as_ref()
        .is_some_and(|s| s.completeness_status == "COMPLETE_OFFICIAL");
    let status = if latest.is_some() && !complete {
        "PARTIAL_LINEUP".to_string()
    } else {
        readiness(&kickoff, known, now, complete)
    };
    Ok(LineupStatus {
        match_id,
        status: status.clone(),
        lineup_snapshot_id: latest.as_ref().map(|s| s.id),
        captured_at: latest.as_ref().map(|s| s.captured_at.clone()),
        revision_status: if complete {
            "REVISION_PENDING_MODEL".into()
        } else {
            "UNAVAILABLE".into()
        },
    })
}

pub fn readiness(kickoff: &str, known: bool, now: &str, complete: bool) -> String {
    if complete {
        return "OFFICIAL_LINEUP_READY".into();
    }
    if !known {
        return "WAITING_FOR_LINEUP".into();
    }
    let Ok(k) = DateTime::parse_from_rfc3339(kickoff) else {
        return "WAITING_FOR_LINEUP".into();
    };
    let Ok(n) = DateTime::parse_from_rfc3339(now) else {
        return "WAITING_FOR_LINEUP".into();
    };
    let delta = k.with_timezone(&Utc) - n.with_timezone(&Utc);
    if delta > Duration::minutes(LINEUP_CHECK_START_MINUTES_BEFORE_KICKOFF) {
        "NOT_EXPECTED_YET".into()
    } else if delta >= Duration::minutes(-LINEUP_CHECK_STOP_MINUTES_AFTER_KICKOFF) {
        "WAITING_FOR_LINEUP".into()
    } else {
        "LINEUP_REVISION_UNAVAILABLE".into()
    }
}

pub fn generate_features(
    connection: &Connection,
    match_id: i64,
) -> Result<LineupFeatureSet, String> {
    let snapshot = latest(connection, match_id)?.ok_or_else(|| "LINEUP_NOT_FOUND".to_string())?;
    if snapshot.completeness_status != "COMPLETE_OFFICIAL" {
        return Err("LINEUP_NOT_COMPLETE".into());
    }
    let (kickoff, home, away): (String, i64, i64) = connection
        .query_row(
            "SELECT kickoff_at,home_team_id,away_team_id FROM matches WHERE id=?1",
            [match_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    let mut counts: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for (label, team) in [("home", home), ("away", away)] {
        let rows: Vec<(Option<i64>,bool,Option<i64>,Option<String>)> = connection.prepare("SELECT p.player_id,p.started,p.minutes_played,p.position FROM player_match_participation p JOIN matches m ON m.id=p.match_id WHERE p.team_id=?1 AND m.kickoff_at < ?2 ORDER BY m.kickoff_at,p.id").map_err(|e|e.to_string())?.query_map(params![team,kickoff],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(|e|e.to_string())?.collect::<Result<_,_>>().map_err(|e|e.to_string())?;
        let starters = rows.iter().filter(|r| r.1).count();
        let unique_players: BTreeSet<i64> = rows.iter().filter_map(|r| r.0).collect();
        let prior_matches: i64 = connection.query_row("SELECT COUNT(*) FROM matches WHERE (home_team_id=?1 OR away_team_id=?1) AND kickoff_at < ?2 AND status='finished'",params![team,kickoff],|r|r.get(0)).map_err(|e|e.to_string())?;
        let latest_starts: BTreeSet<i64> = rows
            .iter()
            .rev()
            .filter(|r| r.1)
            .filter_map(|r| r.0)
            .take(5)
            .collect();
        let lineup: Vec<(Option<i64>,Option<String>,bool)> = connection.prepare("SELECT player_id,position,goalkeeper FROM lineup_players WHERE lineup_snapshot_id=?1 AND side=?2 AND role='STARTER' ORDER BY id").map_err(|e|e.to_string())?.query_map(params![snapshot.id, if label=="home"{"HOME"}else{"AWAY"}],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|e|e.to_string())?.collect::<Result<_,_>>().map_err(|e|e.to_string())?;
        let resolved = lineup.iter().filter(|r| r.0.is_some()).count();
        let continuity = lineup
            .iter()
            .filter(|r| r.0.is_some_and(|id| latest_starts.contains(&id)))
            .count();
        let goalkeeper = lineup
            .iter()
            .filter(|r| r.2)
            .filter(|r| r.0.is_some_and(|id| unique_players.contains(&id)))
            .count();
        let mut positions = BTreeMap::<String, usize>::new();
        for (_, pos, _) in &lineup {
            if let Some(p) = pos {
                *positions.entry(p.clone()).or_default() += 1;
            }
        }
        counts.insert(label.into(),serde_json::json!({"starter_continuity_count":continuity,"changed_starters":11-continuity,"regular_starter_presence":if prior_matches==0 {0.0} else {continuity as f64/11.0},"recent_start_share":if starters==0 {0.0} else {latest_starts.len() as f64/5.0},"goalkeeper_continuity":goalkeeper,"position_counts":positions,"prior_appearance_strength":unique_players.len(),"prior_starts":starters,"prior_match_sample":prior_matches,"resolution_count":resolved}));
    }
    let home_resolved = counts["home"]["resolution_count"].as_u64().unwrap_or(0);
    let away_resolved = counts["away"]["resolution_count"].as_u64().unwrap_or(0);
    let prior_sample = counts["home"]["prior_match_sample"].as_i64().unwrap_or(0)
        + counts["away"]["prior_match_sample"].as_i64().unwrap_or(0);
    let quality = if home_resolved == 11 && away_resolved == 11 && prior_sample > 0 {
        "COMPLETE"
    } else if home_resolved == 11 && away_resolved == 11 {
        "INSUFFICIENT_PLAYER_HISTORY"
    } else {
        "PARTIAL_PLAYER_RESOLUTION"
    };
    let features = serde_json::json!({"version":LINEUP_FEATURE_VERSION,"home":counts["home"],"away":counts["away"],"home_minus_away_continuity":counts["home"]["starter_continuity_count"].as_i64().unwrap_or(0)-counts["away"]["starter_continuity_count"].as_i64().unwrap_or(0)});
    let serialized = serde_json::to_string(&features).map_err(|e| e.to_string())?;
    let id: i64 = connection.query_row("SELECT id FROM lineup_feature_sets WHERE match_id=?1 AND lineup_snapshot_id=?2 AND feature_version=?3",params![match_id,snapshot.id,LINEUP_FEATURE_VERSION],|r|r.get(0)).optional().map_err(|e|e.to_string())?.unwrap_or_else(|| { connection.execute("INSERT INTO lineup_feature_sets(match_id,lineup_snapshot_id,feature_version,quality_status,home_resolution_count,away_resolution_count,home_starter_count,away_starter_count,historical_match_sample,minutes_coverage,position_coverage,features_json) VALUES(?1,?2,?3,?4,?5,?6,11,11,?7,0,1,?8)",params![match_id,snapshot.id,LINEUP_FEATURE_VERSION,quality,home_resolved,away_resolved,counts["home"]["prior_match_sample"].as_i64().unwrap_or(0)+counts["away"]["prior_match_sample"].as_i64().unwrap_or(0),serialized]).unwrap(); connection.last_insert_rowid() });
    Ok(LineupFeatureSet {
        id,
        match_id,
        lineup_snapshot_id: snapshot.id,
        feature_version: LINEUP_FEATURE_VERSION.into(),
        quality_status: quality.into(),
        features,
    })
}

pub fn pending_revision_for_match(
    connection: &Connection,
    match_id: i64,
    lineup_snapshot_id: i64,
) -> Result<usize, String> {
    let feature_id = connection.query_row("SELECT id FROM lineup_feature_sets WHERE match_id=?1 AND lineup_snapshot_id=?2 AND feature_version=?3",params![match_id,lineup_snapshot_id,LINEUP_FEATURE_VERSION],|r|r.get::<_,i64>(0)).optional().map_err(|e|e.to_string())?;
    let Some(feature_id) = feature_id else {
        return Ok(0);
    };
    let mut stmt=connection.prepare("SELECT p.id,p.model_version_id,mv.version_identifier,p.calibration_version FROM predictions p JOIN model_versions mv ON mv.id=p.model_version_id WHERE p.match_id=?1").map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map([match_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut n = 0;
    for row in rows {
        let (prediction, _, model, calibration) = row.map_err(|e| e.to_string())?;
        n += connection.execute("INSERT OR IGNORE INTO prediction_revisions(prediction_id,match_id,revision_type,parent_prediction_id,lineup_snapshot_id,lineup_feature_set_id,generated_at,revision_reason,model_version,calibration_version,revision_status,revised_probability) VALUES(?1,?2,'LINEUP_AWARE_PREMATCH',?1,?3,?4,?5,'OFFICIAL_LINEUP_PENDING_MODEL',?6,?7,'REVISION_PENDING_MODEL',NULL)",params![prediction,match_id,lineup_snapshot_id,feature_id,Utc::now().to_rfc3339(),model,calibration]).map_err(|e|e.to_string())?;
    }
    Ok(n)
}

pub fn engine_status(connection: &Connection) -> Result<LineupEngineStatus, String> {
    let official:i64=connection.query_row("SELECT COUNT(DISTINCT match_id) FROM lineup_snapshots WHERE completeness_status='COMPLETE_OFFICIAL'",[],|r|r.get(0)).map_err(|e|e.to_string())?;
    let partial:i64=connection.query_row("SELECT COUNT(DISTINCT match_id) FROM lineup_snapshots WHERE completeness_status<>'COMPLETE_OFFICIAL'",[],|r|r.get(0)).map_err(|e|e.to_string())?;
    let rows: i64 = connection
        .query_row("SELECT COUNT(*) FROM player_match_participation", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    let total: i64 = connection
        .query_row("SELECT COUNT(*) FROM lineup_players", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let resolved: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM lineup_players WHERE player_id IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(LineupEngineStatus {
        providers: vec!["local".into()],
        provider_availability: "LIVE_LINEUP_PROVIDER_NOT_CONFIGURED".into(),
        matches_waiting_today: 0,
        matches_inside_refresh_window: 0,
        official_complete_lineups: official,
        partial_lineups: partial,
        unresolved_lineup_matches: if total > resolved { 1 } else { 0 },
        player_resolution_coverage: if total == 0 {
            0.0
        } else {
            resolved as f64 / total as f64
        },
        latest_refresh: connection
            .query_row("SELECT MAX(captured_at) FROM lineup_snapshots", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())?
            .flatten(),
        historical_participation_rows: rows,
        lineup_feature_version: LINEUP_FEATURE_VERSION.into(),
        adjustment_model_status: ADJUSTMENT_MODEL_STATUS.into(),
    })
}

pub fn due_matches(connection: &Connection, now: &str) -> Result<Vec<i64>, String> {
    let mut stmt = connection.prepare("SELECT m.id FROM matches m WHERE m.kickoff_time_known=1 AND m.status IN ('scheduled','postponed') AND datetime(m.kickoff_at) <= datetime(?1, '+' || ?2 || ' minutes') AND datetime(m.kickoff_at) >= datetime(?1, '-' || ?3 || ' minutes') AND NOT EXISTS (SELECT 1 FROM lineup_snapshots s WHERE s.match_id=m.id AND s.completeness_status='COMPLETE_OFFICIAL') ORDER BY m.kickoff_at,m.id").map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map(
            params![
                now,
                LINEUP_CHECK_START_MINUTES_BEFORE_KICKOFF,
                LINEUP_CHECK_STOP_MINUTES_AFTER_KICKOFF
            ],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

pub fn refresh_match(
    _connection: &Connection,
    _match_id: i64,
) -> Result<LineupImportResult, String> {
    Err("LIVE_LINEUP_PROVIDER_NOT_CONFIGURED".to_string())
}
