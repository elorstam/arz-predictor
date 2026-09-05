use std::{path::Path, time::Instant};

use serde::{Deserialize, Serialize};

use super::{
    acquisition::download_csv,
    cache::{self, CacheMetadata},
    catalog::{all_datasets, DatasetDefinition},
    orchestrator::{import_cached_bytes, ImportSummary},
};
use crate::{database::Database, repositories::quality};

pub const CORE_6_SEASONS: &str = "core_6_seasons";
pub const CORE_RECENT_3_SEASONS: &str = "core_recent_3_seasons";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedBootstrapRequest {
    pub preset: String,
    pub acquisition_mode: String,
    #[serde(default = "default_stale_fallback")]
    pub allow_stale_cache_on_refresh_failure: bool,
}

fn default_stale_fallback() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct CachedBootstrapDatasetResult {
    pub league_code: String,
    pub season_code: String,
    pub competition: String,
    pub acquisition_source: Option<String>,
    pub cache_path: String,
    pub cache_age_seconds: Option<u64>,
    pub downloaded: bool,
    pub imported: bool,
    pub stale_cache_used: bool,
    pub refresh_error: Option<String>,
    pub rows_seen: usize,
    pub matches_inserted: usize,
    pub matches_updated: usize,
    pub teams_created: usize,
    pub statistics_inserted: usize,
    pub statistics_updated: usize,
    pub rows_failed: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CachedBootstrapSummary {
    pub preset: String,
    pub datasets_requested: usize,
    pub datasets_available: usize,
    pub datasets_imported: usize,
    pub datasets_failed: usize,
    pub rows_seen: usize,
    pub matches_inserted: usize,
    pub matches_updated: usize,
    pub teams_created: usize,
    pub statistics_inserted: usize,
    pub statistics_updated: usize,
    pub elapsed_ms: u128,
    pub datasets: Vec<CachedBootstrapDatasetResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapDatasetStatus {
    pub league_code: String,
    pub season_code: String,
    pub competition: String,
    pub season: String,
    pub cached: bool,
    pub cache_size_bytes: Option<u64>,
    pub cache_parseable: bool,
    pub last_successful_import_at: Option<String>,
    pub last_import_status: Option<String>,
    pub match_count: i64,
    pub finished_match_count: i64,
    pub matches_with_statistics: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapStatus {
    pub preset: String,
    pub preset_dataset_count: usize,
    pub datasets_cached: usize,
    pub datasets_imported_successfully: usize,
    pub datasets_missing: usize,
    pub datasets_with_failed_last_import: usize,
    pub total_cached_bytes: u64,
    pub total_normalized_matches: i64,
    pub total_teams: i64,
    pub earliest_normalized_match: Option<String>,
    pub latest_normalized_match: Option<String>,
    pub datasets: Vec<BootstrapDatasetStatus>,
}

pub fn preset_datasets(preset: &str) -> Result<Vec<&'static DatasetDefinition>, String> {
    let seasons: &[&str] = match preset {
        CORE_6_SEASONS => &["2122", "2223", "2324", "2425", "2526", "2627"],
        CORE_RECENT_3_SEASONS => &["2425", "2526", "2627"],
        _ => return Err(format!("unsupported cached bootstrap preset: {preset}")),
    };
    Ok(all_datasets()
        .iter()
        .filter(|dataset| seasons.contains(&dataset.season_code))
        .collect())
}

pub fn app_data_dir(database: &Database) -> Result<&Path, String> {
    database
        .path()
        .parent()
        .ok_or_else(|| "database path has no application-data parent".to_string())
}

pub fn cache_local_file(
    database: &Database,
    dataset: &DatasetDefinition,
    source: &Path,
) -> Result<CacheMetadata, String> {
    cache::cache_local_file(app_data_dir(database)?, dataset, source)
}

pub fn import_cached_dataset(
    database: &Database,
    dataset: &DatasetDefinition,
) -> Result<ImportSummary, String> {
    let app_data = app_data_dir(database)?;
    let bytes = cache::valid_cached_bytes(app_data, dataset).map_err(|error| error.to_string())?;
    let path = cache::dataset_path(app_data, dataset)?;
    import_cached_bytes(database, dataset, &path, &bytes)
}

pub async fn bootstrap_cached(
    database: &Database,
    request: &CachedBootstrapRequest,
) -> Result<CachedBootstrapSummary, String> {
    if !matches!(
        request.acquisition_mode.as_str(),
        "cache_only" | "cache_first" | "http_refresh"
    ) {
        return Err(
            "acquisitionMode must be 'cache_only', 'cache_first', or 'http_refresh'".to_string(),
        );
    }
    let datasets = preset_datasets(&request.preset)?;
    let app_data = app_data_dir(database)?.to_path_buf();
    let started = Instant::now();
    let mut summary = CachedBootstrapSummary {
        preset: request.preset.clone(),
        datasets_requested: datasets.len(),
        datasets_available: 0,
        datasets_imported: 0,
        datasets_failed: 0,
        rows_seen: 0,
        matches_inserted: 0,
        matches_updated: 0,
        teams_created: 0,
        statistics_inserted: 0,
        statistics_updated: 0,
        elapsed_ms: 0,
        datasets: Vec::with_capacity(datasets.len()),
    };

    for dataset in datasets {
        let metadata_before = cache::metadata(&app_data, dataset)?;
        let mut downloaded = false;
        let mut stale = false;
        let mut refresh_error_message = None;
        let mut acquisition_source = None;
        let acquired = match request.acquisition_mode.as_str() {
            "cache_only" => cache::valid_cached_bytes(&app_data, dataset)
                .map(|bytes| (bytes, "cache".to_string())),
            "cache_first" if metadata_before.parseable => {
                cache::valid_cached_bytes(&app_data, dataset)
                    .map(|bytes| (bytes, "cache".to_string()))
            }
            "cache_first" => acquire_http_to_cache(&app_data, dataset)
                .await
                .map(|bytes| {
                    downloaded = true;
                    (bytes, "http".to_string())
                }),
            "http_refresh" => match acquire_http_to_cache(&app_data, dataset).await {
                Ok(bytes) => {
                    downloaded = true;
                    Ok((bytes, "http".to_string()))
                }
                Err(refresh_error)
                    if request.allow_stale_cache_on_refresh_failure
                        && metadata_before.parseable =>
                {
                    stale = true;
                    refresh_error_message = Some(refresh_error.to_string());
                    cache::valid_cached_bytes(&app_data, dataset)
                        .map(|bytes| (bytes, "cache".to_string()))
                        .map_err(|cache_error| super::acquisition::AcquisitionError {
                            category: cache_error.category,
                            message: format!(
                                "refresh failed ({refresh_error}); cached fallback failed: {}",
                                cache_error.message
                            ),
                        })
                }
                Err(error) => Err(error),
            },
            _ => unreachable!(),
        };

        let path = cache::dataset_path(&app_data, dataset)?;
        let mut result = CachedBootstrapDatasetResult {
            league_code: dataset.league_code.to_string(),
            season_code: dataset.season_code.to_string(),
            competition: dataset.competition_name.to_string(),
            acquisition_source: None,
            cache_path: path.display().to_string(),
            cache_age_seconds: metadata_before.age_seconds,
            downloaded,
            imported: false,
            stale_cache_used: stale,
            refresh_error: refresh_error_message,
            rows_seen: 0,
            matches_inserted: 0,
            matches_updated: 0,
            teams_created: 0,
            statistics_inserted: 0,
            statistics_updated: 0,
            rows_failed: 0,
            error: None,
        };
        match acquired {
            Ok((bytes, source)) => {
                summary.datasets_available += 1;
                acquisition_source = Some(source);
                match import_cached_bytes(database, dataset, &path, &bytes) {
                    Ok(imported) => apply_import(&mut summary, &mut result, &imported),
                    Err(error) => result.error = Some(error),
                }
            }
            Err(error) => result.error = Some(error.to_string()),
        }
        result.acquisition_source = acquisition_source;
        result.cache_age_seconds = cache::metadata(&app_data, dataset)?.age_seconds;
        if result.imported {
            summary.datasets_imported += 1;
        } else {
            summary.datasets_failed += 1;
        }
        summary.datasets.push(result);
    }
    summary.elapsed_ms = started.elapsed().as_millis();
    Ok(summary)
}

async fn acquire_http_to_cache(
    app_data_dir: &Path,
    dataset: &DatasetDefinition,
) -> Result<Vec<u8>, super::acquisition::AcquisitionError> {
    let acquired = download_csv(&dataset.source_url()).await?;
    cache::store_validated_bytes(app_data_dir, dataset, &acquired.bytes).map_err(|message| {
        super::acquisition::AcquisitionError {
            category: "cache_write".to_string(),
            message,
        }
    })?;
    Ok(acquired.bytes)
}

fn apply_import(
    aggregate: &mut CachedBootstrapSummary,
    result: &mut CachedBootstrapDatasetResult,
    imported: &ImportSummary,
) {
    result.imported = true;
    result.rows_seen = imported.rows_seen;
    result.matches_inserted = imported.matches_inserted;
    result.matches_updated = imported.matches_updated;
    result.teams_created = imported.teams_created;
    result.statistics_inserted = imported.statistics_inserted;
    result.statistics_updated = imported.statistics_updated;
    result.rows_failed = imported.rows_failed;
    aggregate.rows_seen += imported.rows_seen;
    aggregate.matches_inserted += imported.matches_inserted;
    aggregate.matches_updated += imported.matches_updated;
    aggregate.teams_created += imported.teams_created;
    aggregate.statistics_inserted += imported.statistics_inserted;
    aggregate.statistics_updated += imported.statistics_updated;
}

pub fn status(database: &Database, preset: &str) -> Result<BootstrapStatus, String> {
    let datasets = preset_datasets(preset)?;
    let app_data = app_data_dir(database)?;
    let connection = database.connection()?;
    let mut rows = Vec::with_capacity(datasets.len());
    for dataset in datasets {
        let metadata = cache::metadata(app_data, dataset)?;
        let database_status = quality::dataset_bootstrap_status(&connection, &dataset.key())
            .map_err(|error| error.to_string())?;
        rows.push(BootstrapDatasetStatus {
            league_code: dataset.league_code.to_string(),
            season_code: dataset.season_code.to_string(),
            competition: dataset.competition_name.to_string(),
            season: dataset.season.to_string(),
            cached: metadata.exists && metadata.parseable,
            cache_size_bytes: metadata.size_bytes,
            cache_parseable: metadata.parseable,
            last_successful_import_at: database_status.last_successful_import_at,
            last_import_status: database_status.last_import_status,
            match_count: database_status.match_count,
            finished_match_count: database_status.finished_match_count,
            matches_with_statistics: database_status.matches_with_statistics,
        });
    }
    let totals = quality::normalized_totals(&connection).map_err(|error| error.to_string())?;
    Ok(BootstrapStatus {
        preset: preset.to_string(),
        preset_dataset_count: rows.len(),
        datasets_cached: rows.iter().filter(|row| row.cached).count(),
        datasets_imported_successfully: rows
            .iter()
            .filter(|row| row.last_successful_import_at.is_some())
            .count(),
        datasets_missing: rows.iter().filter(|row| !row.cached).count(),
        datasets_with_failed_last_import: rows
            .iter()
            .filter(|row| row.last_import_status.as_deref() == Some("failed"))
            .count(),
        total_cached_bytes: rows.iter().filter_map(|row| row.cache_size_bytes).sum(),
        total_normalized_matches: totals.matches,
        total_teams: totals.teams,
        earliest_normalized_match: totals.earliest_match,
        latest_normalized_match: totals.latest_match,
        datasets: rows,
    })
}

pub fn cache_stats(database: &Database) -> Result<super::cache::CacheStats, String> {
    cache::stats(app_data_dir(database)?)
}
