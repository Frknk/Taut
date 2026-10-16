//! Assembly of the public status page: pure functions over rows already read.

use chrono::NaiveDate;
use serde::Serialize;
use taut_model::{MonitorStatus, uptime_percent};

/// Days of history on the page (the retention of the raw results).
pub const DAYS: u16 = 90;

/// One day of one monitor.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayBar {
    /// UTC day.
    pub day: NaiveDate,
    /// Checks that day.
    pub checks: i64,
    /// Failed checks that day.
    pub failures: i64,
    /// Percentage of healthy checks; `None` without checks.
    pub uptime: Option<f64>,
}

/// Overall state shown at the top of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Overall {
    /// No monitor is down.
    Operational,
    /// Some monitors are down.
    PartialOutage,
    /// Every monitor that is checked is down.
    MajorOutage,
}

/// One bar per day from `first` for [`DAYS`] days; days without rows have no checks.
/// `rows` are `(day, checks, failures)` in any order.
pub fn bars(first: NaiveDate, rows: &[(NaiveDate, i64, i64)]) -> Vec<DayBar> {
    first
        .iter_days()
        .take(usize::from(DAYS))
        .map(|day| {
            let (checks, failures) = rows
                .iter()
                .filter(|(row_day, ..)| *row_day == day)
                .fold((0, 0), |(c, f), (_, checks, failures)| {
                    (c + checks, f + failures)
                });
            DayBar {
                day,
                checks,
                failures,
                uptime: uptime_percent(checks, failures),
            }
        })
        .collect()
}

/// Uptime over a run of bars.
pub fn overall_uptime(bars: &[DayBar]) -> Option<f64> {
    let checks = bars.iter().map(|bar| bar.checks).sum();
    let failures = bars.iter().map(|bar| bar.failures).sum();
    uptime_percent(checks, failures)
}

/// Overall state from the status of each monitor; paused and pending ones do not count.
pub fn overall(statuses: &[MonitorStatus]) -> Overall {
    let checked = statuses
        .iter()
        .filter(|status| matches!(status, MonitorStatus::Up | MonitorStatus::Down))
        .count();
    let down = statuses
        .iter()
        .filter(|status| **status == MonitorStatus::Down)
        .count();
    match down {
        0 => Overall::Operational,
        _ if down == checked => Overall::MajorOutage,
        _ => Overall::PartialOutage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    #[test]
    fn bars_cover_ninety_days_with_gaps_empty() {
        let first = NaiveDate::from_ymd_opt(2026, 7, 19).unwrap();
        let bars = bars(first, &[(date(15), 1440, 3), (date(16), 100, 0)]);
        assert_eq!(bars.len(), 90);
        assert_eq!(bars[0].day, first);
        assert_eq!(bars[0].uptime, None);
        let last = &bars[89];
        assert_eq!(last.day, date(16));
        assert_eq!(last.uptime, Some(100.0));
        let fifteenth = &bars[88];
        assert_eq!(fifteenth.checks, 1440);
        assert!((fifteenth.uptime.unwrap() - 99.791_666).abs() < 1e-3);
        assert!((overall_uptime(&bars).unwrap() - 99.805_825).abs() < 1e-3);
    }

    #[test]
    fn overall_ignores_paused_and_pending() {
        use MonitorStatus::{Down, Paused, Pending, Up};
        assert_eq!(overall(&[Up, Paused, Pending]), Overall::Operational);
        assert_eq!(overall(&[Up, Down]), Overall::PartialOutage);
        assert_eq!(overall(&[Down, Paused]), Overall::MajorOutage);
        assert_eq!(overall(&[]), Overall::Operational);
    }
}
