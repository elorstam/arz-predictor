use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{Coupon, CouponSelection};

pub struct NewCoupon<'a> {
    pub coupon_type: &'a str,
    pub target_date: &'a str,
    pub model_version_id: Option<i64>,
    pub source_candidate_run_id: Option<i64>,
    pub total_decimal_odd: Option<f64>,
    pub reference_stake: Option<f64>,
    pub notes: Option<&'a str>,
}

pub struct NewCouponSelection<'a> {
    pub coupon_id: i64,
    pub prediction_id: Option<i64>,
    pub source_candidate_id: Option<i64>,
    pub selection_order: i64,
    pub match_id: i64,
    pub market_snapshot: &'a str,
    pub selection_snapshot: &'a str,
    pub line_value_snapshot: Option<f64>,
    pub model_probability_snapshot: f64,
    pub odd_snapshot: Option<f64>,
}

pub fn insert(connection: &Connection, coupon: &NewCoupon<'_>) -> rusqlite::Result<i64> {
    connection.execute(
        "INSERT INTO coupons (
            coupon_type, target_date, model_version_id, source_candidate_run_id,
            total_decimal_odd, reference_stake, notes
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            coupon.coupon_type,
            coupon.target_date,
            coupon.model_version_id,
            coupon.source_candidate_run_id,
            coupon.total_decimal_odd,
            coupon.reference_stake,
            coupon.notes
        ],
    )?;
    Ok(connection.last_insert_rowid())
}

pub fn find_by_id(connection: &Connection, id: i64) -> rusqlite::Result<Option<Coupon>> {
    connection
        .query_row(
            "SELECT id, coupon_type, target_date, created_at, model_version_id,
                    source_candidate_run_id, status, total_decimal_odd, reference_stake,
                    settled_return, settled_profit, settled_at, notes
             FROM coupons WHERE id = ?1",
            [id],
            |row| {
                Ok(Coupon {
                    id: row.get(0)?,
                    coupon_type: row.get(1)?,
                    target_date: row.get(2)?,
                    created_at: row.get(3)?,
                    model_version_id: row.get(4)?,
                    source_candidate_run_id: row.get(5)?,
                    status: row.get(6)?,
                    total_decimal_odd: row.get(7)?,
                    reference_stake: row.get(8)?,
                    settled_return: row.get(9)?,
                    settled_profit: row.get(10)?,
                    settled_at: row.get(11)?,
                    notes: row.get(12)?,
                })
            },
        )
        .optional()
}

pub fn insert_selection(
    connection: &Connection,
    selection: &NewCouponSelection<'_>,
) -> rusqlite::Result<i64> {
    connection.execute(
        "INSERT INTO coupon_selections (
            coupon_id, prediction_id, source_candidate_id, selection_order, match_id,
            market_snapshot, selection_snapshot, line_value_snapshot,
            model_probability_snapshot, odd_snapshot
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            selection.coupon_id,
            selection.prediction_id,
            selection.source_candidate_id,
            selection.selection_order,
            selection.match_id,
            selection.market_snapshot,
            selection.selection_snapshot,
            selection.line_value_snapshot,
            selection.model_probability_snapshot,
            selection.odd_snapshot
        ],
    )?;
    Ok(connection.last_insert_rowid())
}

pub fn list_selections(
    connection: &Connection,
    coupon_id: i64,
) -> rusqlite::Result<Vec<CouponSelection>> {
    let mut statement = connection.prepare(
        "SELECT id, coupon_id, prediction_id, source_candidate_id, selection_order,
                match_id, market_snapshot, selection_snapshot, line_value_snapshot,
                model_probability_snapshot, odd_snapshot, status, settled_at, created_at
         FROM coupon_selections
         WHERE coupon_id = ?1
         ORDER BY selection_order",
    )?;
    let selections = statement
        .query_map([coupon_id], |row| {
            Ok(CouponSelection {
                id: row.get(0)?,
                coupon_id: row.get(1)?,
                prediction_id: row.get(2)?,
                source_candidate_id: row.get(3)?,
                selection_order: row.get(4)?,
                match_id: row.get(5)?,
                market_snapshot: row.get(6)?,
                selection_snapshot: row.get(7)?,
                line_value_snapshot: row.get(8)?,
                model_probability_snapshot: row.get(9)?,
                odd_snapshot: row.get(10)?,
                status: row.get(11)?,
                settled_at: row.get(12)?,
                created_at: row.get(13)?,
            })
        })?
        .collect();
    selections
}

pub fn add_system_size(
    connection: &Connection,
    coupon_id: i64,
    system_size: i64,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO coupon_system_sizes (coupon_id, system_size) VALUES (?1, ?2)",
        params![coupon_id, system_size],
    )?;
    Ok(())
}

pub fn system_sizes(connection: &Connection, coupon_id: i64) -> rusqlite::Result<Vec<i64>> {
    let mut statement = connection.prepare(
        "SELECT system_size FROM coupon_system_sizes WHERE coupon_id = ?1 ORDER BY system_size",
    )?;
    let sizes = statement
        .query_map([coupon_id], |row| row.get(0))?
        .collect();
    sizes
}

pub fn settle(
    connection: &Connection,
    coupon_id: i64,
    status: &str,
    settled_return: f64,
    settled_profit: f64,
    settled_at: &str,
) -> rusqlite::Result<bool> {
    let changed = connection.execute(
        "UPDATE coupons
         SET status = ?2, settled_return = ?3, settled_profit = ?4, settled_at = ?5
         WHERE id = ?1 AND status = 'pending'",
        params![
            coupon_id,
            status,
            settled_return,
            settled_profit,
            settled_at
        ],
    )?;
    Ok(changed == 1)
}

pub fn settle_selection(
    connection: &Connection,
    selection_id: i64,
    status: &str,
    settled_at: &str,
) -> rusqlite::Result<bool> {
    let changed = connection.execute(
        "UPDATE coupon_selections
         SET status = ?2, settled_at = ?3
         WHERE id = ?1 AND status = 'pending'",
        params![selection_id, status, settled_at],
    )?;
    Ok(changed == 1)
}
