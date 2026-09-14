"""Offline, temporally blocked goal-model research; never modifies production state."""
import os
for _key in ('OMP_NUM_THREADS', 'OPENBLAS_NUM_THREADS', 'MKL_NUM_THREADS'):
    os.environ[_key] = '1'
import argparse
import hashlib
import json
import math
import sqlite3
from collections import defaultdict
from datetime import date, timedelta
from pathlib import Path
import joblib
import numpy as np
from scipy.special import expit, logit
from scipy.stats import poisson
from sklearn.ensemble import HistGradientBoostingClassifier
from sklearn.impute import SimpleImputer
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import brier_score_loss, log_loss
from sklearn.pipeline import make_pipeline
from sklearn.preprocessing import StandardScaler
from goal_market_selection import load_csv_odds


def safe(value, default=0.):
    return default if value is None else float(value)


def goal_features(row):
    """Only the exported pre-decision snapshot; never reads outcomes or prices."""
    f = row['features']
    values = {}
    league = max(.5, safe(f['league']['goals_per_match'], 2.6))
    for side in ('home', 'away'):
        t = f[side]
        for window in ('overall_last5', 'overall_last10', 'venue_last5', 'venue_last10'):
            for key in ('goals_for_avg', 'goals_against_avg', 'over_2_5_rate', 'over_3_5_rate', 'btts_rate', 'clean_sheet_rate', 'failed_to_score_rate', 'sample_size'):
                values[f'{side}_{window}_{key}'] = safe(t[window].get(key), np.nan)
        for key in ('shots_for_avg', 'shots_on_target_for_avg', 'shots_against_avg', 'shots_on_target_against_avg'):
            values[f'{side}_{key}'] = safe(t['shooting_last5'].get(key), np.nan)
        opp = safe(t['average_opponent_elo_last10'], 1500.)
        adjustment = 10 ** ((opp - 1500.) / 400.)
        values[f'{side}_opponent_adjusted_attack'] = safe(t['overall_last10']['goals_for_avg'], league/2) / (league/2) * adjustment
        values[f'{side}_opponent_adjusted_defense'] = safe(t['overall_last10']['goals_against_avg'], league/2) / (league/2) / adjustment
        for metric in ('goals_for_avg', 'goals_against_avg'):
            values[f'{side}_{metric}_trend'] = safe(t['overall_last5'][metric], league/2) - safe(t['overall_last10'][metric], league/2)
        values[f'{side}_history'] = f['data_quality'][f'history_matches_{side}']
    def rate(side, metric):
        t = f[side]['venue_last10']
        n = t['sample_size']
        return (safe(t[metric], league/2)*n + league/2*5)/(n+5)
    lh = rate('home', 'goals_for_avg') * rate('away', 'goals_against_avg') / (league/2)
    la = rate('away', 'goals_for_avg') * rate('home', 'goals_against_avg') / (league/2)
    values.update(league_goals=league, league_btts=safe(f['league']['btts_rate'], np.nan),
                  poisson_home=lh, poisson_away=la, poisson_3plus=poisson.sf(2, lh+la),
                  poisson_4plus=poisson.sf(3, lh+la), poisson_btts=(1-math.exp(-lh))*(1-math.exp(-la)),
                  odds_implied_prior=np.nan)
    return values


class ProductionGoals:
    """Current goal component: train-only median/std + fixed Poisson GD, all past fit rows."""
    def fit(self, x, goals):
        self.keep = np.any(np.isfinite(x), axis=0)
        z = x[:, self.keep]
        self.median = np.nanmedian(z, axis=0)
        z = np.where(np.isfinite(z), z, self.median)
        self.mean, self.std = z.mean(0), z.std(0)
        self.variable = self.std**2 > 1e-14
        z = self.transform(x)
        self.intercept = np.log(np.maximum(goals.mean(0), .05))
        self.weights = np.zeros((z.shape[1], 2))
        for _ in range(800):
            error = np.exp(np.clip(z @ self.weights + self.intercept, -5, 5)) - goals
            self.intercept = np.clip(self.intercept - .015*error.mean(0), -5, 5)
            self.weights = np.clip(self.weights - .015*(z.T@error/len(z)+.002*self.weights), -5, 5)
        return self

    def transform(self, x):
        z = x[:, self.keep]
        missing = ~np.isfinite(z)
        z = np.where(missing, self.median, z)
        z = ((z[:, self.variable] - self.mean[self.variable])/self.std[self.variable])
        return np.stack((z, missing[:, self.variable]), axis=2).reshape(len(x), -1)

    def predict(self, x, line):
        lambdas = np.exp(np.clip(self.transform(x)@self.weights+self.intercept, -5, 5))
        return poisson.sf(int(line), lambdas.sum(1))


def calibrate_fit(p, y):
    return LogisticRegression(C=1., max_iter=1000).fit(logit(np.clip(p, 1e-6, 1-1e-6)).reshape(-1, 1), y)


def calibrate(model, p):
    return model.predict_proba(logit(np.clip(p, 1e-6, 1-1e-6)).reshape(-1, 1))[:, 1]


def metrics(y, p):
    bins = []
    for lower in np.arange(0., 1., .1):
        bin_id = np.minimum((p*10).astype(int), 9)
        mask = bin_id == round(lower*10)
        if mask.any():
            bins.append(dict(lower=float(lower), n=int(mask.sum()), probability=float(p[mask].mean()), observed=float(y[mask].mean())))
    return dict(n=len(y), brier=float(brier_score_loss(y, p)), log_loss=float(log_loss(y, p, labels=[0, 1])),
                calibration_gap=float(p.mean()-y.mean()), ece=sum(b['n']*abs(b['probability']-b['observed']) for b in bins)/len(y),
                classification_accuracy=float(np.mean((p >= .5)==y)), reliability=bins)


def returns(profits):
    if not profits:
        return dict(bets=0, roi=None, max_drawdown=None, longest_losing_streak=None)
    balance = peak = drawdown = 0.
    streak = longest = 0
    for profit in profits:
        balance += profit
        peak = max(peak, balance)
        drawdown = max(drawdown, peak-balance)
        streak = streak+1 if profit < 0 else 0
        longest = max(longest, streak)
    return dict(bets=len(profits), roi=balance/len(profits), profit_units=balance, max_drawdown=drawdown,
                longest_losing_streak=longest, hit_rate=sum(p > 0 for p in profits)/len(profits))


def coupon_day(rows):
    distinct = {}
    for row in sorted(rows, key=lambda r: (-r['p'], r['match_id'])):
        distinct.setdefault(row['match_id'], row)
    selected = list(distinct.values())[:5]
    if len(selected) < 5:
        return dict(status='INSUFFICIENT', selections=selected)
    if any(r['odds'] is None for r in selected):
        return dict(status='PRICE_MISSING', selections=selected)
    odds = math.prod(r['odds'] for r in selected)
    won = all(r['won'] for r in selected)
    return dict(status='SETTLED_PRICE_PROXY', selections=selected, combined_odds=odds, won=won, profit=odds-1 if won else -1.)


def partitions(rows):
    days = sorted(set(r['date'] for r in rows))
    edges = [int(len(days)*p) for p in (.4, .55, .7, .85, 1.)]
    result = []
    for start, stop in zip(edges, edges[1:]):
        past = days[:start]
        fit_end, tune_end, cal_end = [past[int(len(past)*p)] for p in (.65, .8, .9)]
        test_start = days[start]
        test_end = days[stop] if stop < len(days) else '9999-01-01'
        # Exclude labels in the 48 hours preceding every next-stage day.
        before = lambda d: (date.fromisoformat(d)-timedelta(days=2)).isoformat()
        masks = [lambda d: d < before(fit_end),
                 lambda d: fit_end <= d < before(tune_end),
                 lambda d: tune_end <= d < before(cal_end),
                 lambda d: cal_end <= d < before(test_start),
                 lambda d: test_start <= d < test_end]
        indices = [np.array([i for i, r in enumerate(rows) if fn(r['date'])], dtype=int) for fn in masks]
        result.append(indices)
    return result


def candidates():
    for c in (.1, 1., 10.):
        yield f'logistic_C{c}', make_pipeline(SimpleImputer(add_indicator=True, keep_empty_features=True), StandardScaler(), LogisticRegression(C=c, max_iter=2000))
    for leaves in (7, 15):
        for ridge in (5., 20.):
            yield f'boost_leaves{leaves}_L2{ridge}', make_pipeline(SimpleImputer(add_indicator=True, keep_empty_features=True), HistGradientBoostingClassifier(max_iter=150, max_leaf_nodes=leaves, l2_regularization=ridge, learning_rate=.05, early_stopping=False, random_state=20260912))


def bootstrap_mean(values):
    if not values:
        return None
    values = np.array(values)
    rng = np.random.default_rng(20260912)
    means = np.mean(values[rng.integers(0,len(values),size=(5000,len(values)))],axis=1)
    return dict(blocks=len(values), mean=float(values.mean()), lower95=float(np.quantile(means,.025)), upper95=float(np.quantile(means,.975)))


def run(args):
    out = Path(args.output)
    out.mkdir(parents=True, exist_ok=True)
    raw = Path(args.features).read_bytes()
    all_rows = [json.loads(line) for line in raw.splitlines()]
    rows = [r for r in all_rows if min(r['features']['data_quality']['history_matches_home'], r['features']['data_quality']['history_matches_away']) >= 5]
    assert all(r['history_before'] < r['decision_at'] < r['kickoff'] for r in rows)
    x = np.array([list(goal_features(r).values()) for r in rows])
    base_x = np.array([r['baseline_raw'] for r in rows], dtype=float)
    goals = np.array([[r['hg'], r['ag']] for r in rows])
    appdata = Path(os.environ['APPDATA'])/'com.footballpredictor.app'
    db = sqlite3.connect((appdata/'football-predictor.sqlite3').as_uri()+'?mode=ro', uri=True)
    prices, provenance = load_csv_odds(appdata/'data'/'football-data', db)
    team_names = dict(db.execute('SELECT id,normalized_name FROM teams'))
    db.close()
    result = dict(status='REJECTED', activated=False, feature_sha256=hashlib.sha256(raw).hexdigest(), historical_matches=len(all_rows), model_ready=len(rows),
                  feature_names=list(goal_features(rows[0])), odds=provenance, markets={}, folds=[],
                  causal_policy='Midnight Istanbul decision; history embargo 24h; stage labels embargo 48h; four expanding date-block folds; no outcome/price ranking leakage.',
                  pricing_limitation='B365 non-closing prices lack capture timestamps. All ROI is retrospective price-proxy research, not verified executable return. Odds prior masked; no closing prices used.',
                  baseline='Production Poisson goal architecture causally refit on the same past fitting rows; not the present artifact applied backward.')
    predictions = defaultdict(list)
    for fold, (fit, tune, cal, select, test) in enumerate(partitions(rows), 1):
        print(f'Fold {fold}: fit/tune/cal/select/test '+str([len(a) for a in (fit,tune,cal,select,test)]), flush=True)
        refit = np.concatenate((fit, tune))
        baseline = ProductionGoals().fit(base_x[refit], goals[refit])
        audit = dict(fold=fold, periods={name:dict(n=len(ids), first=rows[ids[0]]['date'], last=rows[ids[-1]]['date']) for name, ids in zip(('fit','tune','calibrate','select','test'), (fit,tune,cal,select,test))}, models={})
        for line in (2.5, 3.5):
            y = (goals.sum(1) > line).astype(int)
            leaderboard = []
            fitted = []
            for name, model in candidates():
                model.fit(x[fit], y[fit])
                loss = float(log_loss(y[tune], model.predict_proba(x[tune])[:,1]))
                leaderboard.append(dict(model=name, validation_log_loss=loss))
                fitted.append((loss, name, model))
            _, name, model = min(fitted, key=lambda v:(v[0],v[1]))
            model.fit(x[refit], y[refit])
            platt = calibrate_fit(model.predict_proba(x[cal])[:,1], y[cal])
            ps = model.predict_proba(x[select])[:,1]
            pcs = calibrate(platt, ps)
            bs = baseline.predict(base_x[select], line)
            alpha = min((0., .25, .5, .75, 1.), key=lambda a: log_loss(y[select], a*pcs+(1-a)*bs))
            selection_predictions = dict(dedicated=ps, calibrated=pcs, ensemble=alpha*pcs+(1-alpha)*bs)
            nominated = min(selection_predictions, key=lambda k:log_loss(y[select], selection_predictions[k]))
            p = model.predict_proba(x[test])[:,1]
            pc = calibrate(platt, p)
            b = baseline.predict(base_x[test], line)
            for variant, probability in dict(baseline=b, dedicated=p, calibrated=pc, ensemble=alpha*pc+(1-alpha)*b, nominated=dict(dedicated=p,calibrated=pc,ensemble=alpha*pc+(1-alpha)*b)[nominated]).items():
                for i, prob in zip(test, probability):
                    r = rows[i]
                    predictions[(line,variant)].append(dict(match_id=r['match_id'], date=r['date'], kickoff=r['kickoff'], home=r['home'], away=r['away'], home_name=team_names.get(r['home']), away_name=team_names.get(r['away']), p=float(prob), won=int(y[i]), odds=prices.get((r['match_id'],line)), fold=fold))
            audit['models'][str(line)] = dict(selected=name, ensemble_dedicated_weight=alpha, nominated=nominated, tuning=leaderboard)
            joblib.dump(dict(model=model, calibration=platt, baseline=baseline, ensemble_weight=alpha, nominated=nominated, feature_names=result['feature_names'], metadata=audit['periods']), out/f'fold-{fold}-over-{line}.joblib')
        result['folds'].append(audit)
    first, last = date.fromisoformat(all_rows[0]['date']), date.fromisoformat(all_rows[-1]['date'])
    saturdays = [(first+timedelta(days=n)).isoformat() for n in range((last-first).days+1) if (first+timedelta(days=n)).weekday()==5]
    for (line, variant), records in predictions.items():
        records.sort(key=lambda r:(r['date'],r['kickoff'],r['match_id']))
        y, p = np.array([r['won'] for r in records]), np.array([r['p'] for r in records])
        market = result['markets'].setdefault(str(line), {})
        item = metrics(y,p)
        # Fixed old probability floors for comparable single-bet research; no tuning on test.
        bets = [r for r in records if r['odds'] and r['p'] >= (0.64 if line==2.5 else .52) and r['p']*r['odds'] > 1.]
        item['single_bets_price_proxy'] = returns([r['odds']*r['won']-1 for r in bets])
        item['single_bets_price_proxy']['average_odds'] = float(np.mean([r['odds'] for r in bets])) if bets else None
        days = defaultdict(list)
        for r in records:
            days[r['date']].append(r)
        coupons = []
        for day in saturdays:
            c = coupon_day(days[day]) if day >= records[0]['date'] else dict(status='TRAINING_WARMUP',selections=[])
            coupons.append(dict(date=day, **c))
        item['coupons_price_proxy'] = returns([c['profit'] for c in coupons if 'profit' in c])
        complete_picks = [c for c in coupons if len(c['selections']) == 5]
        pick_wins = [all(r['won'] for r in c['selections']) for c in complete_picks]
        item['five_pick_outcomes_without_price_requirement'] = dict(days=len(pick_wins), wins=sum(pick_wins), hit_rate=sum(pick_wins)/len(pick_wins) if pick_wins else None)
        item['odds_rows'] = sum(r['odds'] is not None for r in records)
        item['latest_fold'] = metrics(y[np.array([r['fold']==4 for r in records])],p[np.array([r['fold']==4 for r in records])])
        market[variant] = item
        (out/f'over-{line}-{variant}-predictions.json').write_text(json.dumps(records),encoding='utf-8')
        (out/f'over-{line}-{variant}-saturdays.json').write_text(json.dumps(coupons,indent=2),encoding='utf-8')
    result['reasons'] = []
    for line, market in result['markets'].items():
        for variant, item in market.items():
            original = predictions[(float(line),'baseline')]
            comparison = predictions[(float(line),variant)]
            day_delta = defaultdict(list)
            for b_row, v_row in zip(original,comparison):
                assert b_row['match_id'] == v_row['match_id']
                day_delta[b_row['date']].append((v_row['p']-v_row['won'])**2-(b_row['p']-b_row['won'])**2)
            item['paired_brier_day_bootstrap'] = bootstrap_mean([np.mean(v) for v in day_delta.values()])
            base_c = json.loads((out/f'over-{line}-baseline-saturdays.json').read_text())
            var_c = json.loads((out/f'over-{line}-{variant}-saturdays.json').read_text())
            item['paired_coupon_roi_bootstrap'] = bootstrap_mean([v['profit']-b['profit'] for b,v in zip(base_c,var_c) if 'profit' in b and 'profit' in v])
        b,n = market['baseline'],market['nominated']
        if n['brier'] >= b['brier'] or n['log_loss'] >= b['log_loss']:
            result['reasons'].append(f'Over {line}: pre-nominated model does not improve both OOS Brier and log loss.')
        br,nr = b['coupons_price_proxy']['roi'],n['coupons_price_proxy']['roi']
        if nr is None or br is None or nr <= max(0.,br):
            result['reasons'].append(f'Over {line}: no demonstrated positive five-leg coupon return improvement.')
        ci = n['paired_coupon_roi_bootstrap']
        if ci and ci['lower95'] <= 0:
            result['reasons'].append(f'Over {line}: paired Saturday bootstrap does not establish a coupon advantage.')
    result['cross_market_coherence'] = {variant:sum(a['p']>b['p'] for a,b in zip(predictions[(3.5,variant)], predictions[(2.5,variant)])) for variant in result['markets']['2.5']}
    if result['cross_market_coherence']['nominated']:
        result['reasons'].append(f"Independent nominated models violate P(4+) <= P(3+) on {result['cross_market_coherence']['nominated']} OOS matches; a joint coherent model is required before activation.")
    result['reasons'].append('Historical odds lack verified pre-decision capture timestamps; Over 3.5 has no matched historical prices. Activation evidence is incomplete.')
    (out/'evaluation.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
    report = ['# Goal Market Model V2 — offline OOS evaluation', '', '**REJECTED; production remains unchanged.**','',result['causal_policy'], '', result['baseline'], '',result['pricing_limitation'],'',f"Historical matches: {len(all_rows)}; minimum-five-history ready: {len(rows)}.", '', '| Market / variant | OOS n | Brier | Log loss | ECE | Bet n | Bet ROI proxy | Coupon n | Coupon ROI proxy | Coupon max DD | Longest loss streak |','|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|']
    def percent(value): return 'N/A' if value is None else f'{value:.2%}'
    for line, market in result['markets'].items():
        for name,m in market.items():
            s,c = m['single_bets_price_proxy'],m['coupons_price_proxy']
            report.append(f"| O{line} {name} | {m['n']} | {m['brier']:.5f} | {m['log_loss']:.5f} | {m['ece']:.5f} | {s['bets']} | {percent(s['roi'])} | {c['bets']} | {percent(c['roi'])} | {c['max_drawdown']} | {c['longest_losing_streak']} |")
    report += ['', 'Rejection reasons:', *['- '+r for r in result['reasons']], '', 'Full probabilities, reliability bins, hit rates, individual drawdown, exact five selections and combined odds for every Saturday, split manifests and fitted models are in the adjacent JSON/joblib artifacts. Early Saturdays are explicitly TRAINING_WARMUP; missing prices are PRICE_MISSING, never imaginary settlements.']
    report += ['', '## Hit rates and drawdown', '', '| Market / variant | Single-bet hit rate | Mean bet odds | Single-bet max drawdown (u) | Coupon hit rate |', '|---|---:|---:|---:|---:|']
    for line, market in result['markets'].items():
        for name, m in market.items():
            s,c=m['single_bets_price_proxy'],m['coupons_price_proxy']
            report.append(f"| O{line} {name} | {percent(s.get('hit_rate'))} | {s['average_odds']} | {s['max_drawdown']} | {percent(c.get('hit_rate'))} |")
    report += ['', '## Scope and interpretation', '',
               '- Fixed one-unit stakes. Coupon returns settle all five legs together; combined odds are the product of five real recorded prices. No commission, taxes, limits or execution slippage are modeled.',
               '- Best-five probability ranking is a fixed evaluation rule, not a production publication policy or recommendation. Saturday selection is frozen before the day; no minimum coupon count is forced.',
               '- Odds-implied prior is a reserved, entirely missing feature because no qualifying capture timestamps exist. It is not synthesized from closing prices. Opponent adjustment uses prior opponent Elo relative to the league scoring baseline.',
               '- Labels and historical statistics are reconstructed from the current database. The embargo prevents event-time leakage; original publication/correction vintages are unavailable, so this is not an ingestion-vintage replay.',
               '- Baseline uses the exact production goal estimator and preprocessing, refitted on all eligible past fit+tune rows for a fair data comparison. The present production artifact and present calibration are never applied backward.',
               '- Model hyperparameters are chosen on tune rows; Platt calibration on a later separate block; ensemble weights and the nominated variant on another later block. None use that fold\'s test labels. Earlier test blocks can become training history in later folds.',
               '- The nominated variant is selected before each test block. It avoids claiming the best variant chosen after viewing pooled OOS results.',
               '- No production model, policy, database row, coupon or frontend was changed.', '',
               'Over 3.5 five-pick outcome-only checks (prices missing; these are not settled monetary coupons): ' + '; '.join(f"{name}: {m['five_pick_outcomes_without_price_requirement']['wins']}/{m['five_pick_outcomes_without_price_requirement']['days']}" for name,m in result['markets']['3.5'].items()) + '.', '',
               '## Historical odds audit', '',
               '- Local football-data CSV audit and exact fixture mappings are recorded with SHA-256 hashes in evaluation.json. Over 3.5 has zero usable matched prices.',
               '- [Football-data methodology](https://www.football-data.co.uk/downloadm.php): non-C quotes are distinct from closing prices; local files contain no per-quote capture timestamp.',
               '- [Public Club Football Match Data](https://github.com/xgabora/Club-Football-Match-Data): the documented free schema provides Over25/Under25, not the needed Over35 history.',
               '- [Footiqo public database](https://footiqo.com/database/): advertises 1xBet closing goal-line prices. Two bounded advertised-export requests returned empty bodies; the second explicitly recorded HTTP 200. No history was imported or inferred from these responses.',
               '- [Footiqo premium sample](https://footiqo.com/premium/premium-football-database/) requires email/marketing consent; no subscription or purchase was made. Closing-only data would still not establish pre-day executable coupon prices.', '',
               '## Uncertainty', '', 'Paired bootstrap resamples complete days for Brier differences and complete Saturdays for coupon-profit differences (5,000 deterministic draws). Negative Brier differences favor V2; positive coupon differences favor V2. Intervals are exploratory and do not correct for all research choices.', '', '| Market / variant | Coupon ROI difference, percentage points | 95% lower | 95% upper |', '|---|---:|---:|---:|']
    for line, market in result['markets'].items():
        for name,m in market.items():
            ci=m['paired_coupon_roi_bootstrap']
            if ci:
                report.append(f"| O{line} {name} | {100*ci['mean']:.2f} | {100*ci['lower95']:.2f} | {100*ci['upper95']:.2f} |")
    (out/'report.md').write_text('\n'.join(report)+'\n',encoding='utf-8')
    print('\n'.join(report),flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--features',default='.tmp-dataflow/goal-v2-features.jsonl')
    parser.add_argument('--output',default='.tmp-dataflow/goal-v2')
    run(parser.parse_args())
