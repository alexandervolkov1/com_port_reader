use std::{
    collections::VecDeque,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::process_recorder::ProcessMeasurement;

use super::ScenarioTriggerTarget;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ThresholdDirection {
    Above,
    Below,
}

impl ThresholdDirection {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Above => "above",
            Self::Below => "below",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScenarioCondition {
    pub(crate) kind: ScenarioConditionKind,
    pub(crate) hold: Duration,
}

impl ScenarioCondition {
    pub(crate) const fn kind_name(&self) -> &'static str {
        self.kind.name()
    }

    pub(crate) fn primary_series(&self) -> Option<&str> {
        self.kind.primary_series()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ScenarioConditionKind {
    Threshold {
        series: String,
        direction: ThresholdDirection,
        threshold: f64,
        hysteresis: f64,
    },
    Inside {
        series: String,
        minimum: f64,
        maximum: f64,
        hysteresis: f64,
    },
    Outside {
        series: String,
        minimum: f64,
        maximum: f64,
        hysteresis: f64,
    },
    Stable {
        series: String,
        target: f64,
        tolerance: f64,
        hysteresis: f64,
    },
    Rate {
        series: String,
        direction: ThresholdDirection,
        threshold: f64,
        window: Duration,
        hysteresis: f64,
    },
    Stale {
        series: String,
        duration: Duration,
    },
    All(Vec<ScenarioCondition>),
    Any(Vec<ScenarioCondition>),
}

impl ScenarioConditionKind {
    pub(crate) const fn name(&self) -> &'static str {
        match self {
            Self::Threshold { direction, .. } => direction.as_str(),
            Self::Rate {
                direction: ThresholdDirection::Above,
                ..
            } => "rate_above",
            Self::Rate {
                direction: ThresholdDirection::Below,
                ..
            } => "rate_below",
            Self::Inside { .. } => "inside",
            Self::Outside { .. } => "outside",
            Self::Stable { .. } => "stable",
            Self::Stale { .. } => "stale",
            Self::All(_) => "all",
            Self::Any(_) => "any",
        }
    }

    fn primary_series(&self) -> Option<&str> {
        match self {
            Self::Threshold { series, .. }
            | Self::Inside { series, .. }
            | Self::Outside { series, .. }
            | Self::Stable { series, .. }
            | Self::Rate { series, .. }
            | Self::Stale { series, .. } => Some(series),
            Self::All(_) | Self::Any(_) => None,
        }
    }
}

pub(super) struct ScenarioConditionState {
    pub(super) condition: ScenarioCondition,
    pub(super) target: ScenarioTriggerTarget,
    node: ConditionNodeState,
}

impl ScenarioConditionState {
    pub(super) fn new(
        condition: ScenarioCondition,
        target: ScenarioTriggerTarget,
        system_now: SystemTime,
    ) -> Self {
        let started_at = system_time_seconds(system_now);
        let node = ConditionNodeState::new(&condition, started_at);

        Self {
            condition,
            target,
            node,
        }
    }

    pub(super) fn update(
        &mut self,
        measurement: &ProcessMeasurement,
        now: Instant,
        unix_now: f64,
    ) -> bool {
        self.node.update(measurement, now, unix_now) && self.node.matches()
    }

    pub(super) fn tick(&mut self, now: Instant, unix_now: f64) -> bool {
        self.node.tick(now, unix_now) && self.node.matches()
    }
}

struct ConditionNodeState {
    hold: Duration,
    matching: MatchState,
    kind: ConditionNodeKind,
}

enum ConditionNodeKind {
    Threshold {
        series: String,
        direction: ThresholdDirection,
        threshold: f64,
        hysteresis: f64,
    },
    Inside {
        series: String,
        minimum: f64,
        maximum: f64,
        hysteresis: f64,
    },
    Outside {
        series: String,
        minimum: f64,
        maximum: f64,
        hysteresis: f64,
    },
    Stable {
        series: String,
        target: f64,
        tolerance: f64,
        hysteresis: f64,
    },
    Rate {
        series: String,
        direction: ThresholdDirection,
        threshold: f64,
        window: Duration,
        hysteresis: f64,
        history: VecDeque<RateSample>,
    },
    Stale {
        series: String,
        duration: Duration,
        last_timestamp: f64,
    },
    All(Vec<ConditionNodeState>),
    Any(Vec<ConditionNodeState>),
}

#[derive(Clone, Copy)]
struct RateSample {
    timestamp: f64,
    value: f64,
}

impl ConditionNodeState {
    fn new(condition: &ScenarioCondition, started_at: f64) -> Self {
        let kind = match &condition.kind {
            ScenarioConditionKind::Threshold {
                series,
                direction,
                threshold,
                hysteresis,
            } => ConditionNodeKind::Threshold {
                series: series.clone(),
                direction: *direction,
                threshold: *threshold,
                hysteresis: *hysteresis,
            },
            ScenarioConditionKind::Inside {
                series,
                minimum,
                maximum,
                hysteresis,
            } => ConditionNodeKind::Inside {
                series: series.clone(),
                minimum: *minimum,
                maximum: *maximum,
                hysteresis: *hysteresis,
            },
            ScenarioConditionKind::Outside {
                series,
                minimum,
                maximum,
                hysteresis,
            } => ConditionNodeKind::Outside {
                series: series.clone(),
                minimum: *minimum,
                maximum: *maximum,
                hysteresis: *hysteresis,
            },
            ScenarioConditionKind::Stable {
                series,
                target,
                tolerance,
                hysteresis,
            } => ConditionNodeKind::Stable {
                series: series.clone(),
                target: *target,
                tolerance: *tolerance,
                hysteresis: *hysteresis,
            },
            ScenarioConditionKind::Rate {
                series,
                direction,
                threshold,
                window,
                hysteresis,
            } => ConditionNodeKind::Rate {
                series: series.clone(),
                direction: *direction,
                threshold: *threshold,
                window: *window,
                hysteresis: *hysteresis,
                history: VecDeque::new(),
            },
            ScenarioConditionKind::Stale { series, duration } => ConditionNodeKind::Stale {
                series: series.clone(),
                duration: *duration,
                last_timestamp: started_at,
            },
            ScenarioConditionKind::All(conditions) => ConditionNodeKind::All(
                conditions
                    .iter()
                    .map(|condition| Self::new(condition, started_at))
                    .collect(),
            ),
            ScenarioConditionKind::Any(conditions) => ConditionNodeKind::Any(
                conditions
                    .iter()
                    .map(|condition| Self::new(condition, started_at))
                    .collect(),
            ),
        };

        Self {
            hold: condition.hold,
            matching: MatchState::default(),
            kind,
        }
    }

    fn matches(&self) -> bool {
        self.matching.active
    }

    fn update(&mut self, measurement: &ProcessMeasurement, now: Instant, unix_now: f64) -> bool {
        if !measurement.timestamp.is_finite() || !measurement.value.is_finite() {
            return false;
        }

        let previous_match = self.matching.raw;
        let raw = match &mut self.kind {
            ConditionNodeKind::Threshold {
                series,
                direction,
                threshold,
                hysteresis,
            } if series == &measurement.series_name => Some(threshold_match(
                *direction,
                measurement.value,
                *threshold,
                *hysteresis,
                previous_match,
            )),
            ConditionNodeKind::Inside {
                series,
                minimum,
                maximum,
                hysteresis,
            } if series == &measurement.series_name => Some(if previous_match {
                measurement.value >= *minimum - *hysteresis
                    && measurement.value <= *maximum + *hysteresis
            } else {
                measurement.value >= *minimum && measurement.value <= *maximum
            }),
            ConditionNodeKind::Outside {
                series,
                minimum,
                maximum,
                hysteresis,
            } if series == &measurement.series_name => Some(if previous_match {
                measurement.value < *minimum + *hysteresis
                    || measurement.value > *maximum - *hysteresis
            } else {
                measurement.value < *minimum || measurement.value > *maximum
            }),
            ConditionNodeKind::Stable {
                series,
                target,
                tolerance,
                hysteresis,
            } if series == &measurement.series_name => {
                let allowed = if previous_match {
                    *tolerance + *hysteresis
                } else {
                    *tolerance
                };
                Some((measurement.value - *target).abs() <= allowed)
            }
            ConditionNodeKind::Rate {
                series,
                direction,
                threshold,
                window,
                hysteresis,
                history,
            } if series == &measurement.series_name => {
                update_rate_history(history, measurement, *window).map(|rate| {
                    threshold_match(*direction, rate, *threshold, *hysteresis, previous_match)
                })
            }
            ConditionNodeKind::Stale {
                series,
                duration,
                last_timestamp,
            } if series == &measurement.series_name => {
                *last_timestamp = last_timestamp.max(measurement.timestamp);
                Some(unix_now - *last_timestamp >= duration.as_secs_f64())
            }
            ConditionNodeKind::All(children) => {
                let mut updated = false;
                for child in children.iter_mut() {
                    updated |= child.update(measurement, now, unix_now);
                }
                updated.then(|| children.iter().all(Self::matches))
            }
            ConditionNodeKind::Any(children) => {
                let mut updated = false;
                for child in children.iter_mut() {
                    updated |= child.update(measurement, now, unix_now);
                }
                updated.then(|| children.iter().any(Self::matches))
            }
            _ => None,
        };

        if let Some(raw) = raw {
            self.matching.update(raw, now, self.hold);
            true
        } else {
            false
        }
    }

    fn tick(&mut self, now: Instant, unix_now: f64) -> bool {
        let raw = match &mut self.kind {
            ConditionNodeKind::Stale {
                duration,
                last_timestamp,
                ..
            } => Some(unix_now - *last_timestamp >= duration.as_secs_f64()),
            ConditionNodeKind::All(children) => {
                let mut updated = false;
                for child in children.iter_mut() {
                    updated |= child.tick(now, unix_now);
                }
                updated.then(|| children.iter().all(Self::matches))
            }
            ConditionNodeKind::Any(children) => {
                let mut updated = false;
                for child in children.iter_mut() {
                    updated |= child.tick(now, unix_now);
                }
                updated.then(|| children.iter().any(Self::matches))
            }
            _ => None,
        };

        if let Some(raw) = raw {
            self.matching.update(raw, now, self.hold);
            true
        } else {
            false
        }
    }
}

#[derive(Default)]
struct MatchState {
    raw: bool,
    matching_since: Option<Instant>,
    active: bool,
}

impl MatchState {
    fn update(&mut self, raw: bool, now: Instant, hold: Duration) {
        if !raw {
            self.raw = false;
            self.matching_since = None;
            self.active = false;
            return;
        }

        if !self.raw {
            self.matching_since = Some(now);
        }
        self.raw = true;

        let matching_since = self
            .matching_since
            .expect("matching condition must have a monotonic start time");
        self.active = now.duration_since(matching_since) >= hold;
    }
}

fn threshold_match(
    direction: ThresholdDirection,
    value: f64,
    threshold: f64,
    hysteresis: f64,
    previous_match: bool,
) -> bool {
    match (direction, previous_match) {
        (ThresholdDirection::Above, false) => value > threshold,
        (ThresholdDirection::Above, true) => value > threshold - hysteresis,
        (ThresholdDirection::Below, false) => value < threshold,
        (ThresholdDirection::Below, true) => value < threshold + hysteresis,
    }
}

fn update_rate_history(
    history: &mut VecDeque<RateSample>,
    measurement: &ProcessMeasurement,
    window: Duration,
) -> Option<f64> {
    if history
        .back()
        .is_some_and(|sample| measurement.timestamp <= sample.timestamp)
    {
        return None;
    }

    history.push_back(RateSample {
        timestamp: measurement.timestamp,
        value: measurement.value,
    });

    let cutoff = measurement.timestamp - window.as_secs_f64();
    while history.len() >= 2 && history[1].timestamp <= cutoff {
        history.pop_front();
    }

    let first = history.front()?;
    let last = history.back()?;
    let elapsed = last.timestamp - first.timestamp;
    if elapsed < window.as_secs_f64() || elapsed <= 0.0 {
        return None;
    }

    Some((last.value - first.value) / elapsed)
}

fn system_time_seconds(time: SystemTime) -> f64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64(),
        Err(error) => -error.duration().as_secs_f64(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connection::ConnectionId, data::SeriesId};

    fn measurement(series: &str, timestamp: f64, value: f64) -> ProcessMeasurement {
        ProcessMeasurement {
            connection_id: ConnectionId::PRIMARY,
            series_id: SeriesId::new(1),
            series_name: series.to_owned(),
            timestamp,
            value,
        }
    }

    fn state(kind: ScenarioConditionKind, hold: Duration) -> ScenarioConditionState {
        ScenarioConditionState::new(
            ScenarioCondition { kind, hold },
            ScenarioTriggerTarget::Callback("callback".to_owned()),
            UNIX_EPOCH + Duration::from_secs(100),
        )
    }

    fn above(series: &str, threshold: f64) -> ScenarioCondition {
        ScenarioCondition {
            kind: ScenarioConditionKind::Threshold {
                series: series.to_owned(),
                direction: ThresholdDirection::Above,
                threshold,
                hysteresis: 0.0,
            },
            hold: Duration::ZERO,
        }
    }

    #[test]
    fn threshold_hold_uses_monotonic_time_and_hysteresis() {
        let start = Instant::now();
        let mut condition = state(
            ScenarioConditionKind::Threshold {
                series: "temperature".to_owned(),
                direction: ThresholdDirection::Above,
                threshold: 150.0,
                hysteresis: 2.0,
            },
            Duration::from_secs(5),
        );

        assert!(!condition.update(&measurement("temperature", 100.0, 151.0), start, 100.0));
        assert!(!condition.update(
            &measurement("temperature", 104.0, 149.0),
            start + Duration::from_secs(4),
            104.0,
        ));
        assert!(condition.update(
            &measurement("temperature", 105.0, 149.0),
            start + Duration::from_secs(5),
            105.0,
        ));

        let mut reset = state(
            ScenarioConditionKind::Threshold {
                series: "temperature".to_owned(),
                direction: ThresholdDirection::Above,
                threshold: 150.0,
                hysteresis: 2.0,
            },
            Duration::from_secs(5),
        );
        assert!(!reset.update(&measurement("temperature", 100.0, 151.0), start, 100.0));
        assert!(!reset.update(
            &measurement("temperature", 104.0, 148.0),
            start + Duration::from_secs(4),
            104.0,
        ));
        assert!(!reset.update(
            &measurement("temperature", 105.0, 151.0),
            start + Duration::from_secs(5),
            105.0,
        ));
    }

    #[test]
    fn inside_and_outside_respect_boundaries_and_hysteresis() {
        let now = Instant::now();
        let mut inside = state(
            ScenarioConditionKind::Inside {
                series: "temperature".to_owned(),
                minimum: 10.0,
                maximum: 20.0,
                hysteresis: 1.0,
            },
            Duration::ZERO,
        );
        assert!(inside.update(&measurement("temperature", 100.0, 10.0), now, 100.0));
        assert!(inside.update(&measurement("temperature", 101.0, 9.5), now, 101.0));
        assert!(!inside.update(&measurement("temperature", 102.0, 8.9), now, 102.0));

        let mut outside = state(
            ScenarioConditionKind::Outside {
                series: "temperature".to_owned(),
                minimum: 10.0,
                maximum: 20.0,
                hysteresis: 1.0,
            },
            Duration::ZERO,
        );
        assert!(!outside.update(&measurement("temperature", 100.0, 10.0), now, 100.0));
        assert!(outside.update(&measurement("temperature", 101.0, 9.0), now, 101.0));
        assert!(outside.update(&measurement("temperature", 102.0, 10.5), now, 102.0));
        assert!(!outside.update(&measurement("temperature", 103.0, 11.0), now, 103.0));
    }

    #[test]
    fn stable_condition_restarts_hold_after_leaving_tolerance() {
        let start = Instant::now();
        let mut condition = state(
            ScenarioConditionKind::Stable {
                series: "temperature".to_owned(),
                target: 100.0,
                tolerance: 1.0,
                hysteresis: 0.0,
            },
            Duration::from_secs(5),
        );

        assert!(!condition.update(&measurement("temperature", 100.0, 100.5), start, 100.0));
        assert!(!condition.update(
            &measurement("temperature", 104.0, 102.0),
            start + Duration::from_secs(4),
            104.0,
        ));
        assert!(!condition.update(
            &measurement("temperature", 110.0, 99.5),
            start + Duration::from_secs(10),
            110.0,
        ));
        assert!(condition.update(
            &measurement("temperature", 115.0, 100.0),
            start + Duration::from_secs(15),
            115.0,
        ));
    }

    #[test]
    fn rate_uses_measurement_timestamps_and_bounded_window_history() {
        let start = Instant::now();
        let mut condition = state(
            ScenarioConditionKind::Rate {
                series: "temperature".to_owned(),
                direction: ThresholdDirection::Above,
                threshold: 1.5,
                window: Duration::from_secs(10),
                hysteresis: 0.0,
            },
            Duration::ZERO,
        );

        for (second, value) in [(0_u32, 0.0), (3, 6.0), (9, 18.0), (11, 22.0)] {
            let timestamp = 100.0 + f64::from(second);
            let triggered = condition.update(
                &measurement("temperature", timestamp, value),
                start + Duration::from_secs(u64::from(second)),
                timestamp,
            );
            assert_eq!(triggered, second >= 10);
        }

        for second in 12_u32..=100 {
            let timestamp = 100.0 + f64::from(second);
            assert!(condition.update(
                &measurement("temperature", timestamp, f64::from(second) * 2.0),
                start + Duration::from_secs(u64::from(second)),
                timestamp,
            ));
        }

        let ConditionNodeKind::Rate { history, .. } = &condition.node.kind else {
            panic!("expected rate condition state");
        };
        assert!(history.len() <= 12, "history length was {}", history.len());
    }

    #[test]
    fn rate_hysteresis_uses_the_computed_window_slope() {
        let start = Instant::now();
        let mut condition = state(
            ScenarioConditionKind::Rate {
                series: "temperature".to_owned(),
                direction: ThresholdDirection::Above,
                threshold: 1.5,
                window: Duration::from_secs(2),
                hysteresis: 0.5,
            },
            Duration::ZERO,
        );

        assert!(!condition.update(&measurement("temperature", 100.0, 0.0), start, 100.0));
        assert!(condition.update(
            &measurement("temperature", 102.0, 4.0),
            start + Duration::from_secs(2),
            102.0,
        ));
        assert!(condition.update(
            &measurement("temperature", 104.0, 7.0),
            start + Duration::from_secs(4),
            104.0,
        ));
        assert!(!condition.update(
            &measurement("temperature", 106.0, 9.0),
            start + Duration::from_secs(6),
            106.0,
        ));
    }

    #[test]
    fn stale_uses_measurement_timestamp_and_ticks_without_samples() {
        let start = Instant::now();
        let mut condition = state(
            ScenarioConditionKind::Stale {
                series: "temperature".to_owned(),
                duration: Duration::from_secs(5),
            },
            Duration::ZERO,
        );

        assert!(!condition.tick(start + Duration::from_secs(4), 104.0));
        assert!(!condition.update(
            &measurement("temperature", 104.0, 100.0),
            start + Duration::from_secs(4),
            104.0,
        ));
        assert!(!condition.tick(start + Duration::from_secs(8), 108.9));
        assert!(condition.tick(start + Duration::from_secs(9), 109.0));
    }

    #[test]
    fn all_and_any_compose_conditions_from_different_series() {
        let now = Instant::now();
        let mut all = state(
            ScenarioConditionKind::All(vec![above("temperature", 100.0), above("pressure", 5.0)]),
            Duration::ZERO,
        );
        assert!(!all.update(&measurement("temperature", 100.0, 101.0), now, 100.0));
        assert!(all.update(&measurement("pressure", 101.0, 6.0), now, 101.0));

        let mut any = state(
            ScenarioConditionKind::Any(vec![above("temperature", 100.0), above("pressure", 5.0)]),
            Duration::ZERO,
        );
        assert!(any.update(&measurement("pressure", 100.0, 6.0), now, 100.0));
    }

    #[test]
    fn ignores_non_increasing_rate_timestamps() {
        let start = Instant::now();
        let mut condition = state(
            ScenarioConditionKind::Rate {
                series: "temperature".to_owned(),
                direction: ThresholdDirection::Below,
                threshold: -1.0,
                window: Duration::from_secs(2),
                hysteresis: 0.0,
            },
            Duration::ZERO,
        );

        assert!(!condition.update(&measurement("temperature", 100.0, 10.0), start, 100.0));
        assert!(!condition.update(&measurement("temperature", 99.0, 0.0), start, 100.0));
        assert!(condition.update(
            &measurement("temperature", 102.0, 6.0),
            start + Duration::from_secs(2),
            102.0,
        ));
    }
}
