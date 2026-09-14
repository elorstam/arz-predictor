//! One explicit publication manifest shared by Today, Candidates and Coupons.
//! A category publication (currently BTTS) replaces only that category's pool.
use super::{candidate_engine as ce, coupon_engine as coupons};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct Publication {
    pub id: String,
    pub business_date: String,
    pub candidate_run_id: i64,
    pub category_run_ids: BTreeMap<String, i64>,
}

pub fn publication(c: &Connection, date: &str) -> Result<Publication, String> {
    let base = ce::selected_run_id(c, date)?;
    let btts: Option<i64> = c.query_row(
        "SELECT b.candidate_run_id FROM btts_daily_runs b JOIN candidate_engine_runs r ON r.id=b.candidate_run_id WHERE b.business_date=?1 AND r.business_date=?1 AND r.status='COMPLETED' AND (?2 IS NULL OR julianday(r.generated_at)>=julianday((SELECT generated_at FROM candidate_engine_runs WHERE id=?2))) ORDER BY b.id DESC LIMIT 1",
        rusqlite::params![date,base], |r| r.get(0)).optional().map_err(|e|e.to_string())?;
    let base = base.or(btts).ok_or("no candidate run")?;
    let mut category_run_ids: BTreeMap<String, i64> = ce::CATEGORIES
        .iter()
        .map(|x| (x.to_string(), base))
        .collect();
    if let Some(id) = btts {
        category_run_ids.insert("BTTS_YES".into(), id);
    }
    Ok(Publication {
        id: format!("{date}:base:{base}:btts:{}", btts.unwrap_or(base)),
        business_date: date.into(),
        candidate_run_id: base,
        category_run_ids,
    })
}

#[derive(Debug, Serialize)]
pub struct SelectionSource {
    pub source_run_id: i64,
    pub source_policy_version: String,
    pub selection_status: &'static str,
    pub strict_qualified: bool,
    pub confidence: Option<f64>,
}

pub fn selection_status(category: &str) -> &'static str {
    match category {
        "OVER_25" | "BTTS_YES" => "DAILY_RANKED",
        "HIGH_CONFIDENCE" => "HIGH_CONFIDENCE",
        "COMPOUND" => "KATLAMA_ELIGIBLE",
        _ => "STRICT_QUALIFIED",
    }
}

#[derive(Debug, Serialize)]
pub struct DailyView {
    #[serde(flatten)]
    pub run: ce::DailyRun,
    pub publication_id: String,
    pub category_run_ids: BTreeMap<String, i64>,
    pub selection_sources: BTreeMap<i64, SelectionSource>,
    pub category_counts: BTreeMap<String, usize>,
    pub lineup_verified: bool,
}

pub fn view(c: &Connection, p: &Publication) -> Result<DailyView, String> {
    let mut run = ce::get(
        c,
        &ce::GetRequest {
            business_date: p.business_date.clone(),
            run_id: Some(p.candidate_run_id),
            category: None,
        },
    )?;
    let mut policies = BTreeMap::from([(p.candidate_run_id, run.policy_version.clone())]);
    for (category, id) in &p.category_run_ids {
        if *id == p.candidate_run_id {
            continue;
        }
        let overlay = ce::get(
            c,
            &ce::GetRequest {
                business_date: p.business_date.clone(),
                run_id: Some(*id),
                category: Some(category.clone()),
            },
        )?;
        policies.insert(*id, overlay.policy_version.clone());
        run.candidates.retain(|x| x.category != *category);
        run.candidates.extend(overlay.candidates);
        run.exclusions.retain(|x| x.category != *category);
        run.exclusions.extend(overlay.exclusions);
    }
    run.candidates
        .retain(|x| x.qualification_state == "QUALIFIED");
    run.candidates.sort_by(ce::compare_candidates);
    let lineup_verified = run.prediction_context.as_deref() == Some("LINEUP_AWARE")
        && ce::valid_lineup_run(c, p.candidate_run_id)?;
    if !lineup_verified {
        run.prediction_context = Some("BASE".into());
    }
    let selection_sources = run
        .candidates
        .iter()
        .map(|x| {
            let source_run_id = p.category_run_ids[&x.category];
            let source_policy_version = policies[&source_run_id].clone();
            // Frozen legacy goal selections actually passed the former strict
            // policy. Do not rewrite their provenance as a newly ranked pool.
            let status = if matches!(x.category.as_str(), "OVER_25" | "BTTS_YES")
                && source_policy_version == "candidate_policy_v1"
            {
                "STRICT_QUALIFIED"
            } else {
                selection_status(&x.category)
            };
            (
                x.id,
                SelectionSource {
                    source_run_id,
                    source_policy_version,
                    selection_status: status,
                    strict_qualified: status != "DAILY_RANKED",
                    confidence: x.score_components.get("confidence").copied(),
                },
            )
        })
        .collect();
    let mut category_counts: BTreeMap<String, usize> =
        ce::CATEGORIES.iter().map(|x| (x.to_string(), 0)).collect();
    let mut union = BTreeSet::new();
    for x in &run.candidates {
        *category_counts.entry(x.category.clone()).or_default() += 1;
        union.insert((
            x.match_id,
            x.market.clone(),
            x.selection.clone(),
            x.line.map(|v| v.to_string()),
        ));
    }
    category_counts.insert("ALL".into(), union.len());
    Ok(DailyView {
        run,
        publication_id: p.id.clone(),
        category_run_ids: p.category_run_ids.clone(),
        selection_sources,
        category_counts,
        lineup_verified,
    })
}

#[derive(Debug, Serialize)]
pub struct DailyOutput {
    pub publication: Publication,
    pub run: DailyView,
    pub coupons: Vec<coupons::Coupon>,
}

pub fn get(c: &Connection, date: &str) -> Result<DailyOutput, String> {
    let tx = c.unchecked_transaction().map_err(|e| e.to_string())?;
    let publication = publication(&tx, date)?;
    let run = view(&tx, &publication)?;
    let coupons = coupons::get_daily(&tx, date, None)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(DailyOutput {
        publication,
        run,
        coupons,
    })
}
