use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{
    acquisition::{read_local_csv, AcquisitionError},
    catalog::{all_datasets, DatasetDefinition},
    parser::parse_csv,
};

#[derive(Debug, Clone, Serialize)]
pub struct CacheMetadata {
    pub dataset: String,
    pub path: String,
    pub exists: bool,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<String>,
    pub age_seconds: Option<u64>,
    pub sha256: Option<String>,
    pub parseable: bool,
    pub validation_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    pub cache_root: String,
    pub file_count: usize,
    pub dataset_count: usize,
    pub total_bytes: u64,
    pub invalid_or_unrecognized_files: Vec<String>,
}

pub fn cache_root(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("data").join("football-data")
}

pub fn dataset_path(app_data_dir: &Path, dataset: &DatasetDefinition) -> Result<PathBuf, String> {
    validate_catalog_component(dataset.season_code)?;
    validate_catalog_component(dataset.league_code)?;
    let root = cache_root(app_data_dir);
    let path = root
        .join(dataset.season_code)
        .join(format!("{}.csv", dataset.league_code));
    if !path.starts_with(&root) {
        return Err("resolved cache path escaped provider cache root".to_string());
    }
    Ok(path)
}

fn validate_catalog_component(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    let valid = !value.is_empty()
        && path.components().count() == 1
        && matches!(path.components().next(), Some(Component::Normal(_)))
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err(format!("unsafe catalog path component: {value}"))
    }
}

pub fn validate_bytes(bytes: &[u8], dataset: &DatasetDefinition) -> Result<(), String> {
    let parsed = parse_csv(bytes, dataset).map_err(|error| format!("malformed_csv: {error}"))?;
    if parsed.rows_seen > 0 && parsed.rows.is_empty() && !parsed.issues.is_empty() {
        return Err(format!(
            "malformed_csv: no valid rows; first issue: {}",
            parsed.issues[0].message
        ));
    }
    Ok(())
}

pub fn cache_local_file(
    app_data_dir: &Path,
    dataset: &DatasetDefinition,
    source: &Path,
) -> Result<CacheMetadata, String> {
    let bytes = read_local_csv(source).map_err(|error| error.to_string())?;
    store_validated_bytes(app_data_dir, dataset, &bytes)
}

pub fn store_validated_bytes(
    app_data_dir: &Path,
    dataset: &DatasetDefinition,
    bytes: &[u8],
) -> Result<CacheMetadata, String> {
    validate_bytes(bytes, dataset)?;
    let destination = dataset_path(app_data_dir, dataset)?;
    let parent = destination
        .parent()
        .ok_or_else(|| "cache destination has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create cache directory '{}': {error}",
            parent.display()
        )
    })?;

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(
        ".{}.{}.{}.tmp",
        dataset.league_code,
        std::process::id(),
        nonce
    ));
    let mut file = fs::File::create_new(&temporary).map_err(|error| {
        format!(
            "failed to create cache temporary file '{}': {error}",
            temporary.display()
        )
    })?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("failed to write cache temporary file: {error}"));
    }
    drop(file);
    replace_file(&temporary, &destination)?;
    metadata(app_data_dir, dataset)
}

fn replace_file(temporary: &Path, destination: &Path) -> Result<(), String> {
    if !destination.exists() {
        return fs::rename(temporary, destination).map_err(|error| {
            let _ = fs::remove_file(temporary);
            format!(
                "failed to commit cache file '{}': {error}",
                destination.display()
            )
        });
    }

    let backup = destination.with_extension("csv.previous");
    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|error| format!("failed to clear cache backup: {error}"))?;
    }
    fs::rename(destination, &backup)
        .map_err(|error| format!("failed to stage existing cache file: {error}"))?;
    match fs::rename(temporary, destination) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(&backup, destination);
            let _ = fs::remove_file(temporary);
            Err(format!("failed to replace cache file: {error}"))
        }
    }
}

pub fn valid_cached_bytes(
    app_data_dir: &Path,
    dataset: &DatasetDefinition,
) -> Result<Vec<u8>, AcquisitionError> {
    let path = dataset_path(app_data_dir, dataset).map_err(|message| AcquisitionError {
        category: "cache_path".to_string(),
        message,
    })?;
    let bytes = read_local_csv(&path)?;
    validate_bytes(&bytes, dataset).map_err(|message| AcquisitionError {
        category: "invalid_cache".to_string(),
        message,
    })?;
    Ok(bytes)
}

pub fn metadata(app_data_dir: &Path, dataset: &DatasetDefinition) -> Result<CacheMetadata, String> {
    let path = dataset_path(app_data_dir, dataset)?;
    if !path.exists() {
        return Ok(CacheMetadata {
            dataset: dataset.key(),
            path: path.display().to_string(),
            exists: false,
            size_bytes: None,
            modified_at: None,
            age_seconds: None,
            sha256: None,
            parseable: false,
            validation_error: None,
        });
    }
    let file_metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    let modified = file_metadata.modified().ok();
    let modified_at = modified.and_then(system_time_text);
    let age_seconds = modified
        .and_then(|time| SystemTime::now().duration_since(time).ok())
        .map(|v| v.as_secs());
    let validation = read_local_csv(&path)
        .map_err(|error| error.to_string())
        .and_then(|bytes| {
            validate_bytes(&bytes, dataset)?;
            Ok(format!("{:x}", Sha256::digest(&bytes)))
        });
    Ok(CacheMetadata {
        dataset: dataset.key(),
        path: path.display().to_string(),
        exists: true,
        size_bytes: Some(file_metadata.len()),
        modified_at,
        age_seconds,
        sha256: validation.as_ref().ok().cloned(),
        parseable: validation.is_ok(),
        validation_error: validation.err(),
    })
}

fn system_time_text(time: SystemTime) -> Option<String> {
    let timestamp = time.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    chrono::DateTime::from_timestamp(timestamp, 0).map(|value| value.to_rfc3339())
}

pub fn stats(app_data_dir: &Path) -> Result<CacheStats, String> {
    let root = cache_root(app_data_dir);
    let recognized: std::collections::HashMap<PathBuf, &DatasetDefinition> = all_datasets()
        .iter()
        .filter_map(|dataset| {
            dataset_path(app_data_dir, dataset)
                .ok()
                .map(|path| (path, dataset))
        })
        .collect();
    let mut result = CacheStats {
        cache_root: root.display().to_string(),
        file_count: 0,
        dataset_count: 0,
        total_bytes: 0,
        invalid_or_unrecognized_files: Vec::new(),
    };
    if !root.exists() {
        return Ok(result);
    }
    for season_entry in fs::read_dir(&root).map_err(|error| error.to_string())? {
        let season_entry = season_entry.map_err(|error| error.to_string())?;
        if !season_entry.path().is_dir() {
            result
                .invalid_or_unrecognized_files
                .push(season_entry.path().display().to_string());
            continue;
        }
        for file_entry in fs::read_dir(season_entry.path()).map_err(|error| error.to_string())? {
            let file_entry = file_entry.map_err(|error| error.to_string())?;
            let path = file_entry.path();
            if path.is_file() {
                result.file_count += 1;
                result.total_bytes += file_entry
                    .metadata()
                    .map_err(|error| error.to_string())?
                    .len();
                if let Some(dataset) = recognized.get(&path) {
                    result.dataset_count += 1;
                    if read_local_csv(&path)
                        .map_err(|error| error.to_string())
                        .and_then(|bytes| validate_bytes(&bytes, dataset))
                        .is_err()
                    {
                        result
                            .invalid_or_unrecognized_files
                            .push(path.display().to_string());
                    }
                } else {
                    result
                        .invalid_or_unrecognized_files
                        .push(path.display().to_string());
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
pub(crate) fn validate_component_for_test(value: &str) -> Result<(), String> {
    validate_catalog_component(value)
}
