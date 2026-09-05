use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Competition {
    pub id: i64,
    pub name: String,
    pub country: Option<String>,
    pub current_season: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Team {
    pub id: i64,
    pub normalized_name: String,
    pub country: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub id: i64,
    pub competition_id: i64,
    pub season: String,
    pub home_team_id: i64,
    pub away_team_id: i64,
    pub kickoff_at: String,
    pub status: String,
    pub final_home_goals: Option<i64>,
    pub final_away_goals: Option<i64>,
    pub halftime_home_goals: Option<i64>,
    pub halftime_away_goals: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub scheduled_local_date: Option<String>,
    pub kickoff_time_known: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CouponCandidateRun {
    pub id: i64,
    pub target_date: String,
    pub coupon_type: String,
    pub generated_at: String,
    pub model_version_id: i64,
    pub status: String,
    pub target_candidate_count: i64,
    pub qualified_candidate_count: i64,
    pub generation_config_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CouponCandidate {
    pub id: i64,
    pub candidate_run_id: i64,
    pub prediction_id: i64,
    pub rank: i64,
    pub ranking_score: f64,
    pub model_probability_snapshot: f64,
    pub iddaa_odd_snapshot: Option<f64>,
    pub market_snapshot: String,
    pub selection_snapshot: String,
    pub line_value_snapshot: Option<f64>,
    pub explanation_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Coupon {
    pub id: i64,
    pub coupon_type: String,
    pub target_date: String,
    pub created_at: String,
    pub model_version_id: Option<i64>,
    pub source_candidate_run_id: Option<i64>,
    pub status: String,
    pub total_decimal_odd: Option<f64>,
    pub reference_stake: Option<f64>,
    pub settled_return: Option<f64>,
    pub settled_profit: Option<f64>,
    pub settled_at: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CouponSelection {
    pub id: i64,
    pub coupon_id: i64,
    pub prediction_id: Option<i64>,
    pub source_candidate_id: Option<i64>,
    pub selection_order: i64,
    pub match_id: i64,
    pub market_snapshot: String,
    pub selection_snapshot: String,
    pub line_value_snapshot: Option<f64>,
    pub model_probability_snapshot: f64,
    pub odd_snapshot: Option<f64>,
    pub status: String,
    pub settled_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct DatabaseHealth {
    pub status: &'static str,
    pub database_path: String,
    pub foreign_keys_enabled: bool,
    pub applied_migrations: i64,
}

#[derive(Debug, Serialize)]
pub struct DatabaseStats {
    pub competitions: i64,
    pub teams: i64,
    pub provider_team_mappings: i64,
    pub matches: i64,
    pub match_statistics: i64,
    pub provider_match_mappings: i64,
    pub odds_snapshots: i64,
    pub predictions: i64,
    pub model_versions: i64,
    pub coupon_candidate_runs: i64,
    pub coupon_candidates: i64,
    pub coupons: i64,
    pub coupon_selections: i64,
    pub coupon_system_sizes: i64,
    pub provider_competition_mappings: i64,
    pub data_import_runs: i64,
    pub provider_competition_metadata: i64,
    pub popularity_snapshots: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FootballDataQualitySummary {
    pub competition_id: i64,
    pub competition_name: String,
    pub country: Option<String>,
    pub season: String,
    pub match_count: i64,
    pub finished_match_count: i64,
    pub scheduled_match_count: i64,
    pub matches_with_shots: i64,
    pub matches_with_shots_on_target: i64,
    pub matches_with_corners: i64,
    pub matches_with_cards: i64,
    pub matches_with_halftime: i64,
    pub earliest_match_date: Option<String>,
    pub latest_match_date: Option<String>,
    pub last_successful_import_at: Option<String>,
}
