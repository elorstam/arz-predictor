"""Frozen temporal OOS experiment. Opens application SQLite read-only."""
import argparse
import hashlib
import json
import os
from collections import defaultdict
from datetime import date, timedelta
from pathlib import Path
import sqlite3
import joblib
from goal_core_v3 import (np, derive, causal_features, JointGoals, JointCalibrator,
                          ProductionReplay, project_production)
from goal_model_v2 import partitions, metrics, returns, bootstrap_mean
from goal_market_selection import load_csv_odds, stamp

MARKETS = ('over25', 'btts')
VARIANTS = ('A_production', 'B_joint', 'C_calibrated', 'D_ensemble', 'N_nominated')
EXCLUDED_START = '2026-09-12'


def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2, allow_nan=False), encoding='utf-8')


def ready(r):
    return min(r['features']['data_quality']['history_matches_home'], r['features']['data_quality']['history_matches_away']) >= 5 and r['features']['data_quality']['kickoff_quality'] == 'KNOWN'


def load_prices(db, targets):
    """Latest quote at decision; do not backfill an expired or invalid quote."""
    quotes = defaultdict(list)
    audit = defaultdict(int)
    query = "SELECT id,match_id,model_market_type,model_selection,line_value,odd,captured_at,provider FROM iddaa_model_odds WHERE model_market_type IN ('TOTAL_GOALS','BTTS') ORDER BY julianday(captured_at),id"
    for q in db.execute(query):
        market = 'over25' if q['model_market_type'] == 'TOTAL_GOALS' and q['line_value'] == 2.5 else 'btts' if q['model_market_type'] == 'BTTS' else None
        if market:
            side = 'yes' if q['model_selection'] in ('OVER', 'YES') else 'no' if q['model_selection'] in ('UNDER', 'NO') else None
            if side:
                quotes[(q['match_id'], market, side)].append(dict(q))
    prices, priors, latest = {}, {}, {}
    for r in targets:
        for market in MARKETS:
            sides = {}
            for side in ('yes', 'no'):
                past = [q for q in quotes[(r['match_id'], market, side)] if stamp(q['captured_at']) <= stamp(r['decision_at'])]
                if past:
                    q = past[-1]
                    age = (stamp(r['decision_at'])-stamp(q['captured_at'])).total_seconds()/3600
                    valid = np.isfinite(q['odd']) and q['odd'] > 1 and 0 <= age <= 24
                    sides[side] = q if valid else None
                    if side == 'yes':
                        latest[(r['match_id'], market)] = dict(odds=q['odd'], captured_at=q['captured_at'], age_hours=age, valid=bool(valid))
                        if valid:
                            prices[(r['match_id'], market)] = q['odd']
            if all(sides.get(s) for s in ('yes', 'no')):
                y, n = sides['yes'], sides['no']
                # A prior requires one contemporaneous two-sided bookmaker market.
                if y['provider'] == n['provider'] and y['captured_at'] == n['captured_at']:
                    priors[(r['match_id'], market)] = (1/y['odd'])/(1/y['odd']+1/n['odd'])
                    audit[market+'_priors'] += 1
    audit.update({m+'_valid_quotes': sum(k[1] == m for k in prices) for m in MARKETS})
    return prices, priors, latest, dict(audit)


def loss(goals, predictions):
    result = []
    for m in MARKETS:
        y = (goals.sum(1) > 2) if m == 'over25' else (goals.min(1) > 0)
        p = np.clip(predictions[m], 1e-10, 1-1e-10)
        result.append(float(-(y*np.log(p)+(1-y)*np.log1p(-p)).mean()))
    return float(np.mean(result))


def fit_fold(rows, x, raw, goals, ids):
    fit, tune, cal, select = ids[:4]
    refit = np.concatenate((fit, tune))
    dates = [date.fromisoformat(r['date']) for r in rows]
    tuning = []
    for ridge in (.1, 1., 10.):
        model = JointGoals(ridge).fit(x[fit], goals[fit], [dates[i] for i in fit])
        tuning.append((loss(goals[tune], derive(model.predict(x[tune]))), ridge))
    ridge = min(tuning)[1]
    joint = JointGoals(ridge).fit(x[refit], goals[refit], [dates[i] for i in refit])
    baseline = ProductionReplay().fit(raw[fit], goals[fit])
    baseline.choose_blend(raw[tune], goals[tune])
    blend = baseline.blend
    baseline.fit(raw[refit], goals[refit])
    baseline.blend = blend
    cal_days = sorted({rows[i]['date'] for i in cal})
    cal_edge = cal_days[len(cal_days)//2]
    embargo = (date.fromisoformat(cal_edge)-timedelta(days=2)).isoformat()
    cal_fit = np.array([i for i in cal if rows[i]['date'] < embargo])
    cal_valid = np.array([i for i in cal if rows[i]['date'] >= cal_edge])
    baseline.fit_public_calibration(raw[cal_fit], goals[cal_fit], raw[cal_valid], goals[cal_valid])
    calibrator = JointCalibrator().fit(joint.predict(x[cal]), goals[cal])
    b = joint.predict(x[select])
    c = calibrator.predict(b)
    a = baseline.probabilities(raw[select])
    projected = project_production(b, a['over25'], a['btts'])
    alpha = min((.25, .5, .75), key=lambda w: loss(goals[select], derive(w*c+(1-w)*projected)))
    losses = {'B_joint': loss(goals[select], derive(b)), 'C_calibrated': loss(goals[select], derive(c)),
              'D_ensemble': loss(goals[select], derive(alpha*c+(1-alpha)*projected))}
    nominated = min(losses, key=losses.get)
    metadata = dict(ridge=ridge, tuning=tuning, calibration_theta=calibrator.theta.tolist(),
                    production_btts_logistic_weight=float(baseline.blend), production_public_calibration=baseline.public_calibration,
                    production_calibration_periods={name:dict(n=len(idx),first=rows[idx[0]]['date'],last=rows[idx[-1]]['date']) for name,idx in [('fit',cal_fit),('validate',cal_valid)]}, ensemble_joint_weight=alpha,
                    nominated=nominated, selection_losses=losses,
                    periods={name:dict(n=len(idx), first=rows[idx[0]]['date'], last=rows[idx[-1]]['date']) for name, idx in zip(('fit','tune','calibrate','select','test'), ids)})
    return dict(joint=joint, baseline=baseline, calibrator=calibrator, alpha=alpha, nominated=nominated, metadata=metadata)


def predict(bundle, x, raw):
    a = bundle['baseline'].probabilities(raw)
    b = bundle['joint'].predict(x)
    c = bundle['calibrator'].predict(b)
    projected = project_production(b, a['over25'], a['btts'])
    d = bundle['alpha']*c+(1-bundle['alpha'])*projected
    result = dict(A_production=a, B_joint=derive(b), C_calibrated=derive(c), D_ensemble=derive(d))
    result['N_nominated'] = result[bundle['nominated']]
    return result


def qualifies(row, price_field='odds'):
    odd = row.get(price_field)
    return bool(odd is not None and np.isfinite(odd) and odd > 1 and row['p'] >= .64 and row['p']*odd > 1)


def betting(records, saturdays, price_field):
    bets = [r for r in records if qualifies(r, price_field)]
    stats = returns([r[price_field]*r['won']-1 for r in bets])
    stats.update(qualified_bets=len(bets), average_odds=float(np.mean([r[price_field] for r in bets])) if bets else None)
    stats.setdefault('hit_rate', None)
    groups = defaultdict(list)
    for r in bets:
        groups[r['date']].append(r)
    coupons = []
    for day in saturdays:
        distinct = {}
        for r in sorted(groups[day], key=lambda r: (-r['p'], r['match_id'])):
            distinct.setdefault(r['match_id'], r)
        picks = list(distinct.values())[:5] if len(distinct) >= 5 else []
        c = dict(date=day, valid_selections=len(distinct), status='QUALIFIED' if picks else 'INSUFFICIENT', picks=picks)
        if picks:
            odd = math_product(r[price_field] for r in picks)
            won = all(r['won'] for r in picks)
            c.update(combined_odds=odd, won=won, profit=odd-1 if won else -1.)
        coupons.append(c)
    played = [c for c in coupons if c['picks']]
    coupon_stats = returns([c['profit'] for c in played])
    coupon_stats.update(saturdays_tested=len(saturdays), saturdays_with_at_least_five=len(played),
                        average_combined_odds=float(np.mean([c['combined_odds'] for c in played])) if played else None)
    coupon_stats.setdefault('hit_rate', None)
    return dict(single=stats, coupons=coupon_stats), coupons


def math_product(items):
    import math
    return math.prod(items)


def evaluate(records, saturdays, out):
    summary = {}
    for key, data in records.items():
        market, variant = key
        data.sort(key=lambda r:(r['date'], r['kickoff'], r['match_id']))
        item = metrics(np.array([r['won'] for r in data]), np.array([r['p'] for r in data]))
        item['probability_floor_count_without_price'] = sum(r['p'] >= .64 for r in data)
        for name, price in [('verified', 'odds'), ('price_proxy', 'proxy_odds')]:
            item[name], coupons = betting(data, saturdays, price)
            item[name]['priced_matches'] = sum(r.get(price) is not None for r in data)
            write(out/f'{market}-{variant}-{name}-saturdays.json', coupons)
        write(out/f'{market}-{variant}-predictions.json', data)
        summary.setdefault(market, {})[variant] = item
    for market, variants in summary.items():
        for variant, item in variants.items():
            deltas = defaultdict(list)
            for b, v in zip(records[(market, 'A_production')], records[(market, variant)]):
                assert b['match_id'] == v['match_id']
                deltas[b['date']].append((v['p']-v['won'])**2-(b['p']-b['won'])**2)
            item['paired_day_brier_difference'] = bootstrap_mean([np.mean(v) for v in deltas.values()])
            for kind in ('verified','price_proxy'):
                a_coupons = json.loads((out/f'{market}-A_production-{kind}-saturdays.json').read_text())
                v_coupons = json.loads((out/f'{market}-{variant}-{kind}-saturdays.json').read_text())
                if any(c['picks'] for c in a_coupons+v_coupons):
                    item[kind]['paired_profit_per_saturday_difference'] = bootstrap_mean([v.get('profit',0)-a.get('profit',0) for a,v in zip(a_coupons,v_coupons)])
                else:
                    item[kind]['paired_profit_per_saturday_difference'] = None
            last = [r for r in records[(market, variant)] if r['fold'] == 4]
            item['last_fold'] = metrics(np.array([r['won'] for r in last]), np.array([r['p'] for r in last]))
    return summary


def adoption(summary):
    reasons = []
    # Predeclared materiality: >=1% relative Brier AND log-loss improvement in
    # both markets; >=10% more valid selections; noninferior coupon ROI/DD.
    for market, variants in summary.items():
        a, n = variants['A_production'], variants['N_nominated']
        if not (n['brier'] <= .99*a['brier'] and n['log_loss'] <= .99*a['log_loss'] and n['paired_day_brier_difference']['upper95'] < 0):
            reasons.append(f'{market}: material probability improvement is not established.')
        av, nv = a['verified'], n['verified']
        if nv['single']['qualified_bets'] < max(1, 1.1*av['single']['qualified_bets']):
            reasons.append(f'{market}: improved usable selection availability is not established.')
        if n['probability_floor_count_without_price'] < a['probability_floor_count_without_price']:
            reasons.append(f"{market}: probability-floor availability falls from {a['probability_floor_count_without_price']} to {n['probability_floor_count_without_price']} matches before requiring a price.")
        ac, nc = av['coupons'], nv['coupons']
        if nc['roi'] is None or ac['roi'] is None or nc['saturdays_with_at_least_five'] < ac['saturdays_with_at_least_five'] or nc['roi'] < ac['roi'] or nc['max_drawdown'] > ac['max_drawdown']:
            reasons.append(f'{market}: nondegrading coupon performance cannot be established.')
    reasons.append('Historical results end before timestamped Iddaa odds begin; verified historical BTTS/Over 2.5 execution and coupon evidence is unavailable.')
    reasons.append('These historical OOS periods were examined in V2 research; they are temporal OOS, not a fresh research holdout. Production deployment-vintage replay is unavailable.')
    return reasons


def live_report(db, live, bundle, x, latest, prices, out, fixture_counts):
    raw = np.array([r['baseline_raw'] for r in live], dtype=float)
    preds = predict(bundle, x, raw)
    active = db.execute('SELECT id,version_identifier,training_cutoff FROM model_versions WHERE is_active=1').fetchone()
    actual = {}
    for r in db.execute("SELECT p.*,pr.base_home_lambda,pr.base_away_lambda FROM predictions p JOIN prediction_runs pr ON pr.id=p.prediction_run_id WHERE p.model_version_id=? AND (p.market='BTTS' AND p.selection='YES' OR p.market='TOTAL_GOALS' AND p.selection='OVER' AND p.line_value=2.5) ORDER BY julianday(p.created_at),p.id", (active['id'],)):
        actual.setdefault((r['match_id'], 'btts' if r['market']=='BTTS' else 'over25'), []).append(dict(r))
    names = dict(db.execute('SELECT id,normalized_name FROM teams'))
    result = dict(challenger=bundle['nominated'], retained_model=active['version_identifier'], days={})
    full = []
    for i, r in enumerate(live):
        for market in MARKETS:
            key = (r['match_id'], market)
            quote = latest.get(key, {})
            prior = [p for p in actual.get(key, []) if stamp(p['created_at']) <= stamp(r['decision_at']) and stamp(active['training_cutoff']) < stamp(r['decision_at'])]
            production = prior[-1] if prior else None
            for source, p in [('retained_production', production['public_probability'] if production else None), ('nominated_challenger', float(preds['N_nominated'][market][i]))]:
                if p is None:
                    continue
                odd = quote.get('odds')
                full.append(dict(date=r['date'], match_id=r['match_id'], kickoff=r['kickoff'], market=market, source=source,
                                 home=names.get(r['home']), away=names.get(r['away']), p=p, odds=odd,
                                 ev=p*odd-1 if odd else None, valid_odds=key in prices,
                                 qualified=bool(key in prices and p>=.64 and p*prices[key]>1), odds_captured_at=quote.get('captured_at'),
                                 odds_age_hours=quote.get('age_hours'), decision_at=r['decision_at'],
                                 lambda_home=production['base_home_lambda'] if source=='retained_production' else float(preds['N_nominated']['lambda_home'][i]),
                                 lambda_away=production['base_away_lambda'] if source=='retained_production' else float(preds['N_nominated']['lambda_away'][i])))
    for day in ('2026-09-12', '2026-09-13', '2026-09-14'):
        result['days'][day] = dict(total_fixtures=fixture_counts[day], feature_ready_matches=sum(r['date']==day for r in live))
        for source in ('retained_production', 'nominated_challenger'):
            selected = [r for r in full if r['date']==day and r['source']==source]
            item = dict(prediction_ready_matches=len(set(r['match_id'] for r in selected)))
            for market in MARKETS:
                rows = [r for r in selected if r['market']==market]
                item[market] = dict(qualified_count=sum(r['qualified'] for r in rows), valid_odds_count=sum(r['valid_odds'] for r in rows),
                                    top10=sorted(rows, key=lambda r:(-r['p'],r['match_id']))[:10])
            result['days'][day][source] = item
    write(out/'sep-12-13-14.json', result)
    write(out/'sep-12-13-14-all-predictions.json', full)
    return result


def report(result, live, out):
    fmt = lambda x: 'N/A' if x is None else f'{x:.5f}'
    lines = ['# GOAL CORE V3', '', '**REJECTED. Production and coupon rules are unchanged.**', '',
             'Four expanding date-block folds; fit/tune/calibration/selection/test blocks separated by 48-hour label embargoes. Match features are frozen 24 hours before midnight Istanbul. September 12–14 never enter fitting, calibration, threshold selection, or OOS scoring.', '',
             'A is a causal refit of production Poisson + logistic BTTS architecture, its tune-selected BTTS blend, and its family Platt public calibration with log-loss/ECE acceptance and raw fallback. The calibration block is split chronologically into fit/validation halves with a 48-hour embargo. It is not a replay of archived deployment artifacts. B is the shared Poisson matrix; C calibrates its joint event masses; D mixes C with a coherent projection of A. N is nominated before each OOS block using average Over 2.5/BTTS log loss.', '',
             f"History: {result['historical_matches']} matches; ready: {result['ready_matches']}; OOS: {result['oos_matches']} matches; {result['saturdays_tested']} Saturdays including empty days.", '',
             '## Probability quality', '', '| Market / model | Brier | Log loss | ECE | Calibration gap | Probability >=64% |', '|---|---:|---:|---:|---:|---:|']
    for m, models in result['markets'].items():
        for name, s in models.items():
            lines.append(f"| {m} / {name} | {s['brier']:.5f} | {s['log_loss']:.5f} | {s['ece']:.5f} | {s['calibration_gap']:.5f} | {s['probability_floor_count_without_price']} |")
    lines += ['', '## Verified betting evidence', '', 'There are no qualifying timestamped historical quotes. For every model and both markets: qualified bets = 0; Saturdays with >=5 valid selections = 0; hit rate, single/coupon ROI, average odds, drawdown, and losing streak are N/A. Missing evidence is not a zero return.', '',
              '## Over 2.5 retrospective price proxy', '', 'Recorded non-closing B365 prices lack quote capture times. These results are descriptive and do not count as verified valid selections for adoption. BTTS has no matched historical prices, so its monetary metrics remain N/A.', '',
              '| Model | Qualified bets | Hit rate | ROI | Mean odds | Max DD (u) | Longest losses |', '|---|---:|---:|---:|---:|---:|---:|']
    for name, s in result['markets']['over25'].items():
        b = s['price_proxy']['single']
        lines.append(f"| {name} | {b['qualified_bets']} | {fmt(b['hit_rate'])} | {fmt(b['roi'])} | {fmt(b['average_odds'])} | {fmt(b['max_drawdown'])} | {b['longest_losing_streak']} |")
    lines += ['', '| Model | Saturdays tested | >=5 proxy-qualified | Coupon hit rate | Coupon ROI | Mean combined odds | Max DD (u) | Longest losses |', '|---|---:|---:|---:|---:|---:|---:|---:|']
    for name, s in result['markets']['over25'].items():
        c = s['price_proxy']['coupons']
        lines.append(f"| {name} | {c['saturdays_tested']} | {c['saturdays_with_at_least_five']} | {fmt(c['hit_rate'])} | {fmt(c['roi'])} | {fmt(c['average_combined_odds'])} | {fmt(c['max_drawdown'])} | {c['longest_losing_streak']} |")
    proxy_a = result['markets']['over25']['A_production']['price_proxy']['coupons']['bets']
    proxy_n = result['markets']['over25']['N_nominated']['price_proxy']['coupons']['bets']
    lines += ['', f'{proxy_n} proxy-priced coupons qualify for the nominated strategy, versus {proxy_a} for A. A larger observed coupon ROI is not sufficient evidence of an advantage with this sample. The paired all-Saturday profit bootstrap includes zero stakes on inactive days and is stored in evaluation.json; it is a profit-per-calendar-Saturday difference, not a difference in ROI per coupon.']
    lines += ['', 'Coupon evaluation uses the top five distinct fixtures only after each passes the unchanged 0.64 probability and positive-EV requirements. Fewer than five means no coupon. Markets are evaluated separately; no double counting a fixture inside a coupon. One unit per single/coupon, ordered by kickoff/id and Saturday respectively. Fees/taxes/slippage are excluded.', '',
              '## Adoption', '', 'Predeclared gate: >=1% relative improvement in both Brier and log loss for each market, paired day-bootstrap Brier upper bound below zero, >=10% more verified usable selections, and nondegrading verified coupon availability, ROI and drawdown. The bootstrap resamples day means (5,000 draws); it is exploratory because V2 already examined these dates.', '', *['- '+r for r in result['reasons']], '',
              '## September 12/13/14 after selection', '', f"Historical nomination: **{live['challenger']}**. Retained production: **{live['retained_model']}**. Daily rows below use retained production public probabilities; challenger tables are in sep-12-13-14.json.", '',
              'Daily snapshots use midnight Istanbul and the available stored data. Prices older than 24 hours are shown for audit, with descriptive EV, but cannot qualify. September 14 is a frozen-data forecast, not evidence that future quotes will be available.', '',
              '| Date | Feature ready | Production prediction ready | Over 2.5 qualified | BTTS qualified |', '|---|---:|---:|---:|---:|']
    for day, data in live['days'].items():
        p = data['retained_production']
        lines.append(f"| {day} | {data['feature_ready_matches']} | {p['prediction_ready_matches']} | {p['over25']['qualified_count']} | {p['btts']['qualified_count']} |")
    lines += ['', '| Date | Total fixtures | Challenger prediction ready | Challenger Over 2.5 qualified | Challenger BTTS qualified |', '|---|---:|---:|---:|---:|']
    for day, data in live['days'].items():
        p = data['nominated_challenger']
        lines.append(f"| {day} | {data['total_fixtures']} | {p['prediction_ready_matches']} | {p['over25']['qualified_count']} | {p['btts']['qualified_count']} |")
    for day, data in live['days'].items():
        for market in MARKETS:
            lines += ['', f'### {day} {market} top 10 — retained production', '', '| Match | Probability | Odds | EV | Valid quote | Qualified |', '|---|---:|---:|---:|---|---|']
            for r in data['retained_production'][market]['top10']:
                lines.append(f"| {r['home']} – {r['away']} | {r['p']:.5f} | {fmt(r['odds'])} | {fmt(r['ev'])} | {r['valid_odds']} | {r['qualified']} |")
    for day, data in live['days'].items():
        for market in MARKETS:
            lines += ['', f'### {day} {market} top 10 — nominated challenger (not adopted)', '', '| Match | Probability | Odds | EV | Valid quote | Qualified |', '|---|---:|---:|---:|---|---|']
            for r in data['nominated_challenger'][market]['top10']:
                lines.append(f"| {r['home']} – {r['away']} | {r['p']:.5f} | {fmt(r['odds'])} | {fmt(r['ev'])} | {r['valid_odds']} | {r['qualified']} |")
    lines += ['', '## Artifacts and limits', '',
              'evaluation.json contains all metrics, reliability bins, fold dates, fit settings, hash provenance and bootstrap intervals. Per-match prediction JSON and per-Saturday selections/settlements are saved for every model/market. Saved joblib models are research-only. The final model uses historical stage selection before these September dates; target results are ignored.', '',
              'The matrix uses goals 0–40 with normalized negligible-tail truncation (lambda cap 8). Calibration and ensemble preserve normalization/nonnegativity, Over 3.5 <= Over 2.5 and exact BTTS cell mass. Reported lambda_home/lambda_away are marginal expected goals after calibration/mixture; a mixture need not itself have Poisson marginals. Raw B uses Poisson intensities. Recency decay is 90 days in team histories and 365 days in fitting. Opponent attack/defense adjustment uses opponents’ own pre-match venue snapshots with five-game shrinkage.', '',
              'Untimestamped prices never enter features. A market prior requires both sides from the same bookmaker and capture timestamp before the decision, with age <=24h. No historical priors pass; unseen live-only prior columns are masked to avoid applying untrained market effects. Original data publication/correction vintages are unavailable; feature causality is event-time, not ingestion-vintage.', '',
              'GOAL CORE V3 REJECTED — material OOS improvement and verified usable-selection/coupon evidence are not established.']
    (out/'report.md').write_text('\n'.join(lines)+'\n', encoding='utf-8')
    Path('GOAL_CORE_V3_REPORT.md').write_text('\n'.join(lines)+'\n', encoding='utf-8')


def run(args):
    out = Path(args.output)
    out.mkdir(parents=True, exist_ok=True)
    history_bytes = Path(args.features).read_bytes()
    history = [json.loads(line) for line in history_bytes.splitlines()]
    history = sorted([r for r in history if r['date'] < EXCLUDED_START], key=lambda r:(r['date'], r['kickoff'], r['match_id']))
    rows = [r for r in history if ready(r)]
    live_all = [json.loads(line) for line in Path(args.live_features).read_bytes().splitlines()]
    live = [r for r in live_all if ready(r)]
    dbpath = Path(os.environ['APPDATA'])/'com.footballpredictor.app'/'football-predictor.sqlite3'
    db = sqlite3.connect(dbpath.as_uri()+'?mode=ro', uri=True)
    db.row_factory = sqlite3.Row
    db.execute('BEGIN')
    prices, priors, latest, audit = load_prices(db, rows+live)
    proxy, provenance = load_csv_odds(dbpath.parent/'data'/'football-data', db)
    print('Building causal features', flush=True)
    x, feature_names = causal_features(rows, history, priors)
    live_x, live_names = causal_features(live, history, priors)
    assert feature_names == live_names
    # A market unavailable throughout training cannot gain an unlearned live effect.
    all_missing = ~np.isfinite(x).any(0)
    live_x[:, all_missing] = np.nan
    raw = np.array([r['baseline_raw'] for r in rows], dtype=float)
    goals = np.array([[r['hg'], r['ag']] for r in rows])
    records = defaultdict(list)
    folds = partitions(rows)
    metadata = []
    for fold, ids in enumerate(folds, 1):
        print(f'Fold {fold}: {[len(v) for v in ids]}', flush=True)
        bundle = fit_fold(rows, x, raw, goals, ids)
        metadata.append(bundle['metadata'])
        joblib.dump(bundle, out/f'fold-{fold}.joblib')
        test = ids[4]
        p = predict(bundle, x[test], raw[test])
        for variant in VARIANTS:
            for market in MARKETS:
                for offset, i in enumerate(test):
                    r = rows[i]
                    records[(market, variant)].append(dict(match_id=r['match_id'], date=r['date'], kickoff=r['kickoff'], fold=fold,
                        p=float(p[variant][market][offset]), over35=float(p[variant]['over35'][offset]),
                        lambda_home=float(p[variant]['lambda_home'][offset]), lambda_away=float(p[variant]['lambda_away'][offset]),
                        expected_total_goals=float(p[variant]['expected_total_goals'][offset]),
                        won=int(goals[i].sum()>2) if market=='over25' else int(goals[i].min()>0),
                        odds=prices.get((r['match_id'],market)), proxy_odds=proxy.get((r['match_id'],2.5)) if market=='over25' else None))
    first = date.fromisoformat(rows[folds[0][4][0]]['date'])
    last = date.fromisoformat(rows[folds[-1][4][-1]]['date'])
    saturdays = [(first+timedelta(days=i)).isoformat() for i in range((last-first).days+1) if (first+timedelta(days=i)).weekday()==5]
    summary = evaluate(records, saturdays, out)
    result = dict(status='REJECTED', activated=False, historical_matches=len(history), ready_matches=len(rows),
                  oos_matches=sum(len(f[4]) for f in folds), saturdays_tested=len(saturdays),
                  input_sha256=hashlib.sha256(history_bytes).hexdigest(), live_input_sha256=hashlib.sha256(Path(args.live_features).read_bytes()).hexdigest(),
                  source_sha256={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in map(Path,('tools/goal_core_v3.py','tools/evaluate_goal_core_v3.py','tools/goal_model_v2.py','tools/goal_market_selection.py','src-tauri/examples/goal_v3_export.rs'))},
                  feature_names=feature_names, price_audit=audit, proxy_price_provenance=provenance, folds=metadata, markets=summary,
                  reasons=adoption(summary), historical_verified_price_count=sum(k[0] in {r['match_id'] for r in rows} for k in prices))
    # Final historical nomination: chronological 65/80/90% staging, no target days.
    days = sorted({r['date'] for r in rows})
    edges = [days[int(len(days)*p)] for p in (.65,.8,.9)]
    end_before = lambda d:(date.fromisoformat(d)-timedelta(days=2)).isoformat()
    masks = [lambda d:d<end_before(edges[0]), lambda d:edges[0]<=d<end_before(edges[1]),
             lambda d:edges[1]<=d<end_before(edges[2]), lambda d:edges[2]<=d<EXCLUDED_START]
    final_ids = [np.array([i for i,r in enumerate(rows) if f(r['date'])]) for f in masks]
    print('Fitting frozen September challenger', flush=True)
    final = fit_fold(rows, x, raw, goals, final_ids)
    joblib.dump(dict(**final, feature_names=feature_names, untrained_columns=all_missing), out/'final-research-model.joblib')
    result['final_selection'] = final['metadata']
    fixture_counts = {day:sum(r['date']==day for r in live_all) for day in ('2026-09-12','2026-09-13','2026-09-14')}
    live_result = live_report(db, live, final, live_x, latest, prices, out, fixture_counts)
    db.close()
    write(out/'evaluation.json', result)
    report(result, live_result, out)
    print(json.dumps({k:result[k] for k in ('status','oos_matches','saturdays_tested','reasons')}, indent=2), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--features', default='.tmp-dataflow/goal-v2-features.jsonl')
    parser.add_argument('--live-features', default='.tmp-dataflow/goal-v3-live-features.jsonl')
    parser.add_argument('--output', default='.tmp-dataflow/goal-v3')
    run(parser.parse_args())
