use rusqlite::{params, Connection, OptionalExtension};

use crate::models::Competition;

pub fn insert(
    connection: &Connection,
    name: &str,
    country: Option<&str>,
    current_season: Option<&str>,
) -> rusqlite::Result<i64> {
    connection.execute(
        "INSERT INTO competitions (name, country, current_season) VALUES (?1, ?2, ?3)",
        params![name, country, current_season],
    )?;
    Ok(connection.last_insert_rowid())
}

#[allow(dead_code)]
pub fn find_by_id(connection: &Connection, id: i64) -> rusqlite::Result<Option<Competition>> {
    connection
        .query_row(
            "SELECT id, name, country, current_season FROM competitions WHERE id = ?1",
            [id],
            |row| {
                Ok(Competition {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    country: row.get(2)?,
                    current_season: row.get(3)?,
                })
            },
        )
        .optional()
}
