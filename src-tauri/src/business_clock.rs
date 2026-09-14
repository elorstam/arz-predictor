//! Business context only. Persisted audit/ingestion/settlement timestamps use real time.
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct Snapshot {
    pub utc_ms: i64,
    pub business_date: String,
    pub overridden: bool,
    pub revision: u64,
}
#[derive(Default)]
struct Clock {
    injected: Option<DateTime<Utc>>,
    revision: u64,
}
impl Clock {
    fn snapshot(&self, real: DateTime<Utc>) -> Snapshot {
        let now = self.injected.unwrap_or(real);
        Snapshot {
            utc_ms: now.timestamp_millis(),
            business_date: now
                .with_timezone(&chrono_tz::Europe::Istanbul)
                .date_naive()
                .to_string(),
            overridden: self.injected.is_some(),
            revision: self.revision,
        }
    }
    #[cfg(debug_assertions)]
    fn set(&mut self, value: Option<&str>) -> Result<(), String> {
        let next = value
            .map(|s| DateTime::parse_from_rfc3339(s).map(|d| d.with_timezone(&Utc)))
            .transpose()
            .map_err(|_| "INVALID_CLOCK_DATETIME")?;
        if self.injected != next {
            self.injected = next;
            self.revision += 1;
        }
        Ok(())
    }
}
#[cfg(debug_assertions)]
static CLOCK: std::sync::Mutex<Clock> = std::sync::Mutex::new(Clock {
    injected: None,
    revision: 0,
});
pub fn snapshot() -> Snapshot {
    #[cfg(debug_assertions)]
    {
        CLOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot(Utc::now())
    }
    #[cfg(not(debug_assertions))]
    {
        Clock::default().snapshot(Utc::now())
    }
}
pub fn now() -> DateTime<Utc> {
    DateTime::from_timestamp_millis(snapshot().utc_ms).expect("valid business clock")
}
pub fn today() -> NaiveDate {
    now()
        .with_timezone(&chrono_tz::Europe::Istanbul)
        .date_naive()
}
pub fn date() -> String {
    today().to_string()
}
pub fn set_test(value: Option<&str>) -> Result<Snapshot, String> {
    #[cfg(debug_assertions)]
    {
        if std::env::var("ARZ_TEST_BUSINESS_CLOCK").as_deref() != Ok("1") {
            return Err("TEST_BUSINESS_CLOCK_DISABLED".into());
        }
        if crate::automatic_refresh::status().is_ok_and(|s| s.state != "IDLE") {
            return Err("WAIT_FOR_REFRESH_IDLE".into());
        }
        let mut clock = CLOCK.lock().map_err(|_| "CLOCK_LOCK")?;
        clock.set(value)?;
        Ok(clock.snapshot(Utc::now()))
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = value;
        Err("TEST_BUSINESS_CLOCK_DISABLED".into())
    }
}
#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;
    #[test]
    fn midnight_override_is_ephemeral_and_does_not_change_wall_time() {
        let real = DateTime::parse_from_rfc3339("2026-09-14T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut c = Clock::default();
        assert_eq!(c.snapshot(real).utc_ms, real.timestamp_millis());
        c.set(Some("2026-09-14T23:59:50+03:00")).unwrap();
        assert_eq!(c.snapshot(real).business_date, "2026-09-14");
        c.set(Some("2026-09-15T00:00:10+03:00")).unwrap();
        assert_eq!(c.snapshot(real).business_date, "2026-09-15");
        let revision = c.revision;
        c.set(Some("2026-09-15T00:00:10+03:00")).unwrap();
        assert_eq!(c.revision, revision);
        assert!(!Clock::default().snapshot(real).overridden);
        c.set(None).unwrap();
        assert_eq!(c.snapshot(real).utc_ms, real.timestamp_millis());
        assert!(!c.snapshot(real).overridden);
    }
    #[test]
    fn invalid_override_keeps_previous_clock() {
        let mut c = Clock::default();
        assert!(c.set(Some("invalid")).is_err());
        assert!(c.injected.is_none());
        assert_eq!(c.revision, 0);
    }
}
