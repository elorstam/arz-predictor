//! Exact, country-scoped discovery and a durable per-source fallback ledger.
use crate::assets::{EntityIdentity, EntityMetadataProvider, WikimediaProvider};
use crate::repositories::resolution::normalize_team_name as norm;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use std::{collections::BTreeSet, sync::OnceLock};

#[derive(Deserialize)]
struct CatalogEntry {
    name: String,
    country: String,
    url: String,
    provider: String,
}
fn catalog() -> &'static Vec<CatalogEntry> {
    static DATA: OnceLock<Vec<CatalogEntry>> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("logo_catalog.json")).unwrap())
}
pub fn country(value: &str) -> String {
    match norm(value).as_str() {
        "turkiye" | "turkey" => "turkey".into(),
        "usa" | "united states" => "united states".into(),
        "republic of ireland" => "ireland".into(),
        "czechia" => "czech republic".into(),
        "the netherlands" => "netherlands".into(),
        _ => norm(value),
    }
}
fn league_country(name: &str) -> Option<String> {
    // Explicit country prefixes from the Iddaa competition identity, not team-name guesses.
    let prefixes = [
        ("İngiltere", "England"),
        ("Almanya", "Germany"),
        ("İtalya", "Italy"),
        ("Fransa", "France"),
        ("İspanya", "Spain"),
        ("Türkiye", "Turkey"),
        ("Hollanda", "Netherlands"),
        ("Belçika", "Belgium"),
        ("Portekiz", "Portugal"),
        ("İskoçya", "Scotland"),
        ("Avusturya", "Austria"),
        ("İsviçre", "Switzerland"),
        ("ABD", "United States"),
        ("Arjantin", "Argentina"),
        ("Brezilya", "Brazil"),
        ("Japonya", "Japan"),
        ("Güney Kore", "South Korea"),
        ("Norveç", "Norway"),
        ("İsveç", "Sweden"),
        ("Danimarka", "Denmark"),
        ("Finlandiya", "Finland"),
        ("İzlanda", "Iceland"),
        ("İrlanda", "Ireland"),
        ("Kuzey İrlanda", "Northern Ireland"),
        ("Galler", "Wales"),
        ("Avustralya", "Australia"),
        ("Batı Avustralya", "Australia"),
        ("Yeni Zelanda", "New Zealand"),
        ("Polonya", "Poland"),
        ("Çekya", "Czech Republic"),
        ("Çek Cumhuriyeti", "Czech Republic"),
        ("Slovakya", "Slovakia"),
        ("Slovenya", "Slovenia"),
        ("Hırvatistan", "Croatia"),
        ("Bosna Hersek", "Bosnia and Herzegovina"),
        ("Sırbistan", "Serbia"),
        ("Karadağ", "Montenegro"),
        ("Kosova", "Kosovo"),
        ("Bulgaristan", "Bulgaria"),
        ("Romanya", "Romania"),
        ("Macaristan", "Hungary"),
        ("Yunanistan", "Greece"),
        ("Kıbrıs", "Cyprus"),
        ("Malta", "Malta"),
        ("Albania", "Albania"),
        ("Arnavutluk", "Albania"),
        ("Andorra", "Andorra"),
        ("Lüksemburg", "Luxembourg"),
        ("Litvanya", "Lithuania"),
        ("Letonya", "Latvia"),
        ("Estonya", "Estonia"),
        ("Ukrayna", "Ukraine"),
        ("Rusya", "Russia"),
        ("Belarus", "Belarus"),
        ("Gürcistan", "Georgia"),
        ("Ermenistan", "Armenia"),
        ("Azerbaycan", "Azerbaijan"),
        ("Kazakistan", "Kazakhstan"),
        ("Özbekistan", "Uzbekistan"),
        ("Çin", "China"),
        ("Tayland", "Thailand"),
        ("Vietnam", "Vietnam"),
        ("Endonezya", "Indonesia"),
        ("Malezya", "Malaysia"),
        ("Singapur", "Singapore"),
        ("Hindistan", "India"),
        ("İran", "Iran"),
        ("Irak", "Iraq"),
        ("İsrail", "Israel"),
        ("Suudi Arabistan", "Saudi Arabia"),
        ("Birleşik Arap Emirlikleri", "United Arab Emirates"),
        ("Katar", "Qatar"),
        ("Bahreyn", "Bahrain"),
        ("Kuveyt", "Kuwait"),
        ("Mısır", "Egypt"),
        ("Fas", "Morocco"),
        ("Cezayir", "Algeria"),
        ("Tunus", "Tunisia"),
        ("Güney Afrika", "South Africa"),
        ("Meksika", "Mexico"),
        ("Kanada", "Canada"),
        ("Şili", "Chile"),
        ("Kolombiya", "Colombia"),
        ("Peru", "Peru"),
        ("Uruguay", "Uruguay"),
        ("Paraguay", "Paraguay"),
        ("Bolivya", "Bolivia"),
        ("Ekvador", "Ecuador"),
        ("Venezuela", "Venezuela"),
        ("Guatemala", "Guatemala"),
        ("Kosta Rika", "Costa Rica"),
        ("Honduras", "Honduras"),
        ("El Salvador", "El Salvador"),
        ("Nikaragua", "Nicaragua"),
        ("Panama", "Panama"),
    ];
    prefixes
        .iter()
        .find(|(prefix, _)| name == *prefix || name.starts_with(&format!("{prefix} ")))
        .map(|(_, v)| country(v))
}
pub fn identity(
    c: &Connection,
    item: &EntityIdentity,
) -> Result<(Vec<String>, Option<String>), String> {
    let mut names = vec![item.name.clone()];
    if item.entity_type != "TEAM" {
        return Ok((names, item.country.clone()));
    }
    let mut q=c.prepare("SELECT external_team_name FROM provider_team_mappings WHERE team_id=?1 ORDER BY CASE provider WHEN 'football-data.co.uk' THEN 0 ELSE 1 END").map_err(|e|e.to_string())?;
    let canonical = q
        .query_map([item.entity_id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    names.extend(canonical);
    let mut q = c
        .prepare("SELECT normalized_alias FROM team_aliases WHERE team_id=?1 ORDER BY id")
        .map_err(|e| e.to_string())?;
    names.extend(
        q.query_map([item.entity_id], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?,
    );
    let mut seen = BTreeSet::new();
    names.retain(|x| seen.insert(norm(x)));
    let mut countries = BTreeSet::new();
    if let Some(v) = item.country.as_deref().filter(|x| !x.is_empty()) {
        countries.insert(country(v));
    }
    let mut q=c.prepare("SELECT DISTINCT co.name,co.country FROM matches m JOIN competitions co ON co.id=m.competition_id WHERE m.home_team_id=?1 OR m.away_team_id=?1").map_err(|e|e.to_string())?;
    for row in q
        .query_map([item.entity_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| e.to_string())?
    {
        let (league, ct) = row.map_err(|e| e.to_string())?;
        if let Some(ct) = ct
            .filter(|x| !x.is_empty())
            .map(|x| country(&x))
            .or_else(|| league_country(&league))
        {
            countries.insert(ct);
        }
    }
    let ct = if countries.len() == 1 {
        countries.into_iter().next()
    } else {
        None
    };
    // Explicit abbreviations, scoped to country. Their full provider identities are
    // documented in crest_catalog / TheSportsDB; no fuzzy search is accepted.
    let aliases = [
        ("england", "brighton", "Brighton & Hove Albion"),
        ("england", "coventry", "Coventry City"),
        ("portugal", "sp lisbon", "Sporting CP"),
        ("england", "wolves", "Wolverhampton Wanderers"),
        ("england", "sheffield united", "Sheffield United"),
        ("germany", "m gladbach", "Borussia Monchengladbach"),
        ("netherlands", "excelsior", "Excelsior"),
    ];
    for (scope, short, full) in aliases {
        if ct.as_deref() == Some(scope)
            && names.iter().any(|x| norm(x) == short)
            && !names.iter().any(|x| norm(x) == norm(full))
        {
            names.push(full.into());
        }
    }
    Ok((names, ct))
}
pub fn add_source(
    c: &Connection,
    item: &EntityIdentity,
    provider: &str,
    url: &str,
    kind: &str,
    key: &str,
    context: &str,
    priority: i64,
) -> Result<(), String> {
    c.execute("INSERT OR IGNORE INTO logo_sources(entity_type,entity_id,provider,url,kind,lookup_key,identity_context,priority) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![item.entity_type,item.entity_id,provider,url,kind,key,context,priority]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn seed(c: &Connection, item: &EntityIdentity) -> Result<(), String> {
    let done: i64 = c
        .query_row(
            "SELECT discovery_version FROM entity_assets WHERE entity_type=?1 AND entity_id=?2",
            params![item.entity_type, item.entity_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if done >= 3 {
        return Ok(());
    }
    if done == 1 {
        c.execute("UPDATE logo_sources SET state='PENDING',error_kind=NULL WHERE entity_type=?1 AND entity_id=?2 AND error_kind='SOURCE_NOT_DISCOVERED'",params![item.entity_type,item.entity_id]).map_err(|e|e.to_string())?;
    }
    let (names, ct) = identity(c, item)?;
    let context = ct.clone().unwrap_or_default();
    // Persisted provider IDs are authoritative only within their own namespace.
    if item.entity_type == "TEAM" {
        let mut q=c.prepare("SELECT provider,external_team_id FROM provider_team_mappings WHERE team_id=?1 AND provider IN ('football-data.org','thesportsdb')").map_err(|e|e.to_string())?;
        for row in q
            .query_map([item.entity_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
        {
            let (p, id) = row.map_err(|e| e.to_string())?;
            if !id.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let (u, k) = if p == "football-data.org" {
                (
                    format!("https://crests.football-data.org/{id}.png"),
                    "IMAGE",
                )
            } else {
                (
                    format!("https://www.thesportsdb.com/api/v1/json/123/lookupteam.php?id={id}"),
                    "SPORTSDB",
                )
            };
            add_source(c, item, &p, &u, k, &id, &context, 0)?;
        }
    }
    let prior:Option<(Option<String>,Option<String>,Option<String>)>=c.query_row("SELECT source_provider,source_url,last_error FROM entity_assets WHERE entity_type=?1 AND entity_id=?2",params![item.entity_type,item.entity_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;
    if let Some((Some(p), Some(u), err)) = prior {
        add_source(c, item, &p, &u, "IMAGE", "persisted", &context, 1)?;
        if err.as_deref().is_some_and(|e| {
            e.starts_with("HTTP_404:") || e.contains("corrupt") || e.contains("not an image")
        }) {
            c.execute("UPDATE logo_sources SET state='TERMINAL',error_kind=?3 WHERE entity_type=?1 AND entity_id=?2 AND url=?4",params![item.entity_type,item.entity_id,if err.unwrap().starts_with("HTTP_404:"){"SOURCE_404"}else{"INVALID_IMAGE"},u]).map_err(|e|e.to_string())?;
        }
    }
    if let Some(ref ct) = ct {
        for (n, name) in names.iter().enumerate() {
            let matches: Vec<_> = catalog()
                .iter()
                .filter(|x| country(&x.country) == *ct && norm(&x.name) == norm(name))
                .collect();
            // Same-country conflicting exact identities are rejected, never ranked fuzzily.
            for x in &matches {
                let urls: BTreeSet<_> = matches
                    .iter()
                    .filter(|other| other.provider == x.provider)
                    .map(|other| other.url.as_str())
                    .collect();
                if urls.len() != 1 {
                    continue;
                }
                add_source(
                    c,
                    item,
                    &x.provider,
                    &x.url,
                    "IMAGE",
                    name,
                    ct,
                    10 + n as i64,
                )?;
            }
        }
        for (n, name) in names.iter().take(6).enumerate() {
            let mut url =
                reqwest::Url::parse("https://www.thesportsdb.com/api/v1/json/123/searchteams.php")
                    .unwrap();
            url.query_pairs_mut().append_pair("t", name);
            add_source(
                c,
                item,
                "thesportsdb",
                url.as_str(),
                "SPORTSDB",
                name,
                ct,
                30 + n as i64,
            )?;
            let query = EntityIdentity {
                name: name.clone(),
                country: Some(ct.clone()),
                ..item.clone()
            };
            add_source(
                c,
                item,
                "wikimedia-pageimages-v3",
                &WikimediaProvider.discovery_url(&query),
                "WIKI",
                name,
                ct,
                50 + n as i64,
            )?;
        }
    }
    c.execute("INSERT INTO logo_attempts(entity_type,entity_id,provider,stage,outcome,detail) VALUES(?1,?2,'local-catalog-v1','DISCOVERY',?3,?4)",params![item.entity_type,item.entity_id,if ct.is_some(){"CATALOG_SEARCHED"}else{"PROVIDER_MAPPING_MISSING"},format!("names={}; country={context}",names.join(" | "))]).map_err(|e|e.to_string())?;
    c.execute("UPDATE entity_assets SET discovery_version=3,discovery_state='SEARCHED',failure_kind=CASE WHEN ?3='' THEN 'PROVIDER_MAPPING_MISSING' ELSE failure_kind END WHERE entity_type=?1 AND entity_id=?2",params![item.entity_type,item.entity_id,context]).map_err(|e|e.to_string())?;
    Ok(())
}

#[derive(Debug)]
pub struct Source {
    pub id: i64,
    pub provider: String,
    pub url: String,
    pub kind: String,
    pub key: String,
    pub context: String,
    pub attempts: i64,
}
pub fn next(c: &Connection, item: &EntityIdentity) -> Result<Option<Source>, String> {
    c.query_row("SELECT id,provider,url,kind,lookup_key,identity_context,attempts FROM logo_sources s WHERE entity_type=?1 AND entity_id=?2 AND state IN ('PENDING','RETRY') AND (next_retry_at IS NULL OR julianday(next_retry_at)<=julianday('now')) AND NOT EXISTS(SELECT 1 FROM logo_provider_backoff b WHERE b.provider=s.provider AND julianday(b.next_retry_at)>julianday('now')) ORDER BY priority,id LIMIT 1",params![item.entity_type,item.entity_id],|r|Ok(Source{id:r.get(0)?,provider:r.get(1)?,url:r.get(2)?,kind:r.get(3)?,key:r.get(4)?,context:r.get(5)?,attempts:r.get(6)?})).optional().map_err(|e|e.to_string())
}
pub fn sports_badge(source: &Source, bytes: &[u8]) -> Option<(String, String)> {
    let json: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let rows = json["teams"].as_array()?;
    let matches: Vec<_> = rows
        .iter()
        .filter(|x| {
            if x["strSport"] != "Soccer"
                || country(x["strCountry"].as_str().unwrap_or("")) != source.context
            {
                return false;
            }
            if source.url.contains("lookupteam.php") {
                return x["idTeam"].as_str() == Some(source.key.as_str());
            }
            let mut names = vec![x["strTeam"].as_str().unwrap_or("")];
            names.extend(x["strTeamAlternate"].as_str().unwrap_or("").split(','));
            names.iter().any(|name| norm(name) == norm(&source.key))
        })
        .collect();
    if matches.len() != 1 {
        return None;
    }
    let x = matches[0];
    let url = x["strBadge"].as_str()?;
    let host = reqwest::Url::parse(url).ok()?.host_str()?.to_string();
    if !url.starts_with("https://")
        || !(host == "thesportsdb.com" || host.ends_with(".thesportsdb.com"))
    {
        return None;
    }
    Some((url.into(), x["idTeam"].as_str()?.into()))
}
pub fn settle(c: &Connection, item: &EntityIdentity) -> Result<(), String> {
    let pending:i64=c.query_row("SELECT count(*) FROM logo_sources WHERE entity_type=?1 AND entity_id=?2 AND state IN ('PENDING','RETRY')",params![item.entity_type,item.entity_id],|r|r.get(0)).map_err(|e|e.to_string())?;
    if pending == 0 {
        c.execute("UPDATE entity_assets SET status='UNAVAILABLE',discovery_state=CASE WHEN failure_kind='PROVIDER_MAPPING_MISSING' THEN 'UNAVAILABLE' WHEN failure_kind='INVALID_IMAGE' THEN 'INVALID' ELSE 'SOURCE_NOT_FOUND' END,discovery_completed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),next_retry_at=NULL,retry_state=NULL WHERE entity_type=?1 AND entity_id=?2 AND status<>'READY'",params![item.entity_type,item.entity_id]).map_err(|e|e.to_string())?;
    } else if next(c, item)?.is_some() {
        c.execute("UPDATE entity_assets SET status='QUEUED',next_retry_at=NULL,retry_state=NULL,discovery_state='SEARCHED' WHERE entity_type=?1 AND entity_id=?2 AND status<>'READY'",params![item.entity_type,item.entity_id]).map_err(|e|e.to_string())?;
    } else {
        c.execute("UPDATE entity_assets SET status='MISSING',next_retry_at=datetime('now','+60 seconds'),discovery_state='RETRY_LATER',retry_state=CASE WHEN EXISTS(SELECT 1 FROM logo_sources s JOIN logo_provider_backoff b ON b.provider=s.provider WHERE s.entity_type=?1 AND s.entity_id=?2 AND s.state IN ('PENDING','RETRY') AND julianday(b.next_retry_at)>julianday('now')) THEN 'RATE_LIMITED' ELSE NULL END WHERE entity_type=?1 AND entity_id=?2 AND status<>'READY'",params![item.entity_type,item.entity_id]).map_err(|e|e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    #[test]
    fn country_scoped_exact_catalog_and_aliases() {
        let db = Database::open_in_memory().unwrap();
        let c = db.connection().unwrap();
        c.execute("INSERT INTO teams(id,normalized_name,country) VALUES(1,'Arsenal','Brazil'),(2,'Arsenal','England')",[]).unwrap();
        for id in [1, 2] {
            c.execute(
                "INSERT INTO entity_assets(entity_type,entity_id) VALUES('TEAM',?1)",
                [id],
            )
            .unwrap();
            let item = EntityIdentity {
                entity_type: "TEAM".into(),
                entity_id: id,
                name: "Arsenal".into(),
                country: Some(if id == 1 { "Brazil" } else { "England" }.into()),
            };
            seed(&c, &item).unwrap();
        }
        let n: i64 = c
            .query_row(
                "SELECT count(*) FROM logo_sources WHERE entity_id=1 AND url LIKE '%/england/%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
        let n: i64 = c
            .query_row(
                "SELECT count(*) FROM logo_sources WHERE entity_id=2 AND kind='IMAGE'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(n > 0);
    }
    #[test]
    fn sportsdb_rejects_wrong_country_and_ambiguous_names() {
        let s = Source {
            id: 1,
            provider: "thesportsdb".into(),
            url: "searchteams.php".into(),
            kind: "SPORTSDB".into(),
            key: "United".into(),
            context: "england".into(),
            attempts: 0,
        };
        assert!(sports_badge(&s,br#"{"teams":[{"strSport":"Soccer","strTeam":"United","strCountry":"Brazil","strBadge":"https://www.thesportsdb.com/a.png","idTeam":"1"}]}"#).is_none());
        assert_eq!(
            league_country("İngiltere Federasyon Kupası"),
            Some("england".into())
        );
        assert_eq!(league_country("UEFA Champions League"), None);
    }
}
