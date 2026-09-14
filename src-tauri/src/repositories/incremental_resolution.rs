//! Current bulletin identities only. Never invokes the historical resolver scan/merge.
use super::{iddaa::Resolution, resolution};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

fn canonical(c: &Connection, id: i64) -> rusqlite::Result<bool> {
    c.query_row("SELECT EXISTS(SELECT 1 FROM provider_team_mappings WHERE team_id=?1 AND provider='football-data.co.uk')",[id],|r|r.get(0))
}
fn supported(c: &Connection, id: i64) -> rusqlite::Result<bool> {
    c.query_row("SELECT EXISTS(SELECT 1 FROM provider_competition_mappings WHERE competition_id=?1 AND provider='football-data.co.uk')",[id],|r|r.get(0))
}
fn learn(
    c: &Connection,
    comp: i64,
    name: &str,
    team: i64,
    method: &str,
    score: f64,
) -> rusqlite::Result<()> {
    c.execute("INSERT INTO provider_scoped_team_aliases(provider,competition_id,normalized_alias,source_name,team_id,method,confidence) VALUES('iddaa',?1,?2,?3,?4,?5,?6) ON CONFLICT(provider,competition_id,normalized_alias) DO NOTHING",params![comp,resolution::normalize_team_name(name),name,team,method,score])?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct Choice {
    pub team_id: i64,
    pub name: String,
    pub confidence: f64,
}
fn choices(c: &Connection, comp: i64, name: &str) -> rusqlite::Result<Vec<Choice>> {
    let key = resolution::normalize_team_name(name);
    let target = resolution::provider_team_key("iddaa", &key);
    let mut q=c.prepare("SELECT DISTINCT t.id,t.normalized_name,EXISTS(SELECT 1 FROM matches m WHERE m.competition_id=?1 AND (m.home_team_id=t.id OR m.away_team_id=t.id)) FROM teams t JOIN provider_team_mappings p ON p.team_id=t.id AND p.provider='football-data.co.uk' WHERE EXISTS(SELECT 1 FROM matches m JOIN competitions co ON co.id=m.competition_id WHERE (m.home_team_id=t.id OR m.away_team_id=t.id) AND (co.id=?1 OR (co.country IS NOT NULL AND co.country=(SELECT country FROM competitions WHERE id=?1))))")?;
    let rows = q
        .query_map([comp], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = Vec::new();
    // Exact identities can follow promotion/relegation inside the same country.
    // Fuzzy comparisons remain strictly inside the event's competition.
    for (id, n, in_competition) in rows {
        let nk = resolution::normalize_team_name(&n);
        let score = if nk == target {
            1.0
        } else if in_competition {
            resolution::name_similarity(&key, &nk)
        } else {
            0.0
        };
        if score >= 0.75 {
            out.push(Choice {
                team_id: id,
                name: n,
                confidence: score,
            })
        }
    }
    out.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then(a.team_id.cmp(&b.team_id))
    });
    out.truncate(5);
    Ok(out)
}
fn safe_choice(rows: &[Choice]) -> Option<&Choice> {
    rows.first().filter(|best| {
        best.confidence >= 0.92
            && rows
                .get(1)
                .is_none_or(|second| best.confidence - second.confidence >= 0.05)
    })
}

/// Event IDs are authoritative identities; the feed has no documented team IDs.
/// A known event's canonical home/away slot survives provider spelling changes.
pub fn team(
    c: &Connection,
    event: &str,
    comp: i64,
    name: &str,
    home: bool,
    country: Option<&str>,
) -> rusqlite::Result<Resolution> {
    if !supported(c, comp)? {
        return super::iddaa::resolve_team(c, name, country);
    }
    let previous:Option<i64>=c.query_row("SELECT CASE WHEN ?3 THEN m.home_team_id ELSE m.away_team_id END FROM provider_match_mappings p JOIN matches m ON m.id=p.match_id WHERE p.provider='iddaa' AND p.external_match_id=?1 AND m.competition_id=?2",params![event,comp,home],|r|r.get(0)).optional()?;
    if let Some(id) = previous {
        if canonical(c, id)? {
            learn(c, comp, name, id, "PROVIDER_EVENT_SLOT", 1.0)?;
            return Ok(Resolution { id, created: false });
        }
    }
    let scoped:Option<i64>=c.query_row("SELECT team_id FROM provider_scoped_team_aliases WHERE provider='iddaa' AND competition_id=?1 AND normalized_alias=?2",params![comp,resolution::normalize_team_name(name)],|r|r.get(0)).optional()?;
    if let Some(id) = scoped {
        return Ok(Resolution { id, created: false });
    }
    let mapped:Option<i64>=c.query_row("SELECT p.team_id FROM provider_team_mappings p WHERE p.provider='iddaa' AND p.external_team_id=?1 AND EXISTS(SELECT 1 FROM provider_team_mappings h WHERE h.team_id=p.team_id AND h.provider='football-data.co.uk') AND EXISTS(SELECT 1 FROM matches m WHERE m.competition_id=?2 AND (m.home_team_id=p.team_id OR m.away_team_id=p.team_id))",params![name,comp],|r|r.get(0)).optional()?;
    if let Some(id) = mapped {
        learn(c, comp, name, id, "PROVIDER_MAPPING", 1.0)?;
        return Ok(Resolution { id, created: false });
    }
    // A literal canonical identity in this competition is stronger evidence
    // than a normalized name shared by multiple teams (for example FC suffixes).
    let exact: Option<i64> = c.query_row(
        "SELECT t.id FROM teams t WHERE t.normalized_name=?1 AND t.country=?2
         AND EXISTS(SELECT 1 FROM provider_team_mappings p WHERE p.team_id=t.id AND p.provider='football-data.co.uk')
         AND EXISTS(SELECT 1 FROM matches m WHERE m.competition_id=?3 AND (m.home_team_id=t.id OR m.away_team_id=t.id))",
        params![name,country,comp], |r|r.get(0),
    ).optional()?;
    if let Some(id) = exact {
        return Ok(Resolution { id, created: false });
    }
    let rows = choices(c, comp, name)?;
    if let Some(best) = safe_choice(&rows) {
        learn(
            c,
            comp,
            name,
            best.team_id,
            if best.confidence == 1.0 {
                "EXACT_OR_CURATED_ALIAS"
            } else {
                "COMPETITION_SAFE_FUZZY"
            },
            best.confidence,
        )?;
        return Ok(Resolution {
            id: best.team_id,
            created: false,
        });
    }
    // Unknown names are isolated by competition; they cannot pollute global aliases.
    let key = format!(
        "competition:{comp}:name:{}",
        resolution::normalize_team_name(name)
    );
    let id:Option<i64>=c.query_row("SELECT team_id FROM provider_team_mappings WHERE provider='iddaa' AND external_team_id=?1",[&key],|r|r.get(0)).optional()?;
    if let Some(id) = id {
        return Ok(Resolution { id, created: false });
    }
    c.execute(
        "INSERT INTO teams(normalized_name,country) VALUES(?1,?2)",
        params![name, country],
    )?;
    let id = c.last_insert_rowid();
    c.execute("INSERT INTO provider_team_mappings(provider,external_team_id,external_team_name,team_id) VALUES('iddaa',?1,?2,?3)",params![key,name,id])?;
    Ok(Resolution { id, created: true })
}

pub fn enqueue(c: &Connection, id: i64, event: &str) -> rusqlite::Result<()> {
    let relevant:bool=c.query_row("SELECT status='scheduled' AND kickoff_at>strftime('%Y-%m-%dT%H:%M:%SZ','now') AND EXISTS(SELECT 1 FROM provider_competition_mappings p WHERE p.competition_id=m.competition_id AND p.provider='football-data.co.uk') FROM matches m WHERE id=?1",[id],|r|r.get(0))?;
    if relevant {
        c.execute("INSERT INTO daily_resolution_queue(match_id,provider_event_id,status) VALUES(?1,?2,'NEW') ON CONFLICT(match_id) DO NOTHING",params![id,event])?;
    }
    Ok(())
}
#[derive(Default, Debug, Serialize)]
pub struct Batch {
    pub processed: usize,
    pub resolved: usize,
    pub manual_review: usize,
    pub remaining: i64,
    pub elapsed_ms: u128,
}
pub fn process(c: &Connection, limit: usize) -> rusqlite::Result<Batch> {
    let started = std::time::Instant::now();
    let mut result = Batch::default();
    c.execute("UPDATE daily_resolution_queue SET status='RETRY_LATER',next_retry_at=NULL,reason='Interrupted work recovered' WHERE status='AUTO_RESOLVING' AND last_attempt_at<strftime('%Y-%m-%dT%H:%M:%fZ','now','-10 minutes')",[])?;
    let mut q=c.prepare("SELECT q.match_id,q.provider_event_id,m.competition_id,m.home_team_id,m.away_team_id,h.normalized_name,a.normalized_name FROM daily_resolution_queue q JOIN matches m ON m.id=q.match_id JOIN teams h ON h.id=m.home_team_id JOIN teams a ON a.id=m.away_team_id WHERE q.status IN ('NEW','RETRY_LATER','AMBIGUOUS','MANUAL_REVIEW') AND (q.next_retry_at IS NULL OR q.next_retry_at<=strftime('%Y-%m-%dT%H:%M:%fZ','now')) AND m.status='scheduled' AND m.kickoff_at>strftime('%Y-%m-%dT%H:%M:%SZ','now') ORDER BY m.kickoff_at,q.match_id LIMIT ?1")?;
    let rows = q
        .query_map([limit.min(128) as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(q);
    for (id, event, comp, home, away, hn, an) in rows {
        c.execute("UPDATE daily_resolution_queue SET status='AUTO_RESOLVING',attempts=attempts+1,last_attempt_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE match_id=?1",[id])?;
        let attempt = (|| -> rusqlite::Result<(String, serde_json::Value)> {
            if !supported(c, comp)? {
                return Ok(("UNSUPPORTED".into(), serde_json::json!([])));
            }
            let h = if canonical(c, home)? {
                home
            } else {
                team(c, &event, comp, &hn, true, None)?.id
            };
            let a = if canonical(c, away)? {
                away
            } else {
                team(c, &event, comp, &an, false, None)?.id
            };
            if h != a && canonical(c, h)? && canonical(c, a)? {
                c.execute(
                    "UPDATE matches SET home_team_id=?2,away_team_id=?3 WHERE id=?1",
                    params![id, h, a],
                )?;
                return Ok(("RESOLVED".into(), serde_json::json!([])));
            }
            Ok((
                "MANUAL_REVIEW".into(),
                serde_json::json!({"home":choices(c,comp,&hn)?,"away":choices(c,comp,&an)?,"home_provider_key":hn,"away_provider_key":an,"reason":if h==a{"SELF_MATCH_REJECTED"}else{"NO_UNIQUE_HIGH_CONFIDENCE_IDENTITY"}}),
            ))
        })();
        let (status, evidence, reason) = match attempt {
            Ok((s, e)) => (s, e, None),
            Err(e) => (
                "RETRY_LATER".into(),
                serde_json::json!([]),
                Some(e.to_string()),
            ),
        };
        c.execute("UPDATE daily_resolution_queue SET status=?2,evidence_json=?3,reason=?4,next_retry_at=CASE WHEN ?2='RETRY_LATER' THEN strftime('%Y-%m-%dT%H:%M:%fZ','now','+5 minutes') WHEN ?2='MANUAL_REVIEW' THEN strftime('%Y-%m-%dT%H:%M:%fZ','now','+1 day') ELSE NULL END,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE match_id=?1",params![id,status,evidence.to_string(),reason])?;
        result.processed += 1;
        if status == "RESOLVED" {
            result.resolved += 1
        }
        if status == "MANUAL_REVIEW" {
            result.manual_review += 1
        }
    }
    result.remaining=c.query_row("SELECT count(*) FROM daily_resolution_queue q JOIN matches m ON m.id=q.match_id WHERE q.status IN ('NEW','AUTO_RESOLVING','RETRY_LATER','AMBIGUOUS','MANUAL_REVIEW') AND m.status='scheduled' AND m.kickoff_at>strftime('%Y-%m-%dT%H:%M:%SZ','now') AND EXISTS(SELECT 1 FROM provider_competition_mappings p WHERE p.competition_id=m.competition_id AND p.provider='football-data.co.uk')",[],|r|r.get(0))?;
    result.elapsed_ms = started.elapsed().as_millis();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    fn seed(c: &Connection, name: &str, comp: i64) -> i64 {
        c.execute(
            "INSERT INTO teams(normalized_name,country) VALUES(?1,'Germany')",
            [name],
        )
        .unwrap();
        let id = c.last_insert_rowid();
        c.execute("INSERT INTO provider_team_mappings(provider,external_team_id,external_team_name,team_id) VALUES('football-data.co.uk',?1,?1,?2)",params![format!("{comp}:{name}"),id]).unwrap();
        c.execute("INSERT INTO matches(competition_id,season,home_team_id,away_team_id,kickoff_at,status) VALUES(?1,'2098',?2,1,'2098-01-01T12:00:00Z','finished')",params![comp,id]).unwrap();
        id
    }
    fn event(c: &Connection, comp: i64, key: &str, home: &str, away: &str, day: i64) -> i64 {
        let h = team(c, key, comp, home, true, None).unwrap().id;
        let a = team(c, key, comp, away, false, None).unwrap().id;
        let date = format!("2099-01-{day:02}");
        let (id, _) = super::super::iddaa::upsert_match(
            c,
            &super::super::iddaa::MatchInput {
                external_event_id: key,
                competition_id: comp,
                season: "2099",
                home_team_id: h,
                away_team_id: a,
                kickoff_at: &format!("{date}T12:00:00Z"),
                local_date: &date,
            },
        )
        .unwrap();
        enqueue(c, id, key).unwrap();
        id
    }
    #[test]
    fn literal_identity_survives_normalized_collision_without_learning_ambiguous_alias() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        c.execute(
            "INSERT INTO teams(id,normalized_name) VALUES(1,'Opponent')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO competitions(id,name,country) VALUES(1,'Bundesliga','Germany')",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO provider_competition_mappings(provider,external_competition_id,competition_id) VALUES('football-data.co.uk','D1',1)", []).unwrap();
        let plain = seed(&c, "United", 1);
        let suffixed = seed(&c, "United FC", 1);
        assert!(safe_choice(&choices(&c, 1, "United FC").unwrap()).is_none());
        let before: i64 = c
            .query_row("SELECT COUNT(*) FROM teams", [], |r| r.get(0))
            .unwrap();
        for (name, expected) in [("United", plain), ("United FC", suffixed)] {
            let selected = team(&c, "new", 1, name, true, Some("Germany")).unwrap();
            assert_eq!(selected.id, expected);
            assert!(!selected.created);
        }
        assert_eq!(
            c.query_row::<i64, _, _>("SELECT COUNT(*) FROM teams", [], |r| r.get(0))
                .unwrap(),
            before
        );
        assert_eq!(
            c.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM provider_scoped_team_aliases",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            0
        );
        assert!(safe_choice(&choices(&c, 1, "FC United").unwrap()).is_none());
        let ambiguous = team(&c, "ambiguous", 1, "FC United", true, Some("Germany")).unwrap();
        assert_ne!(ambiguous.id, plain);
        assert_ne!(ambiguous.id, suffixed);
    }

    #[test]
    fn four_days_reuse_learn_new_teams_and_isolate_real_ambiguity() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        c.execute(
            "INSERT INTO teams(id,normalized_name) VALUES(1,'Historical opponent')",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO competitions(id,name,country) VALUES(1,'Bundesliga','Germany'),(2,'Unrelated league','Other')",[]).unwrap();
        c.execute("INSERT INTO provider_competition_mappings(provider,external_competition_id,competition_id) VALUES('football-data.co.uk','D1',1),('football-data.co.uk','OTHER',2)",[]).unwrap();
        let bayern = seed(&c, "Bayern Munich", 1);
        let dortmund = seed(&c, "Dortmund", 1);
        let d1 = event(&c, 1, "day1", "Bayern Münih", "B Dortmund", 1);
        let batch = process(&c, 2).unwrap();
        assert_eq!(batch.remaining, 0);
        assert_eq!(batch.resolved, 1);
        let ids: (i64, i64) = c
            .query_row(
                "SELECT home_team_id,away_team_id FROM matches WHERE id=?1",
                [d1],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(ids, (bayern, dortmund));
        // Same stable event ID, new provider spelling: authoritative slot reused.
        let reused = event(&c, 1, "day1", "Bayern M.", "Dortmund B.", 1);
        assert_eq!(reused, d1);
        let changes = c.total_changes();
        let same = team(&c, "new-event", 1, "Bayern M.", true, None).unwrap();
        assert_eq!(same.id, bayern);
        assert_eq!(changes, c.total_changes());
        let d2 = event(&c, 1, "day2", "Bayern M.", "Dortmund B.", 2);
        assert_ne!(d2, d1);
        assert_eq!(process(&c, 2).unwrap().remaining, 0);
        c.execute(
            "INSERT INTO competitions(id,name,country) VALUES(3,'Bundesliga 2','Germany')",
            [],
        )
        .unwrap();
        let new = seed(&c, "Elversberg", 3); // Newly promoted: exact identity, same country.
        let d3 = event(&c, 1, "day3", "Elversberg", "Bayern M.", 3);
        assert_eq!(
            c.query_row::<i64, _, _>("SELECT home_team_id FROM matches WHERE id=?1", [d3], |r| r
                .get(0))
                .unwrap(),
            new
        );
        assert_eq!(process(&c, 2).unwrap().remaining, 0);
        seed(&c, "United City A", 1);
        seed(&c, "United City B", 1);
        seed(&c, "United City", 2); // Exact name in an unrelated competition must never win.
        let d4 = event(&c, 1, "day4", "United City", "Bayern M.", 4);
        let batch = process(&c, 2).unwrap();
        assert_eq!(batch.manual_review, 1);
        assert_eq!(batch.remaining, 1);
        let (status, evidence): (String, String) = c
            .query_row(
                "SELECT status,evidence_json FROM daily_resolution_queue WHERE match_id=?1",
                [d4],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "MANUAL_REVIEW");
        assert!(evidence.contains("United City A"));
        assert!(evidence.contains("United City B"));
        assert_eq!(
            process(&c, 2).unwrap().processed,
            0,
            "ambiguous event respects backoff"
        );
        let aliases:i64=c.query_row("SELECT count(*) FROM provider_scoped_team_aliases WHERE normalized_alias='unitedcity'",[],|r|r.get(0)).unwrap();
        assert_eq!(aliases, 0);
        c.execute("UPDATE matches SET status='finished' WHERE id=?1", [d4])
            .unwrap();
        assert_eq!(
            process(&c, 2).unwrap().remaining,
            0,
            "stale historical ambiguity cannot block current readiness"
        );
    }
    #[test]
    fn interrupted_work_is_retried_but_recent_work_is_not_duplicated() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        c.execute(
            "INSERT INTO teams(id,normalized_name) VALUES(1,'Opponent')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO competitions(id,name) VALUES(1,'Bundesliga')",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO provider_competition_mappings(provider,external_competition_id,competition_id) VALUES('football-data.co.uk','D1',1)",[]).unwrap();
        seed(&c, "Bayern Munich", 1);
        seed(&c, "Dortmund", 1);
        let id = event(&c, 1, "interrupted", "Bayern Munich", "Dortmund", 1);
        c.execute("UPDATE daily_resolution_queue SET status='AUTO_RESOLVING',last_attempt_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE match_id=?1",[id]).unwrap();
        assert_eq!(process(&c, 1).unwrap().processed, 0);
        c.execute("UPDATE daily_resolution_queue SET last_attempt_at='2000-01-01T00:00:00Z' WHERE match_id=?1",[id]).unwrap();
        assert_eq!(process(&c, 1).unwrap().resolved, 1);
    }
}
