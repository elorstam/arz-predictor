import copy
import hashlib
import json
import math
from datetime import date, timedelta
from pathlib import Path
import sqlite3
import unittest
from goal_core_v3 import (np, BTTS, OVER25, OVER35, derive, score_matrix, JointCalibrator,
                          ProductionReplay, project_production, causal_features)
from evaluate_goal_core_v3 import betting, load_prices, qualifies
from goal_model_v2 import partitions


class GoalCoreV3Tests(unittest.TestCase):
    def test_joint_mass_and_analytic_poisson_agree_including_extremes(self):
        rates = np.array([[.05,.05],[.2,4],[1.7,1.1],[8,8],[8,.05]])
        matrix = score_matrix(rates)
        p = derive(matrix)
        np.testing.assert_allclose(matrix.sum((1,2)), 1, atol=1e-14)
        np.testing.assert_allclose(p['btts'], np.prod(1-np.exp(-rates),1), atol=1e-14)
        np.testing.assert_allclose(p['expected_total_goals'], rates.sum(1), atol=1e-12)
        np.testing.assert_allclose(p['lambda_home'], rates[:,0], atol=1e-12)
        self.assertTrue(np.all(p['over35']<=p['over25']))
        self.assertTrue(np.all(matrix>=0))

    def test_invalid_intensities_and_invalid_matrices_fail_closed(self):
        for rates in ([[0,1]],[[-1,1]],[[np.nan,1]],[[np.inf,1]],[[9,1]]):
            with self.assertRaises(ValueError):
                score_matrix(rates)
        with self.assertRaises(ValueError):
            derive(score_matrix([[1,1]])*.9)

    def test_calibration_and_ensemble_preserve_exact_event_mass(self):
        rng = np.random.default_rng(3)
        rates = rng.uniform(.1,4,(100,2))
        matrix = score_matrix(rates)
        goals = rng.poisson(rates)
        calibrated = JointCalibrator().fit(matrix, goals).predict(matrix)
        projected = project_production(matrix, np.linspace(.05,.95,100), np.linspace(.95,.05,100))
        np.testing.assert_allclose(derive(projected)['over25'], np.linspace(.05,.95,100), atol=1e-8)
        np.testing.assert_allclose(derive(projected)['btts'], np.linspace(.95,.05,100), atol=1e-8)
        for m in (calibrated, projected, .75*calibrated+.25*projected):
            p = derive(m)
            np.testing.assert_array_equal(p['btts'], m[:,BTTS].sum(1))
            np.testing.assert_array_equal(p['over25'], m[:,OVER25].sum(1))
            np.testing.assert_array_equal(p['over35'], m[:,OVER35].sum(1))
            self.assertTrue(np.all(p['over35']<=p['over25']))

    def test_coupon_filters_before_ranking_and_never_forces_five(self):
        rows = [dict(match_id=i,date='2026-08-01',p=.7,odds=1.5,won=1) for i in range(7)]
        rows[0]['odds']=None
        rows[1]['p']=.99
        rows[1]['odds']=1.001  # Highest probability but negative EV.
        stats, coupons = betting(rows,['2026-08-01','2026-08-08'],'odds')
        self.assertEqual(stats['single']['qualified_bets'],5)
        self.assertEqual(stats['coupons']['saturdays_tested'],2)
        self.assertEqual(coupons[0]['combined_odds'],1.5**5)
        self.assertEqual(coupons[0]['profit'],1.5**5-1)
        rows[2]['p']=.63
        _, coupons = betting(rows*2,['2026-08-01'],'odds')
        self.assertEqual(coupons[0]['picks'],[])
        self.assertEqual(coupons[0]['valid_selections'],4)
        self.assertFalse(qualifies(dict(p=.8,odds=float('nan'))))

    def test_odds_prior_rejects_future_stale_and_mismatched_captures(self):
        db = sqlite3.connect(':memory:')
        db.row_factory = sqlite3.Row
        db.execute('CREATE TABLE iddaa_model_odds(id INTEGER,match_id INTEGER,model_market_type TEXT,model_selection TEXT,line_value REAL,odd REAL,captured_at TEXT,provider TEXT)')
        rows = [(1,1,'BTTS','YES',None,2.,'2026-08-01T11:00:00Z','iddaa'),
                (2,1,'BTTS','NO',None,2.,'2026-08-01T11:00:00Z','iddaa'),
                (3,1,'BTTS','YES',None,9.,'2026-08-02T11:00:00Z','iddaa'),
                (4,2,'BTTS','YES',None,2.,'2026-07-30T11:00:00Z','iddaa'),
                (5,3,'BTTS','YES',None,2.,'2026-08-01T10:00:00Z','iddaa'),
                (6,3,'BTTS','NO',None,2.,'2026-08-01T11:00:00Z','iddaa')]
        db.executemany('INSERT INTO iddaa_model_odds VALUES(?,?,?,?,?,?,?,?)',rows)
        targets = [dict(match_id=i,decision_at='2026-08-01T12:00:00Z') for i in (1,2,3)]
        prices,priors,latest,_ = load_prices(db,targets)
        self.assertEqual(prices[(1,'btts')],2.)
        self.assertEqual(priors,{(1,'btts'):.5})
        self.assertNotIn((2,'btts'),prices)
        self.assertFalse(latest[(2,'btts')]['valid'])
        db.close()

    def test_future_and_target_outcomes_do_not_change_history_features(self):
        source = Path('.tmp-dataflow/goal-v2-features.jsonl')
        if not source.exists():
            self.skipTest('Historical export unavailable')
        with source.open() as handle:
            history = [json.loads(next(handle)) for _ in range(80)]
        target = history[40]
        before,_ = causal_features([target],history)
        poisoned = copy.deepcopy(history)
        for row in poisoned:
            if row['kickoff'] >= target['history_before']:
                row.update(hg=999,ag=999)
        altered = copy.deepcopy(target)
        altered.update(hg=1000,ag=1000,odds=10000)
        after,_ = causal_features([altered],poisoned)
        np.testing.assert_allclose(before,after,equal_nan=True)

    def test_production_binary_and_platt_match_scalar_reference(self):
        x=np.array([[0.],[1.],[2.]])
        goals=np.array([[1,0],[2,2],[3,0]])
        model=ProductionReplay().fit(x,goals)
        z=model.transform(x)
        w=np.zeros(z.shape[1]); b=0.
        for _ in range(700):
            errors=[1/(1+math.exp(-(b+sum(a*c for a,c in zip(w,row)))))-int(min(g)>0) for row,g in zip(z,goals)]
            b-=.08*sum(errors)/3
            w=np.array([v-.08*(sum(z[i,j]*errors[i] for i in range(3))/3+.002*v) for j,v in enumerate(w)])
        np.testing.assert_allclose(w,model.binary_weights,atol=1e-12)
        self.assertAlmostEqual(b,model.binary_intercept,places=12)
        p=np.array([.2,.7,.8]); y=np.array([0.,0.,1.])
        a,b=1.,0.
        for _ in range(500):
            logits=[math.log(q/(1-q)) for q in p]
            err=[1/(1+math.exp(-max(-35,min(35,a*z+b))))-t for z,t in zip(logits,y)]
            a=max(.05,min(5,a-.03*sum(e*z for e,z in zip(err,logits))/3))
            b=max(-5,min(5,b-.03*sum(err)/3))
        np.testing.assert_allclose(model.platt(p,y),(a,b),atol=1e-12)

    def test_partitions_and_saved_artifacts(self):
        rows=[dict(date=(date(2020,1,1)+timedelta(days=i)).isoformat()) for i in range(1000)]
        seen=set()
        for ids in partitions(rows):
            for a,b in zip(ids,ids[1:]):
                self.assertLess(rows[a[-1]]['date'],(date.fromisoformat(rows[b[0]]['date'])-timedelta(days=2)).isoformat())
            self.assertFalse(seen.intersection(ids[-1])); seen.update(ids[-1])
        root=Path('.tmp-dataflow/goal-v3')
        if not (root/'evaluation.json').exists():
            self.skipTest('Evaluation artifacts unavailable')
        result=json.loads((root/'evaluation.json').read_text())
        self.assertEqual(result['historical_verified_price_count'],0)
        self.assertFalse(result['activated'])
        for filename, expected in result['source_sha256'].items():
            self.assertEqual(hashlib.sha256(Path(filename).read_bytes()).hexdigest(),expected)
        for fold in result['folds']:
            self.assertLess(fold['periods']['test']['last'],'2026-09-12')
            p=fold['production_calibration_periods']
            self.assertLess(p['fit']['last'],(date.fromisoformat(p['validate']['first'])-timedelta(days=2)).isoformat())
        for variant in ('A_production','B_joint','C_calibrated','D_ensemble','N_nominated'):
            predictions=json.loads((root/f'over25-{variant}-predictions.json').read_text())
            self.assertEqual(len(predictions),result['oos_matches'])
            self.assertEqual(len({r['match_id'] for r in predictions}),len(predictions))
            self.assertTrue(all(r['over35']<=r['p']+1e-14 for r in predictions))
        for path in root.glob('*-saturdays.json'):
            for coupon in json.loads(path.read_text()):
                picks=coupon['picks']
                self.assertEqual(len(picks),len(set(r['match_id'] for r in picks)))
                self.assertIn(len(picks),(0,5))
                if picks:
                    price='proxy_odds' if 'price_proxy' in path.name else 'odds'
                    self.assertTrue(all(qualifies(r,price) for r in picks))
                    odd=math.prod(r[price] for r in picks)
                    self.assertAlmostEqual(odd,coupon['combined_odds'])
                    self.assertAlmostEqual(coupon['profit'],odd-1 if all(r['won'] for r in picks) else -1)


if __name__ == '__main__':
    unittest.main()
