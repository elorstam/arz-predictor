//! One coverage interpretation shared by bulletin, readiness and popularity.
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CoverageState {
    ModelUnsupported,
    ResolutionFailed,
    ModelInputMissing,
    ModelSupported,
    ModelReady,
}

pub fn classify(c: &Connection, match_id: i64) -> rusqlite::Result<CoverageState> {
    let identity: Option<(bool,bool)> = c.query_row("SELECT EXISTS(SELECT 1 FROM provider_competition_mappings p WHERE p.competition_id=m.competition_id AND p.provider='football-data.co.uk'), EXISTS(SELECT 1 FROM provider_team_mappings p WHERE p.team_id=m.home_team_id AND p.provider='football-data.co.uk') AND EXISTS(SELECT 1 FROM provider_team_mappings p WHERE p.team_id=m.away_team_id AND p.provider='football-data.co.uk') FROM matches m WHERE m.id=?1", [match_id], |r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    match identity {
        Some((false, _)) => return Ok(CoverageState::ModelUnsupported),
        Some((true, false)) | None => return Ok(CoverageState::ResolutionFailed),
        _ => {}
    }
    let row: Option<(bool, bool, bool, bool)> = c.query_row(
        "SELECT
          EXISTS(SELECT 1 FROM provider_competition_mappings p WHERE p.competition_id=m.competition_id AND p.provider='football-data.co.uk'),
          EXISTS(SELECT 1 FROM provider_team_mappings p WHERE p.team_id=m.home_team_id AND p.provider='football-data.co.uk') AND
          EXISTS(SELECT 1 FROM provider_team_mappings p WHERE p.team_id=m.away_team_id AND p.provider='football-data.co.uk'),
          (SELECT COUNT(*) FROM matches h WHERE h.status='finished' AND h.kickoff_at<m.kickoff_at AND (h.home_team_id=m.home_team_id OR h.away_team_id=m.home_team_id))>=5 AND
          (SELECT COUNT(*) FROM matches h WHERE h.status='finished' AND h.kickoff_at<m.kickoff_at AND (h.home_team_id=m.away_team_id OR h.away_team_id=m.away_team_id))>=5,
          EXISTS(SELECT 1 FROM predictions p WHERE p.match_id=m.id AND p.public_probability IS NOT NULL)
         FROM matches m WHERE m.id=?1", [match_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))
    ).optional()?;
    Ok(match row {
        Some((false, _, _, _)) => CoverageState::ModelUnsupported,
        Some((true, false, _, _)) | None => CoverageState::ResolutionFailed,
        Some((true, true, false, _)) => CoverageState::ModelInputMissing,
        Some((true, true, true, true)) => CoverageState::ModelReady,
        Some((true, true, true, false)) => CoverageState::ModelSupported,
    })
}
