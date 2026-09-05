mod acquisition;
mod dto;
pub(crate) mod markets;
mod orchestrator;
mod popularity;
mod popularity_dto;

pub use orchestrator::{import_local_bulletin, refresh_bulletin, IngestionIssue, RefreshSummary};
pub use popularity::{refresh as refresh_popularity, PopularityIssue, PopularityRefreshSummary};

pub const PROVIDER_ID: &str = "iddaa";

#[cfg(test)]
mod tests;
