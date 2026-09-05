use chrono_tz::Tz;
use serde::Serialize;

const BASE_URL: &str = "https://www.football-data.co.uk/mmz4281";

#[derive(Debug, Clone, Copy)]
pub struct DatasetDefinition {
    pub league_code: &'static str,
    pub season_code: &'static str,
    pub competition_name: &'static str,
    pub country: &'static str,
    pub season: &'static str,
    pub timezone: Tz,
    pub is_current_season: bool,
}

impl DatasetDefinition {
    pub fn key(&self) -> String {
        format!("{}:{}", self.league_code, self.season_code)
    }

    pub fn source_url(&self) -> String {
        format!("{BASE_URL}/{}/{}.csv", self.season_code, self.league_code)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SupportedDataset {
    pub league_code: &'static str,
    pub season_code: &'static str,
    pub competition_name: &'static str,
    pub country: &'static str,
    pub season: &'static str,
    pub timezone: String,
    pub source_url: String,
    pub season_status: &'static str,
}

macro_rules! dataset {
    ($code:literal, $season_code:literal, $name:literal, $country:literal, $season:literal, $tz:path, $current:literal) => {
        DatasetDefinition {
            league_code: $code,
            season_code: $season_code,
            competition_name: $name,
            country: $country,
            season: $season,
            timezone: $tz,
            is_current_season: $current,
        }
    };
}

// Each entry is backed by a published football-data.co.uk dataset link.
// Bundesliga D1 2026/27 is deliberately absent because it is not published yet.
const DATASETS: &[DatasetDefinition] = &[
    dataset!(
        "E0",
        "2627",
        "Premier League",
        "England",
        "2026/27",
        chrono_tz::Europe::London,
        true
    ),
    dataset!(
        "E0",
        "2526",
        "Premier League",
        "England",
        "2025/26",
        chrono_tz::Europe::London,
        false
    ),
    dataset!(
        "E0",
        "2425",
        "Premier League",
        "England",
        "2024/25",
        chrono_tz::Europe::London,
        false
    ),
    dataset!(
        "E0",
        "2324",
        "Premier League",
        "England",
        "2023/24",
        chrono_tz::Europe::London,
        false
    ),
    dataset!(
        "E0",
        "2223",
        "Premier League",
        "England",
        "2022/23",
        chrono_tz::Europe::London,
        false
    ),
    dataset!(
        "E0",
        "2122",
        "Premier League",
        "England",
        "2021/22",
        chrono_tz::Europe::London,
        false
    ),
    dataset!(
        "E1",
        "2627",
        "Championship",
        "England",
        "2026/27",
        chrono_tz::Europe::London,
        true
    ),
    dataset!(
        "E1",
        "2526",
        "Championship",
        "England",
        "2025/26",
        chrono_tz::Europe::London,
        false
    ),
    dataset!(
        "E1",
        "2425",
        "Championship",
        "England",
        "2024/25",
        chrono_tz::Europe::London,
        false
    ),
    dataset!(
        "E1",
        "2324",
        "Championship",
        "England",
        "2023/24",
        chrono_tz::Europe::London,
        false
    ),
    dataset!(
        "E1",
        "2223",
        "Championship",
        "England",
        "2022/23",
        chrono_tz::Europe::London,
        false
    ),
    dataset!(
        "E1",
        "2122",
        "Championship",
        "England",
        "2021/22",
        chrono_tz::Europe::London,
        false
    ),
    dataset!(
        "SP1",
        "2627",
        "La Liga",
        "Spain",
        "2026/27",
        chrono_tz::Europe::Madrid,
        true
    ),
    dataset!(
        "SP1",
        "2526",
        "La Liga",
        "Spain",
        "2025/26",
        chrono_tz::Europe::Madrid,
        false
    ),
    dataset!(
        "SP1",
        "2425",
        "La Liga",
        "Spain",
        "2024/25",
        chrono_tz::Europe::Madrid,
        false
    ),
    dataset!(
        "SP1",
        "2324",
        "La Liga",
        "Spain",
        "2023/24",
        chrono_tz::Europe::Madrid,
        false
    ),
    dataset!(
        "SP1",
        "2223",
        "La Liga",
        "Spain",
        "2022/23",
        chrono_tz::Europe::Madrid,
        false
    ),
    dataset!(
        "SP1",
        "2122",
        "La Liga",
        "Spain",
        "2021/22",
        chrono_tz::Europe::Madrid,
        false
    ),
    dataset!(
        "D1",
        "2526",
        "Bundesliga",
        "Germany",
        "2025/26",
        chrono_tz::Europe::Berlin,
        false
    ),
    dataset!(
        "D1",
        "2425",
        "Bundesliga",
        "Germany",
        "2024/25",
        chrono_tz::Europe::Berlin,
        false
    ),
    dataset!(
        "D1",
        "2324",
        "Bundesliga",
        "Germany",
        "2023/24",
        chrono_tz::Europe::Berlin,
        false
    ),
    dataset!(
        "D1",
        "2223",
        "Bundesliga",
        "Germany",
        "2022/23",
        chrono_tz::Europe::Berlin,
        false
    ),
    dataset!(
        "D1",
        "2122",
        "Bundesliga",
        "Germany",
        "2021/22",
        chrono_tz::Europe::Berlin,
        false
    ),
    dataset!(
        "I1",
        "2627",
        "Serie A",
        "Italy",
        "2026/27",
        chrono_tz::Europe::Rome,
        true
    ),
    dataset!(
        "I1",
        "2526",
        "Serie A",
        "Italy",
        "2025/26",
        chrono_tz::Europe::Rome,
        false
    ),
    dataset!(
        "I1",
        "2425",
        "Serie A",
        "Italy",
        "2024/25",
        chrono_tz::Europe::Rome,
        false
    ),
    dataset!(
        "I1",
        "2324",
        "Serie A",
        "Italy",
        "2023/24",
        chrono_tz::Europe::Rome,
        false
    ),
    dataset!(
        "I1",
        "2223",
        "Serie A",
        "Italy",
        "2022/23",
        chrono_tz::Europe::Rome,
        false
    ),
    dataset!(
        "I1",
        "2122",
        "Serie A",
        "Italy",
        "2021/22",
        chrono_tz::Europe::Rome,
        false
    ),
    dataset!(
        "F1",
        "2627",
        "Ligue 1",
        "France",
        "2026/27",
        chrono_tz::Europe::Paris,
        true
    ),
    dataset!(
        "F1",
        "2526",
        "Ligue 1",
        "France",
        "2025/26",
        chrono_tz::Europe::Paris,
        false
    ),
    dataset!(
        "F1",
        "2425",
        "Ligue 1",
        "France",
        "2024/25",
        chrono_tz::Europe::Paris,
        false
    ),
    dataset!(
        "F1",
        "2324",
        "Ligue 1",
        "France",
        "2023/24",
        chrono_tz::Europe::Paris,
        false
    ),
    dataset!(
        "F1",
        "2223",
        "Ligue 1",
        "France",
        "2022/23",
        chrono_tz::Europe::Paris,
        false
    ),
    dataset!(
        "F1",
        "2122",
        "Ligue 1",
        "France",
        "2021/22",
        chrono_tz::Europe::Paris,
        false
    ),
    dataset!(
        "N1",
        "2627",
        "Eredivisie",
        "Netherlands",
        "2026/27",
        chrono_tz::Europe::Amsterdam,
        true
    ),
    dataset!(
        "N1",
        "2526",
        "Eredivisie",
        "Netherlands",
        "2025/26",
        chrono_tz::Europe::Amsterdam,
        false
    ),
    dataset!(
        "N1",
        "2425",
        "Eredivisie",
        "Netherlands",
        "2024/25",
        chrono_tz::Europe::Amsterdam,
        false
    ),
    dataset!(
        "N1",
        "2324",
        "Eredivisie",
        "Netherlands",
        "2023/24",
        chrono_tz::Europe::Amsterdam,
        false
    ),
    dataset!(
        "N1",
        "2223",
        "Eredivisie",
        "Netherlands",
        "2022/23",
        chrono_tz::Europe::Amsterdam,
        false
    ),
    dataset!(
        "N1",
        "2122",
        "Eredivisie",
        "Netherlands",
        "2021/22",
        chrono_tz::Europe::Amsterdam,
        false
    ),
    dataset!(
        "P1",
        "2627",
        "Primeira Liga",
        "Portugal",
        "2026/27",
        chrono_tz::Europe::Lisbon,
        true
    ),
    dataset!(
        "P1",
        "2526",
        "Primeira Liga",
        "Portugal",
        "2025/26",
        chrono_tz::Europe::Lisbon,
        false
    ),
    dataset!(
        "P1",
        "2425",
        "Primeira Liga",
        "Portugal",
        "2024/25",
        chrono_tz::Europe::Lisbon,
        false
    ),
    dataset!(
        "P1",
        "2324",
        "Primeira Liga",
        "Portugal",
        "2023/24",
        chrono_tz::Europe::Lisbon,
        false
    ),
    dataset!(
        "P1",
        "2223",
        "Primeira Liga",
        "Portugal",
        "2022/23",
        chrono_tz::Europe::Lisbon,
        false
    ),
    dataset!(
        "P1",
        "2122",
        "Primeira Liga",
        "Portugal",
        "2021/22",
        chrono_tz::Europe::Lisbon,
        false
    ),
    dataset!(
        "B1",
        "2627",
        "Jupiler League",
        "Belgium",
        "2026/27",
        chrono_tz::Europe::Brussels,
        true
    ),
    dataset!(
        "B1",
        "2526",
        "Jupiler League",
        "Belgium",
        "2025/26",
        chrono_tz::Europe::Brussels,
        false
    ),
    dataset!(
        "B1",
        "2425",
        "Jupiler League",
        "Belgium",
        "2024/25",
        chrono_tz::Europe::Brussels,
        false
    ),
    dataset!(
        "B1",
        "2324",
        "Jupiler League",
        "Belgium",
        "2023/24",
        chrono_tz::Europe::Brussels,
        false
    ),
    dataset!(
        "B1",
        "2223",
        "Jupiler League",
        "Belgium",
        "2022/23",
        chrono_tz::Europe::Brussels,
        false
    ),
    dataset!(
        "B1",
        "2122",
        "Jupiler League",
        "Belgium",
        "2021/22",
        chrono_tz::Europe::Brussels,
        false
    ),
    dataset!(
        "T1",
        "2627",
        "Futbol Ligi 1",
        "Turkey",
        "2026/27",
        chrono_tz::Europe::Istanbul,
        true
    ),
    dataset!(
        "T1",
        "2526",
        "Futbol Ligi 1",
        "Turkey",
        "2025/26",
        chrono_tz::Europe::Istanbul,
        false
    ),
    dataset!(
        "T1",
        "2425",
        "Futbol Ligi 1",
        "Turkey",
        "2024/25",
        chrono_tz::Europe::Istanbul,
        false
    ),
    dataset!(
        "T1",
        "2324",
        "Futbol Ligi 1",
        "Turkey",
        "2023/24",
        chrono_tz::Europe::Istanbul,
        false
    ),
    dataset!(
        "T1",
        "2223",
        "Futbol Ligi 1",
        "Turkey",
        "2022/23",
        chrono_tz::Europe::Istanbul,
        false
    ),
    dataset!(
        "T1",
        "2122",
        "Futbol Ligi 1",
        "Turkey",
        "2021/22",
        chrono_tz::Europe::Istanbul,
        false
    ),
];

pub fn all_datasets() -> &'static [DatasetDefinition] {
    DATASETS
}

pub fn find_dataset(league_code: &str, season_code: &str) -> Option<&'static DatasetDefinition> {
    DATASETS.iter().find(|dataset| {
        dataset.league_code.eq_ignore_ascii_case(league_code) && dataset.season_code == season_code
    })
}

pub fn supported_datasets() -> Vec<SupportedDataset> {
    DATASETS
        .iter()
        .map(|dataset| SupportedDataset {
            league_code: dataset.league_code,
            season_code: dataset.season_code,
            competition_name: dataset.competition_name,
            country: dataset.country,
            season: dataset.season,
            timezone: dataset.timezone.name().to_string(),
            source_url: dataset.source_url(),
            season_status: if dataset.is_current_season {
                "current"
            } else {
                "historical"
            },
        })
        .collect()
}
