use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;

pub const VERY_HIGH_NAME_SIMILARITY: f64 = 0.92;
pub const REVIEW_NAME_SIMILARITY: f64 = 0.75;
pub const MIN_WINNER_MARGIN: f64 = 0.05;
pub const KICKOFF_TOLERANCE_MINUTES: i64 = 180;
pub const NEAR_DATE_DAYS: i64 = 1;

pub fn normalize_team_name(value: &str) -> String {
    let mut out = String::new();
    for ch in value.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            out.push(ch);
        } else {
            out.push(' ');
        }
    }
    let mut words: Vec<&str> = out.split_whitespace().collect();
    if words.len() > 1 && matches!(words.last(), Some(&"fc" | &"f c")) {
        words.pop();
    }
    words.join(" ")
}

fn alias_target(key: &str) -> Option<&'static str> {
    match key {
        "man united" | "man utd" | "manchester utd" => Some("manchester united"),
        "paris sg" | "psg" => Some("paris saint germain"),
        _ => None,
    }
}

fn canonical_team_key(key: &str) -> &str {
    alias_target(key).unwrap_or(key)
}

pub fn canonical_competition_key(value: &str) -> String {
    let key = normalize_team_name(value);
    match key.as_str() {
        "england premier league" | "ingiltere premier lig" => "premier league".into(),
        "turkiye super lig" | "futbol ligi 1" => "super lig".into(),
        _ => key,
    }
}

pub fn competition_alias_compatible(
    a: &str,
    b: &str,
    country_a: Option<&str>,
    country_b: Option<&str>,
) -> bool {
    let country_ok = country_a.is_none() || country_b.is_none() || country_a == country_b;
    country_ok && canonical_competition_key(a) == canonical_competition_key(b)
}

fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let aa: Vec<char> = a.chars().collect();
    let bb: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=bb.len()).collect();
    for (i, ca) in aa.iter().enumerate() {
        let mut cur = vec![i + 1; bb.len() + 1];
        for (j, cb) in bb.iter().enumerate() {
            cur[j + 1] = (prev[j + 1] + 1)
                .min(cur[j] + 1)
                .min(prev[j] + usize::from(ca != cb));
        }
        prev = cur;
    }
    let max_len = aa.len().max(bb.len()) as f64;
    if max_len == 0.0 {
        1.0
    } else {
        1.0 - prev[bb.len()] as f64 / max_len
    }
}

#[derive(Default)]
struct FixtureContext {
    competition_match: bool,
    opponent_match: bool,
    date_match: &'static str,
    kickoff_difference_minutes: Option<i64>,
    kickoff_compatible: Option<bool>,
    home_away_match: bool,
    historical_membership: &'static str,
}

fn fixture_context(
    c: &Connection,
    mapping_id: i64,
    source_team: i64,
    candidate_team: i64,
    provider: &str,
) -> rusqlite::Result<FixtureContext> {
    let mut out = FixtureContext {
        home_away_match: true,
        date_match: "UNKNOWN",
        historical_membership: "UNKNOWN",
        ..Default::default()
    };
    let mut s=c.prepare("SELECT m.competition_id,m.scheduled_local_date,m.kickoff_at,m.home_team_id,m.away_team_id,m.kickoff_time_known FROM matches m JOIN provider_match_mappings pm ON pm.match_id=m.id WHERE pm.id=?1 OR (pm.provider=?2 AND (m.home_team_id=?3 OR m.away_team_id=?3))")?;
    let rows = s.query_map(params![mapping_id, provider, source_team], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, bool>(5)?,
        ))
    })?;
    for row in rows {
        let (comp, date, kick, home, away, kick_known) = row?;
        let source_home = home == source_team;
        let opponent = if source_home { away } else { home };
        let mut q=c.prepare("SELECT m.scheduled_local_date,m.kickoff_at,m.home_team_id,m.away_team_id,m.kickoff_time_known FROM matches m WHERE m.competition_id=?1 AND (m.home_team_id=?2 OR m.away_team_id=?2)")?;
        for rr in q.query_map(params![comp, candidate_team], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, bool>(4)?,
            ))
        })? {
            let (d2, k2, h2, a2, k2_known) = rr?;
            out.competition_match = true;
            out.home_away_match =
                (source_home && h2 == candidate_team) || (!source_home && a2 == candidate_team);
            if (source_home && a2 == opponent) || (!source_home && h2 == opponent) {
                out.opponent_match = true;
            }
            if date == d2 && date.is_some() {
                out.date_match = "SAME_DATE";
            } else if date.is_some() && d2.is_some() {
                out.date_match = "NEAR_DATE";
            }
            if kick_known && k2_known {
                if let (Ok(t1), Ok(t2)) = (
                    chrono::DateTime::parse_from_rfc3339(&kick),
                    chrono::DateTime::parse_from_rfc3339(&k2),
                ) {
                    let mins = (t1.timestamp() - t2.timestamp()).abs() / 60;
                    out.kickoff_difference_minutes = Some(mins);
                    out.kickoff_compatible = Some(mins <= KICKOFF_TOLERANCE_MINUTES);
                }
            }
        }
    }
    let membership:i64=c.query_row("SELECT COUNT(*) FROM matches WHERE competition_id IN (SELECT competition_id FROM matches WHERE home_team_id=?1 OR away_team_id=?1) AND (home_team_id=?2 OR away_team_id=?2)",params![source_team,candidate_team],|r|r.get(0)).unwrap_or(0);
    let countries: (Option<String>, Option<String>) = c
        .query_row(
            "SELECT a.country,b.country FROM teams a, teams b WHERE a.id=?1 AND b.id=?2",
            params![source_team, candidate_team],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((None, None));
    out.historical_membership =
        if countries.0.is_some() && countries.1.is_some() && countries.0 != countries.1 {
            "CONFLICT"
        } else if membership > 0 {
            "MATCH"
        } else {
            "UNKNOWN"
        };
    Ok(out)
}

fn team_names_equivalent(c: &Connection, a: i64, b: i64) -> rusqlite::Result<bool> {
    let na: String = c.query_row("SELECT normalized_name FROM teams WHERE id=?1", [a], |r| {
        r.get(0)
    })?;
    let nb: String = c.query_row("SELECT normalized_name FROM teams WHERE id=?1", [b], |r| {
        r.get(0)
    })?;
    let ka = normalize_team_name(&na);
    let kb = normalize_team_name(&nb);
    Ok(canonical_team_key(&ka) == canonical_team_key(&kb))
}
fn kickoff_compat(a: &str, a_known: bool, b: &str, b_known: bool) -> (Option<i64>, Option<bool>) {
    if !a_known || !b_known {
        return (None, None);
    }
    match (
        chrono::DateTime::parse_from_rfc3339(a),
        chrono::DateTime::parse_from_rfc3339(b),
    ) {
        (Ok(x), Ok(y)) => {
            let d = (x.timestamp() - y.timestamp()).abs() / 60;
            (Some(d), Some(d <= KICKOFF_TOLERANCE_MINUTES))
        }
        _ => (None, None),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolutionCandidate {
    pub id: i64,
    pub provider: String,
    pub source_mapping_id: i64,
    pub source_name: String,
    pub candidate_team_id: i64,
    pub candidate_name: String,
    pub score: f64,
    pub level: String,
    pub reason: String,
    pub status: String,
    pub evidence: serde_json::Value,
    pub runner_up_score: Option<f64>,
    pub winner_margin: Option<f64>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ScanSummary {
    pub provider_teams_scanned: i64,
    pub already_resolved: i64,
    pub exact_matches: i64,
    pub very_high_confidence: i64,
    pub review_candidates: i64,
    pub unresolved: i64,
    pub duplicate_teams: i64,
    pub duplicate_matches: i64,
    pub fixture_exact_matches: i64,
    pub fixture_very_high_confidence: i64,
    pub fixture_review_matches: i64,
    pub fixture_incompatible_matches: i64,
    pub candidates: Vec<ResolutionCandidate>,
    pub match_candidates: Vec<MatchResolutionCandidate>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MatchResolutionCandidate {
    pub source_match_id: i64,
    pub candidate_match_id: i64,
    pub source_provider: String,
    pub candidate_provider: String,
    pub home_resolution_class: String,
    pub away_resolution_class: String,
    pub competition_match: bool,
    pub date_classification: String,
    pub kickoff_difference_minutes: Option<i64>,
    pub kickoff_compatible: Option<bool>,
    pub home_away_match: bool,
    pub result_compatible: bool,
    pub confidence_class: String,
    pub evidence: serde_json::Value,
}
#[derive(Debug, Clone, Serialize)]
pub struct ApplySummary {
    pub teams_linked: i64,
    pub teams_merged: i64,
    pub matches_merged: i64,
    pub competition_mappings_resolved: i64,
    pub skipped_review: i64,
}

pub fn scan(connection: &Connection) -> rusqlite::Result<ScanSummary> {
    let mut stmt = connection.prepare("SELECT ptm.id,ptm.provider,ptm.external_team_name,ptm.team_id,t.normalized_name,t.country FROM provider_team_mappings ptm JOIN teams t ON t.id=ptm.team_id ORDER BY ptm.id")?;
    let mut rows = stmt.query([])?;
    let mut candidates = Vec::new();
    let mut resolved = 0;
    let mut exact = 0;
    let mut high = 0;
    let mut review = 0;
    let mut unresolved = 0;
    while let Some(r) = rows.next()? {
        let (mid, provider, source, source_id, _country): (
            i64,
            String,
            Option<String>,
            i64,
            Option<String>,
        ) = (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(5)?);
        let source = source.unwrap_or_default();
        let key = normalize_team_name(&source);
        let target_key = alias_target(&key).unwrap_or(&key);
        let mut best: Option<(i64, String, f64, String)> = None;
        let mut runner_up_score: Option<f64> = None;
        let mut ts =
            connection.prepare("SELECT id,normalized_name,country FROM teams WHERE id <> ?1")?;
        let mut tr = ts.query([source_id])?;
        while let Some(t) = tr.next()? {
            let id: i64 = t.get(0)?;
            let name: String = t.get(1)?;
            let country: Option<String> = t.get(2)?;
            // Retain country-conflicting candidates so the resolver can expose
            // explicit CONFLICT evidence; they remain ineligible for linking.
            let nk = normalize_team_name(&name);
            let country_conflict = _country.is_some() && country.is_some() && _country != country;
            let (score, level, reason) = if nk == target_key {
                (
                    1.0,
                    "EXACT",
                    "normalized identity or curated alias".to_string(),
                )
            } else {
                let s = similarity(&key, &nk);
                if s >= VERY_HIGH_NAME_SIMILARITY {
                    (
                        s,
                        "REVIEW",
                        "strong name similarity requires fixture context before auto-resolution"
                            .to_string(),
                    )
                } else if s >= 0.75 {
                    (s, "REVIEW", "name similarity requires review".to_string())
                } else {
                    (s, "UNRESOLVED", "insufficient evidence".to_string())
                }
            };
            let context = fixture_context(connection, mid, source_id, id, &provider)?;
            let score = score
                + if context.opponent_match { 0.15 } else { 0.0 }
                + if context.date_match == "SAME_DATE" {
                    0.12
                } else if context.date_match == "NEAR_DATE" {
                    0.03
                } else {
                    0.0
                }
                + if context.competition_match { 0.08 } else { 0.0 }
                + if context.kickoff_compatible == Some(true) {
                    0.05
                } else {
                    0.0
                };
            let level = if level == "REVIEW"
                && score >= 0.78
                && context.opponent_match
                && context.competition_match
                && context.date_match == "SAME_DATE"
                && context.home_away_match
            {
                "VERY_HIGH"
            } else if country_conflict {
                "UNRESOLVED"
            } else {
                level
            };
            if best.as_ref().map(|b| score > b.2).unwrap_or(true) {
                runner_up_score = best.as_ref().map(|b| b.2).or(runner_up_score);
                best = Some((
                    id,
                    name,
                    score,
                    format!(
                        "{level}: {reason}; context={}",
                        serde_json::json!({"competition_match":context.competition_match,"opponent_match":context.opponent_match,"date_match":context.date_match,"kickoff_difference_minutes":context.kickoff_difference_minutes,"kickoff_compatible":context.kickoff_compatible,"home_away_match":context.home_away_match,"historical_membership":context.historical_membership})
                    ),
                ));
            } else if runner_up_score.map(|v| score > v).unwrap_or(true) {
                runner_up_score = Some(score);
            }
        }
        if let Some((cid, cname, score, reason)) = best {
            let candidate_country: Option<String> =
                connection
                    .query_row("SELECT country FROM teams WHERE id=?1", [cid], |r| r.get(0))?;
            let country_match =
                _country.is_none() || candidate_country.is_none() || _country == candidate_country;
            let mut level = reason.split(':').next().unwrap_or("UNRESOLVED").to_string();
            let winner_margin = runner_up_score.map(|v| score - v);
            if level == "VERY_HIGH" && winner_margin.is_some_and(|m| m < MIN_WINNER_MARGIN) {
                level = "REVIEW".to_string();
            }
            if level == "EXACT" {
                exact += 1
            } else if level == "VERY_HIGH" {
                high += 1
            } else if level == "REVIEW" {
                review += 1
            } else {
                unresolved += 1
            };
            let status = connection
                .query_row(
                    "SELECT status FROM team_resolution_candidates WHERE source_team_mapping_id=?1 AND (candidate_team_id=?2 OR status='accepted') ORDER BY CASE status WHEN 'accepted' THEN 0 ELSE 1 END LIMIT 1",
                    params![mid, cid],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or_else(|| "pending".to_string());
            candidates.push(ResolutionCandidate {
                id: mid,
                provider: provider.clone(),
                source_mapping_id: mid,
                source_name: source,
                candidate_team_id: cid,
                candidate_name: cname,
                score: score.min(1.0),
                level,
                reason: reason.clone(),
                status,
                runner_up_score,
                winner_margin,
                evidence: serde_json::json!({
                    "name_similarity": score,
                    "normalized_exact": reason.starts_with("EXACT"),
                    "curated_alias": alias_target(&key).is_some(),
                    "country_match": country_match,
                    "competition_match": reason.contains("\"competition_match\":true"),
                    "opponent_match": reason.contains("\"opponent_match\":true"),
                    "date_match": if reason.contains("\"date_match\":\"SAME_DATE\"") { "SAME_DATE" } else { "UNKNOWN" },
                    "kickoff_compatible": if reason.contains("\"kickoff_compatible\":true") { true } else { false },
                    "home_away_match": !reason.contains("\"home_away_match\":false")
                }),
            });
        } else {
            resolved += 1;
        }
    }
    let mut match_candidates = Vec::new();
    let mut ms=connection.prepare("SELECT a.id,pa.provider,a.competition_id,a.scheduled_local_date,a.kickoff_at,a.home_team_id,a.away_team_id,b.id,pb.provider,b.competition_id,b.scheduled_local_date,b.kickoff_at,b.home_team_id,b.away_team_id,a.status,a.final_home_goals,a.final_away_goals,b.status,b.final_home_goals,b.final_away_goals,a.kickoff_time_known,b.kickoff_time_known FROM matches a JOIN provider_match_mappings pa ON pa.match_id=a.id JOIN matches b ON b.id>a.id JOIN provider_match_mappings pb ON pb.match_id=b.id AND pb.provider<>pa.provider")?;
    for row in ms.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, i64>(7)?,
            r.get::<_, String>(8)?,
            r.get::<_, i64>(9)?,
            r.get::<_, Option<String>>(10)?,
            r.get::<_, String>(11)?,
            r.get::<_, i64>(12)?,
            r.get::<_, i64>(13)?,
            r.get::<_, String>(14)?,
            r.get::<_, Option<i64>>(15)?,
            r.get::<_, Option<i64>>(16)?,
            r.get::<_, String>(17)?,
            r.get::<_, Option<i64>>(18)?,
            r.get::<_, Option<i64>>(19)?,
            r.get::<_, bool>(20)?,
            r.get::<_, bool>(21)?,
        ))
    })? {
        let (
            id,
            sp,
            sc,
            sd,
            sk,
            sh,
            sa,
            cid,
            cp,
            cc,
            cd,
            ck,
            ch,
            ca,
            ss,
            sfg,
            sag,
            cs,
            cfg,
            cag,
            sk_known,
            ck_known,
        ) = row?;
        let home_eq = sh == ch || team_names_equivalent(connection, sh, ch)?;
        let away_eq = sa == ca || team_names_equivalent(connection, sa, ca)?;
        let orientation = home_eq && away_eq;
        let comp = sc == cc;
        let date = if let (Some(a), Some(b)) = (&sd, &cd) {
            if a == b {
                "SAME_DATE"
            } else if let (Ok(x), Ok(y)) = (
                chrono::NaiveDate::parse_from_str(a, "%Y-%m-%d"),
                chrono::NaiveDate::parse_from_str(b, "%Y-%m-%d"),
            ) {
                if (x - y).num_days().abs() <= NEAR_DATE_DAYS {
                    "NEAR_DATE"
                } else {
                    "INCOMPATIBLE_DATE"
                }
            } else {
                "UNKNOWN"
            }
        } else {
            "UNKNOWN"
        };
        let (diff, kok) = kickoff_compat(&sk, sk_known, &ck, ck_known);
        let result = !(ss == "finished" && cs == "finished" && (sfg, sag) != (cfg, cag));
        let class = if sh == ch
            && sa == ca
            && comp
            && date == "SAME_DATE"
            && kok != Some(false)
            && result
        {
            "EXACT"
        } else if orientation && comp && date == "SAME_DATE" && kok != Some(false) && result {
            "VERY_HIGH"
        } else if orientation && comp && result {
            "REVIEW"
        } else {
            "INCOMPATIBLE"
        };
        match_candidates.push(MatchResolutionCandidate {
            source_match_id: id,
            candidate_match_id: cid,
            source_provider: sp,
            candidate_provider: cp,
            home_resolution_class: if home_eq {
                "EXACT".into()
            } else {
                "REVIEW".into()
            },
            away_resolution_class: if away_eq {
                "EXACT".into()
            } else {
                "REVIEW".into()
            },
            competition_match: comp,
            date_classification: date.into(),
            kickoff_difference_minutes: diff,
            kickoff_compatible: kok,
            home_away_match: orientation,
            result_compatible: result,
            confidence_class: class.into(),
            evidence: serde_json::json!({"source_season":sc,"candidate_season":cc}),
        });
    }
    let duplicate_teams=connection.query_row("SELECT COUNT(*) FROM (SELECT lower(normalized_name),country FROM teams GROUP BY lower(normalized_name),country HAVING COUNT(*)>1)",[],|r|r.get(0))?;
    let duplicate_matches=connection.query_row("SELECT COUNT(*) FROM (SELECT competition_id,season,home_team_id,away_team_id,scheduled_local_date FROM matches GROUP BY 1,2,3,4,5 HAVING COUNT(*)>1)",[],|r|r.get(0))?;
    Ok(ScanSummary {
        provider_teams_scanned: candidates.len() as i64,
        already_resolved: resolved,
        exact_matches: exact,
        very_high_confidence: high,
        review_candidates: review,
        unresolved,
        candidates,
        duplicate_teams,
        duplicate_matches,
        fixture_exact_matches: match_candidates
            .iter()
            .filter(|m| m.confidence_class == "EXACT")
            .count() as i64,
        fixture_very_high_confidence: match_candidates
            .iter()
            .filter(|m| m.confidence_class == "VERY_HIGH")
            .count() as i64,
        fixture_review_matches: match_candidates
            .iter()
            .filter(|m| m.confidence_class == "REVIEW")
            .count() as i64,
        fixture_incompatible_matches: match_candidates
            .iter()
            .filter(|m| m.confidence_class == "INCOMPATIBLE")
            .count() as i64,
        match_candidates,
    })
}

pub fn apply_safe(connection: &mut Connection) -> rusqlite::Result<ApplySummary> {
    let initial_scan = scan(connection)?;
    let skipped_review = initial_scan.review_candidates + initial_scan.unresolved;
    let tx = connection.transaction()?;
    let mut linked = 0;
    let mut merged = 0;
    let mut competition_mappings = 0;
    let mut cs = tx.prepare("SELECT provider,external_competition_id,external_name,country FROM provider_competition_metadata WHERE resolution_status='unresolved' AND external_name IS NOT NULL")?;
    let competition_rows: Vec<(String, String, String, Option<String>)> = cs
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    drop(cs);
    for (provider, external_id, name, country) in competition_rows {
        let cid: Option<i64> = tx.query_row("SELECT id FROM competitions WHERE lower(trim(name))=lower(trim(?1)) AND country IS ?2", params![name,country], |r| r.get(0)).optional()?;
        if let Some(cid) = cid {
            tx.execute("INSERT OR IGNORE INTO provider_competition_mappings(provider,external_competition_id,competition_id) VALUES(?1,?2,?3)", params![provider,external_id,cid])?;
            tx.execute("UPDATE provider_competition_metadata SET resolution_status='resolved' WHERE provider=?1 AND external_competition_id=?2", params![provider,external_id])?;
            competition_mappings += 1;
        }
    }
    for c in &initial_scan.candidates {
        if c.status != "pending" {
            continue;
        }
        tx.execute(
            "INSERT INTO team_resolution_candidates(source_provider,source_team_mapping_id,candidate_team_id,confidence_score,confidence_level,reason_json)
             VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(source_team_mapping_id,candidate_team_id) DO UPDATE SET confidence_score=excluded.confidence_score, confidence_level=excluded.confidence_level, reason_json=excluded.reason_json",
            params![c.provider, c.source_mapping_id, c.candidate_team_id, c.score, c.level, serde_json::json!({"reason": c.reason}).to_string()],
        )?;
    }
    for c in initial_scan
        .candidates
        .into_iter()
        .filter(|c| c.status == "pending" && matches!(c.level.as_str(), "EXACT" | "VERY_HIGH"))
    {
        let source_team: i64 = tx.query_row(
            "SELECT team_id FROM provider_team_mappings WHERE id=?1",
            [c.source_mapping_id],
            |r| r.get(0),
        )?;
        if source_team == c.candidate_team_id {
            continue;
        }
        // Choose the lower normalized team id as the deterministic canonical row.
        // This prevents reciprocal provider candidates from causing a later no-op
        // apply to merge the opposite direction.
        if source_team < c.candidate_team_id {
            continue;
        }
        let already: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM resolution_audit_log WHERE entity_type='TEAM' AND action='TEAM_MERGE' AND source_entity=?1 AND target_entity=?2)", params![source_team.to_string(),c.candidate_team_id.to_string()], |r| r.get(0))?;
        if already {
            continue;
        }
        if merge_team_tx(&tx, source_team, c.candidate_team_id).is_err() {
            continue;
        }
        tx.execute(
            "UPDATE provider_team_mappings SET team_id=?2 WHERE id=?1",
            params![c.source_mapping_id, c.candidate_team_id],
        )?;
        tx.execute("UPDATE team_resolution_candidates SET status='accepted',reviewed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE source_team_mapping_id=?1 AND candidate_team_id=?2",params![c.source_mapping_id,c.candidate_team_id])?;
        linked += 1;
        merged += 1;
        tx.execute("INSERT INTO resolution_audit_log(entity_type,source_entity,target_entity,action,method,confidence,details_json) VALUES('TEAM',?1,?2,'TEAM_MERGE','AUTO',?3,?4)",params![source_team.to_string(),c.candidate_team_id,c.score,serde_json::json!({"reason":c.reason}).to_string()])?;
    }
    tx.commit()?;
    let mut matches_merged = 0;
    // A successful merge invalidates match ids in the scan snapshot. Rescan
    // after each merge so one apply exhausts the unchanged safe work set.
    loop {
        let post = scan(connection)?;
        let mut merged_one = false;
        for candidate in post
            .match_candidates
            .iter()
            .filter(|m| m.confidence_class == "VERY_HIGH" || m.confidence_class == "EXACT")
        {
            if merge_match(
                connection,
                candidate.candidate_match_id,
                candidate.source_match_id,
            )
            .is_ok()
            {
                matches_merged += 1;
                merged_one = true;
                break;
            }
        }
        if !merged_one {
            break;
        }
    }
    let mut result = ApplySummary {
        teams_linked: linked,
        teams_merged: merged,
        matches_merged,
        competition_mappings_resolved: competition_mappings,
        skipped_review,
    };
    if result.teams_merged > 0 || result.matches_merged > 0 {
        let followup = apply_safe(connection)?;
        result.teams_linked += followup.teams_linked;
        result.teams_merged += followup.teams_merged;
        result.matches_merged += followup.matches_merged;
        result.competition_mappings_resolved += followup.competition_mappings_resolved;
    }
    Ok(result)
}

pub fn merge_match(
    connection: &mut Connection,
    duplicate: i64,
    canonical: i64,
) -> rusqlite::Result<()> {
    if duplicate == canonical {
        return Ok(());
    }
    let tx = connection.transaction()?;
    let (ds, dh, da, dstatus, dfg, dag): (String,i64,i64,String,Option<i64>,Option<i64>) = tx.query_row("SELECT season,home_team_id,away_team_id,status,final_home_goals,final_away_goals FROM matches WHERE id=?1", [duplicate], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
    let (cs, ch, ca, cstatus, cfg, cag): (String,i64,i64,String,Option<i64>,Option<i64>) = tx.query_row("SELECT season,home_team_id,away_team_id,status,final_home_goals,final_away_goals FROM matches WHERE id=?1", [canonical], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
    if ds != cs
        || dh != ch
        || da != ca
        || (dstatus == "finished" && cstatus == "finished" && (dfg, dag) != (cfg, cag))
    {
        return Err(rusqlite::Error::InvalidParameterName(
            "contradictory match identity/result".into(),
        ));
    }
    tx.execute("UPDATE matches SET status=CASE WHEN status='finished' OR ?2='finished' THEN 'finished' ELSE status END, final_home_goals=COALESCE(final_home_goals,?3), final_away_goals=COALESCE(final_away_goals,?4) WHERE id=?1", params![canonical,dstatus,dfg,dag])?;
    let mut maps = tx.prepare(
        "SELECT id,provider,external_match_id FROM provider_match_mappings WHERE match_id=?1",
    )?;
    let rows: Vec<(i64, String, String)> = maps
        .query_map([duplicate], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    drop(maps);
    for (id, p, e) in rows {
        let conflict:Option<i64>=tx.query_row("SELECT id FROM provider_match_mappings WHERE match_id=?1 AND provider=?2 AND external_match_id=?3",params![canonical,p,e],|r|r.get(0)).optional()?;
        if conflict.is_some() {
            tx.execute("DELETE FROM provider_match_mappings WHERE id=?1", [id])?;
        } else {
            tx.execute(
                "UPDATE provider_match_mappings SET match_id=?2 WHERE id=?1",
                params![id, canonical],
            )?;
        }
    }
    tx.execute(
        "UPDATE odds_snapshots SET match_id=?2 WHERE match_id=?1",
        params![duplicate, canonical],
    )?;
    tx.execute(
        "UPDATE popularity_snapshots SET match_id=?2 WHERE match_id=?1",
        params![duplicate, canonical],
    )?;
    tx.execute(
        "UPDATE predictions SET match_id=?2 WHERE match_id=?1",
        params![duplicate, canonical],
    )?;
    tx.execute(
        "UPDATE coupon_selections SET match_id=?2 WHERE match_id=?1",
        params![duplicate, canonical],
    )?;
    tx.execute("INSERT INTO match_statistics(match_id,home_shots,away_shots,home_shots_on_target,away_shots_on_target,home_corners,away_corners,home_fouls,away_fouls,home_yellow_cards,away_yellow_cards,home_red_cards,away_red_cards) SELECT ?2,home_shots,away_shots,home_shots_on_target,away_shots_on_target,home_corners,away_corners,home_fouls,away_fouls,home_yellow_cards,away_yellow_cards,home_red_cards,away_red_cards FROM match_statistics WHERE match_id=?1 ON CONFLICT(match_id) DO UPDATE SET home_shots=COALESCE(match_statistics.home_shots,excluded.home_shots),away_shots=COALESCE(match_statistics.away_shots,excluded.away_shots),home_shots_on_target=COALESCE(match_statistics.home_shots_on_target,excluded.home_shots_on_target),away_shots_on_target=COALESCE(match_statistics.away_shots_on_target,excluded.away_shots_on_target),home_corners=COALESCE(match_statistics.home_corners,excluded.home_corners),away_corners=COALESCE(match_statistics.away_corners,excluded.away_corners),home_fouls=COALESCE(match_statistics.home_fouls,excluded.home_fouls),away_fouls=COALESCE(match_statistics.away_fouls,excluded.away_fouls),home_yellow_cards=COALESCE(match_statistics.home_yellow_cards,excluded.home_yellow_cards),away_yellow_cards=COALESCE(match_statistics.away_yellow_cards,excluded.away_yellow_cards),home_red_cards=COALESCE(match_statistics.home_red_cards,excluded.home_red_cards),away_red_cards=COALESCE(match_statistics.away_red_cards,excluded.away_red_cards)",params![duplicate,canonical])?;
    tx.execute(
        "DELETE FROM match_statistics WHERE match_id=?1",
        [duplicate],
    )?;
    tx.execute("DELETE FROM matches WHERE id=?1", [duplicate])?;
    tx.execute("INSERT INTO resolution_audit_log(entity_type,source_entity,target_entity,action,method,details_json) VALUES('MATCH',?1,?2,'MATCH_MERGE','MANUAL',?3)",params![duplicate.to_string(),canonical,serde_json::json!({"season":ds}).to_string()])?;
    tx.commit()
}

pub fn link_mapping(
    connection: &Connection,
    mapping_id: i64,
    team_id: i64,
) -> rusqlite::Result<()> {
    connection.execute(
        "UPDATE provider_team_mappings SET team_id=?2 WHERE id=?1",
        params![mapping_id, team_id],
    )?;
    connection.execute("INSERT INTO resolution_audit_log(entity_type,source_entity,target_entity,action,method) VALUES('TEAM',?1,?2,'TEAM_LINK','MANUAL')",params![mapping_id.to_string(),team_id.to_string()])?;
    Ok(())
}

fn merge_team_tx(tx: &Transaction<'_>, duplicate: i64, canonical: i64) -> rusqlite::Result<()> {
    if duplicate == canonical {
        return Ok(());
    }
    let opposing: i64 = tx.query_row("SELECT COUNT(*) FROM matches WHERE (home_team_id=?1 AND away_team_id=?2) OR (home_team_id=?2 AND away_team_id=?1)", params![duplicate,canonical], |r| r.get(0))?;
    if opposing > 0 {
        return Err(rusqlite::Error::InvalidParameterName(
            "team merge would create home/away self-match".into(),
        ));
    }
    crate::assets::reconcile_team_assets(tx, duplicate, canonical)?;
    tx.execute(
        "UPDATE matches SET home_team_id=?2 WHERE home_team_id=?1 AND away_team_id<>?2",
        params![duplicate, canonical],
    )?;
    tx.execute(
        "UPDATE matches SET away_team_id=?2 WHERE away_team_id=?1 AND home_team_id<>?2",
        params![duplicate, canonical],
    )?;
    let mut ids = tx.prepare("SELECT id FROM provider_team_mappings WHERE team_id=?1")?;
    let list: Vec<i64> = ids
        .query_map([duplicate], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for id in list {
        let conflict:Option<i64>=tx.query_row("SELECT id FROM provider_team_mappings a WHERE a.team_id=?1 AND EXISTS(SELECT 1 FROM provider_team_mappings b WHERE b.id=?2 AND b.provider=a.provider AND b.external_team_id=a.external_team_id)",[canonical,id],|r|r.get(0)).optional()?;
        if conflict.is_some() {
            tx.execute("DELETE FROM provider_team_mappings WHERE id=?1", [id])?;
        } else {
            tx.execute(
                "UPDATE provider_team_mappings SET team_id=?2 WHERE id=?1",
                params![id, canonical],
            )?;
        }
    }
    tx.execute(
        "UPDATE team_aliases SET team_id=?2 WHERE team_id=?1",
        params![duplicate, canonical],
    )?;
    tx.execute(
        "UPDATE team_resolution_candidates SET candidate_team_id=?2 WHERE candidate_team_id=?1",
        params![duplicate, canonical],
    )?;
    tx.execute("DELETE FROM teams WHERE id=?1", [duplicate])?;
    Ok(())
}

pub fn merge_team(
    connection: &mut Connection,
    duplicate: i64,
    canonical: i64,
) -> rusqlite::Result<()> {
    let tx = connection.transaction()?;
    merge_team_tx(&tx, duplicate, canonical)?;
    tx.execute("INSERT INTO resolution_audit_log(entity_type,source_entity,target_entity,action,method) VALUES('TEAM',?1,?2,'TEAM_MERGE','MANUAL')", params![duplicate.to_string(), canonical.to_string()])?;
    tx.commit()
}

pub fn review_queue(connection: &Connection) -> rusqlite::Result<Vec<ResolutionCandidate>> {
    Ok(scan(connection)?
        .candidates
        .into_iter()
        .filter(|c| c.level == "REVIEW" || c.level == "UNRESOLVED")
        .collect())
}

pub fn accept(connection: &Connection, id: i64) -> rusqlite::Result<()> {
    let (mapping,team):(i64,i64)=connection.query_row("SELECT source_team_mapping_id,candidate_team_id FROM team_resolution_candidates WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
    connection.execute(
        "UPDATE provider_team_mappings SET team_id=?2 WHERE id=?1",
        params![mapping, team],
    )?;
    connection.execute("UPDATE team_resolution_candidates SET status='accepted',reviewed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",[id])?;
    connection.execute("INSERT INTO resolution_audit_log(entity_type,source_entity,target_entity,action,method) SELECT 'TEAM',?1,?2,'MANUAL_ACCEPT','MANUAL' WHERE NOT EXISTS(SELECT 1 FROM resolution_audit_log WHERE entity_type='TEAM' AND source_entity=?1 AND target_entity=?2 AND action='MANUAL_ACCEPT' AND method='MANUAL')",params![mapping.to_string(),team.to_string()])?;
    Ok(())
}
pub fn reject(connection: &Connection, id: i64) -> rusqlite::Result<()> {
    let (mapping,team):(i64,i64)=connection.query_row("SELECT source_team_mapping_id,candidate_team_id FROM team_resolution_candidates WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
    connection.execute("UPDATE team_resolution_candidates SET status='rejected',reviewed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",[id])?;
    connection.execute("INSERT INTO resolution_audit_log(entity_type,source_entity,target_entity,action,method) SELECT 'TEAM',?1,?2,'MANUAL_REJECT','MANUAL' WHERE NOT EXISTS(SELECT 1 FROM resolution_audit_log WHERE entity_type='TEAM' AND source_entity=?1 AND target_entity=?2 AND action='MANUAL_REJECT' AND method='MANUAL')",params![mapping.to_string(),team.to_string()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::repositories::teams;

    const PAIRED_SCENARIOS: &[&str] = &[
        "exact_arsenal",
        "fc_suffix_liverpool",
        "alias_man_united",
        "alias_man_utd",
        "alias_paris_sg",
        "alias_psg",
        "punctuation_spacing",
        "context_fuzzy",
        "wrong_country",
        "ambiguous",
        "reversed_fixture",
        "date_mismatch",
        "small_kickoff_delta",
        "large_kickoff_delta",
        "unknown_kickoff",
        "unknown_club",
    ];

    #[test]
    fn normalization_is_conservative_and_deterministic() {
        assert_eq!(
            normalize_team_name(" MANCHESTER-UNITED FC "),
            "manchester united"
        );
        assert_eq!(normalize_team_name("Real Madrid"), "real madrid");
    }

    #[test]
    fn exact_provider_names_are_reported_without_mutating_scan() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        let t1 = teams::insert(&c, "Manchester United", Some("England")).unwrap();
        let t2 = teams::insert(&c, "Manchester United", Some("England")).unwrap_err();
        let _ = t2;
        c.execute("INSERT INTO provider_team_mappings(team_id,provider,external_team_id,external_team_name) VALUES(?1,'iddaa','mu','Manchester United')", [t1]).unwrap();
        let before: i64 = c
            .query_row("SELECT COUNT(*) FROM teams", [], |r| r.get(0))
            .unwrap();
        let result = scan(&c).unwrap();
        let after: i64 = c
            .query_row("SELECT COUNT(*) FROM teams", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(result.provider_teams_scanned, 0);
    }

    #[test]
    fn migration_creates_resolution_tables() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        let count:i64=c.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('team_aliases','team_resolution_candidates','resolution_audit_log')",[],|r|r.get(0)).unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn controlled_two_provider_resolution_and_snapshot_merge() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        let comp = c.execute("INSERT INTO competitions(name,country,current_season) VALUES('Premier League','England','2026/27')",[]).unwrap();
        let comp = c.last_insert_rowid();
        let canonical = teams::insert(&c, "Manchester United", Some("England")).unwrap();
        let opponent = teams::insert(&c, "Arsenal", Some("England")).unwrap();
        let alias = teams::insert(&c, "Man United", Some("England")).unwrap();
        let paris = teams::insert(&c, "Paris Saint-Germain", Some("France")).unwrap();
        let psg = teams::insert(&c, "Paris SG", Some("France")).unwrap();
        for (provider, id, name, team) in [
            (
                "football-data.co.uk",
                "fd-mu",
                "Manchester United",
                canonical,
            ),
            ("football-data.co.uk", "fd-ars", "Arsenal", opponent),
            ("iddaa", "id-mu", "Man United", alias),
            (
                "football-data.co.uk",
                "fd-psg",
                "Paris Saint-Germain",
                paris,
            ),
            ("iddaa", "id-psg", "Paris SG", psg),
        ] {
            c.execute("INSERT INTO provider_team_mappings(team_id,provider,external_team_id,external_team_name) VALUES(?1,?2,?3,?4)",rusqlite::params![team,provider,id,name]).unwrap();
        }
        let fdmap: i64 = c
            .query_row(
                "SELECT id FROM provider_team_mappings WHERE external_team_id='fd-mu'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let idmap: i64 = c
            .query_row(
                "SELECT id FROM provider_team_mappings WHERE external_team_id='id-mu'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        c.execute("INSERT INTO matches(competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(?1,'2026/27',?2,?3,'2026-08-01T15:00:00Z','scheduled','2026-08-01',1)",[comp,canonical,opponent]).unwrap();
        let m1 = c.last_insert_rowid();
        c.execute("INSERT INTO matches(competition_id,season,home_team_id,away_team_id,kickoff_at,status,scheduled_local_date,kickoff_time_known) VALUES(?1,'2026/27',?2,?3,'2026-08-01T15:05:00Z','scheduled','2026-08-01',1)",[comp,alias,opponent]).unwrap();
        let m2 = c.last_insert_rowid();
        c.execute("INSERT INTO provider_match_mappings(match_id,provider,external_match_id) VALUES(?1,'football-data.co.uk','fd-match')",[m1]).unwrap();
        c.execute("INSERT INTO provider_match_mappings(match_id,provider,external_match_id) VALUES(?1,'iddaa','id-match')",[m2]).unwrap();
        c.execute(
            "INSERT INTO match_statistics(match_id,home_shots) VALUES(?1,10)",
            [m1],
        )
        .unwrap();
        c.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,captured_at) VALUES(?1,'iddaa','1','MATCH_RESULT','1',1.5,'2026-08-01T10:00:00Z')",[m2]).unwrap();
        c.execute("INSERT INTO popularity_snapshots(provider,match_id,provider_event_id,metric_type,metric_value,raw_metric_name,captured_at) VALUES('iddaa',?1,'id-match','COUNT',100,'totalPlayed','2026-08-01T10:00:00Z')",[m2]).unwrap();
        let before = scan(&c).unwrap();
        println!(
            "CONTROLLED BEFORE teams={} mappings={} matches={}",
            c.query_row("SELECT COUNT(*) FROM teams", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            c.query_row("SELECT COUNT(*) FROM provider_team_mappings", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            c.query_row("SELECT COUNT(*) FROM matches", [], |r| r.get::<_, i64>(0))
                .unwrap()
        );
        println!(
            "CONTROLLED SCAN exact={} high={} review={} unresolved={} duplicate_matches={}",
            before.exact_matches,
            before.very_high_confidence,
            before.review_candidates,
            before.unresolved,
            before.duplicate_matches
        );
        let mut cm = c;
        let first_apply = apply_safe(&mut cm).unwrap();
        let second_apply = apply_safe(&mut cm).unwrap();
        assert_eq!(second_apply.teams_merged, 0);
        assert_eq!(second_apply.matches_merged, 0);
        assert!(first_apply.matches_merged >= 1);
        println!(
            "CONTROLLED AFTER matches={} stats={} odds={} popularity={} providers={}",
            cm.query_row("SELECT COUNT(*) FROM matches", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            cm.query_row("SELECT COUNT(*) FROM match_statistics", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            cm.query_row("SELECT COUNT(*) FROM odds_snapshots", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            cm.query_row("SELECT COUNT(*) FROM popularity_snapshots", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            cm.query_row("SELECT COUNT(*) FROM provider_match_mappings", [], |r| r
                .get::<_, i64>(0))
                .unwrap()
        );
        let _ = (fdmap, idmap);
    }

    #[test]
    fn phase_4_0_1_full_acceptance() {
        let db = Database::open_in_memory().unwrap();
        let root = std::env::temp_dir().join(format!("resolver-import-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let csv = root.join("E0.csv");
        std::fs::write(&csv, include_str!("fixtures/resolver_universe.csv")).unwrap();
        let dataset = crate::providers::football_data::find_dataset("E0", "2627").unwrap();
        crate::providers::football_data::import_local_dataset(&db, dataset, &csv).unwrap();
        let history_csv = root.join("E1.csv");
        std::fs::write(&history_csv, include_str!("fixtures/resolver_history.csv")).unwrap();
        let history_dataset = crate::providers::football_data::find_dataset("E1", "2526").unwrap();
        crate::providers::football_data::import_local_dataset(&db, history_dataset, &history_csv)
            .unwrap();
        let json = root.join("bulletin.json");
        std::fs::write(
            &json,
            include_bytes!("fixtures/resolver_universe_iddaa.json"),
        )
        .unwrap();
        crate::providers::iddaa::import_local_bulletin(&db, &json).unwrap();
        let conn = db.connection().unwrap();
        let iddaa_matches: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM provider_match_mappings WHERE provider='iddaa'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let football_matches: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM provider_match_mappings WHERE provider='football-data.co.uk'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(iddaa_matches, 16);
        assert_eq!(football_matches, 17);
        assert_eq!(PAIRED_SCENARIOS.len(), 16);
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM provider_competition_metadata WHERE provider='iddaa'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            3
        );
        let competition_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM provider_competition_metadata WHERE provider='iddaa'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(competition_count, 3);
        let summary = scan(&conn).unwrap();
        println!(
            "ACTUAL_IMPORT_PATHS normalized_match_rows={} match_candidate_comparisons={} teams={} exact={} high={} review={} unresolved={}",
            conn.query_row("SELECT COUNT(*) FROM matches",[],|r|r.get::<_,i64>(0)).unwrap(),
            summary.match_candidates.len(),
            summary.provider_teams_scanned,
            summary.exact_matches,
            summary.very_high_confidence,
            summary.review_candidates,
            summary.unresolved
        );
        assert!(summary.exact_matches > 0, "expected exact team decisions");
        assert!(
            summary.review_candidates > 0,
            "expected review team decisions"
        );
        assert!(summary.unresolved > 0, "expected unresolved team decisions");
        assert!(
            summary
                .match_candidates
                .iter()
                .any(|m| matches!(m.confidence_class.as_str(), "EXACT" | "VERY_HIGH")),
            "expected a safely mergeable fixture"
        );
        assert!(
            summary
                .match_candidates
                .iter()
                .any(|m| m.confidence_class == "INCOMPATIBLE" || m.confidence_class == "REVIEW"),
            "expected a non-mergeable fixture"
        );
        assert!(
            summary.candidates.len() >= 10,
            "expected at least ten actual resolver decisions"
        );
        for c in summary.candidates.iter().take(10) {
            println!("DECISION source={} raw={} candidate={} class={} score={:.3} exact={} alias={} evidence={}", c.provider, c.source_name, c.candidate_name, c.level, c.score, c.evidence["normalized_exact"], c.evidence["curated_alias"], c.evidence);
        }
        for name in ["United FC", "Manchester City", "Unknown Athletic"] {
            if let Some(c) = summary.candidates.iter().find(|c| c.source_name == name) {
                println!(
                    "SAFETY source={} candidate={} class={} evidence={}",
                    c.source_name, c.candidate_name, c.level, c.evidence
                );
            }
        }
        let membership = summary
            .candidates
            .iter()
            .map(|c| c.reason.as_str())
            .collect::<Vec<_>>();
        assert!(
            membership
                .iter()
                .any(|r| r.contains("\"historical_membership\":\"MATCH\"")),
            "expected historical MATCH evidence"
        );
        for state in ["MATCH", "CONFLICT", "UNKNOWN"] {
            let examples: Vec<_> = summary
                .candidates
                .iter()
                .filter(|c| {
                    c.reason
                        .contains(&format!("\"historical_membership\":\"{state}\""))
                })
                .collect();
            assert!(!examples.is_empty(), "expected historical {state} evidence");
            let example = examples[0];
            println!(
                "MEMBERSHIP state={state} source={} candidate={} evidence={}",
                example.source_name, example.candidate_name, example.reason
            );
        }

        // Stable test-only manifest: provider event id plus football-data raw
        // home identity and local date, never array position.
        let manifests = [
            ("wrong_country", "910009", "Brighton", "2026-10-10"),
            ("ambiguous", "910010", "Manchester City", "2026-10-11"),
            ("reversed_fixture", "910011", "Galatasaray", "2026-10-17"),
            ("date_mismatch", "910012", "Besiktas", "2026-10-18"),
            ("small_kickoff_delta", "910013", "United FC", "2026-10-24"),
            (
                "large_kickoff_delta",
                "910014",
                "Manchester United FC",
                "2026-10-25",
            ),
            ("unknown_kickoff", "910015", "Paris SG", "2026-10-31"),
            ("unknown_club", "910016", "Unknown Athletic", "2026-11-01"),
        ];
        let mut safety_match_ids = Vec::new();
        for (scenario, event, fd_home, fd_date) in manifests {
            let iddaa_id: i64 = conn.query_row("SELECT match_id FROM provider_match_mappings WHERE provider='iddaa' AND external_match_id=?1", [event], |r| r.get(0)).unwrap();
            let fd_id: i64 = conn.query_row("SELECT pm.match_id FROM provider_match_mappings pm JOIN matches m ON m.id=pm.match_id JOIN teams h ON h.id=m.home_team_id WHERE pm.provider='football-data.co.uk' AND h.normalized_name=?1 AND m.scheduled_local_date=?2", params![fd_home,fd_date], |r| r.get(0)).unwrap();
            let evidence = summary
                .match_candidates
                .iter()
                .find(|m| {
                    (m.source_match_id == iddaa_id && m.candidate_match_id == fd_id)
                        || (m.source_match_id == fd_id && m.candidate_match_id == iddaa_id)
                })
                .unwrap();
            let team_source = match scenario {
                "unknown_club" => "Mystery Rovers",
                _ => fd_home,
            };
            let team_evidence = summary
                .candidates
                .iter()
                .find(|c| c.source_name == team_source)
                .unwrap();
            let historical = ["MATCH", "CONFLICT", "UNKNOWN"]
                .into_iter()
                .find(|s| {
                    team_evidence
                        .reason
                        .contains(&format!("\"historical_membership\":\"{s}\""))
                })
                .unwrap();
            println!("SAFETY_REPORT scenario_id={scenario} source_raw={} candidate_name={} country_evidence={} competition_evidence={} name_similarity={} historical_membership={} opponent_evidence={} date_classification={} kickoff_difference_minutes={:?} kickoff_compatibility={:?} orientation_evidence={} best_score={} runner_up_score={:?} winner_margin={:?} confidence_class={} final_action={}",
                team_evidence.source_name, team_evidence.candidate_name, team_evidence.evidence["country_match"], team_evidence.evidence["competition_match"], team_evidence.evidence["name_similarity"], historical, team_evidence.evidence["opponent_match"], evidence.date_classification, evidence.kickoff_difference_minutes, evidence.kickoff_compatible, evidence.home_away_match, team_evidence.score, team_evidence.runner_up_score, team_evidence.winner_margin, team_evidence.level, if matches!(evidence.confidence_class.as_str(),"EXACT"|"VERY_HIGH") {"SAFE"} else {"UNLINKED"});
            safety_match_ids.push((scenario, iddaa_id, fd_id));
        }
        let pair = |name: &str| {
            summary
                .match_candidates
                .iter()
                .find(|m| {
                    safety_match_ids.iter().any(|(s, a, b)| {
                        *s == name
                            && ((m.source_match_id == *a && m.candidate_match_id == *b)
                                || (m.source_match_id == *b && m.candidate_match_id == *a))
                    })
                })
                .unwrap()
        };
        let wrong_team = summary
            .candidates
            .iter()
            .find(|c| c.source_name == "Brighton" && c.candidate_name == "Brighton")
            .unwrap();
        assert_eq!(wrong_team.evidence["country_match"], false);
        assert!(!matches!(wrong_team.level.as_str(), "EXACT" | "VERY_HIGH"));
        let ambiguous = summary
            .candidates
            .iter()
            .find(|c| c.source_name == "Manchester City")
            .unwrap();
        assert!(ambiguous.runner_up_score.is_some() && ambiguous.winner_margin.is_some());
        assert!(ambiguous.winner_margin.unwrap() < MIN_WINNER_MARGIN);
        assert_eq!(ambiguous.level, "REVIEW");
        assert!(!pair("reversed_fixture").home_away_match);
        assert_eq!(pair("reversed_fixture").confidence_class, "INCOMPATIBLE");
        assert_eq!(
            pair("date_mismatch").date_classification,
            "INCOMPATIBLE_DATE"
        );
        assert!(!matches!(
            pair("date_mismatch").confidence_class.as_str(),
            "EXACT" | "VERY_HIGH"
        ));
        assert!(
            pair("small_kickoff_delta")
                .kickoff_difference_minutes
                .unwrap()
                <= KICKOFF_TOLERANCE_MINUTES
        );
        assert_eq!(pair("small_kickoff_delta").kickoff_compatible, Some(true));
        assert!(
            pair("large_kickoff_delta")
                .kickoff_difference_minutes
                .unwrap()
                > KICKOFF_TOLERANCE_MINUTES
        );
        assert_eq!(pair("large_kickoff_delta").kickoff_compatible, Some(false));
        assert_eq!(pair("unknown_kickoff").kickoff_difference_minutes, None);
        assert_eq!(pair("unknown_kickoff").kickoff_compatible, None);
        let unknown = summary
            .candidates
            .iter()
            .find(|c| c.source_name == "Mystery Rovers")
            .unwrap();
        assert_eq!(unknown.level, "UNRESOLVED");

        // Add immutable historical snapshots to the imported Iddaa side of the
        // Arsenal/Chelsea paired fixture. This is test setup only; production
        // ingestion remains unchanged.
        let iddaa_match: i64 = conn.query_row("SELECT match_id FROM provider_match_mappings WHERE provider='iddaa' AND external_match_id='910001'", [], |r| r.get(0)).unwrap();
        conn.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,alternative_odd,captured_at) VALUES(?1,'iddaa','1','MATCH_RESULT','1',1.70,1.65,'2026-09-12T10:00:00Z')", [iddaa_match]).unwrap();
        conn.execute("INSERT INTO odds_snapshots(match_id,provider,market_code,market_name,selection,odd,alternative_odd,captured_at) VALUES(?1,'iddaa','1','MATCH_RESULT','1',1.62,1.58,'2026-09-12T12:00:00Z')", [iddaa_match]).unwrap();
        conn.execute("INSERT INTO popularity_snapshots(provider,match_id,provider_event_id,metric_type,metric_value,rank_value,raw_metric_name,captured_at) VALUES('iddaa',?1,'910001','COUNT',720,NULL,'totalPlayed','2026-09-12T12:00:00Z')", [iddaa_match]).unwrap();
        conn.execute("INSERT INTO popularity_snapshots(provider,match_id,provider_event_id,metric_type,metric_value,rank_value,raw_metric_name,captured_at) VALUES('iddaa',?1,'910001','RANK',NULL,3,'rank','2026-09-12T12:00:00Z')", [iddaa_match]).unwrap();
        let before_counts = (
            conn.query_row("SELECT COUNT(*) FROM competitions", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM teams", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM matches", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM match_statistics", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row(
                "SELECT COUNT(*) FROM provider_competition_mappings",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM provider_team_mappings", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM provider_match_mappings", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM odds_snapshots", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM popularity_snapshots", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM team_aliases", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM team_resolution_candidates", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM resolution_audit_log", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
        );
        println!("BEFORE {:?}", before_counts);
        let mut apply_conn = conn;
        let applied = apply_safe(&mut apply_conn).unwrap();
        let post = scan(&apply_conn).unwrap();
        println!("APPLY_1 teams={} matches={} post_match_candidates={} post_exact={} post_high={} post_review={} incompatible={}", applied.teams_merged, applied.matches_merged, post.match_candidates.len(), post.fixture_exact_matches, post.fixture_very_high_confidence, post.fixture_review_matches, post.fixture_incompatible_matches);
        assert!(
            applied.matches_merged > 0,
            "safe apply must merge at least one imported paired fixture"
        );
        let canonical_after_1: i64 = apply_conn.query_row("SELECT match_id FROM provider_match_mappings WHERE provider='iddaa' AND external_match_id='910001'", [], |r| r.get(0)).unwrap();
        assert_eq!(apply_conn.query_row::<i64,_,_>("SELECT COUNT(*) FROM provider_match_mappings WHERE match_id=?1 AND provider='football-data.co.uk'", [canonical_after_1], |r| r.get(0)).unwrap(), 1);
        assert!(
            apply_conn
                .query_row::<i64, _, _>(
                    "SELECT COUNT(*) FROM odds_snapshots WHERE match_id=?1",
                    [canonical_after_1],
                    |r| r.get(0)
                )
                .unwrap()
                >= 2
        );
        assert!(
            apply_conn
                .query_row::<i64, _, _>(
                    "SELECT COUNT(*) FROM popularity_snapshots WHERE match_id=?1",
                    [canonical_after_1],
                    |r| r.get(0)
                )
                .unwrap()
                >= 2
        );
        assert!(
            apply_conn
                .query_row::<i64, _, _>(
                    "SELECT COUNT(*) FROM match_statistics WHERE match_id=?1",
                    [canonical_after_1],
                    |r| r.get(0)
                )
                .unwrap()
                >= 1
        );
        let after1_counts = (
            apply_conn
                .query_row("SELECT COUNT(*) FROM competitions", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM teams", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM matches", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM match_statistics", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row(
                    "SELECT COUNT(*) FROM provider_competition_mappings",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM provider_team_mappings", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM provider_match_mappings", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM odds_snapshots", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM popularity_snapshots", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM team_aliases", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM team_resolution_candidates", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM resolution_audit_log", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
        );
        println!("AFTER_APPLY_1 {:?}", after1_counts);
        let applied2 = apply_safe(&mut apply_conn).unwrap();
        println!(
            "APPLY_2 teams={} matches={}",
            applied2.teams_merged, applied2.matches_merged
        );
        for row in apply_conn.prepare("SELECT source_entity,target_entity,details_json FROM resolution_audit_log WHERE entity_type='MATCH' ORDER BY id").unwrap().query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?))).unwrap() { println!("MATCH_AUDIT {:?}",row.unwrap()); }
        assert_eq!(applied2.teams_merged, 0);
        assert_eq!(applied2.matches_merged, 0);
        let after2_counts = (
            apply_conn
                .query_row("SELECT COUNT(*) FROM competitions", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM teams", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM matches", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM match_statistics", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row(
                    "SELECT COUNT(*) FROM provider_competition_mappings",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM provider_team_mappings", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM provider_match_mappings", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM odds_snapshots", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM popularity_snapshots", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM team_aliases", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM team_resolution_candidates", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            apply_conn
                .query_row("SELECT COUNT(*) FROM resolution_audit_log", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
        );
        println!("AFTER_APPLY_2 {:?}", after2_counts);
        let applied3 = apply_safe(&mut apply_conn).unwrap();
        let cand3: i64 = apply_conn
            .query_row("SELECT COUNT(*) FROM team_resolution_candidates", [], |r| {
                r.get(0)
            })
            .unwrap();
        println!(
            "APPLY_3 teams={} matches={} candidates={}",
            applied3.teams_merged, applied3.matches_merged, cand3
        );
        assert_eq!(applied3.teams_merged, 0);
        assert_eq!(applied3.matches_merged, 0);
        for (scenario, a, b) in &safety_match_ids {
            if matches!(
                *scenario,
                "reversed_fixture" | "date_mismatch" | "large_kickoff_delta"
            ) {
                let still_distinct: bool = apply_conn.query_row("SELECT (SELECT match_id FROM provider_match_mappings WHERE match_id IN (?1,?2) AND provider='iddaa' LIMIT 1) <> (SELECT match_id FROM provider_match_mappings WHERE match_id IN (?1,?2) AND provider='football-data.co.uk' LIMIT 1)", params![a,b], |r| r.get(0)).unwrap();
                assert!(still_distinct, "{scenario} must remain distinct");
            }
        }
        assert_eq!(
            cand3, after2_counts.10,
            "candidate metadata must reach a fixed point"
        );
        // Candidate rows are refreshed for newly surfaced alternatives; all
        // domain, mapping, snapshot and audit counts must remain stable.
        assert_eq!(after1_counts.0, after2_counts.0);
        assert_eq!(after1_counts.1, after2_counts.1);
        assert_eq!(after1_counts.2, after2_counts.2);
        assert_eq!(after1_counts.3, after2_counts.3);
        assert_eq!(after1_counts.4, after2_counts.4);
        assert_eq!(after1_counts.5, after2_counts.5);
        assert_eq!(after1_counts.6, after2_counts.6);
        assert_eq!(after1_counts.7, after2_counts.7);
        assert_eq!(after1_counts.8, after2_counts.8);
        assert_eq!(after1_counts.9, after2_counts.9);
        assert_eq!(
            after1_counts.11, after2_counts.11,
            "audit log must be idempotent"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn manual_team_decisions_remain_authoritative() {
        fn review_db() -> (Database, i64, i64, i64) {
            let db = Database::open_in_memory().unwrap();
            let c = db.connection().unwrap();
            let source = teams::insert(&c, "Manchester Unite", Some("England")).unwrap();
            teams::insert(&c, "Manchester United", Some("England")).unwrap();
            teams::insert(&c, "Manchester Unitedd", Some("England")).unwrap();
            c.execute("INSERT INTO provider_team_mappings(team_id,provider,external_team_id,external_team_name) VALUES(?1,'iddaa','manual-review','Manchester Unite')",[source]).unwrap();
            let mapping = c.last_insert_rowid();
            drop(c);
            let mut c = db.connection().unwrap();
            let applied = apply_safe(&mut c).unwrap();
            assert_eq!(applied.teams_merged, 0);
            let candidate: i64 = c.query_row("SELECT id FROM team_resolution_candidates WHERE source_team_mapping_id=?1 AND confidence_level='REVIEW' ORDER BY confidence_score DESC LIMIT 1",[mapping],|r|r.get(0)).unwrap();
            drop(c);
            (db, mapping, source, candidate)
        }

        let (reject_db, reject_mapping, reject_source, reject_candidate) = review_db();
        let mut reject_conn = reject_db.connection().unwrap();
        reject(&reject_conn, reject_candidate).unwrap();
        reject(&reject_conn, reject_candidate).unwrap();
        for _ in 0..2 {
            let _ = scan(&reject_conn).unwrap();
            assert_eq!(apply_safe(&mut reject_conn).unwrap().teams_merged, 0);
        }
        assert_eq!(
            reject_conn
                .query_row::<String, _, _>(
                    "SELECT status FROM team_resolution_candidates WHERE id=?1",
                    [reject_candidate],
                    |r| r.get(0)
                )
                .unwrap(),
            "rejected"
        );
        assert_eq!(
            reject_conn
                .query_row::<i64, _, _>(
                    "SELECT team_id FROM provider_team_mappings WHERE id=?1",
                    [reject_mapping],
                    |r| r.get(0)
                )
                .unwrap(),
            reject_source
        );
        assert_eq!(reject_conn.query_row::<i64,_,_>("SELECT COUNT(*) FROM team_resolution_candidates a WHERE a.source_team_mapping_id=?1 AND a.status='pending' AND a.candidate_team_id=(SELECT candidate_team_id FROM team_resolution_candidates WHERE id=?2)",params![reject_mapping,reject_candidate],|r|r.get(0)).unwrap(),0);
        assert_eq!(reject_conn.query_row::<i64,_,_>("SELECT COUNT(*) FROM resolution_audit_log WHERE action='MANUAL_REJECT' AND source_entity=?1",[reject_mapping.to_string()],|r|r.get(0)).unwrap(),1);

        let (accept_db, accept_mapping, _accept_source, accept_candidate) = review_db();
        let mut accept_conn = accept_db.connection().unwrap();
        let accepted_team: i64 = accept_conn
            .query_row(
                "SELECT candidate_team_id FROM team_resolution_candidates WHERE id=?1",
                [accept_candidate],
                |r| r.get(0),
            )
            .unwrap();
        accept(&accept_conn, accept_candidate).unwrap();
        accept(&accept_conn, accept_candidate).unwrap();
        for _ in 0..2 {
            let _ = scan(&accept_conn).unwrap();
            assert_eq!(apply_safe(&mut accept_conn).unwrap().teams_merged, 0);
        }
        assert_eq!(
            accept_conn
                .query_row::<i64, _, _>(
                    "SELECT team_id FROM provider_team_mappings WHERE id=?1",
                    [accept_mapping],
                    |r| r.get(0)
                )
                .unwrap(),
            accepted_team
        );
        assert_eq!(
            accept_conn
                .query_row::<String, _, _>(
                    "SELECT status FROM team_resolution_candidates WHERE id=?1",
                    [accept_candidate],
                    |r| r.get(0)
                )
                .unwrap(),
            "accepted"
        );
        assert_eq!(accept_conn.query_row::<i64,_,_>("SELECT COUNT(*) FROM team_resolution_candidates WHERE source_team_mapping_id=?1 AND status='pending'",[accept_mapping],|r|r.get(0)).unwrap(),0);
        assert_eq!(accept_conn.query_row::<i64,_,_>("SELECT COUNT(*) FROM resolution_audit_log WHERE action='MANUAL_ACCEPT' AND source_entity=?1",[accept_mapping.to_string()],|r|r.get(0)).unwrap(),1);
        println!("MANUAL_REJECT PASS mapping={reject_mapping} audit_idempotent=true pending_equivalent=0");
        println!("MANUAL_ACCEPT PASS mapping={accept_mapping} canonical_team={accepted_team} audit_idempotent=true contradictory_pending=0");
    }
}
