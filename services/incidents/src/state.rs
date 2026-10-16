//! Health state machine of a monitor (F-16, F-17, F-32). Pure: no I/O.
//!
//! With interval `f`, confirmation period `T_conf` and recovery period `T_rec`
//! (docs/prototipo-1.md):
//!
//! - an incident opens after `k + 1` failures in a row, with `k = ⌈T_conf / f⌉` confirmation
//!   checks after the first failure (defaults 60 s / 60 s: two failures);
//! - it resolves after `m = ⌈T_rec / f⌉` successes in a row, at least one (defaults
//!   60 s / 120 s: two successes); a failure in between restarts the count.
//!
//! Heartbeats already wait their grace period before reporting a miss, so for them one
//! failure opens and one ping resolves.

use chrono::{DateTime, Utc};
use taut_model::MonitorKind;

/// Health stored in `monitor_status.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    /// No conclusive result yet.
    Pending,
    /// Healthy, or failing but not confirmed yet.
    Up,
    /// An incident is open.
    Down,
}

impl Health {
    /// Lowercase name, as stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    /// Parses the stored name; anything unknown is treated as pending.
    pub fn parse(value: &str) -> Self {
        match value {
            "up" => Self::Up,
            "down" => Self::Down,
            _ => Self::Pending,
        }
    }
}

/// Consecutive results needed to change state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    /// Failures in a row that open an incident.
    pub failures_to_open: u32,
    /// Successes in a row that resolve it.
    pub successes_to_close: u32,
}

impl Thresholds {
    /// Thresholds of a monitor from its configuration.
    pub fn new(
        kind: MonitorKind,
        interval_s: i32,
        confirmation_period_s: i32,
        recovery_period_s: i32,
    ) -> Self {
        if kind == MonitorKind::Heartbeat {
            return Self {
                failures_to_open: 1,
                successes_to_close: 1,
            };
        }
        Self {
            failures_to_open: checks_in(confirmation_period_s, interval_s) + 1,
            successes_to_close: checks_in(recovery_period_s, interval_s).max(1),
        }
    }
}

/// `⌈period / interval⌉`, with non-positive values counted as zero.
fn checks_in(period_s: i32, interval_s: i32) -> u32 {
    let period = u32::try_from(period_s).unwrap_or(0);
    let interval = u32::try_from(interval_s).unwrap_or(1).max(1);
    period.div_ceil(interval)
}

/// State of a monitor between results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    /// Current health.
    pub health: Health,
    /// Failures in a row.
    pub consecutive_failures: u32,
    /// Successes in a row.
    pub consecutive_successes: u32,
    /// First failure of the current streak.
    pub failing_since: Option<DateTime<Utc>>,
    /// Cause of that first failure.
    pub failure_cause: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            health: Health::Pending,
            consecutive_failures: 0,
            consecutive_successes: 0,
            failing_since: None,
            failure_cause: None,
        }
    }
}

/// One check result as the state machine sees it.
#[derive(Debug, Clone, Copy)]
pub struct Observation<'a> {
    /// Whether the target was healthy.
    pub ok: bool,
    /// When the check ran.
    pub at: DateTime<Utc>,
    /// Cause of a failure.
    pub error: Option<&'a str>,
}

/// What a result did to the incidents of the monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    /// Nothing to open or resolve.
    None,
    /// Open an incident that started at the first failure of the streak.
    Open {
        /// First failure of the streak.
        started_at: DateTime<Utc>,
        /// Its cause.
        cause: String,
    },
    /// Resolve the open incident.
    Resolve {
        /// The success that completed the recovery.
        resolved_at: DateTime<Utc>,
    },
}

/// Applies one result to `state`.
pub fn apply(
    state: &State,
    observation: &Observation<'_>,
    thresholds: Thresholds,
) -> (State, Transition) {
    let mut next = state.clone();
    if observation.ok {
        next.consecutive_successes = state.consecutive_successes.saturating_add(1);
        next.consecutive_failures = 0;
        next.failing_since = None;
        next.failure_cause = None;
        return match state.health {
            Health::Down if next.consecutive_successes >= thresholds.successes_to_close => {
                next.health = Health::Up;
                (
                    next,
                    Transition::Resolve {
                        resolved_at: observation.at,
                    },
                )
            }
            Health::Pending => {
                next.health = Health::Up;
                (next, Transition::None)
            }
            _ => (next, Transition::None),
        };
    }

    next.consecutive_failures = state.consecutive_failures.saturating_add(1);
    next.consecutive_successes = 0;
    if next.failing_since.is_none() {
        next.failing_since = Some(observation.at);
        next.failure_cause = Some(observation.error.unwrap_or("check failed").to_owned());
    }
    if state.health != Health::Down && next.consecutive_failures >= thresholds.failures_to_open {
        next.health = Health::Down;
        let transition = Transition::Open {
            started_at: next.failing_since.unwrap_or(observation.at),
            cause: next
                .failure_cause
                .clone()
                .unwrap_or_else(|| "check failed".to_owned()),
        };
        return (next, transition);
    }
    (next, Transition::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + seconds, 0).unwrap()
    }

    fn defaults() -> Thresholds {
        Thresholds::new(MonitorKind::Http, 60, 60, 120)
    }

    /// Applies `results` (`true` = healthy) one minute apart; returns the final state and
    /// every transition that was not `None`, with the index of the result that caused it.
    fn run(thresholds: Thresholds, results: &[bool]) -> (State, Vec<(usize, Transition)>) {
        let mut state = State::default();
        let mut transitions = Vec::new();
        for (index, ok) in results.iter().enumerate() {
            let seconds = i64::try_from(index).unwrap() * 60;
            let error = format!("failure #{index}");
            let observation = Observation {
                ok: *ok,
                at: at(seconds),
                error: (!ok).then_some(error.as_str()),
            };
            let (next, transition) = apply(&state, &observation, thresholds);
            if transition != Transition::None {
                transitions.push((index, transition));
            }
            state = next;
        }
        (state, transitions)
    }

    #[test]
    fn thresholds_follow_the_documented_formulas() {
        assert_eq!(
            defaults(),
            Thresholds {
                failures_to_open: 2,
                successes_to_close: 2
            }
        );
        assert_eq!(
            Thresholds::new(MonitorKind::Http, 60, 120, 0),
            Thresholds {
                failures_to_open: 3,
                successes_to_close: 1
            }
        );
        assert_eq!(
            Thresholds::new(MonitorKind::Tcp, 30, 60, 90).failures_to_open,
            3,
            "k = ⌈60 / 30⌉ = 2 confirmation checks"
        );
        assert_eq!(
            Thresholds::new(MonitorKind::Keyword, 300, 60, 120),
            Thresholds {
                failures_to_open: 2,
                successes_to_close: 1
            },
            "periods shorter than the interval still need one check"
        );
        assert_eq!(
            Thresholds::new(MonitorKind::Heartbeat, 60, 600, 600),
            Thresholds {
                failures_to_open: 1,
                successes_to_close: 1
            }
        );
    }

    #[test]
    fn first_success_makes_a_pending_monitor_up() {
        let (state, transitions) = run(defaults(), &[true]);
        assert_eq!(state.health, Health::Up);
        assert!(transitions.is_empty());
    }

    #[test]
    fn one_failure_is_not_an_incident_two_are() {
        let (state, transitions) = run(defaults(), &[true, false]);
        assert_eq!(state.health, Health::Up);
        assert!(transitions.is_empty());

        let (state, transitions) = run(defaults(), &[true, false, false]);
        assert_eq!(state.health, Health::Down);
        assert_eq!(
            transitions,
            vec![(
                2,
                Transition::Open {
                    started_at: at(60),
                    cause: "failure #1".to_owned()
                }
            )],
            "the incident starts at the first failure and keeps its cause"
        );
    }

    #[test]
    fn a_success_between_failures_restarts_confirmation() {
        let (state, transitions) = run(defaults(), &[false, true, false, true, false]);
        assert_eq!(state.health, Health::Up);
        assert!(transitions.is_empty());
    }

    #[test]
    fn an_open_incident_is_not_opened_again() {
        let (_, transitions) = run(defaults(), &[false, false, false, false, false]);
        assert_eq!(transitions.len(), 1);
    }

    #[test]
    fn recovery_needs_consecutive_successes() {
        let (state, transitions) = run(defaults(), &[false, false, true, false, true, true]);
        assert_eq!(state.health, Health::Up);
        assert_eq!(transitions.len(), 2);
        assert_eq!(
            transitions[1],
            (
                5,
                Transition::Resolve {
                    resolved_at: at(300)
                }
            ),
            "the success at index 2 was undone by the failure at index 3"
        );
    }

    #[test]
    fn zero_confirmation_opens_on_the_first_failure() {
        let thresholds = Thresholds::new(MonitorKind::Http, 60, 0, 0);
        let (_, transitions) = run(thresholds, &[false, true]);
        assert!(matches!(transitions[0], (0, Transition::Open { .. })));
        assert!(matches!(transitions[1], (1, Transition::Resolve { .. })));
    }

    #[test]
    fn heartbeats_open_and_resolve_at_once() {
        let thresholds = Thresholds::new(MonitorKind::Heartbeat, 60, 60, 120);
        let (state, transitions) = run(thresholds, &[true, false, true]);
        assert_eq!(state.health, Health::Up);
        assert_eq!(transitions.len(), 2);
    }

    #[test]
    fn health_names_round_trip() {
        for health in [Health::Pending, Health::Up, Health::Down] {
            assert_eq!(Health::parse(health.as_str()), health);
        }
    }
}
