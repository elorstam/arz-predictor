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
