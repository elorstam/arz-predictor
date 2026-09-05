use rusqlite::{params, Connection, OptionalExtension};

use crate::models::Team;

pub fn insert(
    connection: &Connection,
    normalized_name: &str,
    country: Option<&str>,
) -> rusqlite::Result<i64> {
    connection.execute(
        "INSERT INTO teams (normalized_name, country) VALUES (?1, ?2)",
        params![normalized_name, country],
    )?;
    Ok(connection.last_insert_rowid())
}

pub fn find_by_id(connection: &Connection, id: i64) -> rusqlite::Result<Option<Team>> {
    connection
        .query_row(
            "SELECT id, normalized_name, country, created_at, updated_at FROM teams WHERE id = ?1",
            [id],
            |row| {
                Ok(Team {
                    id: row.get(0)?,
                    normalized_name: row.get(1)?,
                    country: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .optional()
}
