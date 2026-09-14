import math
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).parent))
from goal_market_selection import Policy, PRIORS, components, metrics, safety, score, select, split_dates, stamp, tune


def fixture(n=1):
    return dict(match_id=n, probability=.72, expected_goals=3.4, odds=1.6, ev=.152,
                attack=3.2, concession=2.7, btts=.65, league_goals=2.8, quality=.9,
                history=10, prematch_valid=True, odds_age_hours=2, date='2026-09-12', won=1)


class GoalPolicyTests(unittest.TestCase):
    def setUp(self):
        self.policy = Policy(2.5, .5, 0, 50, PRIORS[2.5])

    def test_min_five_max_seven_and_deterministic_ties(self):
        self.assertEqual(select([fixture(i) for i in range(4)], self.policy)['proposed'], [])
        selected = select([fixture(i) for i in reversed(range(10))], self.policy)['proposed']
        self.assertEqual([r['match_id'] for r in selected], list(range(7)))
        five = Policy(2.5,.5,0,50,PRIORS[2.5],5)
        self.assertEqual(len(select([fixture(i) for i in range(10)], five)['proposed']), 5)

    def test_score_cutoff_does_not_fill_with_weak_rows(self):
        strict = Policy(2.5,.5,0,99,PRIORS[2.5])
        result = select([fixture(i) for i in range(20)], strict)
        self.assertEqual(result['survivors'],20)
        self.assertEqual(result['proposed'],[])

    def test_missing_odds_never_get_fabricated_or_scored(self):
        row = fixture()
        row.update(odds=None,ev=None)
        self.assertEqual(safety(row,self.policy),'MISSING_INPUT_OR_PRICE')
        self.assertEqual(select([row],self.policy)['all_scored'],[])
        self.assertIsNone(metrics([row])['roi'])

    def test_stale_future_invalid_inputs_and_history(self):
        for update,expected in [({'odds_age_hours':24.0001},'STALE_OR_FUTURE_ODDS'),
                                ({'odds_age_hours':-1},'STALE_OR_FUTURE_ODDS'),
                                ({'prematch_valid':False},'INVALID_PREMATCH_PROVENANCE'),
                                ({'history':4},'INSUFFICIENT_HISTORY'),
                                ({'probability':math.nan},'MISSING_INPUT_OR_PRICE')]:
            self.assertEqual(safety(dict(fixture(),**update),self.policy),expected)
        self.assertIsNone(safety(dict(fixture(),odds_age_hours=24),self.policy))

    def test_rejected_rows_are_diagnostics_not_picks(self):
        row = dict(fixture(),ev=-.2)
        result = select([row],self.policy)
        self.assertEqual(result['survivors'],0)
        self.assertEqual(result['all_scored'][0]['safety_rejection'],'EV_SAFETY_FLOOR')
        self.assertEqual(result['proposed'],[])

    def test_each_requested_component_contributes(self):
        row = fixture()
        self.assertEqual(len(components(row)),9)
        for field in ('probability','expected_goals','ev','attack','concession','btts','league_goals','quality'):
            other = dict(row)
            other[field] -= .1
            self.assertLess(score(other,self.policy),score(row,self.policy),field)
        self.assertLess(score(dict(row,odds=2),self.policy),score(row,self.policy))

    def test_chronological_split_keeps_entire_dates_together(self):
        rows = [dict(fixture(i),date=f'2026-01-{i//2+1:02}') for i in range(60)]
        train,test,boundary = split_dates(rows)
        self.assertLess(max(r['date'] for r in train),min(r['date'] for r in test))
        self.assertEqual(boundary,'2026-01-22')

    def test_price_less_market_cannot_tune_or_pass(self):
        rows = [dict(fixture(i),date=f'2026-01-{i+1:02}',odds=None,ev=None) for i in range(30)]
        _, evaluation = tune(rows,3.5)
        self.assertEqual(evaluation['status'],'BLOCKED_MISSING_HISTORICAL_MARKET_ODDS')
        self.assertEqual(evaluation['trials'],0)

    def test_coupon_return_uses_actual_product_and_all_results(self):
        rows=[dict(fixture(i),odds=2,won=1) for i in range(5)]
        self.assertEqual(metrics(rows,coupon_mode=True)['coupon_roi'],31)
        rows[0]['won']=0
        self.assertEqual(metrics(rows,coupon_mode=True)['coupon_roi'],-1)
        self.assertIsNone(metrics(rows)['coupon_roi'])

    def test_timezone_and_invalid_config(self):
        self.assertEqual(stamp('2026-09-12T03:00:00+03:00'),stamp('2026-09-12T00:00:00Z'))
        for kwargs in ({'max_selections':4},{'weights':(1,)},{'weights':(math.nan,)*9}):
            params=dict(line=2.5,probability_floor=.5,ev_floor=0,score_cutoff=50,weights=PRIORS[2.5])
            params.update(kwargs)
            with self.assertRaises(ValueError):
                Policy(**params)


if __name__ == '__main__':
    unittest.main()
