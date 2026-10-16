//! When each monitor is due: pure bookkeeping, no I/O.
//!
//! The leader keeps one entry per active monitor with the instant of its next check. A
//! monitor seen for the first time at startup is placed at a stable offset inside its first
//! interval, so a restart with a thousand monitors does not fire them all in the same second;
//! a monitor that was just created or edited is due immediately, so the user sees a result
//! right away.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use taut_model::MonitorKind;
use uuid::Uuid;

/// What the scheduler needs to know about an active monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledMonitor {
    /// Monitor id.
    pub id: Uuid,
    /// Owner.
    pub tenant_id: Uuid,
    /// What it checks.
    pub kind: MonitorKind,
    /// URL or `host:port`; `None` for heartbeats.
    pub target: Option<String>,
    /// Seconds between checks.
    pub interval_s: i32,
    /// Seconds before a check counts as failed.
    pub timeout_s: i32,
    /// Expected HTTP status; `None` means any 2xx.
    pub expected_status: Option<i16>,
    /// Text the body must contain.
    pub keyword: Option<String>,
    /// Heartbeats: extra silence tolerated after the interval.
    pub grace_period_s: i32,
}

impl ScheduledMonitor {
    fn interval(&self) -> Duration {
        Duration::seconds(self.interval_s.into())
    }

    /// Stable offset inside the first interval, derived from the id's random bits.
    fn stagger(&self) -> Duration {
        let interval_ms = u128::try_from(self.interval_s).unwrap_or(60) * 1000;
        let offset = self.id.as_u128() % interval_ms.max(1);
        Duration::milliseconds(i64::try_from(offset).unwrap_or(0))
    }
}

/// A monitor whose check is due, with the instant it was due at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    /// The monitor.
    pub monitor: ScheduledMonitor,
    /// When the check was due.
    pub scheduled_at: DateTime<Utc>,
}

#[derive(Debug)]
struct Entry {
    monitor: ScheduledMonitor,
    next_at: DateTime<Utc>,
}

/// Next check of every active monitor.
#[derive(Debug, Default)]
pub struct Schedule {
    entries: HashMap<Uuid, Entry>,
}

impl Schedule {
    /// Number of scheduled monitors.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no monitor is scheduled.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Next check of `id`, if scheduled.
    pub fn next_at(&self, id: Uuid) -> Option<DateTime<Utc>> {
        self.entries.get(&id).map(|entry| entry.next_at)
    }

    /// Replaces the schedule with the active monitors from the database. Unchanged monitors
    /// keep their next check, changed ones are due now, unknown ones are staggered and the
    /// ones no longer active are dropped.
    pub fn sync(&mut self, monitors: Vec<ScheduledMonitor>, now: DateTime<Utc>) {
        let mut entries = HashMap::with_capacity(monitors.len());
        for monitor in monitors {
            let next_at = match self.entries.remove(&monitor.id) {
                Some(entry) if entry.monitor == monitor => entry.next_at,
                Some(_) => now,
                None => now + monitor.stagger(),
            };
            entries.insert(monitor.id, Entry { monitor, next_at });
        }
        self.entries = entries;
    }

    /// Adds or updates one monitor after an event; it is due now unless nothing changed.
    pub fn upsert(&mut self, monitor: ScheduledMonitor, now: DateTime<Utc>) {
        if let Some(entry) = self.entries.get(&monitor.id)
            && entry.monitor == monitor
        {
            return;
        }
        self.entries.insert(
            monitor.id,
            Entry {
                monitor,
                next_at: now,
            },
        );
    }

    /// Stops scheduling `id` (paused or deleted). Returns whether it was scheduled.
    pub fn remove(&mut self, id: Uuid) -> bool {
        self.entries.remove(&id).is_some()
    }

    /// Returns the monitors due at `now`, oldest first, and moves each to its next interval.
    /// A monitor that fell more than one interval behind skips the missed checks instead of
    /// firing them in a burst.
    pub fn take_due(&mut self, now: DateTime<Utc>) -> Vec<Due> {
        let mut due = Vec::new();
        for entry in self.entries.values_mut() {
            if entry.next_at > now {
                continue;
            }
            due.push(Due {
                monitor: entry.monitor.clone(),
                scheduled_at: entry.next_at,
            });
            let interval = entry.monitor.interval();
            let next = entry.next_at + interval;
            entry.next_at = if next <= now { now + interval } else { next };
        }
        due.sort_by_key(|item| item.scheduled_at);
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(interval_s: i32) -> ScheduledMonitor {
        ScheduledMonitor {
            id: Uuid::now_v7(),
            tenant_id: Uuid::now_v7(),
            kind: MonitorKind::Http,
            target: Some("https://example.com/".to_owned()),
            interval_s,
            timeout_s: 10,
            expected_status: None,
            keyword: None,
            grace_period_s: 60,
        }
    }

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + seconds, 0).unwrap()
    }

    #[test]
    fn first_sync_staggers_inside_the_first_interval() {
        let mut schedule = Schedule::default();
        let monitors: Vec<_> = (0..50).map(|_| monitor(60)).collect();
        schedule.sync(monitors.clone(), at(0));
        assert_eq!(schedule.len(), 50);
        for m in &monitors {
            let next = schedule.next_at(m.id).unwrap();
            assert!(next >= at(0) && next < at(60), "{next}");
        }
        // Fifty random offsets in one minute do not all collapse to the same second.
        let distinct: std::collections::HashSet<_> =
            monitors.iter().map(|m| schedule.next_at(m.id)).collect();
        assert!(distinct.len() > 10);
    }

    #[test]
    fn due_monitors_advance_by_their_interval() {
        let mut schedule = Schedule::default();
        let m = monitor(30);
        schedule.upsert(m.clone(), at(0));
        let due = schedule.take_due(at(0));
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].scheduled_at, at(0));
        assert_eq!(schedule.next_at(m.id), Some(at(30)));
        assert!(schedule.take_due(at(29)).is_empty());
        assert_eq!(schedule.take_due(at(30)).len(), 1);
        assert_eq!(schedule.next_at(m.id), Some(at(60)));
    }

    #[test]
    fn falling_behind_skips_missed_checks() {
        let mut schedule = Schedule::default();
        let m = monitor(30);
        schedule.upsert(m.clone(), at(0));
        let due = schedule.take_due(at(95));
        assert_eq!(due.len(), 1, "one check, not three");
        assert_eq!(schedule.next_at(m.id), Some(at(125)));
    }

    #[test]
    fn sync_keeps_unchanged_reschedules_changed_and_drops_missing() {
        let mut schedule = Schedule::default();
        let kept = monitor(60);
        let changed = monitor(60);
        let dropped = monitor(60);
        for m in [&kept, &changed, &dropped] {
            schedule.upsert(m.clone(), at(0));
        }
        schedule.take_due(at(0));

        let mut edited = changed.clone();
        edited.interval_s = 30;
        schedule.sync(vec![kept.clone(), edited], at(10));
        assert_eq!(schedule.len(), 2);
        assert_eq!(schedule.next_at(kept.id), Some(at(60)));
        assert_eq!(schedule.next_at(changed.id), Some(at(10)));
        assert_eq!(schedule.next_at(dropped.id), None);
    }

    #[test]
    fn upsert_of_an_unchanged_monitor_keeps_its_slot() {
        let mut schedule = Schedule::default();
        let m = monitor(60);
        schedule.upsert(m.clone(), at(0));
        schedule.take_due(at(0));
        schedule.upsert(m.clone(), at(20));
        assert_eq!(schedule.next_at(m.id), Some(at(60)));
        assert!(schedule.remove(m.id));
        assert!(schedule.is_empty());
    }
}
