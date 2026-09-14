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
        // Verified against sportsbook/get_market_config: 2_48 is full-time
        // total corners; 2_49 is first-half and must not be mixed with it.
        Some(48) => NormalizedMarketType::CornersTotal,
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
        NormalizedMarketType::TotalGoals | NormalizedMarketType::CornersTotal => {
            match normalized.as_str() {
                "alt" => Some("UNDER"),
                "üst" | "ust" => Some("OVER"),
                _ => None,
            }
        }
        NormalizedMarketType::BothTeamsToScore => {
            let alias: String = normalized
                .chars()
                .filter(|c| c.is_alphanumeric())
                .map(|c| match c {
                    'ı' => 'i',
                    'ş' => 's',
                    'ğ' => 'g',
                    'ü' => 'u',
                    'ö' => 'o',
                    'ç' => 'c',
                    _ => c,
                })
                .collect();
            match alias.as_str() {
                "var"
                | "kgvar"
                | "karsilikligolvar"
                | "evet"
                | "yes"
                | "bttsyes"
                | "bothteamstoscoreyes" => Some("YES"),
                "yok" | "kgyok" | "karsilikligolyok" | "hayir" | "no" | "bttsno"
                | "bothteamstoscoreno" => Some("NO"),
                _ => None,
            }
        }
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
    fn btts_turkish_and_english_aliases_never_invert_or_guess_combinations() {
        for raw in [
            "Var",
            "KG VAR",
            "KG Var",
            "KG_Var",
            "Karşılıklı Gol Var",
            "Evet",
            "YES",
            "BTTS Yes",
        ] {
            assert_eq!(
                normalize_selection(NormalizedMarketType::BothTeamsToScore, raw),
                Some("YES")
            );
        }
        for raw in [
            "Yok",
            "KG YOK",
            "KG Yok",
            "Karşılıklı Gol Yok",
            "Hayır",
            "HAYIR",
            "NO",
            "BTTS No",
        ] {
            assert_eq!(
                normalize_selection(NormalizedMarketType::BothTeamsToScore, raw),
                Some("NO")
            );
        }
        for raw in ["1", "2", "Var ve Üst", "İlk Yarı Var", ""] {
            assert_eq!(
                normalize_selection(NormalizedMarketType::BothTeamsToScore, raw),
                None
            );
        }
    }

    #[test]
    fn cautious_market_mapping_does_not_guess_line_markets() {
        assert_eq!(
            normalize_market(Some(101)),
            NormalizedMarketType::TotalGoals
        );
        assert_eq!(
            normalize_market(Some(48)),
            NormalizedMarketType::CornersTotal
        );
        assert_eq!(normalize_market(Some(49)), NormalizedMarketType::Unknown);
        assert_eq!(normalize_market(Some(999)), NormalizedMarketType::Unknown);
    }
}
