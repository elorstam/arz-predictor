pub mod assets;
mod commands;
mod database;
pub mod licensing;
pub mod models;
pub mod providers;
pub mod repositories;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let database =
                database::Database::open(app_data_dir.join("football-predictor.sqlite3"))?;
            let asset_manager =
                assets::AssetSyncManager::new(database.path().to_path_buf(), app_data_dir);
            asset_manager.start();
            app.manage(asset_manager);
            app.manage(database);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::license_status,
            commands::license_activate,
            commands::license_verify,
            commands::license_clear_local_token,
            commands::database_health,
            commands::database_stats,
            commands::data_center_status,
            commands::resolver_scan,
            commands::resolver_apply_safe_matches,
            commands::resolver_get_review_queue,
            commands::resolver_accept_team_match,
            commands::resolver_reject_team_match,
            commands::resolver_link_team_mapping,
            commands::merge_normalized_teams,
            commands::merge_normalized_matches,
            commands::football_data_bootstrap,
            commands::football_data_bootstrap_cached,
            commands::football_data_bootstrap_status,
            commands::football_data_cache_local_file,
            commands::football_data_cache_stats,
            commands::football_data_import_cached_dataset,
            commands::football_data_import_dataset,
            commands::football_data_import_local_dataset,
            commands::football_data_network_diagnostic,
            commands::football_data_quality_summary,
            commands::football_data_supported_datasets,
            commands::iddaa_refresh_bulletin,
            commands::iddaa_import_local_bulletin,
            commands::iddaa_bulletin_status,
            commands::iddaa_get_upcoming_matches,
            commands::iddaa_get_latest_odds,
            commands::iddaa_refresh_popularity,
            commands::iddaa_popularity_status,
            commands::iddaa_get_latest_popular_selections,
            commands::asset_sync_scan,
            commands::asset_sync_start,
            commands::asset_sync_status,
            commands::asset_sync_retry_failed,
            commands::entity_logo_path,
            commands::feature_engine_generate_for_match,
            commands::feature_engine_generate_dataset,
            commands::feature_engine_market_readiness,
            commands::feature_engine_quality_summary,
            commands::prediction_model_train,
            commands::prediction_model_validate_artifact,
            commands::prediction_model_activate,
            commands::prediction_generate_for_match,
            commands::prediction_generate_upcoming,
            commands::prediction_engine_status,
            commands::prediction_backtest_run,
            commands::prediction_backtest_summary,
            commands::prediction_backtest_status,
            commands::prediction_calibration_fit,
            commands::prediction_calibration_validate,
            commands::prediction_calibration_activate,
            commands::prediction_calibration_buckets,
            commands::prediction_model_performance,
            commands::prediction_odds_band_performance,
            commands::candidate_engine_generate_daily,
            commands::candidate_engine_get_daily,
            commands::candidate_engine_generate_lineup_revision,
            commands::candidate_engine_get_lineup_revision,
            commands::candidate_engine_status,
            commands::candidate_engine_policy,
            commands::model_supported_populars_get,
            commands::model_performance_get,
            commands::coupon_performance_get,
            commands::coupon_engine_generate_daily,
            commands::coupon_engine_create_draft,
            commands::coupon_engine_update_draft_selections,
            commands::coupon_engine_finalize,
            commands::coupon_engine_get_coupon,
            commands::coupon_engine_lineup_revision_impact,
            commands::coupon_engine_system_preview,
            commands::coupon_engine_get_daily,
            commands::compound_series_start,
            commands::compound_series_status,
            commands::compound_series_settle_step,
            commands::coupon_engine_settle,
            commands::compound_series_generate_step,
            commands::compound_series_cancel,
            commands::lineup_import_local,
            commands::lineup_refresh_match,
            commands::lineup_get_latest,
            commands::lineup_get_history,
            commands::lineup_status_for_match,
            commands::lineup_feature_generate_for_match,
            commands::lineup_feature_quality,
            commands::lineup_refresh_due_matches,
            commands::lineup_engine_status,
            commands::lineup_model_train,
            commands::lineup_model_validate_artifact,
            commands::lineup_model_activate,
            commands::lineup_model_status,
            commands::lineup_prediction_generate_revision
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
