//! Settles immutable production selections using confirmed final result data.
//! Bulletin removal/cancelled status alone is never evidence for a refund.
use super::coupon_engine;
use rusqlite::{params, Connection, TransactionBehavior};

pub fn outcome(
    market: &str,
    selection: &str,
    line: Option<f64>,
    home: i64,
    away: i64,
    corners: Option<i64>,
) -> Result<&'static str, &'static str> {
    let win = match market {
        "MATCH_RESULT" => match selection {
            "HOME" => home > away,
            "DRAW" => home == away,
            "AWAY" => away > home,
            _ => return Err("UNSUPPORTED_SELECTION"),
        },
        "BTTS" | "BOTH_TEAMS_TO_SCORE" => match selection {
            "YES" => home > 0 && away > 0,
            "NO" => home == 0 || away == 0,
            _ => return Err("UNSUPPORTED_SELECTION"),
        },
        "TOTAL_GOALS" | "FULL_TIME_TOTAL_CORNERS" | "CORNERS_TOTAL" => {
            let total = if market == "TOTAL_GOALS" {
                home + away
            } else {
                corners.ok_or("FINAL_CORNERS_UNAVAILABLE")?
            } as f64;
            let line = line
                .filter(|x| x.is_finite())
                .ok_or("SELECTION_LINE_MISSING")?;
            if !matches!(selection, "OVER" | "UNDER") {
                return Err("UNSUPPORTED_SELECTION");
            }
            // Quarter-line Asian markets require split-stake settlement, not a binary result.
            if (line * 2.0).fract() != 0.0 {
                return Err("UNSUPPORTED_ASIAN_LINE");
            }
            if total == line {
                return Ok("VOID");
            }
            if selection == "OVER" {
                total > line
            } else {
                total < line
            }
        }
        _ => return Err("UNSUPPORTED_MARKET"),
    };
    Ok(if win { "WON" } else { "LOST" })
}

pub fn run(c: &Connection) -> Result<usize, String> {
    let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let changed = process(&tx)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(changed)
}

fn process(c: &Connection) -> Result<usize, String> {
    // The active series must never sit behind the historical audit backlog.
    let mut q=c.prepare("SELECT s.id,s.coupon_id,s.match_id,s.market,s.selection,s.line_value FROM phase8_coupon_selections s JOIN phase8_coupons p ON p.id=s.coupon_id LEFT JOIN coupon_selection_settlement_audit a ON a.selection_id=s.id WHERE s.status='PENDING' AND p.status<>'CANCELLED' AND NOT EXISTS(SELECT 1 FROM candidate_run_execution e WHERE e.run_id=p.source_candidate_run_id AND e.purpose='REPLAY') AND (p.status IN ('FINALIZED','SETTLED') OR json_extract(p.metadata_json,'$.published')=1 OR p.series_id IS NOT NULL) ORDER BY EXISTS(SELECT 1 FROM phase8_compound_series cs WHERE cs.latest_coupon_id=p.id AND cs.status='ACTIVE') DESC,COALESCE(a.checked_at,''),s.id LIMIT 256").map_err(|e|e.to_string())?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<f64>>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut changed = 0;
    for (id, _, match_id, market, selection, line) in rows {
        let mut result_query=c.prepare("SELECT r.id,r.final_home_goals,r.final_away_goals,CASE WHEN st.home_corners IS NOT NULL AND st.away_corners IS NOT NULL THEN st.home_corners+st.away_corners END FROM matches m JOIN matches r ON r.competition_id=m.competition_id AND r.home_team_id=m.home_team_id AND r.away_team_id=m.away_team_id AND r.scheduled_local_date=m.scheduled_local_date LEFT JOIN match_statistics st ON st.match_id=r.id WHERE m.id=?1 AND r.status='finished' AND r.final_home_goals IS NOT NULL AND r.final_away_goals IS NOT NULL ORDER BY (r.id=m.id) DESC LIMIT 2").map_err(|e|e.to_string())?;
        let results = result_query
            .query_map([match_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let source = results
            .first()
            .filter(|r| r.0 == match_id || results.len() == 1);
        let future: bool = c
            .query_row(
                "SELECT julianday(kickoff_at)>julianday('now') FROM matches WHERE id=?1",
                [match_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let decision = match source {
            Some((_, h, a, corners)) => outcome(&market, &selection, line, *h, *a, *corners),
            None => Err(if results.len() > 1 {
                "AMBIGUOUS_FINAL_RESULT"
            } else if future {
                "AWAITING_MATCH_COMPLETION"
            } else {
                "FINAL_RESULT_UNAVAILABLE"
            }),
        };
        let (state, reason) = match decision {
            Ok(state) => {
                changed+=c.execute("UPDATE phase8_coupon_selections SET status=?2 WHERE id=?1 AND status='PENDING'",params![id,state]).map_err(|e|e.to_string())?;
                (state, None)
            }
            Err(reason) => (
                if reason == "AWAITING_MATCH_COMPLETION" {
                    "PENDING"
                } else {
                    "PENDING_DATA"
                },
                Some(reason),
            ),
        };
        c.execute("INSERT INTO coupon_selection_settlement_audit(selection_id,state,reason,result_match_id,checked_at) VALUES(?1,?2,?3,?4,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(selection_id) DO UPDATE SET state=excluded.state,reason=excluded.reason,result_match_id=excluded.result_match_id,checked_at=excluded.checked_at",params![id,state,reason,source.map(|r|r.0)]).map_err(|e|e.to_string())?;
    }
    let mut q=c.prepare("SELECT id,total_stake_cents FROM phase8_coupons p WHERE status IN ('DRAFT','FINALIZED') AND NOT EXISTS(SELECT 1 FROM candidate_run_execution e WHERE e.run_id=p.source_candidate_run_id AND e.purpose='REPLAY') AND (total_stake_cents IS NOT NULL OR json_extract(metadata_json,'$.settlement_pending_reason') IS NULL) AND (json_extract(metadata_json,'$.published')=1 OR series_id IS NOT NULL OR status='FINALIZED') AND EXISTS(SELECT 1 FROM phase8_coupon_selections s WHERE s.coupon_id=p.id) AND NOT EXISTS(SELECT 1 FROM phase8_coupon_selections s WHERE s.coupon_id=p.id AND s.status='PENDING') ORDER BY EXISTS(SELECT 1 FROM phase8_compound_series cs WHERE cs.latest_coupon_id=p.id AND cs.status='ACTIVE') DESC,id LIMIT 64").map_err(|e|e.to_string())?;
    let coupons = q
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (id, stake) in coupons {
        c.execute(
            "UPDATE phase8_coupons SET status='FINALIZED' WHERE id=?1 AND status='DRAFT'",
            [id],
        )
        .map_err(|e| e.to_string())?;
        coupon_engine::settle(
            c,
            &coupon_engine::SettlementRequest {
                coupon_id: id,
                outcomes: vec![],
                settled_at: chrono::Utc::now().to_rfc3339(),
            },
        )?;
        if stake.is_none() {
            // Preserve the actual WON/LOST/VOID outcome, but no invented monetary
            // settlement for a suggestion without a recorded stake. The enclosing
            // transaction never exposes the router's zero-stake placeholder.
            c.execute("UPDATE phase8_coupons SET metadata_json=json_set(json_remove(metadata_json,'$.settlement'),'$.financial_state','STAKE_NOT_RECORDED') WHERE id=?1",[id]).map_err(|e|e.to_string())?;
        }
        changed += 1;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supported_results_and_missing_corners() {
        for (m, s, l, h, a, c, result) in [
            ("MATCH_RESULT", "HOME", None, 2, 1, None, Ok("WON")),
            ("MATCH_RESULT", "DRAW", None, 1, 1, None, Ok("WON")),
            ("MATCH_RESULT", "AWAY", None, 2, 1, None, Ok("LOST")),
            ("TOTAL_GOALS", "OVER", Some(2.5), 2, 1, None, Ok("WON")),
            ("TOTAL_GOALS", "OVER", Some(3.5), 2, 1, None, Ok("LOST")),
            ("TOTAL_GOALS", "OVER", Some(3.5), 2, 2, None, Ok("WON")),
            ("BTTS", "YES", None, 2, 1, None, Ok("WON")),
            ("BTTS", "YES", None, 2, 0, None, Ok("LOST")),
            (
                "FULL_TIME_TOTAL_CORNERS",
                "OVER",
                Some(8.5),
                2,
                1,
                None,
                Err("FINAL_CORNERS_UNAVAILABLE"),
            ),
            (
                "FULL_TIME_TOTAL_CORNERS",
                "UNDER",
                Some(10.5),
                2,
                1,
                Some(9),
                Ok("WON"),
            ),
            (
                "FULL_TIME_TOTAL_CORNERS",
                "OVER",
                Some(9.0),
                2,
                1,
                Some(9),
                Ok("VOID"),
            ),
        ] {
            assert_eq!(outcome(m, s, l, h, a, c), result);
        }
    }
}
