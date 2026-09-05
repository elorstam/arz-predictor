use std::{collections::HashMap, path::Path, time::Instant};

use serde::{Deserialize, Serialize};

use super::{
    catalog::{all_datasets, find_dataset, DatasetDefinition},
    orchestrator::{import_dataset, import_local_dataset, ImportSummary},
};
use crate::database::Database;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapRequest {
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub league_codes: Vec<String>,
    #[serde(default)]
    pub season_codes: Vec<String>,
    pub acquisition_mode: String,
    #[serde(default)]
    pub local_files: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapDatasetResult {
    pub dataset: String,
    pub success: bool,
    pub error: Option<String>,
    pub summary: Option<ImportSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapSummary {
    pub datasets_requested: usize,
    pub datasets_completed: usize,
    pub datasets_failed: usize,
    pub rows_seen: usize,
    pub matches_inserted: usize,
    pub matches_updated: usize,
    pub teams_created: usize,
    pub statistics_inserted: usize,
    pub statistics_updated: usize,
    pub rows_skipped: usize,
    pub rows_failed: usize,
    pub elapsed_ms: u128,
    pub datasets: Vec<BootstrapDatasetResult>,
}

fn select_datasets(request: &BootstrapRequest) -> Result<Vec<&'static DatasetDefinition>, String> {
    if request
        .preset
        .as_deref()
        .is_some_and(|preset| preset != "primary")
    {
        return Err(format!(
            "unsupported football-data bootstrap preset: {}",
            request.preset.as_deref().unwrap_or_default()
        ));
    }
    if request.preset.is_none() && request.league_codes.is_empty() {
        return Err("leagueCodes are required when no approved preset is selected".to_string());
    }
    if !matches!(request.acquisition_mode.as_str(), "http" | "local") {
        return Err("acquisitionMode must be 'http' or 'local'".to_string());
    }

    let league_codes: Vec<String> = request
        .league_codes
        .iter()
        .map(|code| code.trim().to_ascii_uppercase())
        .collect();
    let season_codes: Vec<String> = request
        .season_codes
        .iter()
        .map(|code| code.trim().to_string())
        .collect();
    for code in &league_codes {
        if !all_datasets()
            .iter()
            .any(|dataset| dataset.league_code == code)
        {
            return Err(format!("unsupported football-data league code: {code}"));
        }
    }
    for season in &season_codes {
        if !all_datasets()
            .iter()
            .any(|dataset| dataset.season_code == season)
        {
            return Err(format!("unsupported football-data season code: {season}"));
        }
    }
    if !league_codes.is_empty() && !season_codes.is_empty() {
        for league in &league_codes {
            for season in &season_codes {
                if find_dataset(league, season).is_none() {
                    return Err(format!(
                        "unregistered football-data dataset: {league}:{season}"
                    ));
                }
            }
        }
    }

    let selected: Vec<_> = all_datasets()
        .iter()
        .filter(|dataset| {
            (league_codes.is_empty() || league_codes.iter().any(|code| code == dataset.league_code))
                && (season_codes.is_empty()
                    || season_codes.iter().any(|code| code == dataset.season_code))
        })
        .collect();
    if selected.is_empty() {
        return Err("bootstrap selection contains no registered datasets".to_string());
    }
    if request.acquisition_mode == "local" {
        for dataset in &selected {
            if !request.local_files.contains_key(&dataset.key()) {
                return Err(format!(
                    "local file path is required for dataset {}",
                    dataset.key()
                ));
            }
        }
    }
    Ok(selected)
}

pub(crate) async fn bootstrap(
    database: &Database,
    request: &BootstrapRequest,
) -> Result<BootstrapSummary, String> {
    let datasets = select_datasets(request)?;
    let started = Instant::now();
    let mut result = BootstrapSummary {
        datasets_requested: datasets.len(),
        datasets_completed: 0,
        datasets_failed: 0,
        rows_seen: 0,
        matches_inserted: 0,
        matches_updated: 0,
        teams_created: 0,
        statistics_inserted: 0,
        statistics_updated: 0,
        rows_skipped: 0,
        rows_failed: 0,
        elapsed_ms: 0,
        datasets: Vec::with_capacity(datasets.len()),
    };

    for dataset in datasets {
        let imported = if request.acquisition_mode == "http" {
            import_dataset(database, dataset).await
        } else {
            let path = request
                .local_files
                .get(&dataset.key())
                .expect("local paths were validated before bootstrap");
            import_local_dataset(database, dataset, Path::new(path))
        };
        match imported {
            Ok(summary) => {
                result.datasets_completed += 1;
                result.rows_seen += summary.rows_seen;
                result.matches_inserted += summary.matches_inserted;
                result.matches_updated += summary.matches_updated;
                result.teams_created += summary.teams_created;
                result.statistics_inserted += summary.statistics_inserted;
                result.statistics_updated += summary.statistics_updated;
                result.rows_skipped += summary.rows_skipped;
                result.rows_failed += summary.rows_failed;
                result.datasets.push(BootstrapDatasetResult {
                    dataset: dataset.key(),
                    success: true,
                    error: None,
                    summary: Some(summary),
                });
            }
            Err(error) => {
                result.datasets_failed += 1;
                result.datasets.push(BootstrapDatasetResult {
                    dataset: dataset.key(),
                    success: false,
                    error: Some(error),
                    summary: None,
                });
            }
        }
    }
    result.elapsed_ms = started.elapsed().as_millis();
    Ok(result)
}

#[cfg(test)]
pub(crate) fn validate_request(request: &BootstrapRequest) -> Result<usize, String> {
    select_datasets(request).map(|datasets| datasets.len())
}
