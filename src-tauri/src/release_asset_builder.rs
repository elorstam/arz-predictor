#[cfg(test)]
mod tests {
    use crate::{database::Database, providers::football_data, repositories};

    #[test]
    #[ignore = "maintainer-only: downloads public history and builds pinned release assets"]
    fn build_pinned_base_and_calibration_assets() {
        let output = std::env::var("ARZ_RELEASE_ARTIFACT_OUTPUT")
            .expect("ARZ_RELEASE_ARTIFACT_OUTPUT must point to the production resource directory");
        let work = tempfile::tempdir().unwrap();
        let db = Database::open(work.path().join("artifact-builder.sqlite3")).unwrap();
        for dataset in football_data::all_datasets().iter().filter(|dataset| {
            dataset.league_code == "E0" && ["2425", "2526"].contains(&dataset.season_code)
        }) {
            tauri::async_runtime::block_on(football_data::refresh_results(&db, dataset))
                .unwrap_or_else(|error| panic!("{}: {error}", dataset.key()));
        }
        let connection = db.connection().unwrap();
        let features = repositories::features::generate_dataset(&connection, None, None).unwrap();
        assert!(features.generated >= 100);
        let output = std::path::Path::new(&output);
        std::fs::create_dir_all(output).unwrap();
        let model =
            repositories::prediction_engine::train(&connection, "arz-base-1.0.2", output).unwrap();
        let base = repositories::prediction_engine::load_artifact(std::path::Path::new(
            &model.artifact_path,
        ))
        .unwrap();
        let backtest =
            repositories::calibration::walk_forward(&connection, &base.bundle.model_version)
                .unwrap();
        let calibration_path = output.join("calibration.json");
        repositories::calibration::fit_calibration_version(
            &base,
            &backtest,
            &calibration_path,
            Some("arz-base-1.0.2-cal1"),
        )
        .unwrap();
        std::fs::rename(output.join("artifact.json"), output.join("base-model.json")).unwrap();
    }
}
