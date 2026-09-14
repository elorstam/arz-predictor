"""Read-only goal coupon policy research. Never writes application tables or publishes.

Usage: python tools/goal_market_selection.py --output .tmp-dataflow/goal-selection
Only real recorded prices enter ROI/EV. Missing 3.5 prices remain missing.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import html
import itertools
import json
import math
import os
from pathlib import Path
import random
import sqlite3
from collections import Counter, defaultdict
from dataclasses import asdict, dataclass
from datetime import datetime, timedelta, timezone

COMPONENTS = ("probability", "goal_distribution", "ev", "odds", "attack", "concession", "btts", "league", "quality")
PRIORS = {2.5: (.28, .12, .15, .08, .10, .08, .08, .06, .05),
          3.5: (.26, .18, .15, .06, .12, .10, .03, .06, .04)}


def stamp(s):
    value = datetime.fromisoformat(s.replace("Z", "+00:00"))
    return (value if value.tzinfo else value.replace(tzinfo=timezone.utc)).astimezone(timezone.utc)


def business_date(s):
    return (stamp(s) + timedelta(hours=3)).date().isoformat()


def clip(x):
    return max(0., min(1., x))


def finite(x):
    return isinstance(x, (int, float)) and math.isfinite(x)


@dataclass(frozen=True)
class Policy:
    line: float
    probability_floor: float
    ev_floor: float
    score_cutoff: float
    weights: tuple
    max_selections: int = 7
    version: str = "goal-rank-research-v1"

    def __post_init__(self):
        if self.line not in (2.5, 3.5) or not 5 <= self.max_selections <= 7:
            raise ValueError("invalid goal market or publication size")
        if len(self.weights) != len(COMPONENTS) or any(not finite(w) or w < 0 for w in self.weights) or abs(sum(self.weights)-1) > 1e-9:
            raise ValueError("weights must be nonnegative, finite and sum to one")
        if not 0 <= self.probability_floor <= 1 or not 0 <= self.score_cutoff <= 100 or not finite(self.ev_floor):
            raise ValueError("invalid safety or score floor")


def components(row):
    """All features use pre-match snapshots; quality is known before this outcome."""
    return (row["probability"], clip(row["expected_goals"] / 6),
            clip(.5 + row["ev"]), clip(1 / row["odds"]),
            clip(row["attack"] / 5), clip(row["concession"] / 5),
            row["btts"], clip(row["league_goals"] / 4), row["quality"])


def safety(row, policy):
    required = ("probability", "expected_goals", "odds", "ev", "attack", "concession", "btts", "league_goals", "quality")
    if any(not finite(row.get(k)) for k in required):
        return "MISSING_INPUT_OR_PRICE"
    if not 0 <= row["probability"] <= 1 or not 0 <= row["btts"] <= 1:
        return "INVALID_PROBABILITY"
    if not row.get("prematch_valid", False):
        return "INVALID_PREMATCH_PROVENANCE"
    if row.get("history", 0) < 5:
        return "INSUFFICIENT_HISTORY"
    if not 1 < row["odds"] <= 8:
        return "INVALID_OR_EXTREME_ODDS"
    if row.get("odds_age_hours") is not None and not 0 <= row["odds_age_hours"] <= 24:
        return "STALE_OR_FUTURE_ODDS"
    if row["probability"] < policy.probability_floor:
        return "PROBABILITY_SAFETY_FLOOR"
    if row["ev"] < policy.ev_floor:
        return "EV_SAFETY_FLOOR"
    return None


def score(row, policy):
    return 100 * sum(w * x for w, x in zip(policy.weights, components(row)))


def select(rows, policy, diagnostics=True):
    reasons = Counter()
    survivors = []
    scored = []
    seen = set()
    for row in rows:
        reason = safety(row, policy)
        inputs = ("probability", "expected_goals", "ev", "odds", "attack", "concession", "btts", "league_goals", "quality")
        if diagnostics and all(finite(row.get(k)) for k in inputs) and row['odds'] > 1:
            scored.append(dict(row, score=score(row, policy), safety_rejection=reason))
        if reason:
            reasons[reason] += 1
            continue
        if row["match_id"] in seen:
            raise ValueError("duplicate fixture in goal-market input")
        seen.add(row["match_id"])
        survivors.append(dict(row, score=score(row, policy)))
    survivors.sort(key=lambda r: (-r["score"], -r["probability"], r["match_id"]))
    scored.sort(key=lambda r: (-r["score"], -r["probability"], r["match_id"]))
    above = [r for r in survivors if r["score"] >= policy.score_cutoff]
    proposed = above[:policy.max_selections] if len(above) >= 5 else []
    return {"survivors": len(survivors), "score_survivors": len(above),
            "rejections": dict(reasons), "ranked_top10": survivors[:10], "all_scored": scored,
            "proposed": proposed}


def old_selection(rows, line):
    return [r for r in rows if finite(r.get("odds")) and r["odds"] > 1
            and r["probability"] >= (0.64 if line == 2.5 else .52) and r["ev"] > 0
            and r["prematch_valid"]]


def daily_selections(rows, policy):
    days = defaultdict(list)
    for r in rows:
        days[r["date"]].append(r)
    return [r for date in sorted(days) for r in select(days[date], policy, diagnostics=False)["proposed"]]


def metrics(rows, bootstrap=False, coupon_mode=False):
    if not rows:
        return {"bets": 0, "hit_rate": None, "roi": None, "average_odds": None,
                "brier": None, "calibration_gap": None, "coupons": 0, "coupon_roi": None}
    mean = lambda f: sum(f(r) for r in rows) / len(rows)
    priced = all(finite(r.get("odds")) for r in rows)
    result = {"bets": len(rows), "hit_rate": mean(lambda r: r["won"]),
              "roi": mean(lambda r: r["won"] * r["odds"] - 1) if priced else None,
              "average_odds": mean(lambda r: r["odds"]) if priced else None,
              "brier": mean(lambda r: (r["probability"] - r["won"]) ** 2),
              "calibration_gap": mean(lambda r: r["probability"] - r["won"])}
    days = defaultdict(list)
    for r in rows:
        days[r["date"]].append(r)
    # These are coupon returns only when rows are already policy-selected 5–7.
    coupons = [v for v in days.values() if 5 <= len(v) <= 7] if coupon_mode else []
    result.update(coupons=len(coupons), coupon_roi=(sum(math.prod(r["odds"] for r in v) * all(r["won"] for r in v) - 1 for v in coupons) / len(coupons)) if coupons and priced else None)
    if bootstrap and priced:
        groups = [(len(v), sum(r["won"] * r["odds"] - 1 for r in v)) for v in days.values()]
        rng = random.Random(20260912)
        samples = []
        for _ in range(2000):
            draw = [rng.choice(groups) for _ in groups]
            samples.append(sum(x[1] for x in draw) / sum(x[0] for x in draw))
        samples.sort()
        result["roi_day_bootstrap_95"] = [samples[50], samples[1949]]
    return result


def weight_grid(line):
    base = PRIORS[line]
    yield base
    # Small predeclared family: trade probability weight against each contextual signal.
    # No holdout-driven search expansion.
    for target in (1, 2, 4, 5, 7, 8):
        for delta in (-.03, .06):
            w = list(base)
            w[0] -= delta
            w[target] += delta
            yield tuple(w)


def split_dates(rows):
    dates = sorted({r["date"] for r in rows})
    if len(dates) < 20:
        raise ValueError("insufficient chronological dates")
    boundary = dates[int(len(dates) * .70)]
    return ([r for r in rows if r["date"] < boundary],
            [r for r in rows if r["date"] >= boundary], boundary)


def tune(rows, line):
    development, holdout, boundary = split_dates(rows)
    floors = (.45, .50, .55) if line == 2.5 else (.30, .35, .40)
    prices = sum(finite(r.get("odds")) for r in development)
    experimental = Policy(line, floors[1], 0., 55., PRIORS[line])
    if prices == 0:
        return experimental, {"status": "BLOCKED_MISSING_HISTORICAL_MARKET_ODDS", "boundary": boundary,
                             "development_rows": len(development), "holdout_rows": len(holdout),
                             "development_priced": 0, "trials": 0,
                             "probability_only_old_threshold": metrics([r for r in holdout if r["probability"] >= (.64 if line == 2.5 else .52)]),
                             "note": "Probability-only cohort is NOT the old EV-qualified betting policy. ROI and average odds are unavailable."}
    best = None
    frontier = {}
    trials = 0
    for weights, floor, ev, cutoff, limit in itertools.product(weight_grid(line), floors, (-.10, -.05, 0., .02), (45., 50., 55., 60.), (5, 6, 7)):
        p = Policy(line, floor, ev, cutoff, weights, limit)
        selected = daily_selections(development, p)
        m = metrics(selected, coupon_mode=True)
        trials += 1
        if m["bets"] < 150 or m["coupons"] < 20:
            continue
        # Shrink uncertain mean returns, penalize miscalibration, no volume reward.
        variance = sum((r["won"] * r["odds"] - 1 - m["roi"]) ** 2 for r in selected) / len(selected)
        objective = m["roi"] - 1.96 * math.sqrt(variance / len(selected)) - .25 * abs(m["calibration_gap"])
        key = (objective, -sum(abs(a-b) for a,b in zip(weights, PRIORS[line])), floor, ev, cutoff)
        if str(ev) not in frontier or objective > frontier[str(ev)]['development_objective']:
            frontier[str(ev)] = {'development_objective': objective, 'policy': asdict(p), 'metrics': m}
        if best is None or key > best[0]:
            best = (key, p, m)
    if best is None:
        best = (None, experimental, metrics(daily_selections(development, experimental), coupon_mode=True))
    chosen = best[1]
    selected_holdout = daily_selections(holdout, chosen)
    new = metrics(selected_holdout, True, coupon_mode=True)
    old = metrics(old_selection(holdout, line), True)
    old_days = defaultdict(list)
    for row in old_selection(holdout, line):
        old_days[row["date"]].append(row)
    def old_score(r):
        return .55*r['probability'] + .25*clip(r['probability']-1/r['odds']) + .15*clip(r['ev']) + .01*(r['history']>=5)
    old_coupon_rows = [r for day in old_days.values() if len(day) >= 5
                       for r in sorted(day, key=lambda x: (-old_score(x), x["match_id"]))[:7]]
    counts = Counter(r['date'] for r in holdout)
    saturdays = {d for d,n in counts.items() if n>=20 and datetime.fromisoformat(d).weekday()==5}
    selected_days = {r['date'] for r in selected_holdout}
    # Locked before reading final holdout outcomes. Conservative evidence requirement.
    reasons = []
    if new["bets"] < 150 or new["coupons"] < 30:
        reasons.append("INSUFFICIENT_HOLDOUT_BETS_OR_COUPONS")
    if new.get("roi_day_bootstrap_95", [-1])[0] <= 0:
        reasons.append("POSITIVE_ROI_NOT_SUPPORTED_BY_DAY_BLOCK_INTERVAL")
    if new["roi"] is None or old["roi"] is not None and new["roi"] < old["roi"]:
        reasons.append("ROI_BELOW_OLD_POLICY")
    if new["calibration_gap"] is None or abs(new["calibration_gap"]) > .10:
        reasons.append("CALIBRATION_GAP_EXCEEDS_10_PERCENTAGE_POINTS")
    if new["coupon_roi"] is None or new["coupon_roi"] < 0:
        reasons.append("COUPON_RETURNS_NOT_ACCEPTABLE")
    # Historical source differs from the live bookmaker; never authorize a production
    # override from this read-only research command, even if proxy holdout succeeds.
    return chosen, {"status": "OOS_REJECTED" if reasons else "PROXY_PASSED_REQUIRES_IDDAA_VALIDATION",
                    "reasons": reasons, "boundary": boundary, "trials": trials,
                    "development_rows": len(development), "holdout_rows": len(holdout), "development_priced": prices,
                    "development_selected": best[2], "old_threshold": old, "ranking": new,
                    "development_ev_floor_frontier": frontier,
                    "large_saturdays": {'minimum_model_ready':20,'dates':len(saturdays),'ranking_coupon_days':len(saturdays & selected_days),
                                        'old_threshold_coupon_days':len(saturdays & {r['date'] for r in old_coupon_rows})},
                    "holdout_ranking_bets": selected_holdout,
                    "holdout_old_bets": old_selection(holdout, line),
                    "holdout_old_common_coupon_bets": old_coupon_rows,
                    "old_threshold_5_to_7_coupon_rule": metrics(old_coupon_rows, True, coupon_mode=True)}


def load_csv_odds(root, db):
    mapping = dict(db.execute("SELECT external_match_id,match_id FROM provider_match_mappings WHERE provider='football-data.co.uk'"))
    prices, files, conflicts = {}, [], []
    for path in sorted(root.glob("*/*.csv")):
        season_code = path.parent.name
        if len(season_code) != 4 or not season_code.isdigit():
            continue
        season = '20' + season_code[:2] + '/' + season_code[2:]
        raw = path.read_bytes()
        try:
            text = raw.decode('utf-8-sig')
        except UnicodeDecodeError:
            text = raw.decode('cp1252')
        reader = csv.DictReader(text.splitlines())
        headers = reader.fieldnames or []
        files.append({"file": str(path), "sha256": hashlib.sha256(raw).hexdigest(),
                      "over_25": 'B365>2.5' in headers, "over_35": 'B365>3.5' in headers})
        for r in reader:
            date = None
            for form in ('%d/%m/%Y', '%d/%m/%y'):
                try:
                    date = datetime.strptime(r.get('Date', ''), form).date().isoformat()
                    break
                except ValueError:
                    pass
            if date is None or not r.get('HomeTeam') or not r.get('AwayTeam'):
                continue
            identity = '|'.join(' '.join(x.split()).lower() for x in ('football-data.co.uk', path.stem, season, date, r['HomeTeam'], r['AwayTeam']))
            match = mapping.get(hashlib.sha256(identity.encode()).hexdigest())
            if match is None:
                continue
            for line in (2.5, 3.5):
                try:
                    odd = float(r.get(f'B365>{line}', ''))
                except (ValueError, TypeError):
                    continue
                key = (match, line)
                if key in prices and prices[key] != odd:
                    conflicts.append(key)
                else:
                    prices[key] = odd
    for key in conflicts:
        prices.pop(key, None)
    return prices, {"files": files, "matched_prices": len(prices), "conflicts_excluded": len(set(conflicts)),
                    "source": "football-data.co.uk B365 pre-closing quotes; exact provider fixture hash; not Iddaa, not closing C columns"}


def feature_inputs(feature, p, btts, lambdas, quality):
    home, away = feature['home']['overall_last10'], feature['away']['overall_last10']
    pair = lambda key: (home[key] + away[key]) if finite(home.get(key)) and finite(away.get(key)) else None
    return {"probability": p, "expected_goals": sum(lambdas) if all(finite(x) for x in lambdas) else None,
            "attack": pair('goals_for_avg'), "concession": pair('goals_against_avg'),
            "btts": btts, "league_goals": feature['league']['goals_per_match'],
            "history": min(home['sample_size'], away['sample_size']), "quality": quality}


def load_data(db, cache):
    prices, provenance = load_csv_odds(cache, db)
    features = defaultdict(list)
    for r in db.execute('SELECT match_id,feature_json,calculated_at FROM feature_sets'):
        f = json.loads(r['feature_json'])
        f['_calculated_at'] = r['calculated_at']
        features[r['match_id']].append(f)
    for fs in features.values():
        fs.sort(key=lambda f: stamp(f['cutoff_at']))
    def feature_at(match, cutoff):
        return next((f for f in reversed(features[match]) if stamp(f['cutoff_at']) <= stamp(cutoff)), None)
    names = {r['id']: dict(r) for r in db.execute('SELECT m.id,m.kickoff_at,h.normalized_name home,a.normalized_name away FROM matches m JOIN teams h ON h.id=m.home_team_id JOIN teams a ON a.id=m.away_team_id')}
    run = db.execute("SELECT id FROM backtest_runs WHERE status='COMPLETED' ORDER BY id DESC LIMIT 1").fetchone()[0]
    data = [dict(r) for r in db.execute("SELECT b.*,m.kickoff_at FROM backtest_predictions b JOIN matches m ON m.id=b.match_id WHERE run_id=? AND (market='BTTS' AND selection='YES' OR market='TOTAL_GOALS' AND selection='OVER' AND line_value IN(2.5,3.5)) ORDER BY julianday(m.kickoff_at),b.id", (run,))]
    btts = {r['match_id']: r['raw_probability'] for r in data if r['market'] == 'BTTS'}
    historical = {2.5: [], 3.5: []}
    # Expanding, previous-day-only OOS reliability. Never include today's labels.
    buckets = defaultdict(lambda: [0, 0.])
    day_updates, last_date = [], None
    for r in data:
        if r['market'] != 'TOTAL_GOALS':
            continue
        day = business_date(r['kickoff_at'])
        if last_date != day:
            for key, won in day_updates:
                buckets[key][0] += 1
                buckets[key][1] += won
            day_updates, last_date = [], day
        p, line = r['raw_probability'], r['line_value']
        key = (line, min(9, int(p * 10)))
        n, wins = buckets[key]
        observed = (wins + 20 * p) / (n + 20)
        quality = (1 - abs(p - observed)) * (.5 + .5 * n / (n + 50))
        won = 1 if r['settlement'] == 'WON' else 0
        f = feature_at(r['match_id'], r['generated_at'])
        if f and r['settlement'] in ('WON', 'LOST'):
            row = dict(feature_inputs(f, p, btts.get(r['match_id']), (r['base_home_lambda'], r['base_away_lambda']), quality),
                       match_id=r['match_id'], date=day, kickoff=r['kickoff_at'], won=won,
                       odds=prices.get((r['match_id'], line)), odds_age_hours=None,
                       home=names[r['match_id']]['home'], away=names[r['match_id']]['away'],
                       feature_cutoff=f['cutoff_at'], prediction_cutoff=r['generated_at'],
                       prematch_valid=stamp(r['generated_at']) <= stamp(r['kickoff_at']) and r['out_of_sample'] == 1)
            row['ev'] = row['probability'] * row['odds'] - 1 if finite(row['odds']) else None
            historical[line].append(row)
        day_updates.append((key, won))
    live = {}
    for day in ('2026-09-12', '2026-09-13', '2026-09-14'):
        header = db.execute('SELECT r.* FROM candidate_engine_runs r JOIN daily_output_publications d ON d.candidate_run_id=r.id WHERE d.business_date=?', (day,)).fetchone()
        if header is None:
            continue
        cutoff = header['generated_at']
        predictions = [dict(r) for r in db.execute("""WITH ranked AS (SELECT p.*,pr.base_home_lambda,pr.base_away_lambda,m.kickoff_at,
          row_number() OVER(PARTITION BY p.match_id,p.market,p.selection,p.line_value ORDER BY julianday(p.created_at) DESC,p.id DESC) rn
          FROM predictions p JOIN prediction_runs pr ON pr.id=p.prediction_run_id JOIN matches m ON m.id=p.match_id
          WHERE m.scheduled_local_date=? AND julianday(p.created_at)<=julianday(?)
          AND p.model_version_id=(SELECT id FROM model_versions WHERE is_active=1)
          AND (p.market='BTTS' AND p.selection='YES' OR p.market='TOTAL_GOALS' AND p.selection='OVER' AND p.line_value IN(2.5,3.5))) SELECT * FROM ranked WHERE rn=1""", (day, cutoff))]
        live_btts = {r['match_id']: r['public_probability'] for r in predictions if r['market'] == 'BTTS'}
        live[day] = {2.5: [], 3.5: []}
        for r in predictions:
            if r['market'] != 'TOTAL_GOALS':
                continue
            # A forecast snapshot can target a future kickoff but must already have
            # been computed at the selected live as-of. Its target cutoff is not
            # its actual creation time (prediction_runs has the same convention).
            available_features = [f for f in features[r['match_id']]
                                  if stamp(f['_calculated_at']) <= stamp(cutoff)
                                  and stamp(f['cutoff_at']) <= stamp(r['kickoff_at'])]
            f = max(available_features, key=lambda f: stamp(f['_calculated_at']), default=None)
            if f is None:
                continue
            odd = db.execute("SELECT id,odd,captured_at FROM iddaa_model_odds WHERE provider='iddaa' AND match_id=? AND model_market_type='TOTAL_GOALS' AND model_selection='OVER' AND line_value=? AND julianday(captured_at)<=julianday(?) ORDER BY julianday(captured_at) DESC,id DESC LIMIT 1", (r['match_id'], r['line_value'], cutoff)).fetchone()
            p = r['public_probability'] if r['public_probability'] is not None else r['model_probability']
            n = r['bucket_sample_size'] or 0
            quality = (1 - abs(r['bucket_calibration_gap'] or 0)) * (.5 + .5 * n / (n + 50))
            row = dict(feature_inputs(f, p, live_btts.get(r['match_id']), (r['base_home_lambda'], r['base_away_lambda']), quality),
                       match_id=r['match_id'], prediction_id=r['id'], date=day, kickoff=r['kickoff_at'],
                       odds=odd['odd'] if odd else None, odds_id=odd['id'] if odd else None,
                       odds_age_hours=(stamp(cutoff)-stamp(odd['captured_at'])).total_seconds()/3600 if odd else None,
                       home=names[r['match_id']]['home'], away=names[r['match_id']]['away'],
                       feature_cutoff=f['cutoff_at'], prediction_cutoff=r['created_at'], as_of=cutoff,
                       calibration_status=r['calibration_status'],
                       prematch_valid=r['availability']=='AVAILABLE' and stamp(cutoff)<stamp(r['kickoff_at']))
            row['ev'] = p * row['odds'] - 1 if odd else None
            live[day][r['line_value']].append(row)
    provenance['backtest_run'] = run
    provenance['historical_rows'] = {line: len(rows) for line, rows in historical.items()}
    provenance['historical_priced'] = {line: sum(finite(r['odds']) for r in rows) for line, rows in historical.items()}
    return historical, live, provenance


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    app = Path(os.environ.get('APPDATA', '')) / 'com.footballpredictor.app'
    parser.add_argument('--database', type=Path, default=app/'football-predictor.sqlite3')
    parser.add_argument('--cache', type=Path, default=app/'data'/'football-data')
    parser.add_argument('--output', type=Path, default=Path('.tmp-dataflow/goal-selection'))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    db = sqlite3.connect(args.database.resolve().as_uri()+'?mode=ro', uri=True)
    db.row_factory = sqlite3.Row
    historical, live, provenance = load_data(db, args.cache)
    db.close()
    result = {"mode": "RESEARCH_ONLY_NO_PUBLICATION", "provenance": provenance, "markets": {}}
    for line in (2.5, 3.5):
        print('Evaluating', line, 'rows', len(historical[line]), 'priced', provenance['historical_priced'][line], flush=True)
        policy, evaluation = tune(historical[line], line)
        days = {}
        for day, markets in live.items():
            rows = markets[line]
            selection = select(rows, policy)
            # Always leave production publication behind the acceptance decision.
            days[day] = dict(selection, model_ready=len(rows), old_qualified=len(old_selection(rows,line)),
                             published=[], adoption_status=evaluation['status'])
        result['markets'][str(line)] = {"policy": asdict(policy), "evaluation": evaluation, "days": days}
    encoded = json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False)
    (args.output/'evaluation.json').write_text(encoded, encoding='utf-8')
    with (args.output/'oos-bets.csv').open('w',newline='',encoding='utf-8') as file:
        fields = ['market','policy','date','match_id','home','away','probability','odds','ev','score','won','return_per_unit']
        writer = csv.DictWriter(file,fieldnames=fields)
        writer.writeheader()
        for line, market in result['markets'].items():
            for policy, key in [('ranking','holdout_ranking_bets'),('old_threshold','holdout_old_bets'),('old_threshold_common_5_to_7','holdout_old_common_coupon_bets')]:
                for row in market['evaluation'].get(key,[]):
                    writer.writerow(dict({k:row.get(k) for k in fields},market=line,policy=policy,return_per_unit=row['won']*row['odds']-1))
    render_report(result, args.output)
    print(json.dumps({k: {'status':v['evaluation']['status'], 'holdout':v['evaluation'].get('ranking'), 'days':{d:{'ready':x['model_ready'],'survivors':x['survivors'],'proposed':len(x['proposed']),'published':0} for d,x in v['days'].items()}} for k,v in result['markets'].items()},indent=2))


def render_report(result, output):
    def number(v):
        return f'{v:.4f}' if isinstance(v, float) else str(v) if v is not None else 'N/A'
    def table(headers, rows):
        return '\n| '+' | '.join(headers)+' |\n| '+' | '.join('---' for _ in headers)+' |\n'+'\n'.join('| '+' | '.join(number(v) for v in row)+' |' for row in rows)+'\n'
    text = '# Dedicated goal-market ranking evaluation\n\nResearch only. No application tables, candidate rules or coupons were modified.\n\n'
    text += 'Historical odds: exact provider fixture hash matches to cached football-data.co.uk **B365 pre-closing Over 2.5** columns. They are not Iddaa odds. Closing and maximum-bookmaker columns are not used. Missing Over 3.5 odds are never derived from model probabilities. [Provider collection methodology](https://www.football-data.co.uk/downloadm.php).\n\n'
    text += 'Public goal probabilities use the existing OOS-validated raw fallback, because the active goal calibration adjustment is not beneficial. No probability transform is fitted here. Chronological day split: first 70% development, last 30% holdout. Whole dates stay together. All weights/floors/cutoffs are chosen on development only; the holdout cannot retune them. The historical confidence feature uses only outcomes from earlier days.\n\n'
    text += 'Nine score inputs (weighted sum, 0–100): public probability, expected-goals total/6, clipped 0.5+EV, inverse decimal odds, combined recent attack/5, combined concessions/5, BTTS Yes probability, league goals/4, reliability/sample quality. Goal distribution and probability are correlated, so the bounded weight family limits their influence. Recent profiles use each side’s last ten matches. No post-match stats enter the score.\n\n'
    text += 'Safety requires finite inputs, valid prematch provenance, five historical matches per team, odds (1,8], fresh non-future live odds ≤24h, and the market-specific probability/EV floors. Ranking then applies one learned score cutoff, selecting at most seven and publishing only when at least five remain. Research proposals are explicitly separate from published coupons.\n\n'
    text += 'Acceptance was fixed before holdout evaluation: ≥150 holdout bets, ≥30 coupons, positive lower 95% day-block bootstrap ROI bound, ROI no worse than old thresholds, calibration gap ≤10 percentage points and nonnegative actual accumulator ROI. Passing proxy bookmaker evidence still requires Iddaa validation before live adoption. These evidence gates do not reward simply increasing coupon counts.\n'
    for line, market in result['markets'].items():
        text += f'\n## Over {line}\n\n**{market["evaluation"]["status"]}**\n\n'
        p = market['policy']
        text += f"Probability floor {p['probability_floor']}; EV floor {p['ev_floor']}; score cutoff {p['score_cutoff']}/100; target {p['max_selections']} selections (minimum 5). "
        text += 'OOS-tuned experimental policy, not adopted.\n' if line=='2.5' else '**Untuned provisional policy**: historical 3.5 prices are absent. These weights/floors have no ROI validation.\n'
        text += table(['Component','Weight'],zip(COMPONENTS,market['policy']['weights']))
        ev = market['evaluation']
        text += f"\nOOS date boundary: {ev['boundary']}. Development: {ev['development_rows']} rows; holdout: {ev['holdout_rows']} rows. Parameter trials: {ev['trials']}. Rejection reasons: "+', '.join(ev.get('reasons',[]))+'\n'
        if 'ranking' in ev:
            text += 'Single-bet ROI 95% day-block interval: '+str(ev['ranking'].get('roi_day_bootstrap_95'))+'.\n'
            text += table(['EV floor','Best development ROI','Bets','Probability floor','Target selections'],
                          [[floor,v['metrics']['roi'],v['metrics']['bets'],v['policy']['probability_floor'],v['policy']['max_selections']] for floor,v in ev['development_ev_floor_frontier'].items()])
            text += f"\nLarge OOS Saturdays (≥20 model-ready): {ev['large_saturdays']['dates']}; ranking produces a coupon on {ev['large_saturdays']['ranking_coupon_days']}, old thresholds on {ev['large_saturdays']['old_threshold_coupon_days']}. This volume improvement does not establish profitable coupons.\n"
        else:
            cohort=ev['probability_only_old_threshold']
            text += f"\nProbability-only Over 3.5 cohort above 0.52: {cohort['bets']} fixtures, hit rate {cohort['hit_rate']:.4f}, Brier {cohort['brier']:.4f}. This omits the EV gate because prices are missing; it is **not** the old betting policy. Full-policy bet count, ROI and mean odds cannot be established.\n"
        text += table(['Approach','Bets','Hit rate','ROI','Average odds','Brier','Calibration gap','5–7 coupons','Coupon ROI'],
                      [[label]+[ev.get(key,{}).get(k) for k in ('bets','hit_rate','roi','average_odds','brier','calibration_gap','coupons','coupon_roi')]
                       for label,key in [('Old thresholds (single bets)','old_threshold'),('Old thresholds, 5–7 rule','old_threshold_5_to_7_coupon_rule'),('Dedicated rank, 5–7 rule','ranking')]])
        text += table(['Date','Model-ready','Old qualified','Safety survivors','Score survivors','Research 5–7','Published'],
                      [[d,x['model_ready'],x['old_qualified'],x['survivors'],x['score_survivors'],len(x['proposed']),len(x['published'])] for d,x in market['days'].items()])
        for day, selection in market['days'].items():
            text += f'\n### {day}: ranked top 10\n\nRejections: `{json.dumps(selection["rejections"])}`. '
            text += 'Research selection IDs: '+str([r['match_id'] for r in selection['proposed']])+'. Published: none (adoption gate).\n'
            text += table(['Rank','Match','Probability','Odds','EV','Goal score','Proposed'],
                          [[i+1,r['home']+' — '+r['away'],r['probability'],r['odds'],r['ev'],r['score'],r in selection['proposed']] for i,r in enumerate(selection['ranked_top10'])])
            text += '\nTop 10 scored model-ready fixtures, **including excluded diagnostics** (not a coupon):\n'
            text += table(['Rank','Match','Probability','Odds','EV','Goal score','Safety rejection'],
                          [[i+1,r['home']+' — '+r['away'],r['probability'],r['odds'],r['ev'],r['score'],r['safety_rejection'] or 'SURVIVES'] for i,r in enumerate(selection['all_scored'][:10])])
    text += '\n## Reproduction and limits\n\n`python tools/goal_market_selection.py --output .tmp-dataflow/goal-selection`\n\nThe output JSON includes source file hashes, feature/prediction cutoffs, live odds IDs, score inputs, chosen weights and excluded reasons. `oos-bets.csv` records each historical evaluated bet and actual unit return. Historical CSV prices lack an exact collection timestamp per fixture: they support a bookmaker-proxy research backtest, not an exact replay of the live Iddaa feed. The old comparison under a common 5–7 rule uses the existing general score to truncate to seven; this is explicitly a standardized comparison, since the old goal coupon generator had no seven-selection cap. Brier/gap comparisons use different selected cohorts; they are not a claim of model recalibration.\n\nThe OOS model has already been studied previously. An initial diagnostic also evaluated a seven-selection-only policy on the later dates before the research family included 5/6/7 lengths and mild negative-EV floors. The optimizer never uses holdout labels, but this is retrospective analysis, not a claim of a pristine prospective trial. No profitable 5–7-leg product is assumed from single-bet results.\n\nTen targeted unit tests cover publication bounds, stale/future/missing prices, score inputs, diagnostics, determinism, chronological separation and the missing-price adoption block. No Rust/frontend or application code changed, so application builds were not rerun for this research-only implementation.\n'
    (output/'report.md').write_text(text, encoding='utf-8')
    (output/'report.html').write_text('<!doctype html><meta charset="utf-8"><title>ARZ goal ranking research</title><style>body{max-width:1300px;margin:40px auto;font:15px/1.6 system-ui;color:#173633}pre{white-space:pre-wrap;overflow-wrap:anywhere}strong{color:#923b1e}</style><h1>ARZ goal ranking research — unpublished</h1><pre>'+html.escape(text)+'</pre>', encoding='utf-8')


if __name__ == '__main__':
    main()
