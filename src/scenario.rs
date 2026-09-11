use std::{
    collections::{HashMap, HashSet},
    fmt,
    time::{Duration, Instant, SystemTime},
};

use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::{
    app_log::LogHandle,
    application_event::ApplicationEvent,
    lua_worker::{LuaWorkerHandle, LuaWorkerHandleError},
    process_recorder::{ProcessActionId, ProcessMeasurement},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ScenarioId(String);

impl ScenarioId {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, ScenarioDefinitionError> {
        let value = value.into();
        let mut characters = value.chars();
        let valid_first = characters
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_');
        let valid_remaining =
            characters.all(|character| character.is_ascii_alphanumeric() || character == '_');

        if !valid_first || !valid_remaining {
            return Err(ScenarioDefinitionError(format!(
                "Invalid scenario id '{value}': use an ASCII letter or underscore first, followed by letters, digits or underscores",
            )));
        }

        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ScenarioCommand {
    Register {
        id: ScenarioId,
    },
    After {
        id: ScenarioId,
        delay: Duration,
        callback: String,
    },
    At {
        id: ScenarioId,
        deadline: SystemTime,
        callback: String,
    },
    When {
        id: ScenarioId,
        condition: ScenarioCondition,
        callback: String,
    },
    Race {
        id: ScenarioId,
        alternatives: Vec<ScenarioRaceAlternative>,
    },
    Cancel {
        id: ScenarioId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScenarioRaceAlternative {
    pub(crate) trigger: ScenarioRaceTrigger,
    pub(crate) callback: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ScenarioRaceTrigger {
    After { delay: Duration },
    At { deadline: SystemTime },
    When { condition: ScenarioCondition },
}

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
    pub(crate) series: String,
    pub(crate) direction: ThresholdDirection,
    pub(crate) threshold: f64,
    pub(crate) hold: Duration,
    pub(crate) hysteresis: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ScenarioCallbackTrigger {
    Timer {
        delay: Duration,
    },
    AbsoluteTime {
        scheduled_at: SystemTime,
    },
    Measurement {
        series: String,
        value: f64,
        timestamp: f64,
        condition: ScenarioCondition,
    },
}

impl ScenarioCallbackTrigger {
    pub(crate) const fn name(&self) -> &'static str {
        match self {
            Self::Timer { .. } => "timer",
            Self::AbsoluteTime { .. } => "absolute_time",
            Self::Measurement { .. } => "measurement",
        }
    }

    fn reason(&self) -> String {
        match self {
            Self::Timer { delay } => {
                format!("timer elapsed after {} seconds", delay.as_secs_f64())
            }
            Self::AbsoluteTime { scheduled_at } => {
                let timestamp = scheduled_at
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map_or(0.0, |duration| duration.as_secs_f64());
                format!("absolute timer reached Unix timestamp {timestamp}")
            }
            Self::Measurement {
                series,
                value,
                condition,
                ..
            } => format!(
                "measurement '{series}'={value} satisfied {} {}",
                condition.direction.as_str(),
                condition.threshold,
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScenarioCallbackInvocation {
    scenario_id: ScenarioId,
    callback: String,
    fired_at: SystemTime,
    trigger: ScenarioCallbackTrigger,
}

impl ScenarioCallbackInvocation {
    pub(crate) fn new(
        scenario_id: ScenarioId,
        callback: String,
        fired_at: SystemTime,
        trigger: ScenarioCallbackTrigger,
    ) -> Self {
        Self {
            scenario_id,
            callback,
            fired_at,
            trigger,
        }
    }

    pub(crate) fn scenario_id(&self) -> &ScenarioId {
        &self.scenario_id
    }

    pub(crate) fn callback(&self) -> &str {
        &self.callback
    }

    pub(crate) const fn fired_at(&self) -> SystemTime {
        self.fired_at
    }

    pub(crate) const fn trigger(&self) -> &ScenarioCallbackTrigger {
        &self.trigger
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScenarioCallbackResult {
    pub(crate) invocation: ScenarioCallbackInvocation,
    pub(crate) error: Option<String>,
}

struct ScenarioState {
    timers: Vec<ScenarioTimer>,
    conditions: Vec<ScenarioConditionState>,
    races: Vec<ScenarioRaceState>,
    callback_in_flight: bool,
    pending_actions: HashSet<ProcessActionId>,
    waiting_callback: Option<String>,
}

impl ScenarioState {
    fn new() -> Self {
        Self {
            timers: Vec::new(),
            conditions: Vec::new(),
            races: Vec::new(),
            callback_in_flight: false,
            pending_actions: HashSet::new(),
            waiting_callback: None,
        }
    }

    fn is_busy(&self) -> bool {
        self.callback_in_flight || !self.pending_actions.is_empty()
    }

    fn poll_race(
        &mut self,
        now: Instant,
        measurements: &[ProcessMeasurement],
    ) -> Option<ScenarioRaceWinner> {
        let (race_index, winner) = self
            .races
            .iter_mut()
            .enumerate()
            .find_map(|(index, race)| race.poll(now, measurements).map(|winner| (index, winner)))?;

        self.races.remove(race_index);
        Some(winner)
    }
}

struct ScenarioTimer {
    deadline: Instant,
    callback: String,
    trigger: ScenarioCallbackTrigger,
}

impl ScenarioTimer {
    fn after(now: Instant, delay: Duration, callback: String) -> Option<Self> {
        Some(Self {
            deadline: now.checked_add(delay)?,
            callback,
            trigger: ScenarioCallbackTrigger::Timer { delay },
        })
    }

    fn at(
        now: Instant,
        system_now: SystemTime,
        scheduled_at: SystemTime,
        callback: String,
    ) -> Option<Self> {
        let delay = scheduled_at
            .duration_since(system_now)
            .unwrap_or(Duration::ZERO);

        Some(Self {
            deadline: now.checked_add(delay)?,
            callback,
            trigger: ScenarioCallbackTrigger::AbsoluteTime { scheduled_at },
        })
    }
}

struct ScenarioConditionState {
    condition: ScenarioCondition,
    callback: String,
    matching_since: Option<Instant>,
    armed: bool,
}

struct ScenarioRaceState {
    alternatives: Vec<ScenarioRaceAlternativeState>,
}

enum ScenarioRaceAlternativeState {
    Timer(ScenarioTimer),
    Condition(ScenarioConditionState),
}

struct ScenarioRaceWinner {
    alternative_index: usize,
    callback: String,
    trigger: ScenarioCallbackTrigger,
}

impl ScenarioRaceState {
    fn new(
        alternatives: Vec<ScenarioRaceAlternative>,
        now: Instant,
        system_now: SystemTime,
    ) -> Option<Self> {
        if alternatives.is_empty() {
            return None;
        }

        let alternatives = alternatives
            .into_iter()
            .map(|alternative| match alternative.trigger {
                ScenarioRaceTrigger::After { delay } => {
                    ScenarioTimer::after(now, delay, alternative.callback)
                        .map(ScenarioRaceAlternativeState::Timer)
                }
                ScenarioRaceTrigger::At { deadline } => {
                    ScenarioTimer::at(now, system_now, deadline, alternative.callback)
                        .map(ScenarioRaceAlternativeState::Timer)
                }
                ScenarioRaceTrigger::When { condition } => {
                    Some(ScenarioRaceAlternativeState::Condition(
                        ScenarioConditionState::new(condition, alternative.callback),
                    ))
                }
            })
            .collect::<Option<Vec<_>>>()?;

        Some(Self { alternatives })
    }

    fn poll(
        &mut self,
        now: Instant,
        measurements: &[ProcessMeasurement],
    ) -> Option<ScenarioRaceWinner> {
        for (alternative_index, alternative) in self.alternatives.iter_mut().enumerate() {
            let ready = match alternative {
                ScenarioRaceAlternativeState::Timer(timer) => {
                    (timer.deadline <= now).then(|| (timer.callback.clone(), timer.trigger.clone()))
                }
                ScenarioRaceAlternativeState::Condition(condition) => {
                    let series = condition.condition.series.clone();
                    measurements
                        .iter()
                        .filter(|measurement| series == measurement.series_name)
                        .find_map(|measurement| {
                            condition.update(measurement.value, now).then(|| {
                                (
                                    condition.callback.clone(),
                                    ScenarioCallbackTrigger::Measurement {
                                        series: measurement.series_name.clone(),
                                        value: measurement.value,
                                        timestamp: measurement.timestamp,
                                        condition: condition.condition.clone(),
                                    },
                                )
                            })
                        })
                }
            };

            if let Some((callback, trigger)) = ready {
                return Some(ScenarioRaceWinner {
                    alternative_index,
                    callback,
                    trigger,
                });
            }
        }

        None
    }
}

impl ScenarioConditionState {
    fn new(condition: ScenarioCondition, callback: String) -> Self {
        Self {
            condition,
            callback,
            matching_since: None,
            armed: true,
        }
    }

    fn update(&mut self, value: f64, now: Instant) -> bool {
        let matching = match self.condition.direction {
            ThresholdDirection::Above => value > self.condition.threshold,
            ThresholdDirection::Below => value < self.condition.threshold,
        };
        let rearmed = match self.condition.direction {
            ThresholdDirection::Above => {
                value <= self.condition.threshold - self.condition.hysteresis
            }
            ThresholdDirection::Below => {
                value >= self.condition.threshold + self.condition.hysteresis
            }
        };

        if rearmed {
            self.armed = true;
            self.matching_since = None;
            return false;
        }

        if !self.armed {
            return false;
        }

        if matching {
            self.matching_since.get_or_insert(now);
        }

        let Some(matching_since) = self.matching_since else {
            return false;
        };

        if now.duration_since(matching_since) >= self.condition.hold {
            self.armed = false;
            self.matching_since = None;
            return true;
        }

        false
    }
}

pub(crate) struct ScenarioService {
    scenarios: HashMap<ScenarioId, ScenarioState>,
    application_events: Receiver<ApplicationEvent>,
    callback_results: Receiver<ScenarioCallbackResult>,
    callback_result_sender: Sender<ScenarioCallbackResult>,
    lua: LuaWorkerHandle,
    log: LogHandle,
}

impl ScenarioService {
    pub(crate) fn new(
        application_events: Receiver<ApplicationEvent>,
        lua: LuaWorkerHandle,
        log: LogHandle,
    ) -> Self {
        let (callback_result_sender, callback_results) = unbounded();

        Self {
            scenarios: HashMap::new(),
            application_events,
            callback_results,
            callback_result_sender,
            lua,
            log,
        }
    }

    pub(crate) fn execute(&mut self, command: ScenarioCommand) {
        match command {
            ScenarioCommand::Register { id } => {
                let replaced = self
                    .scenarios
                    .insert(id.clone(), ScenarioState::new())
                    .is_some();
                if replaced {
                    self.log.info(format!(
                        "Scenario '{}' restarted; previous tasks were cancelled.",
                        id.as_str(),
                    ));
                } else {
                    self.log
                        .info(format!("Scenario '{}' started.", id.as_str()));
                }
            }

            ScenarioCommand::After {
                id,
                delay,
                callback,
            } => {
                let Some(scenario) = self.scenarios.get_mut(&id) else {
                    self.unknown_scenario(&id);
                    return;
                };
                let Some(timer) = ScenarioTimer::after(Instant::now(), delay, callback) else {
                    self.log.error(format!(
                        "Scenario '{}' delay is outside the supported range.",
                        id.as_str(),
                    ));
                    return;
                };
                scenario.timers.push(timer);
            }

            ScenarioCommand::At {
                id,
                deadline,
                callback,
            } => {
                let Some(scenario) = self.scenarios.get_mut(&id) else {
                    self.unknown_scenario(&id);
                    return;
                };
                let Some(timer) =
                    ScenarioTimer::at(Instant::now(), SystemTime::now(), deadline, callback)
                else {
                    self.log.error(format!(
                        "Scenario '{}' absolute deadline is outside the supported range.",
                        id.as_str(),
                    ));
                    return;
                };
                scenario.timers.push(timer);
            }

            ScenarioCommand::When {
                id,
                condition,
                callback,
            } => {
                let Some(scenario) = self.scenarios.get_mut(&id) else {
                    self.unknown_scenario(&id);
                    return;
                };
                scenario
                    .conditions
                    .push(ScenarioConditionState::new(condition, callback));
            }

            ScenarioCommand::Race { id, alternatives } => {
                let Some(scenario) = self.scenarios.get_mut(&id) else {
                    self.unknown_scenario(&id);
                    return;
                };
                let Some(race) =
                    ScenarioRaceState::new(alternatives, Instant::now(), SystemTime::now())
                else {
                    self.log.error(format!(
                        "Scenario '{}' race is empty or contains a deadline outside the supported range.",
                        id.as_str(),
                    ));
                    return;
                };
                scenario.races.push(race);
            }

            ScenarioCommand::Cancel { id } => {
                if self.scenarios.remove(&id).is_some() {
                    self.log
                        .info(format!("Scenario '{}' cancelled.", id.as_str()));
                } else {
                    self.unknown_scenario(&id);
                }
            }
        }
    }

    pub(crate) fn track_action(&mut self, id: &ScenarioId, action_id: ProcessActionId) {
        if let Some(scenario) = self.scenarios.get_mut(id) {
            scenario.pending_actions.insert(action_id);
        } else {
            self.log.error(format!(
                "Scenario '{}' produced action {} after it stopped.",
                id.as_str(),
                action_id.value(),
            ));
        }
    }

    pub(crate) fn poll(&mut self) {
        self.poll_callback_results();

        let now = Instant::now();
        let fired_at = SystemTime::now();
        let events = self.application_events.try_iter().collect::<Vec<_>>();
        let race_eligible = self
            .scenarios
            .iter()
            .filter(|(_, scenario)| !scenario.is_busy())
            .map(|(id, _)| id.clone())
            .collect::<HashSet<_>>();
        let mut callbacks = Vec::new();
        let mut scheduled = HashSet::new();
        let mut measurements = Vec::new();
        let mut race_winners = Vec::new();

        for (id, scenario) in &mut self.scenarios {
            if scenario.is_busy() {
                continue;
            }

            scenario.timers.retain(|timer| {
                if timer.deadline <= now && !scheduled.contains(id) {
                    callbacks.push(ScenarioCallbackInvocation::new(
                        id.clone(),
                        timer.callback.clone(),
                        fired_at,
                        timer.trigger.clone(),
                    ));
                    scheduled.insert(id.clone());
                    false
                } else {
                    true
                }
            });
        }

        for event in events {
            match event {
                ApplicationEvent::Measurements(event_measurements) => {
                    for measurement in &event_measurements {
                        for (id, scenario) in &mut self.scenarios {
                            if scenario.is_busy() || scheduled.contains(id) {
                                continue;
                            }

                            scenario.conditions.retain_mut(|condition| {
                                if condition.condition.series != measurement.series_name {
                                    return true;
                                }

                                if condition.update(measurement.value, now) {
                                    callbacks.push(ScenarioCallbackInvocation::new(
                                        id.clone(),
                                        condition.callback.clone(),
                                        fired_at,
                                        ScenarioCallbackTrigger::Measurement {
                                            series: measurement.series_name.clone(),
                                            value: measurement.value,
                                            timestamp: measurement.timestamp,
                                            condition: condition.condition.clone(),
                                        },
                                    ));
                                    scheduled.insert(id.clone());
                                    false
                                } else {
                                    true
                                }
                            });
                        }
                    }

                    measurements.extend(event_measurements);
                }

                ApplicationEvent::ActionApplied { action_id, .. } => {
                    self.action_applied(action_id);
                }

                ApplicationEvent::ActionFailed { action_id, error } => {
                    self.action_failed(action_id, error);
                }

                ApplicationEvent::ActionRequested { .. } => {}
            }
        }

        for (id, scenario) in &mut self.scenarios {
            if !race_eligible.contains(id) || scenario.is_busy() || scheduled.contains(id) {
                continue;
            }

            let Some(winner) = scenario.poll_race(now, &measurements) else {
                continue;
            };

            race_winners.push((
                id.clone(),
                winner.alternative_index,
                winner.callback.clone(),
                winner.trigger.reason(),
            ));
            callbacks.push(ScenarioCallbackInvocation::new(
                id.clone(),
                winner.callback,
                fired_at,
                winner.trigger,
            ));
            scheduled.insert(id.clone());
        }

        for (id, alternative_index, callback, reason) in race_winners {
            self.log.info(format!(
                "Scenario '{}' race selected alternative #{} callback '{}': {reason}.",
                id.as_str(),
                alternative_index + 1,
                callback,
            ));
        }

        for invocation in callbacks {
            self.invoke_callback(invocation);
        }
    }

    fn poll_callback_results(&mut self) {
        for result in self.callback_results.try_iter() {
            let id = result.invocation.scenario_id().clone();
            let callback = result.invocation.callback().to_owned();

            if let Some(error) = result.error {
                self.scenarios.remove(&id);
                self.log.error(format!(
                    "Scenario '{}' callback '{callback}' failed; scenario stopped: {error}",
                    id.as_str(),
                ));
            } else if let Some(scenario) = self.scenarios.get_mut(&id) {
                scenario.callback_in_flight = false;
                if scenario.pending_actions.is_empty() {
                    self.log.info(format!(
                        "Scenario '{}' callback '{callback}' completed.",
                        id.as_str(),
                    ));
                } else {
                    scenario.waiting_callback = Some(callback);
                }
            }
        }
    }

    fn action_applied(&mut self, action_id: ProcessActionId) {
        for (id, scenario) in &mut self.scenarios {
            if !scenario.pending_actions.remove(&action_id) {
                continue;
            }

            if scenario.pending_actions.is_empty()
                && !scenario.callback_in_flight
                && let Some(callback) = scenario.waiting_callback.take()
            {
                self.log.info(format!(
                    "Scenario '{}' callback '{callback}' completed.",
                    id.as_str(),
                ));
            }

            return;
        }
    }

    fn action_failed(&mut self, action_id: ProcessActionId, error: String) {
        let failed_scenario = self.scenarios.iter().find_map(|(id, scenario)| {
            scenario
                .pending_actions
                .contains(&action_id)
                .then(|| id.clone())
        });

        if let Some(id) = failed_scenario {
            self.scenarios.remove(&id);
            self.log.error(format!(
                "Scenario '{}' action {} failed; scenario stopped: {error}",
                id.as_str(),
                action_id.value(),
            ));
        }
    }

    fn invoke_callback(&mut self, invocation: ScenarioCallbackInvocation) {
        let id = invocation.scenario_id().clone();
        let callback = invocation.callback().to_owned();

        let Some(scenario) = self.scenarios.get_mut(&id) else {
            return;
        };
        scenario.callback_in_flight = true;

        if let Err(error) = self
            .lua
            .invoke_scenario_callback(invocation, self.callback_result_sender.clone())
        {
            self.stop_after_dispatch_failure(&id, &callback, error);
            return;
        }

        self.log.info(format!(
            "Scenario '{}' triggered callback '{callback}'.",
            id.as_str(),
        ));
    }

    fn stop_after_dispatch_failure(
        &mut self,
        id: &ScenarioId,
        callback: &str,
        error: LuaWorkerHandleError,
    ) {
        self.scenarios.remove(id);
        self.log.error(format!(
            "Scenario '{}' could not invoke callback '{callback}'; scenario stopped: {error}",
            id.as_str(),
        ));
    }

    fn unknown_scenario(&self, id: &ScenarioId) {
        self.log.error(format!(
            "Scenario '{}' is not registered; call app.scenario() first.",
            id.as_str(),
        ));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScenarioDefinitionError(String);

impl fmt::Display for ScenarioDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ScenarioDefinitionError {}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant, SystemTime};

    use super::{
        ScenarioCallbackTrigger, ScenarioCondition, ScenarioConditionState,
        ScenarioRaceAlternative, ScenarioRaceState, ScenarioRaceTrigger, ScenarioState,
        ThresholdDirection,
    };
    use crate::{connection::ConnectionId, data::SeriesId, process_recorder::ProcessMeasurement};

    #[test]
    fn triggers_above_threshold_after_hold_time() {
        let start = Instant::now();
        let mut state = ScenarioConditionState::new(
            ScenarioCondition {
                series: "temperature".to_owned(),
                direction: ThresholdDirection::Above,
                threshold: 150.0,
                hold: Duration::from_secs(5),
                hysteresis: 2.0,
            },
            "hold".to_owned(),
        );

        assert!(!state.update(151.0, start));
        assert!(!state.update(151.0, start + Duration::from_secs(4)));
        assert!(state.update(151.0, start + Duration::from_secs(5)));
        assert!(!state.update(151.0, start + Duration::from_secs(6)));
    }

    #[test]
    fn hysteresis_rearms_threshold_condition() {
        let start = Instant::now();
        let mut state = ScenarioConditionState::new(
            ScenarioCondition {
                series: "temperature".to_owned(),
                direction: ThresholdDirection::Above,
                threshold: 150.0,
                hold: Duration::from_secs(5),
                hysteresis: 2.0,
            },
            "hold".to_owned(),
        );

        assert!(!state.update(151.0, start));
        assert!(!state.update(149.0, start + Duration::from_secs(4)));
        assert!(state.update(149.0, start + Duration::from_secs(5)));

        let mut state = ScenarioConditionState::new(
            ScenarioCondition {
                series: "temperature".to_owned(),
                direction: ThresholdDirection::Above,
                threshold: 150.0,
                hold: Duration::from_secs(5),
                hysteresis: 2.0,
            },
            "hold".to_owned(),
        );
        assert!(!state.update(151.0, start));
        assert!(!state.update(148.0, start + Duration::from_secs(4)));
        assert!(!state.update(151.0, start + Duration::from_secs(5)));
        assert!(state.update(151.0, start + Duration::from_secs(10)));
    }

    #[test]
    fn race_uses_declaration_order_and_cancels_losers() {
        let now = Instant::now();
        let condition = ScenarioCondition {
            series: "temperature".to_owned(),
            direction: ThresholdDirection::Above,
            threshold: 150.0,
            hold: Duration::ZERO,
            hysteresis: 0.0,
        };
        let mut scenario = ScenarioState::new();
        scenario.races.push(
            ScenarioRaceState::new(
                vec![
                    ScenarioRaceAlternative {
                        trigger: ScenarioRaceTrigger::When { condition },
                        callback: "measurement_won".to_owned(),
                    },
                    ScenarioRaceAlternative {
                        trigger: ScenarioRaceTrigger::After {
                            delay: Duration::ZERO,
                        },
                        callback: "timer_lost".to_owned(),
                    },
                ],
                now,
                SystemTime::now(),
            )
            .unwrap(),
        );
        let measurements = vec![ProcessMeasurement {
            connection_id: ConnectionId::PRIMARY,
            series_id: SeriesId::new(1),
            series_name: "temperature".to_owned(),
            timestamp: 10.0,
            value: 151.0,
        }];

        let winner = scenario.poll_race(now, &measurements).unwrap();

        assert_eq!(winner.alternative_index, 0);
        assert_eq!(winner.callback, "measurement_won");
        assert!(matches!(
            winner.trigger,
            ScenarioCallbackTrigger::Measurement { .. }
        ));
        assert!(scenario.races.is_empty());
        assert!(
            scenario
                .poll_race(now + Duration::from_secs(1), &[])
                .is_none()
        );
    }

    #[test]
    fn race_reports_winning_trigger_reason() {
        let trigger = ScenarioCallbackTrigger::Timer {
            delay: Duration::from_secs(30),
        };

        assert_eq!(trigger.reason(), "timer elapsed after 30 seconds");
    }
}
