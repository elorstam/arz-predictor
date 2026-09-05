use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct RawEvent {
    #[serde(default)]
    pub i: Option<i64>,
    #[serde(default)]
    pub hn: Option<String>,
    #[serde(default)]
    pub an: Option<String>,
    #[serde(default)]
    pub sid: Option<i64>,
    #[serde(default)]
    pub ci: Option<i64>,
    #[serde(default)]
    pub d: Option<i64>,
    #[serde(default)]
    pub mbc: Option<i64>,
    #[serde(default, rename = "kOdd")]
    pub k_odd: Option<bool>,
    #[serde(default)]
    pub m: Vec<RawMarket>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawMarket {
    #[serde(default)]
    pub i: Option<i64>,
    #[serde(default)]
    pub t: Option<i64>,
    #[serde(default)]
    pub st: Option<i64>,
    #[serde(default)]
    pub sov: Option<String>,
    #[serde(default)]
    pub mbc: Option<i64>,
    #[serde(default)]
    pub o: Vec<RawSelection>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawSelection {
    #[serde(default)]
    pub no: Option<i64>,
    #[serde(default)]
    pub n: Option<String>,
    #[serde(default)]
    pub odd: Option<f64>,
    #[serde(default)]
    pub wodd: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawCompetition {
    #[serde(default, alias = "id")]
    pub i: Option<i64>,
    #[serde(default, alias = "name")]
    pub n: Option<String>,
    #[serde(default, alias = "country", alias = "countryName", alias = "cn")]
    pub country: Option<String>,
    #[serde(default, alias = "season")]
    pub season: Option<String>,
}

#[derive(Debug)]
pub struct ParsedPayload {
    pub events: Vec<RawEvent>,
    pub competitions: Vec<RawCompetition>,
    pub malformed_events: Vec<String>,
}

pub fn parse_bulletin(bytes: &[u8]) -> Result<ParsedPayload, String> {
    let root: Value =
        serde_json::from_slice(bytes).map_err(|error| format!("malformed_json: {error}"))?;
    let mut event_values = Vec::new();
    collect_event_values(&root, &mut event_values);
    if event_values.is_empty() {
        return Err("malformed_json: no event objects found".to_string());
    }
    let mut events = Vec::new();
    let mut malformed_events = Vec::new();
    for value in event_values {
        match serde_json::from_value::<RawEvent>(value.clone()) {
            Ok(event) => events.push(event),
            Err(error) => malformed_events.push(error.to_string()),
        }
    }
    let competitions = root
        .get("competitions")
        .map(parse_competition_value)
        .transpose()?
        .unwrap_or_default();
    Ok(ParsedPayload {
        events,
        competitions,
        malformed_events,
    })
}

pub fn parse_competitions(bytes: &[u8]) -> Result<Vec<RawCompetition>, String> {
    let root: Value =
        serde_json::from_slice(bytes).map_err(|error| format!("malformed_json: {error}"))?;
    parse_competition_value(&root)
}

fn parse_competition_value(root: &Value) -> Result<Vec<RawCompetition>, String> {
    let mut values = Vec::new();
    collect_competition_values(root, &mut values);
    let mut result = Vec::new();
    for value in values {
        if let Ok(competition) = serde_json::from_value::<RawCompetition>(value.clone()) {
            if competition.i.is_some()
                && competition
                    .n
                    .as_deref()
                    .is_some_and(|name| !name.trim().is_empty())
            {
                result.push(competition);
            }
        }
    }
    Ok(result)
}

fn collect_event_values<'a>(value: &'a Value, found: &mut Vec<&'a Value>) {
    match value {
        Value::Object(object)
            if object.contains_key("i")
                && (object.contains_key("hn") || object.contains_key("an")) =>
        {
            found.push(value);
        }
        Value::Object(object) => {
            for (key, child) in object {
                if key != "competitions" {
                    collect_event_values(child, found);
                }
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_event_values(child, found);
            }
        }
        _ => {}
    }
}

fn collect_competition_values<'a>(value: &'a Value, found: &mut Vec<&'a Value>) {
    match value {
        Value::Object(object)
            if (object.contains_key("i") || object.contains_key("id"))
                && (object.contains_key("n") || object.contains_key("name")) =>
        {
            found.push(value);
        }
        Value::Object(object) => {
            for child in object.values() {
                collect_competition_values(child, found);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_competition_values(child, found);
            }
        }
        _ => {}
    }
}
