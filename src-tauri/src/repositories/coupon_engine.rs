//! Phase 8 final coupon assembly. This module consumes only immutable Phase 7 snapshots.
use super::candidate_engine::{self, Candidate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const COUPON_POLICY_VERSION: &str = "coupon_policy_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CouponPolicy {
    pub version: String,
    pub high_confidence_target_odd: f64,
    pub compound_target_odd: f64,
    pub compound_odd_range: [f64; 2],
    pub compound_max_legs: usize,
    pub katlama_max_step: usize,
}
pub fn policy() -> CouponPolicy {
    CouponPolicy {
        version: COUPON_POLICY_VERSION.into(),
        high_confidence_target_odd: 3.0,
        compound_target_odd: 1.70,
        compound_odd_range: [1.50, 1.80],
        compound_max_legs: 3,
        katlama_max_step: 7,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Coupon {
    pub id: i64,
    pub coupon_type: String,
    pub business_date: String,
    pub source_candidate_run_id: i64,
    pub policy_version: String,
    pub model_version: String,
    pub calibration_version: Option<String>,
    pub generation_cutoff: String,
    pub series_id: Option<i64>,
    pub step_number: Option<usize>,
    pub status: String,
    pub candidate_ids: Vec<i64>,
    pub selections: Vec<CouponSelectionSnapshot>,
    pub system_sizes: Vec<usize>,
    pub columns: Vec<SystemColumn>,
    pub combined_decimal_odd: Option<f64>,
    pub target_odds_reached: bool,
    pub unit_stake_cents: Option<i64>,
    pub total_stake_cents: Option<i64>,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DailyCouponResult {
    pub coupon_type: String,
    pub display_name: String,
    pub business_date: String,
    pub generation_status: String,
    pub unavailable_reason: Option<String>,
    pub coupon_id: Option<i64>,
    pub status: Option<String>,
    pub source_candidate_run_id: Option<i64>,
    pub model_version: Option<String>,
    pub calibration_version: Option<String>,
    pub coupon_policy_version: String,
    pub selection_count: usize,
    pub selections: Vec<CouponSelectionSnapshot>,
    pub combined_odd: Option<f64>,
    pub stake_cents: Option<i64>,
    pub potential_gross_return_cents: Option<i64>,
    pub target_odds_reached: Option<bool>,
    pub system_sizes: Vec<usize>,
    pub column_count: usize,
    pub unit_stake_cents: Option<i64>,
    pub total_stake_cents: Option<i64>,
    pub max_theoretical_gross_return_cents: Option<i64>,
    pub series_id: Option<i64>,
    pub series_status: Option<String>,
    pub current_step: Option<usize>,
    pub starting_stake_cents: Option<i64>,
    pub current_stake_cents: Option<i64>,
}

fn display_name(kind: &str) -> &'static str {
    match kind {
        "DAILY_CORNERS" => "Günün Korner Kuponu",
        "DAILY_OVER_25" => "Günün 2.5 Üst Kuponu",
        "DAILY_OVER_35" => "Günün 3.5 Üst Kuponu",
        "DAILY_BTTS" => "Günün KG Var Kuponu",
        "DAILY_HIGH_CONFIDENCE" => "Günün Yüksek Güven Kuponu",
        "DAILY_SURPRISE_SYSTEM" => "Günün Sürpriz Kuponu",
        "DAILY_COMPOUND" => "Günün Katlama Kuponu",
        _ => "Günün Kuponu",
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CouponSelectionSnapshot {
    pub candidate_id: i64,
    pub match_id: i64,
    pub competition_id: i64,
    pub market: String,
    pub selection: String,
    pub line: Option<f64>,
    pub raw_probability: f64,
    pub public_probability: f64,
    pub calibration_status: String,
    pub calibration_version: Option<String>,
    pub odd: f64,
    pub odds_snapshot_id: Option<i64>,
    pub odds_captured_at: String,
    pub edge: f64,
    pub expected_value: f64,
    pub score: f64,
    pub rank: i64,
    pub correlation_type: String,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemColumn {
    pub column_number: usize,
    pub system_size: usize,
    pub candidate_ids: Vec<i64>,
    pub column_decimal_odd: f64,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenerateRequest {
    pub business_date: String,
    pub candidate_run_id: Option<i64>,
    pub coupon_type: Option<String>,
    pub unit_stake_cents: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DraftRequest {
    pub business_date: String,
    pub candidate_run_id: i64,
    pub coupon_type: String,
    pub candidate_ids: Vec<i64>,
    pub unit_stake_cents: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateRequest {
    pub coupon_id: i64,
    pub candidate_ids: Vec<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemPreviewRequest {
    pub candidate_ids: Vec<i64>,
    pub system_sizes: Vec<usize>,
    pub unit_stake_cents: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemPreview {
    pub selection_count: usize,
    pub system_sizes: Vec<usize>,
    pub columns: Vec<SystemColumn>,
    pub total_stake_cents: i64,
    pub max_theoretical_return_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineupRevisionImpactRequest {
    pub coupon_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineupRevisionImpactItem {
    pub coupon_selection_id: i64,
    pub original_candidate_id: i64,
    pub match_id: i64,
    pub market: String,
    pub selection: String,
    pub line: Option<f64>,
    pub base_probability: f64,
    pub latest_probability: f64,
    pub delta_percentage_points: f64,
    pub old_qualification_status: String,
    pub new_qualification_status: String,
    pub impact_type: String,
    pub lineup_revision_id: Option<i64>,
    pub lineup_snapshot_id: Option<i64>,
    pub family_status: Option<String>,
    pub fallback_reason: Option<String>,
    pub prediction_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineupRevisionImpact {
    pub revision_available: bool,
    pub base_candidate_run_id: i64,
    pub latest_lineup_candidate_run_id: Option<i64>,
    pub affected_selection_count: usize,
    pub items: Vec<LineupRevisionImpactItem>,
}

fn category(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "DAILY_CORNERS" => "CORNERS",
        "DAILY_OVER_25" => "OVER_25",
        "DAILY_OVER_35" => "OVER_35",
        "DAILY_BTTS" => "BTTS_YES",
        "DAILY_HIGH_CONFIDENCE" => "HIGH_CONFIDENCE",
        "DAILY_SURPRISE_SYSTEM" => "SURPRISE",
        "DAILY_COMPOUND" => "COMPOUND",
        _ => return None,
    })
}
fn all_types() -> [&'static str; 7] {
    [
        "DAILY_CORNERS",
        "DAILY_OVER_25",
        "DAILY_OVER_35",
        "DAILY_BTTS",
        "DAILY_HIGH_CONFIDENCE",
        "DAILY_SURPRISE_SYSTEM",
        "DAILY_COMPOUND",
    ]
}
fn combinations(ids: &[i64], size: usize) -> Vec<Vec<i64>> {
    fn go(ids: &[i64], start: usize, n: usize, cur: &mut Vec<i64>, out: &mut Vec<Vec<i64>>) {
        if cur.len() == n {
            out.push(cur.clone());
            return;
        }
        for i in start..ids.len() {
            cur.push(ids[i]);
            go(ids, i + 1, n, cur, out);
            cur.pop();
        }
    }
    let mut out = Vec::new();
    go(ids, 0, size, &mut Vec::new(), &mut out);
    out
}
fn candidate_by_id<'a>(cs: &'a [Candidate], id: i64) -> Option<&'a Candidate> {
    cs.iter().find(|x| x.id == id)
}
fn safe(candidates: &[Candidate]) -> Vec<Candidate> {
    let mut out = Vec::new();
    for c in candidates {
        if out.iter().any(|x: &Candidate| x.match_id == c.match_id) {
            continue;
        }
        if c.correlation_type != "NONE" && out.iter().any(|x: &Candidate| x.match_id == c.match_id)
        {
            continue;
        }
        out.push(c.clone());
    }
    out
}
fn combined(cs: &[Candidate]) -> f64 {
    cs.iter().fold(1.0, |a, c| a * c.iddaa_odd)
}

fn validate_selection_set(kind: &str, selected: &[Candidate]) -> Result<Vec<usize>, String> {
    if selected.is_empty() {
        return Err("EMPTY_SELECTION_SET".into());
    }
    let mut matches = std::collections::BTreeSet::new();
    for x in selected {
        if !matches.insert(x.match_id) {
            let other = selected
                .iter()
                .find(|y| y.id != x.id && y.match_id == x.match_id)
                .unwrap();
            return Err(if x.market == "TOTAL_GOALS"
                && other.market == "TOTAL_GOALS"
                && x.line != other.line
            {
                "NESTED_TOTAL_CONFLICT"
            } else if (x.market == "BTTS" && other.market == "TOTAL_GOALS")
                || (x.market == "TOTAL_GOALS" && other.market == "BTTS")
            {
                "BTTS_GOALS_CONFLICT"
            } else if x.market == "FULL_TIME_TOTAL_CORNERS"
                && other.market == x.market
                && x.line != other.line
            {
                "NESTED_CORNERS_CONFLICT"
            } else {
                "SAME_MATCH_CONFLICT"
            }
            .into());
        }
    }
    if kind == "DAILY_SURPRISE_SYSTEM" {
        return match selected.len() {
            6 => Ok(vec![5, 6]),
            7 => Ok(vec![5, 6, 7]),
            _ => Err("INVALID_SYSTEM_SELECTION_COUNT".into()),
        };
    }
    if kind == "DAILY_COMPOUND" {
        if !(2..=3).contains(&selected.len()) {
            return Err("INVALID_COMPOUND_SELECTION_COUNT".into());
        }
        let odd = combined(selected);
        let p = policy();
        if odd < p.compound_odd_range[0] {
            return Err("COMPOUND_ODDS_BELOW_MINIMUM".into());
        }
        if odd > p.compound_odd_range[1] {
            return Err("COMPOUND_ODDS_ABOVE_MAXIMUM".into());
        }
    }
    Ok(vec![])
}

fn source(
    c: &Connection,
    date: &str,
    run_id: Option<i64>,
) -> Result<candidate_engine::DailyRun, String> {
    let run = candidate_engine::get(
        c,
        &candidate_engine::GetRequest {
            business_date: date.into(),
            run_id,
            category: None,
        },
    )?;
    if run.business_date != date {
        return Err("SOURCE_RUN_DATE_MISMATCH".into());
    }
    Ok(run)
}

fn insert_selections(c: &Connection, coupon_id: i64, cs: &[Candidate]) -> Result<(), String> {
    for (i, x) in cs.iter().enumerate() {
        c.execute("INSERT INTO phase8_coupon_selections(coupon_id,candidate_id,selection_order,match_id,competition_id,market,selection,line_value,raw_probability,public_probability,calibration_status,calibration_version,iddaa_odd,odds_snapshot_id,odds_captured_at,probability_edge,expected_value,score,rank,correlation_type) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",params![coupon_id,x.id,(i+1) as i64,x.match_id,x.competition_id,x.market,x.selection,x.line,x.raw_probability,x.public_probability,x.calibration_status,x.calibration_version,x.iddaa_odd,x.odds_snapshot_id,x.odds_captured_at,x.probability_edge,x.expected_value,x.score,x.rank,x.correlation_type]).map_err(|e|e.to_string())?;
    }
    Ok(())
}

fn insert_coupon(
    c: &Connection,
    run: &candidate_engine::DailyRun,
    kind: &str,
    selected: &[Candidate],
    sizes: &[usize],
    unit: Option<i64>,
    target: bool,
) -> Result<i64, String> {
    if let Some(existing) = c.query_row("SELECT id FROM phase8_coupons WHERE business_date=?1 AND source_candidate_run_id=?2 AND coupon_type=?3 AND policy_version=?4", params![run.business_date,run.run_id,kind,COUPON_POLICY_VERSION], |x| x.get(0)).optional().map_err(|e|e.to_string())? { return Ok(existing); }
    let metadata = serde_json::json!({"source_run_identity":run.run_id,"generation_cutoff":run.generated_at,"naive_independent_probability":selected.iter().fold(1.0,|a,x|a*x.public_probability)});
    let total = unit.map(|u| {
        if sizes.is_empty() {
            u
        } else {
            u * columns(selected, sizes).len() as i64
        }
    });
    let odd = (sizes.is_empty()).then(|| combined(selected));
    c.execute("INSERT INTO phase8_coupons(coupon_type,business_date,source_candidate_run_id,policy_version,model_version,calibration_version,generation_cutoff,combined_decimal_odd,target_odds_reached,system_sizes_json,unit_stake_cents,total_stake_cents,metadata_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",params![kind,run.business_date,run.run_id,COUPON_POLICY_VERSION,run.model_version,run.calibration_version,run.generated_at,odd,target as i64,(!sizes.is_empty()).then(||serde_json::to_string(sizes).unwrap()),unit,total,metadata.to_string()]).map_err(|e|e.to_string())?;
    let id = c.last_insert_rowid();
    insert_selections(c, id, selected)?;
    for col in columns(selected, sizes) {
        c.execute("INSERT INTO phase8_system_columns(coupon_id,column_number,system_size,candidate_ids_json,column_decimal_odd) VALUES(?1,?2,?3,?4,?5)",params![id,col.column_number,col.system_size,serde_json::to_string(&col.candidate_ids).unwrap(),col.column_decimal_odd]).map_err(|e|e.to_string())?;
    }
    Ok(id)
}
fn columns(cs: &[Candidate], sizes: &[usize]) -> Vec<SystemColumn> {
    let ids: Vec<i64> = cs.iter().map(|x| x.id).collect();
    let mut out = Vec::new();
    let mut n = 1;
    for &s in sizes {
        for set in combinations(&ids, s) {
            let picks: Vec<Candidate> = set
                .iter()
                .filter_map(|id| candidate_by_id(cs, *id).cloned())
                .collect();
            out.push(SystemColumn {
                column_number: n,
                system_size: s,
                candidate_ids: set,
                column_decimal_odd: combined(&picks),
                status: "PENDING".into(),
            });
            n += 1;
        }
    }
    out
}

pub fn create_draft(c: &Connection, r: &DraftRequest) -> Result<Coupon, String> {
    let run = source(c, &r.business_date, Some(r.candidate_run_id))?;
    let cat = category(&r.coupon_type).ok_or("INVALID_COUPON_TYPE")?;
    let pool: Vec<Candidate> = run
        .candidates
        .iter()
        .filter(|x| x.category == cat)
        .cloned()
        .collect();
    let selected: Vec<Candidate> = r
        .candidate_ids
        .iter()
        .filter_map(|id| candidate_by_id(&pool, *id).cloned())
        .collect();
    if selected.len() != r.candidate_ids.len() {
        return Err("CANDIDATE_NOT_FOUND_OR_NOT_QUALIFIED".into());
    }
    let sizes = validate_selection_set(&r.coupon_type, &selected)?;
    let id = insert_coupon(
        c,
        &run,
        &r.coupon_type,
        &selected,
        &sizes,
        r.unit_stake_cents,
        false,
    )?;
    get_coupon(c, id)
}

pub fn generate_daily(c: &Connection, r: &GenerateRequest) -> Result<Vec<Coupon>, String> {
    let run = source(c, &r.business_date, r.candidate_run_id)?;
    let kinds: Vec<&str> = r
        .coupon_type
        .as_deref()
        .map(|x| vec![x])
        .unwrap_or_else(|| all_types().to_vec());
    let mut out = Vec::new();
    for kind in kinds {
        let Some(cat) = category(kind) else { continue };
        let mut pool: Vec<Candidate> = run
            .candidates
            .iter()
            .filter(|x| x.category == cat)
            .cloned()
            .collect();
        if kind == "DAILY_SURPRISE_SYSTEM" {
            pool = safe(&pool);
            if pool.len() < 6 {
                continue;
            }
            pool.truncate(7);
        } else if kind == "DAILY_COMPOUND" {
            let pol = policy();
            let mut best = None;
            for n in 2..=pol.compound_max_legs {
                for ids in combinations(&pool.iter().map(|x| x.id).collect::<Vec<_>>(), n) {
                    let picks: Vec<Candidate> = ids
                        .iter()
                        .filter_map(|id| candidate_by_id(&pool, *id).cloned())
                        .collect();
                    let odd = combined(&picks);
                    if odd >= pol.compound_odd_range[0]
                        && odd <= pol.compound_odd_range[1]
                        && best
                            .as_ref()
                            .map(|b: &(f64, Vec<Candidate>)| {
                                picks.iter().map(|x| x.score).sum::<f64>() > b.0
                            })
                            .unwrap_or(true)
                    {
                        best = Some((picks.iter().map(|x| x.score).sum(), picks));
                    }
                }
            }
            pool = best.map(|x| x.1).unwrap_or_default();
            if pool.is_empty() {
                continue;
            }
        } else {
            pool = safe(&pool);
        }
        let target = kind == "DAILY_HIGH_CONFIDENCE"
            && combined(&pool) >= policy().high_confidence_target_odd;
        let sizes = if kind == "DAILY_SURPRISE_SYSTEM" {
            vec![5, 6, 7]
        } else {
            vec![]
        };
        let id = insert_coupon(c, &run, kind, &pool, &sizes, r.unit_stake_cents, target)?;
        out.push(get_coupon(c, id)?);
    }
    Ok(out)
}

pub fn generate_daily_report(
    c: &Connection,
    r: &GenerateRequest,
) -> Result<Vec<DailyCouponResult>, String> {
    let kinds: Vec<String> = r
        .coupon_type
        .as_deref()
        .map(|x| vec![x.to_string()])
        .unwrap_or_else(|| all_types().iter().map(|x| x.to_string()).collect());
    let run = source(c, &r.business_date, r.candidate_run_id).ok();
    let mut output = Vec::with_capacity(kinds.len());
    for kind in kinds {
        if category(&kind).is_none() {
            output.push(DailyCouponResult {
                coupon_type: kind.clone(),
                display_name: display_name(&kind).into(),
                business_date: r.business_date.clone(),
                generation_status: "UNAVAILABLE".into(),
                unavailable_reason: Some("INVALID_COUPON_TYPE".into()),
                coupon_id: None,
                status: None,
                source_candidate_run_id: None,
                model_version: None,
                calibration_version: None,
                coupon_policy_version: COUPON_POLICY_VERSION.into(),
                selection_count: 0,
                selections: vec![],
                combined_odd: None,
                stake_cents: None,
                potential_gross_return_cents: None,
                target_odds_reached: None,
                system_sizes: vec![],
                column_count: 0,
                unit_stake_cents: None,
                total_stake_cents: None,
                max_theoretical_gross_return_cents: None,
                series_id: None,
                series_status: None,
                current_step: None,
                starting_stake_cents: None,
                current_stake_cents: None,
            });
            continue;
        }
        let Some(run) = run.as_ref() else {
            output.push(unavailable_result(
                &kind,
                &r.business_date,
                "SOURCE_CANDIDATE_RUN_NOT_FOUND",
                None,
            ));
            continue;
        };
        let generated = generate_daily(
            c,
            &GenerateRequest {
                business_date: r.business_date.clone(),
                candidate_run_id: Some(run.run_id),
                coupon_type: Some(kind.clone()),
                unit_stake_cents: r.unit_stake_cents,
            },
        )?;
        if let Some(coupon) = generated.into_iter().next() {
            let max_return = if coupon.columns.is_empty() {
                coupon
                    .unit_stake_cents
                    .map(|stake| payout(stake, coupon.combined_decimal_odd.unwrap_or(1.0)))
            } else {
                Some(
                    coupon
                        .columns
                        .iter()
                        .map(|x| payout(coupon.unit_stake_cents.unwrap_or(0), x.column_decimal_odd))
                        .sum(),
                )
            };
            let (series_status, current_step, starting, current) = coupon
                .series_id
                .and_then(|id| series(c, id).ok())
                .map(|s| {
                    (
                        Some(s.status),
                        Some(s.current_step),
                        Some(s.starting_stake_cents),
                        Some(s.current_stake_cents),
                    )
                })
                .unwrap_or((None, None, None, None));
            output.push(DailyCouponResult {
                coupon_type: kind.clone(),
                display_name: display_name(&kind).into(),
                business_date: coupon.business_date.clone(),
                generation_status: "GENERATED".into(),
                unavailable_reason: None,
                coupon_id: Some(coupon.id),
                status: Some(coupon.status.clone()),
                source_candidate_run_id: Some(coupon.source_candidate_run_id),
                model_version: Some(coupon.model_version.clone()),
                calibration_version: coupon.calibration_version.clone(),
                coupon_policy_version: coupon.policy_version.clone(),
                selection_count: coupon.candidate_ids.len(),
                selections: coupon.selections.clone(),
                combined_odd: coupon.combined_decimal_odd,
                stake_cents: coupon.unit_stake_cents,
                potential_gross_return_cents: max_return,
                target_odds_reached: Some(coupon.target_odds_reached),
                system_sizes: coupon.system_sizes.clone(),
                column_count: coupon.columns.len(),
                unit_stake_cents: coupon.unit_stake_cents,
                total_stake_cents: coupon.total_stake_cents,
                max_theoretical_gross_return_cents: max_return,
                series_id: coupon.series_id,
                series_status,
                current_step,
                starting_stake_cents: starting,
                current_stake_cents: current,
            });
        } else {
            let reason = if kind == "DAILY_SURPRISE_SYSTEM" {
                "INSUFFICIENT_QUALIFIED_SURPRISE_SELECTIONS"
            } else if kind == "DAILY_COMPOUND" {
                "NO_QUALIFYING_COMPOUND_COUPON"
            } else {
                "INSUFFICIENT_QUALIFIED_CANDIDATES"
            };
            output.push(unavailable_result(
                &kind,
                &r.business_date,
                reason,
                Some(run),
            ));
        }
    }
    Ok(output)
}

fn unavailable_result(
    kind: &str,
    date: &str,
    reason: &str,
    run: Option<&candidate_engine::DailyRun>,
) -> DailyCouponResult {
    DailyCouponResult {
        coupon_type: kind.into(),
        display_name: display_name(kind).into(),
        business_date: date.into(),
        generation_status: "UNAVAILABLE".into(),
        unavailable_reason: Some(reason.into()),
        coupon_id: None,
        status: None,
        source_candidate_run_id: run.map(|x| x.run_id),
        model_version: run.map(|x| x.model_version.clone()),
        calibration_version: run.and_then(|x| x.calibration_version.clone()),
        coupon_policy_version: COUPON_POLICY_VERSION.into(),
        selection_count: 0,
        selections: vec![],
        combined_odd: None,
        stake_cents: None,
        potential_gross_return_cents: None,
        target_odds_reached: None,
        system_sizes: vec![],
        column_count: 0,
        unit_stake_cents: None,
        total_stake_cents: None,
        max_theoretical_gross_return_cents: None,
        series_id: None,
        series_status: None,
        current_step: None,
        starting_stake_cents: None,
        current_stake_cents: None,
    }
}

pub fn update_draft(c: &Connection, r: &UpdateRequest) -> Result<Coupon, String> {
    let status: String = c
        .query_row(
            "SELECT status FROM phase8_coupons WHERE id=?1",
            [r.coupon_id],
            |x| x.get(0),
        )
        .map_err(|e| e.to_string())?;
    if status != "DRAFT" {
        return Err("COUPON_NOT_DRAFT".into());
    }
    let h:(String,i64,String) = c.query_row("SELECT business_date,source_candidate_run_id,coupon_type FROM phase8_coupons WHERE id=?1",[r.coupon_id],|x|Ok((x.get(0)?,x.get(1)?,x.get(2)?))).map_err(|e|e.to_string())?;
    let run = source(c, &h.0, Some(h.1))?;
    let cat = category(&h.2).ok_or("INVALID_COUPON_TYPE")?;
    let mut selected = Vec::new();
    for id in &r.candidate_ids {
        if selected.iter().any(|x: &Candidate| x.id == *id) {
            return Err("DUPLICATE_CANDIDATE".into());
        }
        if let Some(x) = run.candidates.iter().find(|x| x.id == *id) {
            if x.category != cat {
                return Err("WRONG_CATEGORY".into());
            }
            selected.push(x.clone());
            continue;
        }
        let row: Option<(i64, String, String)> = c.query_row("SELECT run_id,category,qualification_state FROM candidate_engine_candidates WHERE id=?1", [id], |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?))).optional().map_err(|e| e.to_string())?;
        match row {
            None => return Err("CANDIDATE_NOT_FOUND".into()),
            Some((candidate_run, _candidate_category, _state)) if candidate_run != h.1 => {
                return Err("CANDIDATE_FROM_DIFFERENT_RUN".into())
            }
            Some((_candidate_run, candidate_category, _state)) if candidate_category != cat => {
                return Err("WRONG_CATEGORY".into())
            }
            Some((_candidate_run, _candidate_category, state)) if state != "QUALIFIED" => {
                return Err("CANDIDATE_NOT_QUALIFIED".into())
            }
            Some(_) => return Err("CANDIDATE_NOT_FOUND".into()),
        }
    }
    let sizes = validate_selection_set(&h.2, &selected)?;
    c.execute(
        "DELETE FROM phase8_coupon_selections WHERE coupon_id=?1",
        [r.coupon_id],
    )
    .map_err(|e| e.to_string())?;
    insert_selections(c, r.coupon_id, &selected)?;
    let odd = if sizes.is_empty() {
        Some(combined(&selected))
    } else {
        None
    };
    let total = c
        .query_row(
            "SELECT unit_stake_cents FROM phase8_coupons WHERE id=?1",
            [r.coupon_id],
            |x| x.get::<_, Option<i64>>(0),
        )
        .map_err(|e| e.to_string())?;
    let total = total.map(|u| {
        if sizes.is_empty() {
            u
        } else {
            u * columns(&selected, &sizes).len() as i64
        }
    });
    c.execute("UPDATE phase8_coupons SET combined_decimal_odd=?1,system_sizes_json=?2,total_stake_cents=?3,target_odds_reached=?4 WHERE id=?5", params![odd, (!sizes.is_empty()).then(|| serde_json::to_string(&sizes).unwrap()), total, (h.2 == "DAILY_HIGH_CONFIDENCE" && odd.unwrap_or(0.0) >= policy().high_confidence_target_odd) as i64, r.coupon_id]).map_err(|e| e.to_string())?;
    c.execute(
        "DELETE FROM phase8_system_columns WHERE coupon_id=?1",
        [r.coupon_id],
    )
    .map_err(|e| e.to_string())?;
    for col in columns(&selected, &sizes) {
        c.execute("INSERT INTO phase8_system_columns(coupon_id,column_number,system_size,candidate_ids_json,column_decimal_odd) VALUES(?1,?2,?3,?4,?5)", params![r.coupon_id,col.column_number,col.system_size,serde_json::to_string(&col.candidate_ids).unwrap(),col.column_decimal_odd]).map_err(|e| e.to_string())?;
    }
    get_coupon(c, r.coupon_id)
}

pub fn finalize(c: &Connection, id: i64) -> Result<Coupon, String> {
    let changed = c
        .execute(
            "UPDATE phase8_coupons SET status='FINALIZED' WHERE id=?1 AND status='DRAFT'",
            [id],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        let s: String = c
            .query_row("SELECT status FROM phase8_coupons WHERE id=?1", [id], |x| {
                x.get(0)
            })
            .map_err(|e| e.to_string())?;
        if s != "FINALIZED" {
            return Err("COUPON_NOT_DRAFT".into());
        }
    }
    get_coupon(c, id)
}

pub fn get_coupon(c: &Connection, id: i64) -> Result<Coupon, String> {
    let h:(String,String,String,i64,String,Option<String>,String,Option<i64>,Option<i64>,String,Option<f64>,i64,Option<String>,Option<i64>,Option<i64>,String)=c.query_row("SELECT coupon_type,business_date,policy_version,source_candidate_run_id,model_version,calibration_version,generation_cutoff,series_id,step_number,status,combined_decimal_odd,target_odds_reached,system_sizes_json,unit_stake_cents,total_stake_cents,metadata_json FROM phase8_coupons WHERE id=?1",[id],|x|Ok((x.get(0)?,x.get(1)?,x.get(2)?,x.get(3)?,x.get(4)?,x.get(5)?,x.get(6)?,x.get(7)?,x.get(8)?,x.get(9)?,x.get(10)?,x.get::<_,i64>(11)?,x.get(12)?,x.get(13)?,x.get(14)?,x.get(15)?))).map_err(|e|e.to_string())?;
    let mut q=c.prepare("SELECT candidate_id FROM phase8_coupon_selections WHERE coupon_id=?1 ORDER BY selection_order").map_err(|e|e.to_string())?;
    let ids = q
        .query_map([id], |x| x.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<i64>, _>>()
        .map_err(|e| e.to_string())?;
    let mut q=c.prepare("SELECT candidate_id,match_id,competition_id,market,selection,line_value,raw_probability,public_probability,calibration_status,calibration_version,iddaa_odd,odds_snapshot_id,odds_captured_at,probability_edge,expected_value,score,rank,correlation_type,status FROM phase8_coupon_selections WHERE coupon_id=?1 ORDER BY selection_order").map_err(|e|e.to_string())?;
    let selections = q
        .query_map([id], |x| {
            Ok(CouponSelectionSnapshot {
                candidate_id: x.get(0)?,
                match_id: x.get(1)?,
                competition_id: x.get(2)?,
                market: x.get(3)?,
                selection: x.get(4)?,
                line: x.get(5)?,
                raw_probability: x.get(6)?,
                public_probability: x.get(7)?,
                calibration_status: x.get(8)?,
                calibration_version: x.get(9)?,
                odd: x.get(10)?,
                odds_snapshot_id: x.get(11)?,
                odds_captured_at: x.get(12)?,
                edge: x.get(13)?,
                expected_value: x.get(14)?,
                score: x.get(15)?,
                rank: x.get(16)?,
                correlation_type: x.get(17)?,
                status: x.get(18)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut q=c.prepare("SELECT column_number,system_size,candidate_ids_json,column_decimal_odd,status FROM phase8_system_columns WHERE coupon_id=?1 ORDER BY column_number").map_err(|e|e.to_string())?;
    let columns = q
        .query_map([id], |x| {
            let j: String = x.get(2)?;
            Ok(SystemColumn {
                column_number: x.get::<_, i64>(0)? as usize,
                system_size: x.get::<_, i64>(1)? as usize,
                candidate_ids: serde_json::from_str(&j).unwrap(),
                column_decimal_odd: x.get(3)?,
                status: x.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(Coupon {
        id,
        status: h.9,
        coupon_type: h.0,
        business_date: h.1,
        source_candidate_run_id: h.3,
        policy_version: h.2,
        model_version: h.4,
        calibration_version: h.5,
        generation_cutoff: h.6,
        series_id: h.7,
        step_number: h.8.map(|x| x as usize),
        candidate_ids: ids,
        selections,
        system_sizes: h
            .12
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default(),
        columns,
        combined_decimal_odd: h.10,
        target_odds_reached: h.11 == 1,
        unit_stake_cents: h.13,
        total_stake_cents: h.14,
        metadata: serde_json::from_str(&h.15).unwrap_or_default(),
    })
}

pub fn lineup_revision_impact(
    c: &Connection,
    r: &LineupRevisionImpactRequest,
) -> Result<LineupRevisionImpact, String> {
    let (business_date, base_run_id, status): (String, i64, String) = c
        .query_row(
            "SELECT business_date,source_candidate_run_id,status FROM phase8_coupons WHERE id=?1",
            [r.coupon_id],
            |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    let _ = status;
    let base_run = candidate_engine::get(
        c,
        &candidate_engine::GetRequest {
            business_date: business_date.clone(),
            run_id: Some(base_run_id),
            category: None,
        },
    )?;
    let latest = candidate_engine::generate_lineup_revision(
        c,
        &candidate_engine::LineupRevisionGenerateRequest {
            base_run_id,
            business_date,
        },
    )?;
    let mut q = c
        .prepare("SELECT id,candidate_id,match_id,market,selection,line_value,public_probability FROM phase8_coupon_selections WHERE coupon_id=?1 ORDER BY selection_order")
        .map_err(|e| e.to_string())?;
    let rows = q
        .query_map([r.coupon_id], |x| {
            Ok((
                x.get::<_, i64>(0)?,
                x.get::<_, i64>(1)?,
                x.get::<_, i64>(2)?,
                x.get::<_, String>(3)?,
                x.get::<_, String>(4)?,
                x.get::<_, Option<f64>>(5)?,
                x.get::<_, f64>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut items = Vec::new();
    for (selection_id, candidate_id, match_id, market, selection, line, base_probability) in rows {
        let base_candidate = base_run
            .candidates
            .iter()
            .find(|x| x.id == candidate_id)
            .ok_or("CANDIDATE_NOT_FOUND")?;
        let latest_candidate = latest.candidates.iter().find(|x| {
            x.prediction_id == base_candidate.prediction_id && x.category == base_candidate.category
        });
        let mut latest_probability = base_probability;
        let mut latest_status = "QUALIFIED".to_string();
        let mut source = "BASE".to_string();
        let mut revision_id = None;
        let mut snapshot_id = None;
        let mut family_status = None;
        let mut fallback_reason = None;
        if let Some(x) = latest_candidate {
            latest_probability = x.final_public_probability.unwrap_or(x.public_probability);
            latest_status = x.qualification_state.clone();
            source = x.prediction_source.clone().unwrap_or_else(|| "BASE".into());
            revision_id = x.lineup_revision_id;
            snapshot_id = x.lineup_snapshot_id;
            family_status = x.lineup_family_status.clone();
            fallback_reason = x.lineup_fallback_reason.clone();
        } else if let Some((revision_id_value, snapshot_id_value, json)) = c
            .query_row("SELECT id,lineup_snapshot_id,market_payload_json FROM prediction_revisions WHERE prediction_id=?1 AND payload_schema=?2 ORDER BY id DESC LIMIT 1", params![base_candidate.prediction_id, crate::repositories::lineup_model::REVISION_PAYLOAD_SCHEMA], |x| Ok((x.get::<_, i64>(0)?,x.get::<_, Option<i64>>(1)?,x.get::<_, String>(2)?))).optional().map_err(|e| e.to_string())?
        {
            let document: crate::repositories::lineup_model::RevisionMarketPayloadDocument = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            if let Some(m) = document.markets.iter().find(|x| x.market == market && x.selection == selection && x.line.map(f64::to_bits) == line.map(f64::to_bits)) {
                latest_probability = m.final_public_probability.unwrap_or(base_probability);
                latest_status = "NOT_QUALIFIED".into();
                source = if m.family_status == "ACTIVE" { "LINEUP_AWARE" } else { "BASE_RETAINED" }.into();
                revision_id = Some(revision_id_value);
                snapshot_id = snapshot_id_value;
                family_status = Some(m.family_status.clone());
                fallback_reason = m.fallback_reason.clone();
            }
        }
        let changed = (latest_probability - base_probability).abs() > 1e-12;
        let impact_type =
            if base_candidate.qualification_state == "QUALIFIED" && latest_status != "QUALIFIED" {
                "NO_LONGER_QUALIFIED"
            } else if changed && latest_status == "QUALIFIED" {
                "STILL_QUALIFIED"
            } else if changed {
                "PROBABILITY_CHANGED"
            } else {
                "UNCHANGED"
            };
        items.push(LineupRevisionImpactItem {
            coupon_selection_id: selection_id,
            original_candidate_id: candidate_id,
            match_id,
            market,
            selection,
            line,
            base_probability,
            latest_probability,
            delta_percentage_points: (latest_probability - base_probability) * 100.0,
            old_qualification_status: base_candidate.qualification_state.clone(),
            new_qualification_status: latest_status,
            impact_type: impact_type.into(),
            lineup_revision_id: revision_id,
            lineup_snapshot_id: snapshot_id,
            family_status,
            fallback_reason,
            prediction_source: source,
        });
    }
    Ok(LineupRevisionImpact {
        revision_available: latest.lineup_aware_match_count > 0,
        base_candidate_run_id: base_run_id,
        latest_lineup_candidate_run_id: (latest.lineup_aware_match_count > 0)
            .then_some(latest.run_id),
        affected_selection_count: items
            .iter()
            .filter(|x| x.impact_type != "UNCHANGED")
            .count(),
        items,
    })
}

pub fn get_daily(c: &Connection, date: &str, kind: Option<&str>) -> Result<Vec<Coupon>, String> {
    let mut q=c.prepare("SELECT id FROM phase8_coupons WHERE business_date=?1 AND (?2 IS NULL OR coupon_type=?2) ORDER BY id").map_err(|e|e.to_string())?;
    let ids = q
        .query_map(params![date, kind], |x| x.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<i64>, _>>()
        .map_err(|e| e.to_string())?;
    ids.into_iter().map(|id| get_coupon(c, id)).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompoundSeriesStep {
    pub step_number: usize,
    pub coupon_id: i64,
    pub business_date: String,
    pub stake_cents: i64,
    pub combined_odd: f64,
    pub result: String,
    pub settled_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompoundSeries {
    pub id: i64,
    pub business_date: String,
    pub status: String,
    pub current_step: usize,
    pub starting_stake_cents: i64,
    pub current_stake_cents: i64,
    pub completed_steps: usize,
    pub latest_coupon_id: Option<i64>,
    pub reset_count: usize,
    pub history: Vec<CompoundSeriesStep>,
}
pub fn start_series(c: &Connection, date: &str, stake: i64) -> Result<CompoundSeries, String> {
    if stake <= 0 {
        return Err("INVALID_STARTING_STAKE".into());
    }
    if c.query_row::<i64, _, _>(
        "SELECT COUNT(*) FROM phase8_compound_series WHERE status='ACTIVE'",
        [],
        |x| x.get(0),
    )
    .map_err(|e| e.to_string())?
        > 0
    {
        return Err("ACTIVE_SERIES_EXISTS".into());
    }
    c.execute("INSERT INTO phase8_compound_series(business_date,status,current_step,starting_stake_cents,current_stake_cents,metadata_json) VALUES(?1,'ACTIVE',1,?2,?2,'{}')",params![date,stake]).map_err(|e|e.to_string())?;
    series(c, c.last_insert_rowid())
}
pub fn series(c: &Connection, id: i64) -> Result<CompoundSeries, String> {
    let mut value=c.query_row("SELECT id,business_date,status,current_step,starting_stake_cents,current_stake_cents,completed_steps,latest_coupon_id,reset_count FROM phase8_compound_series WHERE id=?1",[id],|x|Ok(CompoundSeries{id:x.get(0)?,business_date:x.get(1)?,status:x.get(2)?,current_step:x.get::<_,i64>(3)? as usize,starting_stake_cents:x.get(4)?,current_stake_cents:x.get(5)?,completed_steps:x.get::<_,i64>(6)? as usize,latest_coupon_id:x.get(7)?,reset_count:x.get::<_,i64>(8)? as usize,history:Vec::new()})).map_err(|e|e.to_string())?;
    let mut statement=c.prepare("SELECT step_number,coupon_id,business_date,stake_cents,combined_odd,result,settled_at FROM phase8_series_steps WHERE series_id=?1 ORDER BY id").map_err(|e|e.to_string())?;
    value.history = statement
        .query_map([id], |row| {
            Ok(CompoundSeriesStep {
                step_number: row.get::<_, i64>(0)? as usize,
                coupon_id: row.get(1)?,
                business_date: row.get(2)?,
                stake_cents: row.get(3)?,
                combined_odd: row.get(4)?,
                result: row.get(5)?,
                settled_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(value)
}
pub fn series_status(c: &Connection) -> Result<Option<CompoundSeries>, String> {
    let id = c
        .query_row(
            "SELECT id FROM phase8_compound_series ORDER BY id DESC LIMIT 1",
            [],
            |x| x.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    id.map(|x| series(c, x)).transpose()
}
pub fn settle_series(
    c: &Connection,
    id: i64,
    won: bool,
    gross_return_cents: i64,
) -> Result<CompoundSeries, String> {
    settle_series_result(c, id, if won { "WON" } else { "LOST" }, gross_return_cents)
}

pub fn settle_series_result(
    c: &Connection,
    id: i64,
    result: &str,
    gross_return_cents: i64,
) -> Result<CompoundSeries, String> {
    let s = series(c, id)?;
    if s.status != "ACTIVE" {
        return Ok(s);
    }
    if let Some(coupon_id) = s.latest_coupon_id {
        let step_result: String = c.query_row("SELECT result FROM phase8_series_steps WHERE coupon_id=?1 ORDER BY id DESC LIMIT 1", [coupon_id], |x| x.get(0)).optional().map_err(|e| e.to_string())?.unwrap_or("UNSETTLED".into());
        if step_result != "UNSETTLED" {
            return Ok(s);
        }
    }
    if result == "WON" {
        if s.current_step >= 7 {
            c.execute("UPDATE phase8_compound_series SET status='COMPLETED',completed_steps=7 WHERE id=?1",[id]).map_err(|e|e.to_string())?;
        } else {
            c.execute("UPDATE phase8_compound_series SET current_step=current_step+1,completed_steps=completed_steps+1,current_stake_cents=?2 WHERE id=?1",params![id,gross_return_cents]).map_err(|e|e.to_string())?;
        }
    } else if result == "LOST" {
        c.execute("UPDATE phase8_compound_series SET status='ACTIVE',current_step=1,current_stake_cents=starting_stake_cents,reset_count=reset_count+1 WHERE id=?1",[id]).map_err(|e|e.to_string())?;
    } else if result == "VOID" {
        // A fully void step is retried at the same step and stake.
        if let Some(coupon_id) = s.latest_coupon_id {
            c.execute("UPDATE phase8_series_steps SET result='VOID',settled_at=?2 WHERE coupon_id=?1 AND result='UNSETTLED'",params![coupon_id,chrono::Utc::now().to_rfc3339()]).map_err(|e|e.to_string())?;
        }
        return series(c, id);
    } else {
        return Err("INVALID_SERIES_RESULT".into());
    }
    if let Some(coupon_id) = s.latest_coupon_id {
        c.execute("UPDATE phase8_series_steps SET result=?2,settled_at=?3 WHERE coupon_id=?1 AND result='UNSETTLED'",params![coupon_id,result,chrono::Utc::now().to_rfc3339()]).map_err(|e|e.to_string())?;
    }
    series(c, id)
}

pub fn generate_compound_step(
    c: &Connection,
    date: &str,
    run_id: Option<i64>,
) -> Result<Coupon, String> {
    let id: i64 = c
        .query_row(
            "SELECT id FROM phase8_compound_series ORDER BY id DESC LIMIT 1",
            [],
            |x| x.get(0),
        )
        .map_err(|_| "NO_ACTIVE_SERIES".to_string())?;
    let s = series(c, id)?;
    if s.status == "COMPLETED" {
        return Err("SERIES_COMPLETED".into());
    }
    if s.status != "ACTIVE" {
        return Err("SERIES_NOT_ACTIVE".into());
    }
    if let Some(existing) = c
        .query_row("SELECT coupon_id FROM phase8_series_steps WHERE series_id=?1 AND step_number=?2 AND business_date=?3", params![s.id, s.current_step as i64, date], |x| x.get(0))
        .optional()
        .map_err(|e| e.to_string())?
    {
        return get_coupon(c, existing);
    }
    let generated = generate_daily(
        c,
        &GenerateRequest {
            business_date: date.into(),
            candidate_run_id: run_id,
            coupon_type: Some("DAILY_COMPOUND".into()),
            unit_stake_cents: Some(s.current_stake_cents),
        },
    )?;
    let mut coupon = generated
        .into_iter()
        .next()
        .ok_or("NO_QUALIFYING_COMPOUND_COUPON")?;
    c.execute("UPDATE phase8_coupons SET series_id=?2,step_number=?3,unit_stake_cents=?4,total_stake_cents=?4 WHERE id=?1",params![coupon.id,s.id,s.current_step,s.current_stake_cents]).map_err(|e|e.to_string())?;
    let potential = payout(
        s.current_stake_cents,
        coupon.combined_decimal_odd.unwrap_or(1.0),
    );
    c.execute("INSERT OR IGNORE INTO phase8_series_steps(series_id,step_number,coupon_id,business_date,stake_cents,combined_odd,potential_return_cents) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![s.id,s.current_step,coupon.id,date,s.current_stake_cents,coupon.combined_decimal_odd.unwrap_or(1.0),potential]).map_err(|e|e.to_string())?;
    c.execute(
        "UPDATE phase8_compound_series SET latest_coupon_id=?2 WHERE id=?1",
        params![s.id, coupon.id],
    )
    .map_err(|e| e.to_string())?;
    coupon = get_coupon(c, coupon.id)?;
    Ok(coupon)
}
pub fn cancel_series(c: &Connection, id: i64) -> Result<CompoundSeries, String> {
    let s = series(c, id)?;
    if s.status == "ACTIVE" {
        c.execute(
            "UPDATE phase8_compound_series SET status='CANCELLED' WHERE id=?1",
            [id],
        )
        .map_err(|e| e.to_string())?;
        return series(c, id);
    }
    Ok(s)
}

pub fn system_preview(c: &Connection, r: &SystemPreviewRequest) -> Result<SystemPreview, String> {
    let mut cs = Vec::new();
    for id in &r.candidate_ids {
        let x:candidate_engine::Candidate=c.query_row("SELECT id,prediction_id,match_id,competition_id,category,market,selection,line_value,raw_probability,public_probability,calibration_status,calibration_version,calibration_bucket,bucket_observed_rate,bucket_sample_size,bucket_calibration_gap,iddaa_odd,odds_snapshot_id,odds_captured_at,implied_probability,probability_edge,expected_value,data_quality,score,score_components_json,rank,same_match_group,correlation_type,compound_eligible,qualification_state,prediction_source,lineup_revision_id,lineup_snapshot_id,base_public_probability,final_public_probability,delta_percentage_points,lineup_family_status,lineup_fallback_reason,lineup_model_version,lineup_model_hash FROM candidate_engine_candidates WHERE id=?1 AND qualification_state='QUALIFIED'",[id],|x|{let j:String=x.get(24)?;Ok(candidate_engine::Candidate{id:x.get(0)?,prediction_id:x.get(1)?,match_id:x.get(2)?,competition_id:x.get(3)?,category:x.get(4)?,market:x.get(5)?,selection:x.get(6)?,line:x.get(7)?,raw_probability:x.get(8)?,public_probability:x.get(9)?,calibration_status:x.get(10)?,calibration_version:x.get(11)?,calibration_bucket:x.get(12)?,bucket_observed_rate:x.get(13)?,bucket_sample_size:x.get::<_,Option<i64>>(14)?.map(|n|n as usize),bucket_calibration_gap:x.get(15)?,iddaa_odd:x.get(16)?,odds_snapshot_id:x.get(17)?,odds_captured_at:x.get(18)?,implied_probability:x.get(19)?,probability_edge:x.get(20)?,expected_value:x.get(21)?,data_quality:x.get(22)?,score:x.get(23)?,score_components:serde_json::from_str(&j).unwrap_or_default(),rank:x.get(25)?,same_match_group:x.get(26)?,correlation_type:x.get(27)?,compound_eligible:x.get::<_,i64>(28)?==1,qualification_state:x.get(29)?,prediction_source:x.get(30)?,lineup_revision_id:x.get(31)?,lineup_snapshot_id:x.get(32)?,base_public_probability:x.get(33)?,final_public_probability:x.get(34)?,delta_percentage_points:x.get(35)?,lineup_family_status:x.get(36)?,lineup_fallback_reason:x.get(37)?,lineup_model_version:x.get(38)?,lineup_model_hash:x.get(39)?})}).optional().map_err(|e|e.to_string())?.ok_or("CANDIDATE_NOT_FOUND")?;
        cs.push(x);
    }
    let cols = columns(&cs, &r.system_sizes);
    if !matches!(r.system_sizes.as_slice(), [5, 6] | [5, 6, 7]) {
        return Err("INVALID_SYSTEM_SIZES".into());
    }
    Ok(SystemPreview {
        selection_count: cs.len(),
        system_sizes: r.system_sizes.clone(),
        total_stake_cents: r.unit_stake_cents * cols.len() as i64,
        max_theoretical_return_cents: cols
            .iter()
            .map(|x| (r.unit_stake_cents as f64 * x.column_decimal_odd).round() as i64)
            .sum(),
        columns: cols,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectionOutcome {
    pub candidate_id: i64,
    pub result: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SettlementRequest {
    pub coupon_id: i64,
    pub outcomes: Vec<SelectionOutcome>,
    pub settled_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SettlementResult {
    pub coupon_id: i64,
    pub status: String,
    pub winning_columns: usize,
    pub losing_columns: usize,
    pub void_columns: usize,
    pub unsettled_columns: usize,
    pub total_columns: usize,
    pub total_stake_cents: i64,
    pub gross_return_cents: i64,
    pub profit_loss_cents: i64,
}

fn payout(stake: i64, odd: f64) -> i64 {
    ((stake as f64) * odd).round() as i64
}
pub fn settle(c: &Connection, r: &SettlementRequest) -> Result<SettlementResult, String> {
    let (kind,status,total_stake,unit): (String,String,Option<i64>,Option<i64>) = c.query_row("SELECT coupon_type,status,total_stake_cents,unit_stake_cents FROM phase8_coupons WHERE id=?1",[r.coupon_id],|x|Ok((x.get(0)?,x.get(1)?,x.get(2)?,x.get(3)?))).map_err(|e|e.to_string())?;
    if status == "SETTLED" {
        let j: String = c
            .query_row(
                "SELECT metadata_json FROM phase8_coupons WHERE id=?1",
                [r.coupon_id],
                |x| x.get(0),
            )
            .map_err(|e| e.to_string())?;
        return serde_json::from_str::<SettlementResult>(
            &serde_json::from_str::<serde_json::Value>(&j).map_err(|e| e.to_string())?
                ["settlement"]
                .to_string(),
        )
        .map_err(|e| e.to_string());
    }
    if status != "FINALIZED" {
        return Err("COUPON_NOT_FINALIZED".into());
    }
    let mut outcomes = std::collections::BTreeMap::new();
    for x in &r.outcomes {
        if !matches!(x.result.as_str(), "WON" | "LOST" | "VOID" | "UNSETTLED") {
            return Err("INVALID_SETTLEMENT_RESULT".into());
        }
        outcomes.insert(x.candidate_id, x.result.clone());
        c.execute(
            "UPDATE phase8_coupon_selections SET status=?2 WHERE coupon_id=?1 AND candidate_id=?3",
            params![r.coupon_id, x.result, x.candidate_id],
        )
        .map_err(|e| e.to_string())?;
    }
    let mut q = c
        .prepare("SELECT candidate_id,iddaa_odd FROM phase8_coupon_selections WHERE coupon_id=?1")
        .map_err(|e| e.to_string())?;
    let legs = q
        .query_map([r.coupon_id], |x| {
            Ok((x.get::<_, i64>(0)?, x.get::<_, f64>(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut gross = 0;
    let mut wins = 0;
    let mut losses = 0;
    let mut voids = 0;
    let mut unsettled = 0;
    let is_system = kind == "DAILY_SURPRISE_SYSTEM";
    if is_system {
        let mut cq=c.prepare("SELECT id,candidate_ids_json,column_decimal_odd FROM phase8_system_columns WHERE coupon_id=?1 ORDER BY column_number").map_err(|e|e.to_string())?;
        for row in cq
            .query_map([r.coupon_id], |x| {
                Ok((
                    x.get::<_, i64>(0)?,
                    x.get::<_, String>(1)?,
                    x.get::<_, f64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
        {
            let (col, j, _) = row.map_err(|e| e.to_string())?;
            let ids: Vec<i64> = serde_json::from_str(&j).map_err(|e| e.to_string())?;
            let mut odd = 1.0;
            let mut lost = false;
            let mut pending = false;
            let mut all_void = true;
            for id in ids {
                let res = outcomes.get(&id).cloned().unwrap_or("UNSETTLED".into());
                let base = legs.iter().find(|x| x.0 == id).map(|x| x.1).unwrap_or(1.0);
                match res.as_str() {
                    "LOST" => lost = true,
                    "UNSETTLED" => pending = true,
                    "WON" => {
                        all_void = false;
                        odd *= base
                    }
                    "VOID" => {}
                    _ => {}
                }
            }
            let state = if lost {
                "LOST"
            } else if pending {
                "UNSETTLED"
            } else if all_void {
                "VOID"
            } else {
                "WON"
            };
            let stored_state = if state == "UNSETTLED" {
                "PENDING"
            } else {
                state
            };
            c.execute(
                "UPDATE phase8_system_columns SET status=?2 WHERE id=?1",
                params![col, stored_state],
            )
            .map_err(|e| e.to_string())?;
            match state {
                "WON" => {
                    wins += 1;
                    gross += payout(unit.unwrap_or(0), odd)
                }
                "LOST" => losses += 1,
                "VOID" => {
                    voids += 1;
                    gross += unit.unwrap_or(0)
                }
                _ => unsettled += 1,
            }
        }
    } else {
        let mut odd = 1.0;
        let mut lost = false;
        let mut pending = false;
        let mut all_void = true;
        for (id, base) in legs {
            match outcomes.get(&id).map(String::as_str).unwrap_or("UNSETTLED") {
                "LOST" => lost = true,
                "UNSETTLED" => pending = true,
                "WON" => {
                    all_void = false;
                    odd *= base
                }
                "VOID" => {}
                _ => {}
            }
        }
        let state = if lost {
            "LOST"
        } else if pending {
            "UNSETTLED"
        } else if all_void {
            "VOID"
        } else {
            "WON"
        };
        let stake = total_stake.unwrap_or(0);
        if state == "WON" {
            gross = payout(stake, odd)
        } else if state == "VOID" {
            gross = stake
        };
        wins = usize::from(state == "WON");
        losses = usize::from(state == "LOST");
        voids = usize::from(state == "VOID");
        unsettled = usize::from(state == "UNSETTLED");
    }
    let total_columns = if is_system {
        wins + losses + voids + unsettled
    } else {
        1
    };
    let final_status = if unsettled > 0 {
        "FINALIZED"
    } else if losses > 0 {
        "SETTLED"
    } else if wins > 0 {
        "SETTLED"
    } else {
        "SETTLED"
    };
    let public_status = if unsettled > 0 {
        "FINALIZED"
    } else if losses > 0 && !is_system {
        "SETTLED"
    } else {
        "SETTLED"
    };
    let result = SettlementResult {
        coupon_id: r.coupon_id,
        status: if unsettled > 0 {
            "UNSETTLED".into()
        } else if losses > 0 && !is_system {
            "LOST".into()
        } else if wins > 0 {
            "WON".into()
        } else {
            "VOID".into()
        },
        winning_columns: wins,
        losing_columns: losses,
        void_columns: voids,
        unsettled_columns: unsettled,
        total_columns,
        total_stake_cents: total_stake.unwrap_or_else(|| unit.unwrap_or(0) * total_columns as i64),
        gross_return_cents: gross,
        profit_loss_cents: gross
            - total_stake.unwrap_or_else(|| unit.unwrap_or(0) * total_columns as i64),
    };
    if unsettled == 0 {
        if let Some(series_id) = c
            .query_row(
                "SELECT series_id FROM phase8_coupons WHERE id=?1",
                [r.coupon_id],
                |x| x.get::<_, Option<i64>>(0),
            )
            .map_err(|e| e.to_string())?
        {
            let _ = settle_series_result(c, series_id, &result.status, result.gross_return_cents)?;
        }
        let old: String = c
            .query_row(
                "SELECT metadata_json FROM phase8_coupons WHERE id=?1",
                [r.coupon_id],
                |x| x.get(0),
            )
            .map_err(|e| e.to_string())?;
        let mut meta: serde_json::Value = serde_json::from_str(&old).unwrap_or_default();
        meta["settlement"] = serde_json::to_value(&result).unwrap();
        c.execute("UPDATE phase8_coupons SET status='SETTLED',settled_at=?2,settlement_result=?3,metadata_json=?4 WHERE id=?1",params![r.coupon_id,r.settled_at,result.status,meta.to_string()]).map_err(|e|e.to_string())?;
    }
    let _ = (final_status, public_status);
    Ok(result)
}
