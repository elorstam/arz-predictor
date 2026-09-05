use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PopularResponse {
    #[serde(default)]
    is_success: bool,
    #[serde(default)]
    data: HashMap<String, Vec<RawPopularSelection>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPopularSelection {
    #[serde(default)]
    pub populer_bet_id: Option<i64>,
    #[serde(default)]
    pub sport_id: Option<i64>,
    #[serde(default)]
    pub event_id: Option<i64>,
    #[serde(default)]
    pub event_name: Option<String>,
    #[serde(default)]
    pub event_date: Option<String>,
    #[serde(default)]
    pub competition_name: Option<String>,
    #[serde(default)]
    pub market_id: Option<i64>,
    #[serde(default)]
    pub market_sub_type: Option<i64>,
    #[serde(default)]
    pub market_name: Option<String>,
    #[serde(default)]
    pub outcome_no: Option<i64>,
    #[serde(default)]
    pub outcome_name: Option<String>,
    #[serde(default)]
    pub special_odds_value: Option<String>,
    #[serde(default)]
    pub total_played: Option<i64>,
    #[serde(default)]
    pub total_played_round_str: Option<String>,
    #[serde(default)]
    pub played_ratio: Option<f64>,
    #[serde(default)]
    pub odd: Option<f64>,
    #[serde(default)]
    pub web_odd: Option<f64>,
}

pub fn parse_popular_football(bytes: &[u8]) -> Result<Vec<RawPopularSelection>, String> {
    let response: PopularResponse =
        serde_json::from_slice(bytes).map_err(|error| format!("malformed_json: {error}"))?;
    if !response.is_success {
        return Err("Iddaa popularity response reported isSuccess=false".to_string());
    }
    Ok(response.data.get("1").cloned().unwrap_or_default())
}
