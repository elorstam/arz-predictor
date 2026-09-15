//! Versioned BASE deployment universe. Uses shipped catalog identities, never local IDs.
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

pub const MODEL_LEAGUES: &[&str] = &["E0", "E1", "SP1", "D1", "I1", "F1", "N1", "P1", "B1", "T1"];
pub const VERSION: &str = "arz-base-1.0.2-scope-v1";

#[derive(Debug, Serialize)]
pub struct Status {
    pub version: &'static str,
    pub required: usize,
    pub registered: usize,
    pub with_history: usize,
    pub historical_matches: i64,
    pub unlinked: usize,
}
impl Status {
    pub fn ready(&self) -> bool {
        self.registered == self.required && self.with_history == self.required && self.unlinked == 0
    }
}

fn links(c: &Connection) -> rusqlite::Result<Vec<(String, i64, i64)>> {
    let mut q = c.prepare("SELECT p.external_competition_id,p.competition_id,COALESCE(meta.external_name,co.name),COALESCE(meta.country,co.country) FROM provider_competition_mappings p JOIN competitions co ON co.id=p.competition_id LEFT JOIN provider_competition_metadata meta ON meta.provider=p.provider AND meta.external_competition_id=p.external_competition_id WHERE p.provider='iddaa'")?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut result = Vec::new();
    for (external, old, name, country) in rows {
        for code in MODEL_LEAGUES {
            let d = crate::providers::football_data::find_dataset(code, "2526")
                .expect("versioned deployment catalog");
            if !super::resolution::competition_alias_compatible(
                &name,
                d.competition_name,
                country.as_deref(),
                Some(d.country),
            ) {
                continue;
            }
            let target: Option<i64> = c.query_row("SELECT competition_id FROM provider_competition_mappings WHERE provider='football-data.co.uk' AND external_competition_id=?1", [code], |r| r.get(0)).optional()?;
            if let Some(target) = target {
                if old != target {
                    result.push((external.clone(), old, target));
                }
            }
        }
    }
    Ok(result)
}

pub fn status(c: &Connection) -> rusqlite::Result<Status> {
    let mut result = Status {
        version: VERSION,
        required: MODEL_LEAGUES.len(),
        registered: 0,
        with_history: 0,
        historical_matches: 0,
        unlinked: links(c)?.len(),
    };
    for code in MODEL_LEAGUES {
        let row: Option<i64> = c.query_row("SELECT (SELECT count(*) FROM matches m WHERE m.competition_id=p.competition_id AND m.status='finished' AND EXISTS(SELECT 1 FROM provider_match_mappings h WHERE h.match_id=m.id AND h.provider='football-data.co.uk')) FROM provider_competition_mappings p WHERE p.provider='football-data.co.uk' AND p.external_competition_id=?1", [code], |r| r.get(0)).optional()?;
        if let Some(count) = row {
            result.registered += 1;
            result.with_history += usize::from(count > 0);
            result.historical_matches += count;
        }
    }
    Ok(result)
}

/// Atomic and idempotent. Existing canonical identities/history are retained.
pub fn repair(c: &Connection) -> rusqlite::Result<usize> {
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    let mut changed = 0;
    for code in MODEL_LEAGUES {
        let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM provider_competition_mappings WHERE provider='football-data.co.uk' AND external_competition_id=?1)", [code], |r| r.get(0))?;
        if !exists {
            let d = crate::providers::football_data::find_dataset(code, "2526")
                .expect("versioned deployment catalog");
            super::ingestion::resolve_competition(
                &tx,
                "football-data.co.uk",
                code,
                d.competition_name,
                d.country,
                d.season,
            )?;
            changed += 1;
        }
    }
    for (external, old, target) in links(&tx)? {
        tx.execute("UPDATE matches SET competition_id=?2 WHERE competition_id=?1 AND EXISTS(SELECT 1 FROM provider_match_mappings p WHERE p.match_id=matches.id AND p.provider='iddaa')", params![old,target])?;
        tx.execute("UPDATE provider_competition_mappings SET competition_id=?2 WHERE provider='iddaa' AND external_competition_id=?1", params![external,target])?;
        changed += 1;
    }
    tx.execute("INSERT INTO daily_resolution_queue(match_id,provider_event_id,status) SELECT m.id,p.external_match_id,'NEW' FROM matches m JOIN provider_match_mappings p ON p.match_id=m.id AND p.provider='iddaa' WHERE m.status='scheduled' AND julianday(m.kickoff_at)>julianday('now') AND EXISTS(SELECT 1 FROM provider_competition_mappings h WHERE h.competition_id=m.competition_id AND h.provider='football-data.co.uk') ON CONFLICT(match_id) DO UPDATE SET status='NEW',next_retry_at=NULL WHERE daily_resolution_queue.status='UNSUPPORTED'", [])?;
    tx.commit()?;
    Ok(changed)
}

#[cfg(test)]
pub(crate) fn test_history(c: &Connection) {
    repair(c).unwrap();
    for code in MODEL_LEAGUES {
        let cid: i64 = c.query_row("SELECT competition_id FROM provider_competition_mappings WHERE provider='football-data.co.uk' AND external_competition_id=?1", [code], |r| r.get(0)).unwrap();
        let home = super::ingestion::resolve_team(
            c,
            "football-data.co.uk",
            &format!("{code} home"),
            "Test",
        )
        .unwrap()
        .id;
        let away = super::ingestion::resolve_team(
            c,
            "football-data.co.uk",
            &format!("{code} away"),
            "Test",
        )
        .unwrap()
        .id;
        for day in 1..=6 {
            super::ingestion::upsert_match(
                c,
                "football-data.co.uk",
                &format!("{code}-{day}"),
                &super::ingestion::ImportedMatch {
                    competition_id: cid,
                    season: "2025/26",
                    home_team_id: home,
                    away_team_id: away,
                    kickoff_at: &format!("2026-01-{day:02}T12:00:00Z"),
                    scheduled_local_date: &format!("2026-01-{day:02}"),
                    kickoff_time_known: true,
                    status: "finished",
                    final_home_goals: Some(2),
                    final_away_goals: Some(1),
                    halftime_home_goals: Some(1),
                    halftime_away_goals: Some(0),
                },
            )
            .unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;

    #[test]
    fn office_shape_517_out_of_scope_events_recovers_only_supported_events() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        test_history(&c);
        let cid: i64 = c.query_row("SELECT competition_id FROM provider_competition_mappings WHERE provider='football-data.co.uk' AND external_competition_id='E0'",[],|r|r.get(0)).unwrap();
        c.execute(
            "INSERT INTO competitions(name,country) VALUES('Unsupported cup','England')",
            [],
        )
        .unwrap();
        let outside = c.last_insert_rowid();
        // Distinct provider teams need incremental resolution after scope recovers.
        c.execute("INSERT INTO teams(normalized_name) VALUES('E0 home')", [])
            .unwrap();
        let home = c.last_insert_rowid();
        c.execute("INSERT INTO teams(normalized_name) VALUES('E0 away')", [])
            .unwrap();
        let away = c.last_insert_rowid();
        for i in 0..517 {
            let date =
                chrono::Utc::now() + chrono::Duration::days(1) + chrono::Duration::minutes(i);
            super::super::ingestion::upsert_match(
                &c,
                "iddaa",
                &format!("office-{i}"),
                &super::super::ingestion::ImportedMatch {
                    competition_id: if i < 2 { cid } else { outside },
                    season: "2026/27",
                    home_team_id: home,
                    away_team_id: away,
                    kickoff_at: &date.to_rfc3339(),
                    scheduled_local_date: &date.date_naive().to_string(),
                    kickoff_time_known: true,
                    status: "scheduled",
                    final_home_goals: None,
                    final_away_goals: None,
                    halftime_home_goals: None,
                    halftime_away_goals: None,
                },
            )
            .unwrap();
        }
        c.execute(
            "DELETE FROM provider_competition_mappings WHERE provider='football-data.co.uk'",
            [],
        )
        .unwrap();
        let count = || {
            c.query_row("SELECT count(*) FROM matches m JOIN provider_match_mappings p ON p.match_id=m.id AND p.provider='iddaa' WHERE EXISTS(SELECT 1 FROM provider_competition_mappings h WHERE h.competition_id=m.competition_id AND h.provider='football-data.co.uk')",[],|r|r.get::<_,i64>(0)).unwrap()
        };
        assert_eq!(count(), 0);
        assert!(!status(&c).unwrap().ready());
        assert_eq!(repair(&c).unwrap(), 10);
        assert_eq!(count(), 2);
        assert_eq!(
            super::super::incremental_resolution::process(&c, 128)
                .unwrap()
                .resolved,
            2
        );
        assert!(status(&c).unwrap().ready());
        let entities: (i64,i64,i64) = c.query_row("SELECT (SELECT count(*) FROM competitions),(SELECT count(*) FROM teams),(SELECT count(*) FROM matches)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(repair(&c).unwrap(), 0);
        assert_eq!(
            super::super::incremental_resolution::process(&c, 128)
                .unwrap()
                .processed,
            0
        );
        assert_eq!(entities,c.query_row("SELECT (SELECT count(*) FROM competitions),(SELECT count(*) FROM teams),(SELECT count(*) FROM matches)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap());
    }

    #[test]
    fn missing_scope_is_not_a_legitimate_empty_fixture_day() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        assert!(!status(&c).unwrap().ready());
        assert_eq!(repair(&c).unwrap(), 10);
        assert!(!status(&c).unwrap().ready()); // Registry alone is insufficient.
        test_history(&c);
        assert!(status(&c).unwrap().ready());
        let report =
            super::super::data_center::status(&c, &Default::default(), chrono::Utc::now()).unwrap();
        assert_eq!(
            report.entity_resolution.status,
            super::super::data_center::ReadinessState::Ready
        );
        assert_eq!(
            report.features.status,
            super::super::data_center::ReadinessState::Ready
        );
        c.execute(
            "DELETE FROM provider_competition_mappings WHERE provider='football-data.co.uk'",
            [],
        )
        .unwrap();
        let broken =
            super::super::data_center::status(&c, &Default::default(), chrono::Utc::now()).unwrap();
        assert_eq!(
            broken.overall_status,
            super::super::data_center::ReadinessState::ActionRequired
        );
        assert_eq!(
            broken.historical_data.technical_reason.as_deref(),
            Some("PRODUCTION_SCOPE_BOOTSTRAP_INCOMPLETE")
        );
        assert!(!broken.can_generate_predictions.ready);
        assert_eq!(repair(&c).unwrap(), 10);
        assert_eq!(status(&c).unwrap().historical_matches, 60);
        let before = c.total_changes();
        assert_eq!(repair(&c).unwrap(), 0);
        assert_eq!(before, c.total_changes());
    }

    #[test]
    fn existing_provider_links_recover_without_admitting_unsupported_competitions() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        let wrong = super::super::iddaa::resolve_competition(
            &c,
            &super::super::iddaa::CompetitionInput {
                external_id: "office-e0",
                name: Some("İngiltere Premier Lig"),
                country: None,
                season: Some("2026/27"),
            },
        )
        .unwrap()
        .unwrap()
        .id;
        let outside = super::super::iddaa::resolve_competition(
            &c,
            &super::super::iddaa::CompetitionInput {
                external_id: "office-cup",
                name: Some("İngiltere EFL Kupası"),
                country: None,
                season: Some("2026/27"),
            },
        )
        .unwrap()
        .unwrap()
        .id;
        test_history(&c);
        let canonical: i64 = c.query_row("SELECT competition_id FROM provider_competition_mappings WHERE provider='football-data.co.uk' AND external_competition_id='E0'",[],|r|r.get(0)).unwrap();
        assert_ne!(wrong, canonical);
        let mapped: i64 = c.query_row("SELECT competition_id FROM provider_competition_mappings WHERE provider='iddaa' AND external_competition_id='office-e0'",[],|r|r.get(0)).unwrap();
        assert_eq!(mapped, canonical);
        assert!(!c.query_row("SELECT EXISTS(SELECT 1 FROM provider_competition_mappings WHERE provider='football-data.co.uk' AND competition_id=?1)",[outside],|r|r.get::<_,bool>(0)).unwrap());
        assert_eq!(repair(&c).unwrap(), 0);
    }
}
