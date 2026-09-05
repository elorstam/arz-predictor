//! KUPON PERFORMANSI: read-only realized coupon economics.
use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use chrono_tz::Europe::Istanbul;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::{coupon_engine::SettlementResult, model_performance::PerformanceWindow};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinancialMode {
    Realized,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CouponPerformanceRequest {
    #[serde(default)]
    pub window: PerformanceWindow,
    pub coupon_type: Option<String>,
    pub as_of: Option<String>,
    pub include_system: Option<bool>,
    pub include_katlama: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CouponFinancialSummary {
    pub coupon_count: usize,
    pub settled_count: usize,
    pub won_count: usize,
    pub lost_count: usize,
    pub void_count: usize,
    /// WIN / (WIN + LOSS). VOID and unsettled coupons are excluded.
    pub hit_rate: Option<f64>,
    pub total_stake_cents: i64,
    pub total_gross_return_cents: i64,
    pub total_net_pnl_cents: i64,
    /// Portfolio ROI: total net P&L / total settled stake.
    pub roi: Option<f64>,
    pub average_stake_cents: Option<f64>,
    pub average_combined_odds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CouponTypeSummary {
    pub coupon_type: String,
    #[serde(flatten)]
    pub metrics: CouponFinancialSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CouponOddsBandSummary {
    pub odds_band: String,
    #[serde(flatten)]
    pub metrics: CouponFinancialSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemSummary {
    pub system_coupon_count: usize,
    pub settled_system_coupon_count: usize,
    pub total_columns: usize,
    pub winning_columns: usize,
    pub losing_columns: usize,
    pub void_columns: usize,
    pub total_stake_cents: i64,
    pub total_gross_return_cents: i64,
    pub net_pnl_cents: i64,
    pub roi: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KatlamaStepSummary {
    pub step: usize,
    pub attempts: usize,
    pub wins: usize,
    pub losses: usize,
    pub voids: usize,
    pub hit_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KatlamaSummary {
    /// Logical attempts; every persisted Step 1 starts a new sequence after a reset.
    pub sequences_started: usize,
    pub active_sequences: usize,
    pub failed_sequences: usize,
    pub completed_sequences: usize,
    pub average_reached_step: Option<f64>,
    pub highest_reached_step: Option<usize>,
    /// Only Step 1 stakes are external capital; rolled winnings are not counted again.
    pub total_initial_stake_cents: i64,
    /// Initial capital belonging only to completed or failed sequences.
    pub realized_initial_stake_cents: i64,
    /// Only terminal Step 7 winnings are realized sequence returns.
    pub total_realized_return_cents: i64,
    pub net_pnl_cents: i64,
    pub roi: Option<f64>,
    pub by_step: Vec<KatlamaStepSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CouponPerformanceResponse {
    pub window: PerformanceWindow,
    pub from_date: Option<String>,
    pub to_date: String,
    pub financial_mode: FinancialMode,
    pub overall: CouponFinancialSummary,
    pub by_coupon_type: Vec<CouponTypeSummary>,
    pub by_odds_band: Vec<CouponOddsBandSummary>,
    pub system_summary: Option<SystemSummary>,
    pub katlama_summary: Option<KatlamaSummary>,
}

#[derive(Clone)]
struct CouponRow {
    coupon_type: String,
    date: NaiveDate,
    status: String,
    combined_odds: Option<f64>,
    settlement: Option<SettlementResult>,
}

fn as_of(value: Option<&str>) -> Result<NaiveDate, String> {
    match value {
        Some(x) if x.len() == 10 => {
            NaiveDate::parse_from_str(x, "%Y-%m-%d").map_err(|_| "INVALID_AS_OF".into())
        }
        Some(x) => DateTime::parse_from_rfc3339(x)
            .map(|x| x.with_timezone(&Istanbul).date_naive())
            .map_err(|_| "INVALID_AS_OF".into()),
        None => Ok(Utc::now().with_timezone(&Istanbul).date_naive()),
    }
}

fn bounds(window: PerformanceWindow, to: NaiveDate) -> (Option<NaiveDate>, NaiveDate) {
    let from = match window {
        PerformanceWindow::Last7Days => Some(to - Duration::days(6)),
        PerformanceWindow::Last30Days => Some(to - Duration::days(29)),
        PerformanceWindow::Season => Some(
            NaiveDate::from_ymd_opt(
                if to.month() >= 7 {
                    to.year()
                } else {
                    to.year() - 1
                },
                7,
                1,
            )
            .expect("valid season boundary"),
        ),
        PerformanceWindow::AllTime => None,
    };
    (from, to)
}

fn settlement(metadata: &str) -> Option<SettlementResult> {
    serde_json::from_str::<serde_json::Value>(metadata)
        .ok()?
        .get("settlement")
        .cloned()
        .and_then(|x| serde_json::from_value(x).ok())
}

fn summarize(rows: &[CouponRow]) -> CouponFinancialSummary {
    let settled = rows
        .iter()
        .filter(|x| x.status == "SETTLED")
        .collect::<Vec<_>>();
    let results = settled
        .iter()
        .filter_map(|x| x.settlement.as_ref())
        .collect::<Vec<_>>();
    // System coupon outcome follows its persisted column settlement. A partial
    // winner is a win; zero wins with losing columns is a loss.
    let outcomes = settled
        .iter()
        .filter_map(|row| {
            row.settlement.as_ref().map(|result| {
                if row.coupon_type == "DAILY_SURPRISE_SYSTEM" {
                    if result.winning_columns > 0 {
                        "WON"
                    } else if result.losing_columns > 0 {
                        "LOST"
                    } else {
                        "VOID"
                    }
                } else {
                    result.status.as_str()
                }
            })
        })
        .collect::<Vec<_>>();
    let won = outcomes.iter().filter(|x| **x == "WON").count();
    let lost = outcomes.iter().filter(|x| **x == "LOST").count();
    let voids = outcomes.iter().filter(|x| **x == "VOID").count();
    let stake: i64 = results.iter().map(|x| x.total_stake_cents).sum();
    let gross: i64 = results.iter().map(|x| x.gross_return_cents).sum();
    let odds = settled
        .iter()
        .filter(|x| x.settlement.is_some())
        .filter_map(|x| x.combined_odds)
        .collect::<Vec<_>>();
    CouponFinancialSummary {
        coupon_count: rows.len(),
        settled_count: results.len(),
        won_count: won,
        lost_count: lost,
        void_count: voids,
        hit_rate: (won + lost > 0).then_some(won as f64 / (won + lost) as f64),
        total_stake_cents: stake,
        total_gross_return_cents: gross,
        total_net_pnl_cents: gross - stake,
        roi: (stake > 0).then_some((gross - stake) as f64 / stake as f64),
        average_stake_cents: (!results.is_empty()).then_some(stake as f64 / results.len() as f64),
        average_combined_odds: (!odds.is_empty())
            .then_some(odds.iter().sum::<f64>() / odds.len() as f64),
    }
}

fn odds_band(odds: f64) -> &'static str {
    if odds < 1.5 {
        "1.00-1.49"
    } else if odds < 2.0 {
        "1.50-1.99"
    } else if odds < 3.0 {
        "2.00-2.99"
    } else if odds < 5.0 {
        "3.00-4.99"
    } else if odds < 10.0 {
        "5.00-9.99"
    } else {
        "10.00+"
    }
}

fn system_summary(rows: &[CouponRow]) -> SystemSummary {
    let systems = rows
        .iter()
        .filter(|x| x.coupon_type == "DAILY_SURPRISE_SYSTEM")
        .collect::<Vec<_>>();
    let results = systems
        .iter()
        .filter_map(|x| x.settlement.as_ref())
        .collect::<Vec<_>>();
    let stake = results.iter().map(|x| x.total_stake_cents).sum::<i64>();
    let gross = results.iter().map(|x| x.gross_return_cents).sum::<i64>();
    SystemSummary {
        system_coupon_count: systems.len(),
        settled_system_coupon_count: results.len(),
        total_columns: results.iter().map(|x| x.total_columns).sum(),
        winning_columns: results.iter().map(|x| x.winning_columns).sum(),
        losing_columns: results.iter().map(|x| x.losing_columns).sum(),
        void_columns: results.iter().map(|x| x.void_columns).sum(),
        total_stake_cents: stake,
        total_gross_return_cents: gross,
        net_pnl_cents: gross - stake,
        roi: (stake > 0).then_some((gross - stake) as f64 / stake as f64),
    }
}

#[derive(Clone)]
struct Step {
    series_id: i64,
    step: usize,
    date: NaiveDate,
    stake: i64,
    result: String,
    gross: i64,
}

fn katlama(
    c: &Connection,
    from: Option<NaiveDate>,
    to: NaiveDate,
) -> Result<KatlamaSummary, String> {
    let mut q = c.prepare("SELECT s.series_id,s.step_number,s.business_date,s.stake_cents,s.result,COALESCE(json_extract(p.metadata_json,'$.settlement.gross_return_cents'),0) FROM phase8_series_steps s JOIN phase8_coupons p ON p.id=s.coupon_id JOIN phase8_compound_series cs ON cs.id=s.series_id WHERE cs.status<>'CANCELLED' ORDER BY s.series_id,s.id").map_err(|e|e.to_string())?;
    let steps = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(series_id, step, date, stake, result, gross)| {
            NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                .ok()
                .map(|date| Step {
                    series_id,
                    step: step as usize,
                    date,
                    stake,
                    result,
                    gross,
                })
        })
        .filter(|x| x.date <= to)
        .collect::<Vec<_>>();
    let mut cycles: Vec<Vec<Step>> = Vec::new();
    for step in steps {
        if step.step == 1
            || cycles
                .last()
                .is_none_or(|x| x.last().is_some_and(|s| s.series_id != step.series_id))
        {
            cycles.push(Vec::new());
        }
        cycles.last_mut().unwrap().push(step);
    }
    cycles.retain(|cycle| {
        cycle
            .first()
            .is_some_and(|step| from.is_none_or(|from| step.date >= from))
    });
    let started = cycles.len();
    let mut active = 0;
    let mut failed = 0;
    let mut completed = 0;
    let mut initial = 0;
    let mut realized_initial = 0;
    let mut realized = 0;
    let mut reached = Vec::new();
    for cycle in &cycles {
        initial += cycle.first().map_or(0, |x| x.stake);
        let max = cycle.iter().map(|x| x.step).max().unwrap_or(0);
        reached.push(max);
        if let Some(last) = cycle.last() {
            if last.result == "LOST" {
                failed += 1;
                realized_initial += cycle[0].stake;
            } else if last.step == 7 && last.result == "WON" {
                completed += 1;
                realized += last.gross;
                realized_initial += cycle[0].stake;
            } else {
                active += 1;
            }
        }
    }
    let by_step = (1..=7)
        .map(|number| {
            let xs = cycles
                .iter()
                .flatten()
                .filter(|x| x.step == number)
                .collect::<Vec<_>>();
            let wins = xs.iter().filter(|x| x.result == "WON").count();
            let losses = xs.iter().filter(|x| x.result == "LOST").count();
            let voids = xs.iter().filter(|x| x.result == "VOID").count();
            KatlamaStepSummary {
                step: number,
                attempts: xs.len(),
                wins,
                losses,
                voids,
                hit_rate: (wins + losses > 0).then_some(wins as f64 / (wins + losses) as f64),
            }
        })
        .collect();
    Ok(KatlamaSummary {
        sequences_started: started,
        active_sequences: active,
        failed_sequences: failed,
        completed_sequences: completed,
        average_reached_step: (!reached.is_empty())
            .then_some(reached.iter().sum::<usize>() as f64 / reached.len() as f64),
        highest_reached_step: reached.into_iter().max(),
        total_initial_stake_cents: initial,
        realized_initial_stake_cents: realized_initial,
        total_realized_return_cents: realized,
        net_pnl_cents: realized - realized_initial,
        roi: (realized_initial > 0)
            .then_some((realized - realized_initial) as f64 / realized_initial as f64),
        by_step,
    })
}

pub fn get(
    c: &Connection,
    request: &CouponPerformanceRequest,
) -> Result<CouponPerformanceResponse, String> {
    let to = as_of(request.as_of.as_deref())?;
    let (from, to) = bounds(request.window, to);
    let include_system = request.include_system.unwrap_or(true);
    let include_katlama = request.include_katlama.unwrap_or(true);
    let mut q=c.prepare("SELECT coupon_type,business_date,status,combined_decimal_odd,metadata_json FROM phase8_coupons WHERE status NOT IN ('DRAFT','CANCELLED') ORDER BY business_date,id").map_err(|e|e.to_string())?;
    let mut rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<f64>>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(kind, date, status, odds, meta)| {
            NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                .ok()
                .map(|date| CouponRow {
                    coupon_type: kind,
                    date,
                    status,
                    combined_odds: odds,
                    settlement: settlement(&meta),
                })
        })
        .filter(|x| x.date <= to && from.is_none_or(|f| x.date >= f))
        .filter(|x| {
            request
                .coupon_type
                .as_deref()
                .is_none_or(|k| x.coupon_type == k)
        })
        .collect::<Vec<_>>();
    // Compound coupons are represented only at sequence level, preventing rolled stakes from entering portfolio stake twice.
    rows.retain(|x| {
        x.coupon_type != "DAILY_COMPOUND"
            && (include_system || x.coupon_type != "DAILY_SURPRISE_SYSTEM")
    });
    let mut types: BTreeMap<String, Vec<CouponRow>> = BTreeMap::new();
    let mut bands: BTreeMap<String, Vec<CouponRow>> = BTreeMap::new();
    for row in &rows {
        types
            .entry(row.coupon_type.clone())
            .or_default()
            .push(row.clone());
        if let Some(o) = row.combined_odds {
            bands
                .entry(odds_band(o).into())
                .or_default()
                .push(row.clone());
        }
    }
    let wants_system = include_system
        && request
            .coupon_type
            .as_deref()
            .is_none_or(|x| x == "DAILY_SURPRISE_SYSTEM");
    let wants_katlama = include_katlama
        && request
            .coupon_type
            .as_deref()
            .is_none_or(|x| x == "DAILY_COMPOUND");
    let system = wants_system.then(|| system_summary(&rows));
    Ok(CouponPerformanceResponse {
        window: request.window,
        from_date: from.map(|x| x.to_string()),
        to_date: to.to_string(),
        financial_mode: FinancialMode::Realized,
        overall: summarize(&rows),
        by_coupon_type: types
            .into_iter()
            .map(|(coupon_type, rows)| CouponTypeSummary {
                coupon_type,
                metrics: summarize(&rows),
            })
            .collect(),
        by_odds_band: {
            let mut values = bands
                .into_iter()
                .map(|(odds_band, rows)| CouponOddsBandSummary {
                    odds_band,
                    metrics: summarize(&rows),
                })
                .collect::<Vec<_>>();
            values.sort_by_key(|x| match x.odds_band.as_str() {
                "1.00-1.49" => 0,
                "1.50-1.99" => 1,
                "2.00-2.99" => 2,
                "3.00-4.99" => 3,
                "5.00-9.99" => 4,
                _ => 5,
            });
            values
        },
        system_summary: system,
        katlama_summary: if wants_katlama {
            Some(katlama(c, from, to)?)
        } else {
            None
        },
    })
}
