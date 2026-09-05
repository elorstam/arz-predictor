use chrono::{LocalResult, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat, TimeZone, Utc};
use chrono_tz::Tz;
use sha2::{Digest, Sha256};

pub struct NormalizedKickoff {
    pub utc: String,
    pub local_date: String,
    pub time_known: bool,
}

pub fn normalize_kickoff(
    date_value: &str,
    time_value: Option<&str>,
    timezone: Tz,
) -> Result<NormalizedKickoff, String> {
    let date = ["%d/%m/%Y", "%d/%m/%y"]
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(date_value.trim(), format).ok())
        .ok_or_else(|| format!("invalid date '{date_value}'"))?;
    let supplied_time = time_value.map(str::trim).filter(|value| !value.is_empty());
    let (time, time_known) = match supplied_time {
        Some(value) => (
            NaiveTime::parse_from_str(value, "%H:%M")
                .map_err(|_| format!("invalid time '{value}'"))?,
            true,
        ),
        None => (
            NaiveTime::from_hms_opt(12, 0, 0).expect("noon is always a valid time"),
            false,
        ),
    };
    let local = NaiveDateTime::new(date, time);
    let zoned = match timezone.from_local_datetime(&local) {
        LocalResult::Single(value) => value,
        LocalResult::Ambiguous(first, second) => first.min(second),
        LocalResult::None => {
            return Err(format!(
                "local date/time {local} does not exist in timezone {}",
                timezone.name()
            ));
        }
    };
    Ok(NormalizedKickoff {
        utc: zoned
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Secs, true),
        local_date: date.format("%Y-%m-%d").to_string(),
        time_known,
    })
}

pub fn provider_match_key(
    provider: &str,
    league_code: &str,
    season: &str,
    local_date: &str,
    home_team: &str,
    away_team: &str,
) -> String {
    fn component(value: &str) -> String {
        value
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    let identity = [
        component(provider),
        component(league_code),
        component(season),
        component(local_date),
        component(home_team),
        component(away_team),
    ]
    .join("|");
    let digest = Sha256::digest(identity.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
