mod acquisition;
mod bootstrap;
mod cache;
mod cached_bootstrap;
mod catalog;
mod normalization;
mod orchestrator;
mod parser;

pub use acquisition::NetworkDiagnostic;
pub(crate) use bootstrap::bootstrap;
pub use bootstrap::{BootstrapRequest, BootstrapSummary};
pub use cache::{CacheMetadata, CacheStats};
pub(crate) use cached_bootstrap::{
    bootstrap_cached, cache_local_file, cache_stats, import_cached_dataset, status,
};
pub use cached_bootstrap::{
    BootstrapStatus, CachedBootstrapRequest, CachedBootstrapSummary, CORE_6_SEASONS,
    CORE_RECENT_3_SEASONS,
};
pub use catalog::{find_dataset, supported_datasets, DatasetDefinition, SupportedDataset};
pub(crate) use orchestrator::import_dataset;
pub(crate) use orchestrator::{diagnose_dataset, import_local_dataset};
pub use orchestrator::{ImportIssue, ImportSummary};

pub const PROVIDER_ID: &str = "football-data.co.uk";

#[cfg(test)]
mod tests;
