"""Shared joint score distribution. Research only: no production writes."""
import os
for _key in ('OMP_NUM_THREADS', 'OPENBLAS_NUM_THREADS', 'MKL_NUM_THREADS'):
    os.environ[_key] = '1'

import math
from collections import defaultdict
import numpy as np
from scipy.optimize import minimize
from scipy.special import expit, logit
from scipy.stats import poisson
from sklearn.impute import SimpleImputer
from sklearn.linear_model import PoissonRegressor
from sklearn.pipeline import make_pipeline
from sklearn.preprocessing import StandardScaler
from goal_model_v2 import ProductionGoals, goal_features
from goal_market_selection import stamp

SCORES = np.arange(41)
TOTAL = SCORES[:, None] + SCORES[None, :]
OVER25 = TOTAL >= 3
OVER35 = TOTAL >= 4
BTTS = (SCORES[:, None] > 0) & (SCORES[None, :] > 0)
GROUP = 2 * OVER25.astype(int) + BTTS.astype(int)
MASKS = np.array([GROUP == i for i in range(4)])


def score_matrix(lambdas):
    """0..40 goals, normalized truncation; omitted tail <3e-16 at lambda<=8."""
    lambdas = np.asarray(lambdas, dtype=float)
    if lambdas.ndim != 2 or lambdas.shape[1] != 2 or not np.isfinite(lambdas).all() or np.any(lambdas <= 0) or np.any(lambdas > 8):
        raise ValueError('Expected positive finite home/away intensities <=8')
    p = poisson.pmf(SCORES[None, :, None], lambdas[:, None, :])
    p /= p.sum(axis=1, keepdims=True)
    return p[:, :, 0, None] * p[:, None, :, 1]


def derive(matrix):
    matrix = np.asarray(matrix)
    if matrix.ndim != 3 or matrix.shape[1:] != (41, 41) or not np.isfinite(matrix).all() or np.any(matrix < 0) or not np.allclose(matrix.sum((1, 2)), 1, atol=1e-12, rtol=0):
        raise ValueError('Invalid score probability matrix')
    result = dict(over25=matrix[:, OVER25].sum(1), over35=matrix[:, OVER35].sum(1),
                  btts=matrix[:, BTTS].sum(1), expected_total_goals=(matrix * TOTAL).sum((1, 2)),
                  lambda_home=(matrix * SCORES[None, :, None]).sum((1, 2)),
                  lambda_away=(matrix * SCORES[None, None, :]).sum((1, 2)))
    assert np.all(result['over35'] <= result['over25'] + 1e-14)
    return result


def group_mass(matrix):
    return np.stack([matrix[:, mask].sum(1) for mask in MASKS], axis=1)


def reweight(matrix, target):
    factors = target / np.maximum(group_mass(matrix), 1e-300)
    out = matrix * factors[:, GROUP]
    return out / out.sum((1, 2), keepdims=True)


def project_production(matrix, over25, btts):
    """IPF embeds production's two marginals into one score distribution.

    Production has no joint BTTS distribution. Preserve within-cell conditional
    score probabilities from the joint model; change only the four cell masses.
    """
    q = group_mass(matrix).reshape(-1, 2, 2)
    a, b = np.clip(over25, 1e-8, 1-1e-8), np.clip(btts, 1e-8, 1-1e-8)
    for _ in range(1000):
        q *= np.stack((1-a, a), 1)[:, :, None] / q.sum(2)[:, :, None]
        q *= np.stack((1-b, b), 1)[:, None, :] / q.sum(1)[:, None, :]
        if np.max(np.abs(q[:, 1, :].sum(1)-a)) < 1e-11:
            break
    if np.max(np.abs(q[:, 1, :].sum(1)-a)) > 1e-8:
        raise ValueError('Production projection did not converge')
    return reweight(matrix, q.reshape(-1, 4))


class JointCalibrator:
    """Two-parameter exponential tilt, fitted on a separate historical block.

    Unlike independent Platt calibration this preserves a normalized joint PMF.
    Objective is observed joint event-cell log loss with fixed L2 shrinkage.
    """
    design = np.array([[0, 0], [0, 1], [1, 0], [1, 1]])

    def fit(self, matrix, goals):
        q = group_mass(matrix)
        y = 2 * (goals.sum(1) >= 3).astype(int) + (goals.min(1) > 0)
        def objective(theta):
            tilted = q * np.exp(self.design @ theta)
            tilted /= tilted.sum(1, keepdims=True)
            loss = -np.log(np.maximum(tilted[np.arange(len(y)), y], 1e-15)).mean() + .01 * (theta @ theta)
            grad = (tilted @ self.design - self.design[y]).mean(0) + .02 * theta
            return loss, grad
        fit = minimize(objective, np.zeros(2), jac=True, method='L-BFGS-B', bounds=[(-2, 2)]*2)
        if not fit.success:
            raise RuntimeError(f'Calibration failed: {fit.message}')
        self.theta = fit.x
        return self

    def predict(self, matrix):
        q = group_mass(matrix) * np.exp(self.design @ self.theta)
        return reweight(matrix, q / q.sum(1, keepdims=True))


class ProductionReplay(ProductionGoals):
    """Production GLM preprocessing/GD + logistic/Poisson BTTS blend replay.

    Current active artifacts/calibration cannot be applied to past fixtures.
    This causally refits the architecture; it is not a deployment-vintage replay.
    """
    def fit(self, x, goals):
        super().fit(x, goals)
        z = self.transform(x)
        y = (goals.min(1) > 0).astype(float)
        self.binary_weights, self.binary_intercept = np.zeros(z.shape[1]), 0.
        for _ in range(700):
            error = expit(z @ self.binary_weights + self.binary_intercept) - y
            self.binary_intercept -= .08 * error.mean()
            self.binary_weights -= .08 * (z.T @ error / len(z) + .002 * self.binary_weights)
        self.blend = .5
        self.public_calibration = {}
        return self

    def components(self, x):
        z = self.transform(x)
        lam = np.exp(np.clip(z @ self.weights + self.intercept, -5, 5))
        binary = expit(np.clip(z @ self.binary_weights + self.binary_intercept, -35, 35))
        return lam, binary, np.prod(1-np.exp(-lam), axis=1)

    def choose_blend(self, x, goals):
        _, binary, p = self.components(x)
        y = (goals.min(1) > 0).astype(float)
        def loss(w):
            q = np.clip(w*binary+(1-w)*p, 1e-8, 1-1e-8)
            return -(y*np.log(q)+(1-y)*np.log1p(-q)).mean()
        self.blend = min(np.arange(21)*.05, key=loss)

    def probabilities(self, x):
        lam, binary, p = self.components(x)
        result = dict(over25=poisson.sf(2, lam.sum(1)), over35=poisson.sf(3, lam.sum(1)),
                    btts=self.blend*binary+(1-self.blend)*p, lambda_home=lam[:, 0],
                    lambda_away=lam[:, 1], expected_total_goals=lam.sum(1))
        for market in ('over25', 'over35', 'btts'):
            param = self.public_calibration.get('BTTS' if market == 'btts' else 'TOTAL_GOALS')
            if param and param['enabled']:
                result[market] = self.cal_binary(result[market], param['a'], param['b'])
        return result

    @staticmethod
    def cal_binary(p, a, b):
        return expit(np.clip(a*logit(np.clip(p, 1e-9, 1-1e-9))+b, -35, 35))

    @staticmethod
    def platt(p, y):
        z = logit(np.clip(p, 1e-9, 1-1e-9))
        a, b = 1., 0.
        for _ in range(500):
            error = expit(np.clip(a*z+b, -35, 35))-y
            a = float(np.clip(a-.03*np.mean(error*z), .05, 5))
            b = float(np.clip(b-.03*error.mean(), -5, 5))
        return a, b

    def fit_public_calibration(self, fit_x, fit_goals, valid_x, valid_goals):
        """Production family Platt GD, log-loss/ECE acceptance and raw fallback.

        Family samples include complementary selections and 1.5/2.5/3.5 lines,
        as in production backtest observations. ECE uses production 5% buckets.
        """
        def samples(x, goals, family):
            lam, binary, p = self.components(x)
            ps = [self.blend*binary+(1-self.blend)*p] if family=='BTTS' else [poisson.sf(k, lam.sum(1)) for k in (1,2,3)]
            ys = [goals.min(1)>0] if family=='BTTS' else [goals.sum(1)>k for k in (1,2,3)]
            return np.concatenate([q for p in ps for q in (p,1-p)]), np.concatenate([q for y in ys for q in (y,~y)]).astype(float)
        def quality(p, y):
            p = np.clip(p, 1e-9, 1-1e-9)
            ids = np.minimum((p*20).astype(int), 19)
            ece = sum(abs(np.sum(p[ids==i]-y[ids==i])) for i in range(20))/len(y)
            loss = -(y*np.log(p)+(1-y)*np.log1p(-p)).mean()
            return float(loss), float(ece)
        for family in ('TOTAL_GOALS','BTTS'):
            p, y = samples(fit_x, fit_goals, family)
            pv, yv = samples(valid_x, valid_goals, family)
            a, b = self.platt(p, y)
            raw_loss, raw_ece = quality(pv, yv)
            cal_loss, cal_ece = quality(self.cal_binary(pv,a,b), yv)
            self.public_calibration[family] = dict(a=a, b=b, enabled=cal_loss<=raw_loss and cal_ece<=raw_ece+.02,
                fit_samples=len(y), validation_samples=len(yv), raw_log_loss=raw_loss, calibrated_log_loss=cal_loss,
                raw_ece=raw_ece, calibrated_ece=cal_ece)


def causal_features(targets, history, priors=None):
    """Actual elapsed-day decay and opponents' pre-match scoring/conceding rates.

    Targets never update history. Each past opponent baseline comes from that
    historical fixture's own frozen snapshot, not from a future team rating.
    """
    priors = priors or {}
    by_team = defaultdict(list)
    for r in sorted(history, key=lambda r: (r['kickoff'], r['match_id'])):
        if r.get('hg') is None or r.get('ag') is None:
            continue
        for side, other, gf, ga in [('home', 'away', r['hg'], r['ag']), ('away', 'home', r['ag'], r['hg'])]:
            f = r['features']
            league = f['league']['goals_per_match'] or 2.6
            opp = f[other]['venue_last10']
            n = opp['sample_size']
            smoothed = lambda key: ((opp.get(key) if opp.get(key) is not None else league/2)*n + league/2*5)/(n+5)
            by_team[r[side]].append((stamp(r['kickoff']), gf, ga, side, gf/max(.2, smoothed('goals_against_avg')), ga/max(.2, smoothed('goals_for_avg'))))
    result = []
    for r in targets:
        if not stamp(r['features']['cutoff_at']) <= stamp(r['history_before']) < stamp(r['decision_at']) < stamp(r['kickoff']):
            raise ValueError('Noncausal snapshot')
        f = r['features']
        v = goal_features(r)
        v.pop('odds_implied_prior')
        league = f['league']['goals_per_match'] or 2.6
        lh = f['league']['home_goals_per_match'] or league*.55
        la = f['league']['away_goals_per_match'] or league*.45
        v.update(league_home_goals=lh, league_away_goals=la, home_advantage=math.log(lh/la))
        for side in ('home', 'away'):
            past = [p for p in by_team[r[side]] if p[0] < stamp(r['history_before'])]
            for venue in (False, True):
                subset = [p for p in past if not venue or p[3] == side]
                for window in (5, 10):
                    recent = subset[-window:]
                    weights = np.array([2**(-max(0, (stamp(r['decision_at'])-p[0]).total_seconds()/86400)/90) for p in recent])
                    prefix = f'{side}_{"venue" if venue else "all"}_decay{window}'
                    for index, key, prior in [(1, 'scoring', league/2), (2, 'conceding', league/2), (4, 'adjusted_attack', 1.), (5, 'adjusted_defense', 1.)]:
                        v[f'{prefix}_{key}'] = (sum(w*p[index] for w, p in zip(weights, recent)) + 5*prior)/(weights.sum()+5)
                    v[f'{prefix}_effective_history'] = weights.sum()
        for market in ('over25', 'btts'):
            prior = priors.get((r['match_id'], market))
            v[f'{market}_market_prior'] = prior if prior is not None else np.nan
        result.append(v)
    return np.array([list(v.values()) for v in result]), list(result[0])


class JointGoals:
    def __init__(self, ridge=.3):
        self.ridge = ridge

    def fit(self, x, goals, days):
        age = np.array([(max(days)-d).days for d in days])
        weights = 2.**(-age/365.)
        self.models = []
        for side in range(2):
            model = make_pipeline(SimpleImputer(add_indicator=True, keep_empty_features=True), StandardScaler(), PoissonRegressor(alpha=self.ridge, max_iter=1000, tol=1e-7))
            model.fit(x, goals[:, side], poissonregressor__sample_weight=weights)
            self.models.append(model)
        return self

    def lambdas(self, x):
        return np.clip(np.stack([m.predict(x) for m in self.models], 1), .05, 8.)

    def predict(self, x):
        return score_matrix(self.lambdas(x))
