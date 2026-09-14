//! Research-only export. Production SQLite is opened READ_ONLY; only TEMP views change.
use chrono::{Duration, NaiveDate};
use football_predictor_lib::repositories::{features, prediction_engine};
use rusqlite::{params, Connection, OpenFlags};
use serde_json::json;
use std::{
    fs::File,
    io::{BufWriter, Write},
};

fn install_causal_view(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TEMP TABLE goal_export_target(id INTEGER,cutoff TEXT,day TEXT); INSERT INTO goal_export_target VALUES(0,'','');")?;
    let columns: Vec<String> = c
        .prepare("PRAGMA main.table_info(matches)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;
    let projection = columns.iter().map(|name| match name.as_str() {
        "kickoff_at" => "CASE WHEN m.id=t.id THEN t.cutoff ELSE m.kickoff_at END AS kickoff_at".to_owned(),
        "scheduled_local_date" => "CASE WHEN m.id=t.id THEN t.day ELSE m.scheduled_local_date END AS scheduled_local_date".to_owned(),
        "kickoff_time_known" => "CASE WHEN m.id=t.id THEN 1 ELSE m.kickoff_time_known END AS kickoff_time_known".to_owned(),
        _ => format!("m.\"{name}\"")
    }).collect::<Vec<_>>().join(",");
    c.execute_batch(&format!("CREATE TEMP VIEW matches AS SELECT {projection} FROM main.matches m CROSS JOIN goal_export_target t"))?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: goal_v2_export <production-database> <output-jsonl>".into());
    }
    let c = Connection::open_with_flags(&args[1], OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    c.execute_batch("BEGIN DEFERRED")?;
    install_causal_view(&c)?;
    let rows = c.prepare("SELECT id,kickoff_at,scheduled_local_date,home_team_id,away_team_id,competition_id,final_home_goals,final_away_goals FROM main.matches WHERE status='finished' AND final_home_goals IS NOT NULL AND final_away_goals IS NOT NULL ORDER BY julianday(kickoff_at),id")?.query_map([],|r| Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?,r.get::<_,i64>(5)?,r.get::<_,i64>(6)?,r.get::<_,i64>(7)?)))?.collect::<Result<Vec<_>,_>>()?;
    let mut output = BufWriter::new(File::create(&args[2])?);
    for (n, (id, kickoff, day, home, away, competition, hg, ag)) in rows.iter().enumerate() {
        let date = NaiveDate::parse_from_str(day, "%Y-%m-%d")?;
        let decision = date.and_hms_opt(0, 0, 0).unwrap() - Duration::hours(3);
        // Conservative 24-hour result-availability embargo; no same-day outcomes.
        let cutoff = decision - Duration::hours(24);
        let cutoff_text = cutoff.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        c.execute(
            "UPDATE goal_export_target SET id=?1,cutoff=?2,day=?3",
            params![id, cutoff_text, cutoff.date().to_string()],
        )?;
        let f = features::generate(&c, *id).map_err(std::io::Error::other)?;
        writeln!(
            output,
            "{}",
            json!({"match_id":id,"kickoff":kickoff,"date":day,"home":home,"away":away,"competition":competition,"hg":hg,"ag":ag,"decision_at":decision.format("%Y-%m-%dT%H:%M:%SZ").to_string(),"history_before":cutoff_text,"baseline_raw":prediction_engine::raw_vector(&f),"features":f})
        )?;
        if n % 1000 == 0 {
            eprintln!("exported {n}/{}", rows.len());
        }
    }
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_day_ignores_same_day_future_and_target_outcomes() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE matches(id INTEGER PRIMARY KEY,competition_id INTEGER,season TEXT,home_team_id INTEGER,away_team_id INTEGER,kickoff_at TEXT,status TEXT,final_home_goals INTEGER,final_away_goals INTEGER,scheduled_local_date TEXT,kickoff_time_known INTEGER); CREATE TABLE match_statistics(match_id INTEGER,home_shots INTEGER,away_shots INTEGER,home_shots_on_target INTEGER,away_shots_on_target INTEGER,home_corners INTEGER,away_corners INTEGER,home_fouls INTEGER,away_fouls INTEGER,home_yellow_cards INTEGER,away_yellow_cards INTEGER,home_red_cards INTEGER,away_red_cards INTEGER);").unwrap();
        let (league, home, away) = (1, 1, 2);
        for day in 1..=14 {
            let d = format!("2026-09-{day:02}");
            c.execute("INSERT INTO matches(competition_id,season,home_team_id,away_team_id,kickoff_at,status,final_home_goals,final_away_goals,scheduled_local_date,kickoff_time_known) VALUES(?1,'2026/27',?2,?3,?4,'finished',2,1,?5,1)",params![league,home,away,format!("{d}T15:00:00Z"),d]).unwrap();
        }
        let target: i64 = c
            .query_row(
                "SELECT id FROM matches WHERE scheduled_local_date='2026-09-12'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        install_causal_view(&c).unwrap();
        c.execute(
            "UPDATE goal_export_target SET id=?1,cutoff='2026-09-10T21:00:00Z',day='2026-09-10'",
            [target],
        )
        .unwrap();
        let before = serde_json::to_value(features::generate(&c, target).unwrap()).unwrap();
        assert_eq!(before["home"]["overall_last10"]["sample_size"], 10);
        c.execute("UPDATE main.matches SET final_home_goals=99,final_away_goals=88 WHERE scheduled_local_date>='2026-09-11'",[]).unwrap();
        let after = serde_json::to_value(features::generate(&c, target).unwrap()).unwrap();
        assert_eq!(before, after);
        let actual: String = c
            .query_row(
                "SELECT kickoff_at FROM main.matches WHERE id=?1",
                [target],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(actual, "2026-09-12T15:00:00Z");
    }
}
