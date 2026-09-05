use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(dead_code)]
pub enum NormalizedMarketType {
    MatchResult,
    TotalGoals,
    BothTeamsToScore,
    DoubleChance,
    GoalRange,
    OddEven,
    HalftimeFulltime,
    TeamTotalGoals,
    CornersTotal,
    Unknown,
}

impl NormalizedMarketType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MatchResult => "MATCH_RESULT",
            Self::TotalGoals => "TOTAL_GOALS",
            Self::BothTeamsToScore => "BOTH_TEAMS_TO_SCORE",
            Self::DoubleChance => "DOUBLE_CHANCE",
            Self::GoalRange => "GOAL_RANGE",
            Self::OddEven => "ODD_EVEN",
            Self::HalftimeFulltime => "HALFTIME_FULLTIME",
            Self::TeamTotalGoals => "TEAM_TOTAL_GOALS",
            Self::CornersTotal => "CORNERS_TOTAL",
            Self::Unknown => "UNKNOWN",
        }
    }
}

pub fn normalize_market(code: Option<i64>) -> NormalizedMarketType {
    match code {
        Some(1) => NormalizedMarketType::MatchResult,
        Some(101) => NormalizedMarketType::TotalGoals,
        Some(89) => NormalizedMarketType::BothTeamsToScore,
        Some(92 | 77) => NormalizedMarketType::DoubleChance,
        Some(4) => NormalizedMarketType::GoalRange,
        Some(91) => NormalizedMarketType::OddEven,
        Some(90) => NormalizedMarketType::HalftimeFulltime,
        _ => NormalizedMarketType::Unknown,
    }
}

pub fn normalize_selection(market: NormalizedMarketType, raw: &str) -> Option<&'static str> {
    let normalized = raw.trim().to_lowercase();
    match market {
        NormalizedMarketType::MatchResult => match normalized.as_str() {
            "1" => Some("HOME"),
            "0" | "x" => Some("DRAW"),
            "2" => Some("AWAY"),
            _ => None,
        },
        NormalizedMarketType::TotalGoals => match normalized.as_str() {
            "alt" => Some("UNDER"),
            "üst" | "ust" => Some("OVER"),
            _ => None,
        },
        NormalizedMarketType::BothTeamsToScore => match normalized.as_str() {
            "var" => Some("YES"),
            "yok" => Some("NO"),
            _ => None,
        },
        NormalizedMarketType::DoubleChance => match normalized.as_str() {
            "1 ve 0" | "1 veya 0" => Some("HOME_OR_DRAW"),
            "1 ve 2" | "1 veya 2" => Some("HOME_OR_AWAY"),
            "0 ve 2" | "0 veya 2" => Some("DRAW_OR_AWAY"),
            _ => None,
        },
        NormalizedMarketType::OddEven => match normalized.as_str() {
            "tek" => Some("ODD"),
            "çift" | "cift" => Some("EVEN"),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cautious_market_mapping_does_not_guess_line_markets() {
        assert_eq!(
            normalize_market(Some(101)),
            NormalizedMarketType::TotalGoals
        );
        assert_eq!(normalize_market(Some(48)), NormalizedMarketType::Unknown);
        assert_eq!(normalize_market(Some(999)), NormalizedMarketType::Unknown);
    }
}
