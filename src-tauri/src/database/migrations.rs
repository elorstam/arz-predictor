use rusqlite::{params, Connection, OptionalExtension};

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_normalized_schema",
        sql: include_str!("../../migrations/0001_initial_normalized_schema.sql"),
    },
    Migration {
        version: 2,
        name: "coupon_tracking",
        sql: include_str!("../../migrations/0002_coupon_tracking.sql"),
    },
    Migration {
        version: 3,
        name: "data_ingestion",
        sql: include_str!("../../migrations/0003_data_ingestion.sql"),
    },
    Migration {
        version: 4,
        name: "iddaa_ingestion",
        sql: include_str!("../../migrations/0004_iddaa_ingestion.sql"),
    },
    Migration {
        version: 5,
        name: "iddaa_popularity",
        sql: include_str!("../../migrations/0005_iddaa_popularity.sql"),
    },
    Migration {
        version: 6,
        name: "entity_resolution",
        sql: include_str!("../../migrations/0006_entity_resolution.sql"),
    },
    Migration {
        version: 7,
        name: "entity_metadata_assets",
        sql: include_str!("../../migrations/0007_entity_metadata_assets.sql"),
    },
    Migration {
        version: 8,
        name: "feature_engine",
        sql: include_str!("../../migrations/0008_feature_engine.sql"),
    },
    Migration {
        version: 9,
        name: "prediction_engine",
        sql: include_str!("../../migrations/0009_prediction_engine.sql"),
    },
    Migration {
        version: 10,
        name: "backtest_calibration",
        sql: include_str!("../../migrations/0010_backtest_calibration.sql"),
    },
    Migration {
        version: 11,
        name: "calibrated_backtest_results",
        sql: include_str!("../../migrations/0011_calibrated_backtest_results.sql"),
    },
    Migration {
        version: 12,
        name: "daily_candidate_engine",
        sql: include_str!("../../migrations/0012_daily_candidate_engine.sql"),
    },
    Migration {
        version: 13,
        name: "candidate_exclusion_diagnostics",
        sql: include_str!("../../migrations/0013_candidate_exclusion_diagnostics.sql"),
    },
    Migration {
        version: 14,
        name: "final_coupon_engine",
        sql: include_str!("../../migrations/0014_final_coupon_engine.sql"),
    },
    Migration {
        version: 15,
        name: "coupon_settlement_progression",
        sql: include_str!("../../migrations/0015_coupon_settlement_progression.sql"),
    },
    Migration {
        version: 16,
        name: "lineup_foundation",
        sql: include_str!("../../migrations/0016_lineup_foundation.sql"),
    },
    Migration {
        version: 17,
        name: "lineup_adjustment_model",
        sql: include_str!("../../migrations/0017_lineup_adjustment_model.sql"),
    },
    Migration {
        version: 18,
        name: "prediction_base_lambdas",
        sql: include_str!("../../migrations/0018_prediction_base_lambdas.sql"),
    },
    Migration {
        version: 19,
        name: "lineup_revision_core_payload",
        sql: include_str!("../../migrations/0019_lineup_revision_core_payload.sql"),
    },
    Migration {
        version: 20,
        name: "candidate_lineup_revision_lineage",
        sql: include_str!("../../migrations/0020_candidate_lineup_revision_lineage.sql"),
    },
    Migration {
        version: 21,
        name: "daily_output_publications",
        sql: include_str!("../../migrations/0021_daily_output_publications.sql"),
    },
    Migration {
        version: 22,
        name: "iddaa_business_dates",
        sql: include_str!("../../migrations/0022_iddaa_business_dates.sql"),
    },
    Migration {
        version: 23,
        name: "asset_queue_and_series_audit",
        sql: include_str!("../../migrations/0023_asset_queue_and_series_audit.sql"),
    },
    Migration {
        version: 24,
        name: "iddaa_corner_market",
        sql: include_str!("../../migrations/0024_iddaa_corner_market.sql"),
    },
    Migration {
        version: 25,
        name: "btts_daily_pipeline",
        sql: include_str!("../../migrations/0025_btts_daily_pipeline.sql"),
    },
    Migration {
        version: 26,
        name: "logo_discovery",
        sql: include_str!("../../migrations/0026_logo_discovery.sql"),
    },
    Migration {
        version: 27,
        name: "incremental_resolution",
        sql: include_str!("../../migrations/0027_incremental_resolution.sql"),
    },
    Migration {
        version: 28,
        name: "coupon_settlement_audit",
        sql: include_str!("../../migrations/0028_coupon_settlement_audit.sql"),
    },
    Migration {
        version: 29,
        name: "popularity_selection_quotes",
        sql: include_str!("../../migrations/0029_popularity_selection_quotes.sql"),
    },
    Migration {
        version: 30,
        name: "model_performance_cache",
        sql: include_str!("../../migrations/0030_model_performance_cache.sql"),
    },
    Migration {
        version: 31,
        name: "model_performance_invalidation",
        sql: include_str!("../../migrations/0031_model_performance_invalidation.sql"),
    },
    Migration {
        version: 32,
        name: "production_history_epoch",
        sql: include_str!("../../migrations/0032_production_history_epoch.sql"),
    },
];

pub fn apply_pending(connection: &mut Connection) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )?;

    for migration in MIGRATIONS {
        let applied = transaction
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE version = ?1",
                [migration.version],
                |_| Ok(()),
            )
            .optional()?
            .is_some();

        if !applied {
            transaction.execute_batch(migration.sql)?;
            transaction.execute(
                "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
                params![migration.version, migration.name],
            )?;
        }
    }

    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_schema_27_and_28_upgrade_preserves_rows_and_bookkeeping() {
        assert_eq!(
            MIGRATIONS.iter().map(|m| m.version).collect::<Vec<_>>(),
            (1..=32).collect::<Vec<_>>()
        );
        for baseline in [27, 28] {
            let mut c = Connection::open_in_memory().unwrap();
            c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,name TEXT NOT NULL,applied_at TEXT NOT NULL DEFAULT 'baseline');").unwrap();
            for m in MIGRATIONS.iter().filter(|m| m.version <= baseline) {
                c.execute_batch(m.sql).unwrap();
                c.execute(
                    "INSERT INTO schema_migrations(version,name) VALUES(?1,?2)",
                    params![m.version, m.name],
                )
                .unwrap();
            }
            c.execute("INSERT INTO teams(id,normalized_name,country) VALUES(100,'Preserved club','England')", []).unwrap();
            apply_pending(&mut c).unwrap();
            apply_pending(&mut c).unwrap();
            let rows: i64 = c
                .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
                .unwrap();
            assert_eq!(rows, 32);
            let unchanged: i64 = c.query_row("SELECT COUNT(*) FROM schema_migrations WHERE version<=?1 AND applied_at='baseline'", [baseline], |r|r.get(0)).unwrap();
            assert_eq!(unchanged, baseline);
            assert_eq!(
                c.query_row::<String, _, _>(
                    "SELECT normalized_name FROM teams WHERE id=100",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
                "Preserved club"
            );
            for table in [
                "coupon_selection_settlement_audit",
                "coupon_result_refresh",
                "popularity_selection_quotes",
            ] {
                assert_eq!(
                    c.query_row::<i64, _, _>(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                        [table],
                        |r| r.get(0)
                    )
                    .unwrap(),
                    1
                );
            }
            assert_eq!(
                c.query_row::<String, _, _>("PRAGMA integrity_check", [], |r| r.get(0))
                    .unwrap(),
                "ok"
            );
        }
    }
}
