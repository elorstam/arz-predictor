use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

pub const FEATURE_ENGINE_VERSION: &str = "fe_v1";
pub const LABEL_VERSION: &str = "label_v1";
pub const MIN_SAMPLE: usize = 3;
pub const STRONG_SAMPLE: usize = 5;
pub const ELO_INITIAL: f64 = 1500.0;
pub const ELO_K: f64 = 20.0;
pub const ELO_HOME_ADVANTAGE: f64 = 60.0;
// V1 Elo is continuous across seasons: no final-table information and no
// season reset/regression are applied. Ratings update only after each finished
// match in chronological order.

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateWindow {
    pub sample_size: usize,
    pub wins: usize,
    pub draws: usize,
    pub losses: usize,
    pub points_per_match: Option<f64>,
    pub goals_for_avg: Option<f64>,
    pub goals_against_avg: Option<f64>,
    pub goal_difference_avg: Option<f64>,
    pub clean_sheet_rate: Option<f64>,
    pub failed_to_score_rate: Option<f64>,
    pub over_1_5_rate: Option<f64>,
    pub over_2_5_rate: Option<f64>,
    pub over_3_5_rate: Option<f64>,
    pub btts_rate: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShootingWindow {
    pub sample_size: usize,
    pub shots_for_avg: Option<f64>,
    pub shots_against_avg: Option<f64>,
    pub shots_on_target_for_avg: Option<f64>,
    pub shots_on_target_against_avg: Option<f64>,
    pub shot_accuracy: Option<f64>,
    pub opponent_shot_accuracy: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CornerWindow {
    pub sample_size: usize,
    pub corners_for_avg: Option<f64>,
    pub corners_against_avg: Option<f64>,
    pub total_match_corners_avg: Option<f64>,
    pub over_7_5_rate: Option<f64>,
    pub over_8_5_rate: Option<f64>,
    pub over_9_5_rate: Option<f64>,
    pub over_10_5_rate: Option<f64>,
    pub over_11_5_rate: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CardWindow {
    pub sample_size: usize,
    pub yellow_for_avg: Option<f64>,
    pub yellow_against_avg: Option<f64>,
    pub red_for_avg: Option<f64>,
    // Raw-card convention: yellow count + red count. This is not bookmaker
    // card points and must not be interpreted as such.
    pub total_team_cards_avg: Option<f64>,
    pub total_match_yellow_avg: Option<f64>,
    pub total_match_red_avg: Option<f64>,
    pub over_2_5_yellow_rate: Option<f64>,
    pub over_3_5_yellow_rate: Option<f64>,
    pub over_4_5_yellow_rate: Option<f64>,
    pub over_5_5_yellow_rate: Option<f64>,
    pub over_6_5_yellow_rate: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FoulWindow {
    pub sample_size: usize,
    pub fouls_committed_avg: Option<f64>,
    pub fouls_drawn_avg: Option<f64>,
    pub combined_match_fouls_avg: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TeamFeatures {
    pub overall_last5: RateWindow,
    pub overall_last10: RateWindow,
    pub venue_last5: RateWindow,
    pub venue_last10: RateWindow,
    pub shooting_last5: ShootingWindow,
    pub shooting_last10: ShootingWindow,
    pub venue_shooting_last5: ShootingWindow,
    pub corners_last5: CornerWindow,
    pub corners_last10: CornerWindow,
    pub venue_corners_last5: CornerWindow,
    pub cards_last5: CardWindow,
    pub cards_last10: CardWindow,
    pub fouls_last5: FoulWindow,
    pub fouls_last10: FoulWindow,
    pub days_since_last_match: Option<f64>,
    pub matches_in_last_7_days: usize,
    pub matches_in_last_14_days: usize,
    pub average_opponent_elo_last5: Option<f64>,
    pub average_opponent_elo_last10: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LeagueBaseline {
    pub sample_size: usize,
    pub goals_per_match: Option<f64>,
    pub home_goals_per_match: Option<f64>,
    pub away_goals_per_match: Option<f64>,
    pub home_win_rate: Option<f64>,
    pub draw_rate: Option<f64>,
    pub away_win_rate: Option<f64>,
    pub btts_rate: Option<f64>,
    pub over_2_5_rate: Option<f64>,
    pub average_total_corners: Option<f64>,
    pub average_total_yellow_cards: Option<f64>,
    pub average_shots: Option<f64>,
    pub average_shots_on_target: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct H2hFeatures {
    pub sample_size: usize,
    pub goals_avg: Option<f64>,
    pub btts_rate: Option<f64>,
    pub over_2_5_rate: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataQuality {
    pub history_matches_home: usize,
    pub history_matches_away: usize,
    pub shots_coverage: f64,
    pub corners_coverage: f64,
    pub cards_coverage: f64,
    pub fouls_coverage: f64,
    pub kickoff_quality: String,
    pub elo_available: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeatureSnapshot {
    pub match_id: i64,
    pub feature_engine_version: String,
    pub cutoff_at: String,
    pub competition_id: i64,
    pub season: String,
    pub home_team_id: i64,
    pub away_team_id: i64,
    pub home: TeamFeatures,
    pub away: TeamFeatures,
    pub home_elo: f64,
    pub away_elo: f64,
    pub elo_difference: f64,
    pub expected_home_result: f64,
    pub league: LeagueBaseline,
    pub h2h: H2hFeatures,
    pub home_matches_played_competition_season: usize,
    pub away_matches_played_competition_season: usize,
    pub data_quality: DataQuality,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainingLabel {
    pub match_id: i64,
    pub match_result: String,
    pub total_goals: i64,
    pub over_1_5: bool,
    pub over_2_5: bool,
    pub over_3_5: bool,
    pub btts: bool,
    pub home_over_0_5: bool,
    pub home_over_1_5: bool,
    pub away_over_0_5: bool,
    pub away_over_1_5: bool,
    pub total_corners: Option<i64>,
    pub corner_thresholds: Option<BTreeMap<String, bool>>,
    pub total_yellow_cards: Option<i64>,
    pub card_thresholds: Option<BTreeMap<String, bool>>,
}

#[derive(Clone)]
struct Hist {
    id: i64,
    competition: i64,
    season: String,
    home: i64,
    away: i64,
    kick: String,
    date: Option<String>,
    known: bool,
    hg: i64,
    ag: i64,
    shots: Option<(i64, i64)>,
    sot: Option<(i64, i64)>,
    corners: Option<(i64, i64)>,
    fouls: Option<(i64, i64)>,
    yellow: Option<(i64, i64)>,
    red: Option<(i64, i64)>,
}
struct Target {
    id: i64,
    competition: i64,
    season: String,
    home: i64,
    away: i64,
    kick: String,
    date: Option<String>,
    known: bool,
}

fn parse_time(v: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(v)
        .ok()
        .map(|v| v.with_timezone(&Utc))
}
fn parse_date(v: Option<&str>) -> Option<NaiveDate> {
    v.and_then(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
}
fn eligible(target: &Target, h: &Hist) -> bool {
    if h.id == target.id {
        return false;
    }
    if target.known {
        if h.known {
            match (parse_time(&h.kick), parse_time(&target.kick)) {
                (Some(a), Some(b)) => a < b,
                _ => false,
            }
        } else {
            match (
                parse_date(h.date.as_deref()),
                parse_date(target.date.as_deref()),
            ) {
                (Some(a), Some(b)) => a < b,
                _ => false,
            }
        }
    } else {
        match (
            parse_date(h.date.as_deref()),
            parse_date(target.date.as_deref()),
        ) {
            (Some(a), Some(b)) => a < b,
            _ => false,
        }
    }
}
fn avg(sum: f64, n: usize) -> Option<f64> {
    (n >= MIN_SAMPLE).then_some(sum / n as f64)
}
fn rate(n: usize, count: usize) -> Option<f64> {
    (n >= MIN_SAMPLE).then_some(count as f64 / n as f64)
}
fn side(h: &Hist, team: i64) -> (i64, i64, bool) {
    if h.home == team {
        (h.hg, h.ag, true)
    } else {
        (h.ag, h.hg, false)
    }
}
fn pair(v: Option<(i64, i64)>, home: bool) -> Option<(i64, i64)> {
    v.map(|(a, b)| if home { (a, b) } else { (b, a) })
}
fn recent<'a>(items: &'a [&Hist], n: usize) -> Vec<&'a Hist> {
    items.iter().rev().take(n).copied().collect()
}

fn form(items: &[&Hist], team: i64, n: usize) -> RateWindow {
    let x = recent(items, n);
    let mut w = 0;
    let mut d = 0;
    let mut l = 0;
    let mut gf = 0;
    let mut ga = 0;
    let mut cs = 0;
    let mut fts = 0;
    let mut o15 = 0;
    let mut o25 = 0;
    let mut o35 = 0;
    let mut btts = 0;
    for h in &x {
        let (a, b, _) = side(h, team);
        gf += a;
        ga += b;
        if a > b {
            w += 1
        } else if a == b {
            d += 1
        } else {
            l += 1
        };
        if b == 0 {
            cs += 1
        };
        if a == 0 {
            fts += 1
        };
        let t = a + b;
        if t > 1 {
            o15 += 1
        };
        if t > 2 {
            o25 += 1
        };
        if t > 3 {
            o35 += 1
        };
        if a > 0 && b > 0 {
            btts += 1
        }
    }
    let z = x.len();
    RateWindow {
        sample_size: z,
        wins: w,
        draws: d,
        losses: l,
        points_per_match: avg((w * 3 + d) as f64, z),
        goals_for_avg: avg(gf as f64, z),
        goals_against_avg: avg(ga as f64, z),
        goal_difference_avg: avg((gf - ga) as f64, z),
        clean_sheet_rate: rate(z, cs),
        failed_to_score_rate: rate(z, fts),
        over_1_5_rate: rate(z, o15),
        over_2_5_rate: rate(z, o25),
        over_3_5_rate: rate(z, o35),
        btts_rate: rate(z, btts),
    }
}
fn shooting(items: &[&Hist], team: i64, n: usize) -> ShootingWindow {
    let mut sf = 0;
    let mut sa = 0;
    let mut stf = 0;
    let mut sta = 0;
    let mut count = 0;
    for h in recent(items, n) {
        let home = h.home == team;
        if let (Some((a, b)), Some((c, d))) = (pair(h.shots, home), pair(h.sot, home)) {
            sf += a;
            sa += b;
            stf += c;
            sta += d;
            count += 1
        }
    }
    ShootingWindow {
        sample_size: count,
        shots_for_avg: avg(sf as f64, count),
        shots_against_avg: avg(sa as f64, count),
        shots_on_target_for_avg: avg(stf as f64, count),
        shots_on_target_against_avg: avg(sta as f64, count),
        shot_accuracy: (count >= MIN_SAMPLE && sf > 0).then_some(stf as f64 / sf as f64),
        opponent_shot_accuracy: (count >= MIN_SAMPLE && sa > 0).then_some(sta as f64 / sa as f64),
    }
}
fn corners(items: &[&Hist], team: i64, n: usize) -> CornerWindow {
    let mut f = 0;
    let mut a = 0;
    let mut count = 0;
    let mut thresholds = [0; 5];
    for h in recent(items, n) {
        if let Some((x, y)) = pair(h.corners, h.home == team) {
            f += x;
            a += y;
            count += 1;
            for (i, t) in [7, 8, 9, 10, 11].iter().enumerate() {
                if x + y > *t {
                    thresholds[i] += 1
                }
            }
        }
    }
    CornerWindow {
        sample_size: count,
        corners_for_avg: avg(f as f64, count),
        corners_against_avg: avg(a as f64, count),
        total_match_corners_avg: avg((f + a) as f64, count),
        over_7_5_rate: rate(count, thresholds[0]),
        over_8_5_rate: rate(count, thresholds[1]),
        over_9_5_rate: rate(count, thresholds[2]),
        over_10_5_rate: rate(count, thresholds[3]),
        over_11_5_rate: rate(count, thresholds[4]),
    }
}
fn cards(items: &[&Hist], team: i64, n: usize) -> CardWindow {
    let mut yf = 0;
    let mut ya = 0;
    let mut rf = 0;
    let mut rt = 0;
    let mut yt = 0;
    let mut count = 0;
    let mut over = [0; 5];
    for h in recent(items, n) {
        if let (Some((a, b)), Some((c, d))) =
            (pair(h.yellow, h.home == team), pair(h.red, h.home == team))
        {
            yf += a;
            ya += b;
            rf += c;
            rt += c + d;
            yt += a + b;
            count += 1;
            for (i, t) in [2, 3, 4, 5, 6].iter().enumerate() {
                if a + b > *t {
                    over[i] += 1
                }
            }
        }
    }
    CardWindow {
        sample_size: count,
        yellow_for_avg: avg(yf as f64, count),
        yellow_against_avg: avg(ya as f64, count),
        red_for_avg: avg(rf as f64, count),
        total_team_cards_avg: avg((yf + rf) as f64, count),
        total_match_yellow_avg: avg(yt as f64, count),
        total_match_red_avg: avg(rt as f64, count),
        over_2_5_yellow_rate: rate(count, over[0]),
        over_3_5_yellow_rate: rate(count, over[1]),
        over_4_5_yellow_rate: rate(count, over[2]),
        over_5_5_yellow_rate: rate(count, over[3]),
        over_6_5_yellow_rate: rate(count, over[4]),
    }
}
fn fouls(items: &[&Hist], team: i64, n: usize) -> FoulWindow {
    let mut f = 0;
    let mut a = 0;
    let mut count = 0;
    for h in recent(items, n) {
        if let Some((x, y)) = pair(h.fouls, h.home == team) {
            f += x;
            a += y;
            count += 1
        }
    }
    FoulWindow {
        sample_size: count,
        fouls_committed_avg: avg(f as f64, count),
        fouls_drawn_avg: avg(a as f64, count),
        combined_match_fouls_avg: avg((f + a) as f64, count),
    }
}

fn load(c: &Connection, target_id: i64) -> Result<(Target, Vec<Hist>), String> {
    let target=c.query_row("SELECT id,competition_id,season,home_team_id,away_team_id,kickoff_at,scheduled_local_date,kickoff_time_known FROM matches WHERE id=?1",[target_id],|r|Ok(Target{id:r.get(0)?,competition:r.get(1)?,season:r.get(2)?,home:r.get(3)?,away:r.get(4)?,kick:r.get(5)?,date:r.get(6)?,known:r.get(7)?})).map_err(|e|e.to_string())?;
    let mut s=c.prepare("SELECT m.id,m.competition_id,m.season,m.home_team_id,m.away_team_id,m.kickoff_at,m.scheduled_local_date,m.kickoff_time_known,m.final_home_goals,m.final_away_goals,st.home_shots,st.away_shots,st.home_shots_on_target,st.away_shots_on_target,st.home_corners,st.away_corners,st.home_fouls,st.away_fouls,st.home_yellow_cards,st.away_yellow_cards,st.home_red_cards,st.away_red_cards FROM matches m LEFT JOIN match_statistics st ON st.match_id=m.id WHERE m.status='finished' AND m.final_home_goals IS NOT NULL AND m.final_away_goals IS NOT NULL AND m.id<>?4 AND ((?1=1 AND ((m.kickoff_time_known=1 AND m.kickoff_at<?2) OR (m.kickoff_time_known=0 AND m.scheduled_local_date<?3))) OR (?1=0 AND m.scheduled_local_date<?3)) ORDER BY COALESCE(m.scheduled_local_date,substr(m.kickoff_at,1,10)),m.kickoff_at,m.id").map_err(|e|e.to_string())?;
    let all = s
        .query_map(
            params![target.known, target.kick, target.date, target.id],
            |r| {
                Ok(Hist {
                    id: r.get(0)?,
                    competition: r.get(1)?,
                    season: r.get(2)?,
                    home: r.get(3)?,
                    away: r.get(4)?,
                    kick: r.get(5)?,
                    date: r.get(6)?,
                    known: r.get(7)?,
                    hg: r.get(8)?,
                    ag: r.get(9)?,
                    shots: opt_pair(r, 10, 11)?,
                    sot: opt_pair(r, 12, 13)?,
                    corners: opt_pair(r, 14, 15)?,
                    fouls: opt_pair(r, 16, 17)?,
                    yellow: opt_pair(r, 18, 19)?,
                    red: opt_pair(r, 20, 21)?,
                })
            },
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let eligible_history = all.into_iter().filter(|h| eligible(&target, h)).collect();
    Ok((target, eligible_history))
}
fn opt_pair(r: &rusqlite::Row<'_>, a: usize, b: usize) -> rusqlite::Result<Option<(i64, i64)>> {
    let x: Option<i64> = r.get(a)?;
    let y: Option<i64> = r.get(b)?;
    Ok(match (x, y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    })
}

fn elo(history: &[Hist], target: &Target) -> (f64, f64, HashMap<i64, Vec<f64>>) {
    let mut ratings: HashMap<i64, f64> = HashMap::new();
    let mut opponents: HashMap<i64, Vec<f64>> = HashMap::new();
    for h in history {
        let hr = *ratings.get(&h.home).unwrap_or(&ELO_INITIAL);
        let ar = *ratings.get(&h.away).unwrap_or(&ELO_INITIAL);
        opponents.entry(h.home).or_default().push(ar);
        opponents.entry(h.away).or_default().push(hr);
        let expected = 1.0 / (1.0 + 10f64.powf((ar - (hr + ELO_HOME_ADVANTAGE)) / 400.0));
        let actual = if h.hg > h.ag {
            1.0
        } else if h.hg == h.ag {
            0.5
        } else {
            0.0
        };
        let delta = ELO_K * (actual - expected);
        ratings.insert(h.home, hr + delta);
        ratings.insert(h.away, ar - delta);
    }
    (
        *ratings.get(&target.home).unwrap_or(&ELO_INITIAL),
        *ratings.get(&target.away).unwrap_or(&ELO_INITIAL),
        opponents,
    )
}
fn opp_avg(v: Option<&Vec<f64>>, n: usize) -> Option<f64> {
    let x = v
        .map(|v| v.iter().rev().take(n).copied().collect::<Vec<_>>())
        .unwrap_or_default();
    avg(x.iter().sum(), x.len())
}
fn team_features(
    history: &[Hist],
    team: i64,
    home_venue: bool,
    target_time: Option<DateTime<Utc>>,
    opponents: &HashMap<i64, Vec<f64>>,
) -> TeamFeatures {
    let all: Vec<&Hist> = history
        .iter()
        .filter(|h| h.home == team || h.away == team)
        .collect();
    let venue: Vec<&Hist> = all
        .iter()
        .copied()
        .filter(|h| (h.home == team) == home_venue)
        .collect();
    let times: Vec<_> = all.iter().filter_map(|h| parse_time(&h.kick)).collect();
    let last = times.last().copied();
    let days = match (target_time, last) {
        (Some(a), Some(b)) => Some((a - b).num_minutes() as f64 / 1440.0),
        _ => None,
    };
    let within = |d: i64| {
        target_time
            .map(|t| {
                times
                    .iter()
                    .filter(|x| **x < t && (t - **x).num_days() <= d)
                    .count()
            })
            .unwrap_or(0)
    };
    TeamFeatures {
        overall_last5: form(&all, team, 5),
        overall_last10: form(&all, team, 10),
        venue_last5: form(&venue, team, 5),
        venue_last10: form(&venue, team, 10),
        shooting_last5: shooting(&all, team, 5),
        shooting_last10: shooting(&all, team, 10),
        venue_shooting_last5: shooting(&venue, team, 5),
        corners_last5: corners(&all, team, 5),
        corners_last10: corners(&all, team, 10),
        venue_corners_last5: corners(&venue, team, 5),
        cards_last5: cards(&all, team, 5),
        cards_last10: cards(&all, team, 10),
        fouls_last5: fouls(&all, team, 5),
        fouls_last10: fouls(&all, team, 10),
        days_since_last_match: days,
        matches_in_last_7_days: within(7),
        matches_in_last_14_days: within(14),
        average_opponent_elo_last5: opp_avg(opponents.get(&team), 5),
        average_opponent_elo_last10: opp_avg(opponents.get(&team), 10),
    }
}
fn league(history: &[Hist], comp: i64) -> LeagueBaseline {
    let x: Vec<&Hist> = history.iter().filter(|h| h.competition == comp).collect();
    let n = x.len();
    let mut hg = 0;
    let mut ag = 0;
    let mut hw = 0;
    let mut dr = 0;
    let mut aw = 0;
    let mut bt = 0;
    let mut o = 0;
    let mut cor = 0;
    let mut cn = 0;
    let mut y = 0;
    let mut yn = 0;
    let mut sh = 0;
    let mut shn = 0;
    let mut st = 0;
    let mut stn = 0;
    for h in x {
        hg += h.hg;
        ag += h.ag;
        if h.hg > h.ag {
            hw += 1
        } else if h.hg == h.ag {
            dr += 1
        } else {
            aw += 1
        };
        if h.hg > 0 && h.ag > 0 {
            bt += 1
        };
        if h.hg + h.ag > 2 {
            o += 1
        };
        if let Some((a, b)) = h.corners {
            cor += a + b;
            cn += 1
        };
        if let Some((a, b)) = h.yellow {
            y += a + b;
            yn += 1
        };
        if let Some((a, b)) = h.shots {
            sh += a + b;
            shn += 1
        };
        if let Some((a, b)) = h.sot {
            st += a + b;
            stn += 1
        }
    }
    LeagueBaseline {
        sample_size: n,
        goals_per_match: avg((hg + ag) as f64, n),
        home_goals_per_match: avg(hg as f64, n),
        away_goals_per_match: avg(ag as f64, n),
        home_win_rate: rate(n, hw),
        draw_rate: rate(n, dr),
        away_win_rate: rate(n, aw),
        btts_rate: rate(n, bt),
        over_2_5_rate: rate(n, o),
        average_total_corners: avg(cor as f64, cn),
        average_total_yellow_cards: avg(y as f64, yn),
        average_shots: avg(sh as f64, shn),
        average_shots_on_target: avg(st as f64, stn),
    }
}

pub fn generate(c: &Connection, match_id: i64) -> Result<FeatureSnapshot, String> {
    let (target, history) = load(c, match_id)?;
    let relevant: Vec<_> = history
        .iter()
        .filter(|h| {
            h.home == target.home
                || h.away == target.home
                || h.home == target.away
                || h.away == target.away
        })
        .collect();
    let shots = relevant
        .iter()
        .filter(|h| h.shots.is_some() && h.sot.is_some())
        .count();
    let corners_n = relevant.iter().filter(|h| h.corners.is_some()).count();
    let cards_n = relevant
        .iter()
        .filter(|h| h.yellow.is_some() && h.red.is_some())
        .count();
    let fouls_n = relevant.iter().filter(|h| h.fouls.is_some()).count();
    let cov = |n| {
        if relevant.is_empty() {
            0.0
        } else {
            n as f64 / relevant.len() as f64
        }
    };
    let (home_elo, away_elo, opponents) = elo(&history, &target);
    let target_time = if target.known {
        parse_time(&target.kick)
    } else {
        None
    };
    let home = team_features(&history, target.home, true, target_time, &opponents);
    let away = team_features(&history, target.away, false, target_time, &opponents);
    let h2: Vec<&Hist> = history
        .iter()
        .filter(|h| {
            (h.home == target.home && h.away == target.away)
                || (h.home == target.away && h.away == target.home)
        })
        .rev()
        .take(5)
        .collect();
    let h2n = h2.len();
    let h2goals = h2.iter().map(|h| h.hg + h.ag).sum::<i64>();
    let h2b = h2.iter().filter(|h| h.hg > 0 && h.ag > 0).count();
    let h2o = h2.iter().filter(|h| h.hg + h.ag > 2).count();
    let home_played = history
        .iter()
        .filter(|h| {
            h.competition == target.competition
                && h.season == target.season
                && (h.home == target.home || h.away == target.home)
        })
        .count();
    let away_played = history
        .iter()
        .filter(|h| {
            h.competition == target.competition
                && h.season == target.season
                && (h.home == target.away || h.away == target.away)
        })
        .count();
    let cutoff = if target.known {
        target.kick.clone()
    } else {
        target.date.clone().ok_or("target has no safe cutoff")?
    };
    let expected = 1.0 / (1.0 + 10f64.powf((away_elo - (home_elo + ELO_HOME_ADVANTAGE)) / 400.0));
    Ok(FeatureSnapshot {
        match_id,
        feature_engine_version: FEATURE_ENGINE_VERSION.into(),
        cutoff_at: cutoff,
        competition_id: target.competition,
        season: target.season,
        home_team_id: target.home,
        away_team_id: target.away,
        home,
        away,
        home_elo,
        away_elo,
        elo_difference: home_elo - away_elo,
        expected_home_result: expected,
        league: league(&history, target.competition),
        h2h: H2hFeatures {
            sample_size: h2n,
            goals_avg: avg(h2goals as f64, h2n),
            btts_rate: rate(h2n, h2b),
            over_2_5_rate: rate(h2n, h2o),
        },
        home_matches_played_competition_season: home_played,
        away_matches_played_competition_season: away_played,
        data_quality: DataQuality {
            history_matches_home: home_played,
            history_matches_away: away_played,
            shots_coverage: cov(shots),
            corners_coverage: cov(corners_n),
            cards_coverage: cov(cards_n),
            fouls_coverage: cov(fouls_n),
            kickoff_quality: if target.known {
                "KNOWN".into()
            } else {
                "DATE_ONLY_CONSERVATIVE".into()
            },
            elo_available: true,
        },
    })
}

pub fn label(c: &Connection, match_id: i64) -> Result<Option<TrainingLabel>, String> {
    let row:Option<(String,Option<i64>,Option<i64>,Option<i64>,Option<i64>,Option<i64>,Option<i64>)>=c.query_row("SELECT m.status,m.final_home_goals,m.final_away_goals,s.home_corners,s.away_corners,s.home_yellow_cards,s.away_yellow_cards FROM matches m LEFT JOIN match_statistics s ON s.match_id=m.id WHERE m.id=?1",[match_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(|e|e.to_string())?;
    let Some((status, Some(h), Some(a), hc, ac, hy, ay)) = row else {
        return Ok(None);
    };
    if status != "finished" {
        return Ok(None);
    }
    let total = h + a;
    let corners = match (hc, ac) {
        (Some(x), Some(y)) => Some(x + y),
        _ => None,
    };
    let yellow = match (hy, ay) {
        (Some(x), Some(y)) => Some(x + y),
        _ => None,
    };
    let thresholds = |v: i64, lines: &[i64]| {
        Some(
            lines
                .iter()
                .map(|x| (format!("over_{}_5", x), v > *x))
                .collect(),
        )
    };
    Ok(Some(TrainingLabel {
        match_id,
        match_result: if h > a {
            "HOME"
        } else if h == a {
            "DRAW"
        } else {
            "AWAY"
        }
        .into(),
        total_goals: total,
        over_1_5: total > 1,
        over_2_5: total > 2,
        over_3_5: total > 3,
        btts: h > 0 && a > 0,
        home_over_0_5: h > 0,
        home_over_1_5: h > 1,
        away_over_0_5: a > 0,
        away_over_1_5: a > 1,
        total_corners: corners,
        corner_thresholds: corners.and_then(|v| thresholds(v, &[7, 8, 9, 10, 11])),
        total_yellow_cards: yellow,
        card_thresholds: yellow.and_then(|v| thresholds(v, &[2, 3, 4, 5, 6])),
    }))
}

pub fn persist(c: &Connection, s: &FeatureSnapshot) -> Result<bool, String> {
    let f = serde_json::to_string(s).map_err(|e| e.to_string())?;
    let q = serde_json::to_string(&s.data_quality).map_err(|e| e.to_string())?;
    c.execute("INSERT OR IGNORE INTO feature_sets(match_id,feature_engine_version,cutoff_at,feature_json,data_quality_json) VALUES(?1,?2,?3,?4,?5)",params![s.match_id,s.feature_engine_version,s.cutoff_at,f,q]).map(|n|n>0).map_err(|e|e.to_string())
}

pub fn persist_label(c: &Connection, label: &TrainingLabel) -> Result<bool, String> {
    let json = serde_json::to_string(label).map_err(|e| e.to_string())?;
    c.execute(
        "INSERT OR IGNORE INTO training_labels(match_id,label_version,label_json) VALUES(?1,?2,?3)",
        params![label.match_id, LABEL_VERSION, json],
    )
    .map(|n| n > 0)
    .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct DatasetSummary {
    pub eligible_matches: usize,
    pub generated: usize,
    pub skipped_insufficient_history: usize,
    pub average_historical_sample_size: f64,
    pub average_shots_coverage: f64,
    pub average_corners_coverage: f64,
    pub average_cards_coverage: f64,
    pub average_fouls_coverage: f64,
}
pub fn generate_dataset(
    c: &Connection,
    competition: Option<i64>,
    season: Option<&str>,
) -> Result<DatasetSummary, String> {
    let mut s=c.prepare("SELECT id FROM matches WHERE status='finished' AND (?1 IS NULL OR competition_id=?1) AND (?2 IS NULL OR season=?2) ORDER BY kickoff_at,id").map_err(|e|e.to_string())?;
    let ids = s
        .query_map(params![competition, season], |r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let mut generated = 0;
    for id in &ids {
        let x = generate(c, *id)?;
        if persist(c, &x)? {
            generated += 1
        }
        if let Some(label) = label(c, *id)? {
            let _ = persist_label(c, &label)?;
        }
        out.push(x)
    }
    let n = out.len();
    let av = |f: fn(&FeatureSnapshot) -> f64| {
        if n == 0 {
            0.0
        } else {
            out.iter().map(f).sum::<f64>() / n as f64
        }
    };
    Ok(DatasetSummary {
        eligible_matches: n,
        generated,
        skipped_insufficient_history: out
            .iter()
            .filter(|x| {
                x.home.overall_last5.sample_size < MIN_SAMPLE
                    || x.away.overall_last5.sample_size < MIN_SAMPLE
            })
            .count(),
        average_historical_sample_size: av(|x| {
            (x.home.overall_last10.sample_size + x.away.overall_last10.sample_size) as f64 / 2.0
        }),
        average_shots_coverage: av(|x| x.data_quality.shots_coverage),
        average_corners_coverage: av(|x| x.data_quality.corners_coverage),
        average_cards_coverage: av(|x| x.data_quality.cards_coverage),
        average_fouls_coverage: av(|x| x.data_quality.fouls_coverage),
    })
}

#[derive(Debug, Serialize)]
pub struct Readiness {
    pub market: String,
    pub state: String,
    pub usable_matches: i64,
    pub feature_coverage: f64,
    pub earliest_usable_date: Option<String>,
    pub latest_usable_date: Option<String>,
    pub competitions_covered: i64,
    pub reason: String,
}
pub fn market_readiness(c: &Connection) -> Result<Vec<Readiness>, String> {
    let base:(i64,Option<String>,Option<String>,i64)=c.query_row("SELECT COUNT(*),MIN(scheduled_local_date),MAX(scheduled_local_date),COUNT(DISTINCT competition_id) FROM matches WHERE status='finished' AND final_home_goals IS NOT NULL AND final_away_goals IS NOT NULL",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(|e|e.to_string())?;
    let stat = |cols: &str| -> Result<i64, String> {
        c.query_row(&format!("SELECT COUNT(*) FROM matches m JOIN match_statistics s ON s.match_id=m.id WHERE m.status='finished' AND {cols}"),[],|r|r.get(0)).map_err(|e|e.to_string())
    };
    let corners = stat("s.home_corners IS NOT NULL AND s.away_corners IS NOT NULL")?;
    let cards = stat("s.home_yellow_cards IS NOT NULL AND s.away_yellow_cards IS NOT NULL")?;
    let mk = |name: &str, n: i64, source: bool| Readiness {
        market: name.into(),
        state: if !source {
            "SOURCE_NOT_AVAILABLE"
        } else if n >= 10 {
            "SUPPORTED"
        } else if n >= 3 {
            "PARTIAL"
        } else {
            "INSUFFICIENT_HISTORY"
        }
        .into(),
        usable_matches: n,
        feature_coverage: if base.0 == 0 {
            0.0
        } else {
            n as f64 / base.0 as f64
        },
        earliest_usable_date: base.1.clone(),
        latest_usable_date: base.2.clone(),
        competitions_covered: base.3,
        reason: if source {
            "finished canonical rows with required labels".into()
        } else {
            "historical source has no first-half field".into()
        },
    };
    Ok(vec![
        mk("MATCH_RESULT", base.0, true),
        mk("TOTAL_GOALS", base.0, true),
        mk("BTTS", base.0, true),
        mk("TEAM_TOTAL_GOALS", base.0, true),
        mk("FULL_TIME_TOTAL_CORNERS", corners, true),
        mk("FIRST_HALF_TOTAL_CORNERS", 0, false),
        mk("TEAM_CORNERS", corners, true),
        mk("FULL_TIME_TOTAL_CARDS", cards, true),
        mk("FIRST_HALF_TOTAL_CARDS", 0, false),
        mk("TEAM_CARDS", cards, true),
    ])
}

#[derive(Debug, Serialize)]
pub struct QualitySummary {
    pub matches_eligible: i64,
    pub features_generated: i64,
    pub insufficient_history: i64,
    pub average_coverage: f64,
    pub competition_coverage: i64,
    pub earliest_snapshot: Option<String>,
    pub latest_snapshot: Option<String>,
    pub feature_engine_version: String,
}
pub fn quality_summary(c: &Connection) -> Result<QualitySummary, String> {
    c.query_row("SELECT (SELECT COUNT(*) FROM matches WHERE status='finished'),COUNT(*),SUM(CASE WHEN json_extract(data_quality_json,'$.history_matches_home')<?1 OR json_extract(data_quality_json,'$.history_matches_away')<?1 THEN 1 ELSE 0 END),AVG((json_extract(data_quality_json,'$.shots_coverage')+json_extract(data_quality_json,'$.corners_coverage')+json_extract(data_quality_json,'$.cards_coverage')+json_extract(data_quality_json,'$.fouls_coverage'))/4.0),COUNT(DISTINCT m.competition_id),MIN(f.cutoff_at),MAX(f.cutoff_at) FROM feature_sets f JOIN matches m ON m.id=f.match_id WHERE f.feature_engine_version=?2",params![MIN_SAMPLE as i64,FEATURE_ENGINE_VERSION],|r|Ok(QualitySummary{matches_eligible:r.get(0)?,features_generated:r.get(1)?,insufficient_history:r.get::<_,Option<i64>>(2)?.unwrap_or(0),average_coverage:r.get::<_,Option<f64>>(3)?.unwrap_or(0.0),competition_coverage:r.get(4)?,earliest_snapshot:r.get(5)?,latest_snapshot:r.get(6)?,feature_engine_version:FEATURE_ENGINE_VERSION.into()})).map_err(|e|e.to_string())
}
