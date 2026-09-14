import copy
import json
import math
import unittest
from datetime import date, timedelta
from pathlib import Path
import numpy as np
from goal_model_v2 import ProductionGoals, coupon_day, goal_features, metrics, partitions, returns


class GoalV2Tests(unittest.TestCase):
    def selections(self, n=5):
        return [dict(match_id=i, p=.8-i*.01, odds=1.5, won=1) for i in range(n)]

    def test_exact_five_real_settlement(self):
        result = coupon_day(self.selections(7))
        self.assertEqual(len(result['selections']), 5)
        self.assertAlmostEqual(result['combined_odds'], 1.5**5)
        self.assertAlmostEqual(result['profit'], 1.5**5-1)
        rows = self.selections()
        rows[2]['won'] = 0
        self.assertEqual(coupon_day(rows)['profit'], -1.)

    def test_short_and_duplicate_pools_never_settle(self):
        self.assertEqual(coupon_day(self.selections(4))['status'], 'INSUFFICIENT')
        self.assertEqual(coupon_day(self.selections(4)*2)['status'], 'INSUFFICIENT')

    def test_missing_odds_never_invented_or_reranked(self):
        rows = self.selections(6)
        rows[0]['odds'] = None
        self.assertEqual(coupon_day(rows)['status'], 'PRICE_MISSING')
        before = [r['match_id'] for r in coupon_day(rows)['selections']]
        rows[-1]['odds'] = 100.
        self.assertEqual(before, [r['match_id'] for r in coupon_day(rows)['selections']])

    def test_drawdown_and_losing_streak(self):
        result = returns([-1, -1, 5, -1, -1, -1])
        self.assertEqual(result['max_drawdown'], 3)
        self.assertEqual(result['longest_losing_streak'], 3)
        self.assertIsNone(returns([])['roi'])

    def test_every_fold_has_temporal_embargo(self):
        rows = [dict(date=(date(2020,1,1)+timedelta(days=i//2)).isoformat()) for i in range(1000)]
        seen = set()
        for blocks in partitions(rows):
            for earlier,later in zip(blocks,blocks[1:]):
                self.assertLess(rows[earlier[-1]]['date'], (date.fromisoformat(rows[later[0]]['date'])-timedelta(days=2)).isoformat())
            self.assertFalse(seen.intersection(blocks[-1]))
            seen.update(blocks[-1])

    def test_train_only_preprocessing_and_poisson_coherence(self):
        x = np.array([[1.,np.nan], [2.,3.], [3.,4.], [4.,5.]])
        y = np.array([[1,0],[2,1],[1,2],[2,0]])
        model = ProductionGoals().fit(x,y)
        mean = model.mean.copy()
        model.predict(np.array([[10000.,10000.]]),2.5)
        np.testing.assert_array_equal(model.mean,mean)
        p25,p35 = model.predict(x,2.5),model.predict(x,3.5)
        self.assertTrue(np.all(p25 >= p35))
        self.assertTrue(np.all((p25>0)&(p25<1)))

    def test_poisson_training_matches_scalar_reference(self):
        x = np.array([[0.],[1.],[2.]])
        y = np.array([[0,1],[1,2],[3,0]])
        model = ProductionGoals().fit(x,y)
        z=model.transform(x)
        weights=np.zeros(z.shape[1]); intercept=np.log(y[:,0].mean())
        for _ in range(800):
            error=[np.exp(np.clip(intercept+sum(a*b for a,b in zip(row,weights)),-5,5))-target for row,target in zip(z,y[:,0])]
            intercept=np.clip(intercept-.015*sum(error)/3,-5,5)
            weights=np.array([np.clip(w-.015*(sum(z[i,j]*error[i] for i in range(3))/3+.002*w),-5,5) for j,w in enumerate(weights)])
        np.testing.assert_allclose(weights,model.weights[:,0],atol=1e-12)
        self.assertAlmostEqual(intercept,model.intercept[0],places=12)

    def test_calibration_bins_count_each_row_once(self):
        m=metrics(np.array([0,1,0,1]),np.array([.1,.2,.3,1.]))
        self.assertEqual(sum(b['n'] for b in m['reliability']),4)

    def test_future_labels_and_prices_cannot_enter_feature_vector(self):
        source=Path('.tmp-dataflow/goal-v2-features.jsonl')
        if not source.exists():
            self.skipTest('Production research export not present')
        with source.open(encoding='utf-8') as f:
            row=json.loads(next(f))
        poisoned=copy.deepcopy(row)
        poisoned.update(hg=9999,ag=9999,odds=10000,final_result='WON')
        np.testing.assert_allclose(list(goal_features(row).values()),list(goal_features(poisoned).values()),equal_nan=True)

    def test_saved_saturday_artifacts_have_real_five_leg_settlements(self):
        root=Path('.tmp-dataflow/goal-v2')
        files=list(root.glob('*-saturdays.json'))
        if not files:
            self.skipTest('OOS artifacts not present')
        self.assertEqual(len(files),10)
        for file in files:
            for coupon in json.loads(file.read_text()):
                picks=coupon['selections']
                self.assertEqual(len({r['match_id'] for r in picks}),len(picks))
                if 'profit' in coupon:
                    self.assertEqual(len(picks),5)
                    self.assertAlmostEqual(coupon['combined_odds'],math.prod(r['odds'] for r in picks))
                    self.assertEqual(coupon['won'],all(r['won'] for r in picks))
                    self.assertAlmostEqual(coupon['profit'],coupon['combined_odds']-1 if coupon['won'] else -1.)


if __name__ == '__main__':
    unittest.main()
