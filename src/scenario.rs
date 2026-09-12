use std::{
    collections::{HashMap, HashSet},
    fmt,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::{
    app_log::LogHandle,
    application_event::ApplicationEvent,
    lua_worker::{LuaWorkerHandle, LuaWorkerHandleError},
    process_recorder::{ProcessActionId, ProcessMeasurement},
};

mod condition;

use condition::ScenarioConditionState;
pub(crate) use condition::{ScenarioCondition, ScenarioConditionKind, ThresholdDirection};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ScenarioId(String);

impl ScenarioId {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, ScenarioDefinitionError> {
        let value = value.into();
        if !is_valid_identifier(&value) {
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ScenarioStageName(String);

impl ScenarioStageName {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, ScenarioDefinitionError> {
        let value = value.into();
        if !is_valid_identifier(&value) {
            return Err(ScenarioDefinitionError(format!(
                "Invalid scenario stage name '{value}': use an ASCII letter or underscore first, followed by letters, digits or underscores",
            )));
        }

        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ScenarioRunId(u64);

impl ScenarioRunId {
    const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScenarioStatus {
    Running,
    Completed,
    Stopped,
    Cancelled,
    Failed,
}

impl ScenarioStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Stopped => "stopped",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    const fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }
}

fn is_valid_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
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
    DefineStage {
        id: ScenarioId,
        name: ScenarioStageName,
        definition: ScenarioStageDefinition,
    },
    Start {
        id: ScenarioId,
        stage: ScenarioStageName,
    },
    OnStop {
        id: ScenarioId,
        callback: String,
    },
    OnError {
        id: ScenarioId,
        callback: String,
    },
    Complete {
        id: ScenarioId,
        reason: String,
    },
    Stop {
        id: ScenarioId,
        reason: String,
    },
    Cancel {
        id: ScenarioId,
    },
}

impl ScenarioCommand {
    fn id(&self) -> &ScenarioId {
        match self {
            Self::Register { id }
            | Self::After { id, .. }
            | Self::At { id, .. }
            | Self::When { id, .. }
            | Self::Race { id, .. }
            | Self::DefineStage { id, .. }
            | Self::Start { id, .. }
            | Self::OnStop { id, .. }
            | Self::OnError { id, .. }
            | Self::Complete { id, .. }
            | Self::Stop { id, .. }
            | Self::Cancel { id } => id,
        }
    }
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScenarioStageDefinition {
    pub(crate) enter: String,
    pub(crate) transitions: Vec<ScenarioStageTransition>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScenarioStageTransition {
    pub(crate) trigger: ScenarioRaceTrigger,
    pub(crate) next: ScenarioStageName,
    pub(crate) reason: Option<String>,
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
        measurement: Option<ScenarioMeasurementContext>,
        condition: ScenarioCondition,
    },
    Stage {
        stage: ScenarioStageName,
        previous_stage: Option<ScenarioStageName>,
        reason: String,
        transition_trigger: Option<String>,
    },
    Stop {
        status: ScenarioStatus,
        reason: String,
    },
    Error {
        error: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScenarioMeasurementContext {
    pub(crate) series: String,
    pub(crate) value: f64,
    pub(crate) timestamp: f64,
}

impl ScenarioCallbackTrigger {
    pub(crate) const fn name(&self) -> &'static str {
        match self {
            Self::Timer { .. } => "timer",
            Self::AbsoluteTime { .. } => "absolute_time",
            Self::Measurement { .. } => "measurement",
            Self::Stage { .. } => "stage",
            Self::Stop { .. } => "stop",
            Self::Error { .. } => "error",
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
                measurement,
                condition,
                ..
            } => measurement.as_ref().map_or_else(
                || format!("condition '{}' became true", condition.kind_name(),),
                |measurement| {
                    format!(
                        "measurement '{}'={} satisfied condition '{}'",
                        measurement.series,
                        measurement.value,
                        condition.kind_name(),
                    )
                },
            ),
            Self::Stage { stage, reason, .. } => {
                format!("entered stage '{}': {reason}", stage.as_str())
            }
            Self::Stop { status, reason } => {
                format!("scenario {}: {reason}", status.as_str())
            }
            Self::Error { error } => format!("scenario failed: {error}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScenarioCallbackInvocation {
    scenario_id: ScenarioId,
    run_id: ScenarioRunId,
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
            run_id: ScenarioRunId::default(),
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

    pub(crate) const fn run_id(&self) -> ScenarioRunId {
        self.run_id
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
    run_id: ScenarioRunId,
    status: ScenarioStatus,
    timers: Vec<ScenarioTimer>,
    conditions: Vec<ScenarioConditionState>,
    races: Vec<ScenarioRaceState>,
    stages: HashMap<ScenarioStageName, ScenarioStageDefinition>,
    current_stage: Option<ScenarioStageName>,
    pending_stage_activation: Option<ScenarioStageName>,
    started: bool,
    callback_in_flight: bool,
    pending_actions: HashSet<ProcessActionId>,
    waiting_callback: Option<String>,
    on_stop: Option<String>,
    on_error: Option<String>,
    finalization: Option<ScenarioFinalization>,
    cleanup_in_flight: bool,
}

impl ScenarioState {
    fn new(run_id: ScenarioRunId) -> Self {
        Self {
            run_id,
            status: ScenarioStatus::Running,
            timers: Vec::new(),
            conditions: Vec::new(),
            races: Vec::new(),
            stages: HashMap::new(),
            current_stage: None,
            pending_stage_activation: None,
            started: false,
            callback_in_flight: false,
            pending_actions: HashSet::new(),
            waiting_callback: None,
            on_stop: None,
            on_error: None,
            finalization: None,
            cleanup_in_flight: false,
        }
    }

    fn is_busy(&self) -> bool {
        self.callback_in_flight || !self.pending_actions.is_empty()
    }

    fn accepts_steps(&self, run_id: ScenarioRunId) -> bool {
        self.run_id == run_id && !self.status.is_terminal()
    }

    fn clear_waits(&mut self) {
        self.timers.clear();
        self.conditions.clear();
        self.races.clear();
        self.pending_stage_activation = None;
    }

    fn poll_race(
        &mut self,
        now: Instant,
        unix_now: f64,
        measurements: &[ProcessMeasurement],
    ) -> Option<ScenarioRaceWinner> {
        let (race_index, winner) =
            self.races
                .iter_mut()
                .enumerate()
                .find_map(|(index, race)| {
                    race.poll(now, unix_now, measurements)
                        .map(|winner| (index, winner))
                })?;

        self.races.remove(race_index);
        Some(winner)
    }

    fn activate_pending_stage(
        &mut self,
        now: Instant,
        system_now: SystemTime,
    ) -> Result<Option<ScenarioStageName>, String> {
        let Some(stage) = self.pending_stage_activation.take() else {
            return Ok(None);
        };
        let definition = self
            .stages
            .get(&stage)
            .expect("pending scenario stage must be defined");

        if !definition.transitions.is_empty() {
            let race = ScenarioRaceState::from_stage_transitions(
                definition.transitions.clone(),
                now,
                system_now,
            )
            .ok_or_else(|| {
                format!(
                    "Stage '{}' contains a deadline outside the supported range",
                    stage.as_str(),
                )
            })?;
            self.races.push(race);
        }

        Ok(Some(stage))
    }

    fn validate_start(&self, stage: &ScenarioStageName) -> Result<(), String> {
        if self.started {
            return Err("scenario stages have already been started".to_owned());
        }
        if self.is_busy() {
            return Err("scenario is waiting for another callback or action".to_owned());
        }
        if !self.stages.contains_key(stage) {
            return Err(format!("start stage '{}' is not defined", stage.as_str()));
        }

        for (source, definition) in &self.stages {
            for transition in &definition.transitions {
                if !self.stages.contains_key(&transition.next) {
                    return Err(format!(
                        "stage '{}' has a transition to unknown stage '{}'",
                        source.as_str(),
                        transition.next.as_str(),
                    ));
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
enum ScenarioCleanupKind {
    Stop,
    Error,
}

impl ScenarioCleanupKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Error => "error",
        }
    }
}

struct ScenarioFinalization {
    intended_status: ScenarioStatus,
    reason: String,
    handler: Option<(ScenarioCleanupKind, String)>,
    cleanup_error: Option<String>,
}

struct ScenarioTimer {
    deadline: Instant,
    target: ScenarioTriggerTarget,
    trigger: ScenarioCallbackTrigger,
}

#[derive(Clone)]
enum ScenarioTriggerTarget {
    Callback(String),
    StageTransition {
        next: ScenarioStageName,
        reason: Option<String>,
    },
}

impl ScenarioTimer {
    fn after(now: Instant, delay: Duration, target: ScenarioTriggerTarget) -> Option<Self> {
        Some(Self {
            deadline: now.checked_add(delay)?,
            target,
            trigger: ScenarioCallbackTrigger::Timer { delay },
        })
    }

    fn at(
        now: Instant,
        system_now: SystemTime,
        scheduled_at: SystemTime,
        target: ScenarioTriggerTarget,
    ) -> Option<Self> {
        let delay = scheduled_at
            .duration_since(system_now)
            .unwrap_or(Duration::ZERO);

        Some(Self {
            deadline: now.checked_add(delay)?,
            target,
            trigger: ScenarioCallbackTrigger::AbsoluteTime { scheduled_at },
        })
    }
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
    target: ScenarioTriggerTarget,
    trigger: ScenarioCallbackTrigger,
}

impl ScenarioRaceState {
    fn new(
        alternatives: Vec<ScenarioRaceAlternative>,
        now: Instant,
        system_now: SystemTime,
    ) -> Option<Self> {
        let alternatives = alternatives
            .into_iter()
            .map(|alternative| {
                (
                    alternative.trigger,
                    ScenarioTriggerTarget::Callback(alternative.callback),
                )
            })
            .collect();

        Self::from_triggers(alternatives, now, system_now)
    }

    fn from_stage_transitions(
        transitions: Vec<ScenarioStageTransition>,
        now: Instant,
        system_now: SystemTime,
    ) -> Option<Self> {
        let alternatives = transitions
            .into_iter()
            .map(|transition| {
                (
                    transition.trigger,
                    ScenarioTriggerTarget::StageTransition {
                        next: transition.next,
                        reason: transition.reason,
                    },
                )
            })
            .collect();

        Self::from_triggers(alternatives, now, system_now)
    }

    fn from_triggers(
        alternatives: Vec<(ScenarioRaceTrigger, ScenarioTriggerTarget)>,
        now: Instant,
        system_now: SystemTime,
    ) -> Option<Self> {
        if alternatives.is_empty() {
            return None;
        }

        let alternatives = alternatives
            .into_iter()
            .map(|(trigger, target)| match trigger {
                ScenarioRaceTrigger::After { delay } => ScenarioTimer::after(now, delay, target)
                    .map(ScenarioRaceAlternativeState::Timer),
                ScenarioRaceTrigger::At { deadline } => {
                    ScenarioTimer::at(now, system_now, deadline, target)
                        .map(ScenarioRaceAlternativeState::Timer)
                }
                ScenarioRaceTrigger::When { condition } => {
                    Some(ScenarioRaceAlternativeState::Condition(
                        ScenarioConditionState::new(condition, target, system_now),
                    ))
                }
            })
            .collect::<Option<Vec<_>>>()?;

        Some(Self { alternatives })
    }

    fn poll(
        &mut self,
        now: Instant,
        unix_now: f64,
        measurements: &[ProcessMeasurement],
    ) -> Option<ScenarioRaceWinner> {
        for (alternative_index, alternative) in self.alternatives.iter_mut().enumerate() {
            let ready = match alternative {
                ScenarioRaceAlternativeState::Timer(timer) => {
                    (timer.deadline <= now).then(|| (timer.target.clone(), timer.trigger.clone()))
                }
                ScenarioRaceAlternativeState::Condition(condition) => {
                    let measurement = measurements
                        .iter()
                        .find(|measurement| condition.update(measurement, now, unix_now));
                    if let Some(measurement) = measurement {
                        Some((
                            condition.target.clone(),
                            ScenarioCallbackTrigger::Measurement {
                                measurement: Some(ScenarioMeasurementContext {
                                    series: measurement.series_name.clone(),
                                    value: measurement.value,
                                    timestamp: measurement.timestamp,
                                }),
                                condition: condition.condition.clone(),
                            },
                        ))
                    } else {
                        condition.tick(now, unix_now).then(|| {
                            (
                                condition.target.clone(),
                                ScenarioCallbackTrigger::Measurement {
                                    measurement: None,
                                    condition: condition.condition.clone(),
                                },
                            )
                        })
                    }
                }
            };

            if let Some((target, trigger)) = ready {
                return Some(ScenarioRaceWinner {
                    alternative_index,
                    target,
                    trigger,
                });
            }
        }

        None
    }
}

pub(crate) struct ScenarioService {
    scenarios: HashMap<ScenarioId, ScenarioState>,
    application_events: Receiver<ApplicationEvent>,
    callback_results: Receiver<ScenarioCallbackResult>,
    callback_result_sender: Sender<ScenarioCallbackResult>,
    lua: LuaWorkerHandle,
    log: LogHandle,
    next_run_id: u64,
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
            next_run_id: 1,
        }
    }

    pub(crate) fn execute(&mut self, command: ScenarioCommand) {
        if !matches!(
            &command,
            ScenarioCommand::Register { .. } | ScenarioCommand::Cancel { .. }
        ) {
            let id = command.id();
            if self.scenarios.get(id).is_some_and(|scenario| {
                scenario.status.is_terminal() || scenario.finalization.is_some()
            }) {
                self.log.error(format!(
                    "Scenario '{}' is already finalizing or finished.",
                    id.as_str(),
                ));
                return;
            }
        }

        match command {
            ScenarioCommand::Register { id } => {
                let run_id = ScenarioRunId::new(self.next_run_id);
                self.next_run_id = self.next_run_id.wrapping_add(1).max(1);
                let replaced = self
                    .scenarios
                    .insert(id.clone(), ScenarioState::new(run_id))
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
                let Some(timer) = ScenarioTimer::after(
                    Instant::now(),
                    delay,
                    ScenarioTriggerTarget::Callback(callback),
                ) else {
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
                let Some(timer) = ScenarioTimer::at(
                    Instant::now(),
                    SystemTime::now(),
                    deadline,
                    ScenarioTriggerTarget::Callback(callback),
                ) else {
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
                scenario.conditions.push(ScenarioConditionState::new(
                    condition,
                    ScenarioTriggerTarget::Callback(callback),
                    SystemTime::now(),
                ));
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

            ScenarioCommand::DefineStage {
                id,
                name,
                definition,
            } => {
                let Some(scenario) = self.scenarios.get_mut(&id) else {
                    self.unknown_scenario(&id);
                    return;
                };
                if scenario.started {
                    self.log.error(format!(
                        "Scenario '{}' cannot define stage '{}' after stage execution started.",
                        id.as_str(),
                        name.as_str(),
                    ));
                    return;
                }
                if scenario.stages.contains_key(&name) {
                    self.log.error(format!(
                        "Scenario '{}' contains duplicate stage '{}'.",
                        id.as_str(),
                        name.as_str(),
                    ));
                    return;
                }
                scenario.stages.insert(name, definition);
            }

            ScenarioCommand::Start { id, stage } => {
                self.start_stage(id, stage);
            }

            ScenarioCommand::OnStop { id, callback } => {
                let Some(scenario) = self.scenarios.get_mut(&id) else {
                    self.unknown_scenario(&id);
                    return;
                };
                if scenario.on_stop.is_some() {
                    self.log.error(format!(
                        "Scenario '{}' stop handler is already defined.",
                        id.as_str(),
                    ));
                } else {
                    scenario.on_stop = Some(callback);
                }
            }

            ScenarioCommand::OnError { id, callback } => {
                let Some(scenario) = self.scenarios.get_mut(&id) else {
                    self.unknown_scenario(&id);
                    return;
                };
                if scenario.on_error.is_some() {
                    self.log.error(format!(
                        "Scenario '{}' error handler is already defined.",
                        id.as_str(),
                    ));
                } else {
                    scenario.on_error = Some(callback);
                }
            }

            ScenarioCommand::Complete { id, reason } => {
                self.request_finalization(&id, ScenarioStatus::Completed, reason, false);
            }

            ScenarioCommand::Stop { id, reason } => {
                self.request_finalization(&id, ScenarioStatus::Stopped, reason, false);
            }

            ScenarioCommand::Cancel { id } => {
                let Some(scenario) = self.scenarios.get_mut(&id) else {
                    self.unknown_scenario(&id);
                    return;
                };
                if scenario.status.is_terminal() {
                    self.log.error(format!(
                        "Scenario '{}' is already finished with status '{}'.",
                        id.as_str(),
                        scenario.status.as_str(),
                    ));
                    return;
                }
                scenario.clear_waits();
                scenario.pending_actions.clear();
                scenario.waiting_callback = None;
                scenario.callback_in_flight = false;
                scenario.finalization = None;
                scenario.cleanup_in_flight = false;
                scenario.status = ScenarioStatus::Cancelled;
                self.log.info(format!(
                    "Scenario '{}' finalized: status=cancelled; reason=cancel() requested.",
                    id.as_str(),
                ));
            }
        }
    }

    pub(crate) fn accepts_step(&self, id: &ScenarioId, run_id: ScenarioRunId) -> bool {
        self.scenarios
            .get(id)
            .is_some_and(|scenario| scenario.accepts_steps(run_id))
    }

    fn request_finalization(
        &mut self,
        id: &ScenarioId,
        status: ScenarioStatus,
        reason: String,
        discard_pending_actions: bool,
    ) {
        let Some(scenario) = self.scenarios.get_mut(id) else {
            self.unknown_scenario(id);
            return;
        };
        if scenario.status.is_terminal() {
            return;
        }
        if scenario.finalization.is_some() && status != ScenarioStatus::Failed {
            return;
        }

        scenario.clear_waits();
        if discard_pending_actions {
            scenario.pending_actions.clear();
            scenario.waiting_callback = None;
        }
        scenario.cleanup_in_flight = false;
        let handler = match status {
            ScenarioStatus::Completed | ScenarioStatus::Stopped => scenario
                .on_stop
                .clone()
                .map(|callback| (ScenarioCleanupKind::Stop, callback)),
            ScenarioStatus::Failed => scenario
                .on_error
                .clone()
                .map(|callback| (ScenarioCleanupKind::Error, callback)),
            ScenarioStatus::Running | ScenarioStatus::Cancelled => None,
        };
        scenario.finalization = Some(ScenarioFinalization {
            intended_status: status,
            reason,
            handler,
            cleanup_error: None,
        });

        self.advance_finalization(id);
    }

    fn advance_finalization(&mut self, id: &ScenarioId) {
        let invocation = {
            let Some(scenario) = self.scenarios.get_mut(id) else {
                return;
            };
            if scenario.is_busy() || scenario.cleanup_in_flight {
                return;
            }
            let Some(finalization) = &scenario.finalization else {
                return;
            };
            let Some((kind, callback)) = &finalization.handler else {
                self.finalize_scenario(id);
                return;
            };

            scenario.cleanup_in_flight = true;
            let trigger = match kind {
                ScenarioCleanupKind::Stop => ScenarioCallbackTrigger::Stop {
                    status: finalization.intended_status,
                    reason: finalization.reason.clone(),
                },
                ScenarioCleanupKind::Error => ScenarioCallbackTrigger::Error {
                    error: finalization.reason.clone(),
                },
            };
            ScenarioCallbackInvocation::new(
                id.clone(),
                callback.clone(),
                SystemTime::now(),
                trigger,
            )
        };

        self.invoke_callback(invocation);
    }

    fn cleanup_failed(&mut self, id: &ScenarioId, callback: &str, error: String) {
        let Some(scenario) = self.scenarios.get_mut(id) else {
            return;
        };
        let Some(finalization) = scenario.finalization.as_mut() else {
            return;
        };
        let original_reason = finalization.reason.clone();
        let cleanup_kind = finalization
            .handler
            .as_ref()
            .map_or("cleanup", |(kind, _)| kind.as_str());
        finalization.cleanup_error = Some(error.clone());
        scenario.callback_in_flight = false;
        scenario.cleanup_in_flight = false;
        scenario.pending_actions.clear();
        scenario.waiting_callback = None;

        self.log.error(format!(
            "Scenario '{}' {cleanup_kind} handler '{callback}' failed: {error}. Original termination reason: {original_reason}",
            id.as_str(),
        ));
        self.finalize_scenario(id);
    }

    fn finalize_scenario(&mut self, id: &ScenarioId) {
        let Some(scenario) = self.scenarios.get_mut(id) else {
            return;
        };
        let Some(finalization) = scenario.finalization.take() else {
            return;
        };
        let status = if finalization.cleanup_error.is_some() {
            ScenarioStatus::Failed
        } else {
            finalization.intended_status
        };
        scenario.status = status;
        scenario.callback_in_flight = false;
        scenario.cleanup_in_flight = false;
        scenario.pending_actions.clear();
        scenario.waiting_callback = None;

        let message = finalization.cleanup_error.map_or_else(
            || {
                format!(
                    "Scenario '{}' finalized: status={}; reason={}.",
                    id.as_str(),
                    status.as_str(),
                    finalization.reason,
                )
            },
            |cleanup_error| {
                format!(
                    "Scenario '{}' finalized: status={}; reason={}; cleanup_error={}.",
                    id.as_str(),
                    status.as_str(),
                    finalization.reason,
                    cleanup_error,
                )
            },
        );
        if status == ScenarioStatus::Failed {
            self.log.error(message);
        } else {
            self.log.info(message);
        }
    }

    fn start_stage(&mut self, id: ScenarioId, stage: ScenarioStageName) {
        let callback = {
            let Some(scenario) = self.scenarios.get_mut(&id) else {
                self.unknown_scenario(&id);
                return;
            };
            if let Err(error) = scenario.validate_start(&stage) {
                self.log.error(format!(
                    "Scenario '{}' could not start stage '{}': {error}.",
                    id.as_str(),
                    stage.as_str(),
                ));
                return;
            }

            scenario.started = true;
            scenario.current_stage = Some(stage.clone());
            scenario.pending_stage_activation = Some(stage.clone());
            scenario
                .stages
                .get(&stage)
                .expect("validated start stage must be defined")
                .enter
                .clone()
        };

        self.log.info(format!(
            "Scenario '{}' starting at stage '{}'.",
            id.as_str(),
            stage.as_str(),
        ));
        self.invoke_callback(ScenarioCallbackInvocation::new(
            id,
            callback,
            SystemTime::now(),
            ScenarioCallbackTrigger::Stage {
                stage,
                previous_stage: None,
                reason: "Scenario started".to_owned(),
                transition_trigger: None,
            },
        ));
    }

    fn prepare_stage_transition(
        &mut self,
        id: &ScenarioId,
        next: ScenarioStageName,
        reason: Option<String>,
        trigger: ScenarioCallbackTrigger,
        fired_at: SystemTime,
    ) -> ScenarioCallbackInvocation {
        let previous = self
            .scenarios
            .get(id)
            .expect("stage transition must belong to a running scenario")
            .current_stage
            .clone()
            .expect("stage transition must have a current stage");
        let reason = reason.unwrap_or_else(|| trigger.reason());
        let transition_trigger = trigger.name().to_owned();
        let callback = {
            let scenario = self
                .scenarios
                .get_mut(id)
                .expect("scenario stage transition must belong to a running scenario");
            let definition = scenario
                .stages
                .get(&next)
                .expect("scenario transition target must be validated before start");
            let callback = definition.enter.clone();
            scenario.current_stage = Some(next.clone());
            scenario.pending_stage_activation = Some(next.clone());
            callback
        };

        self.log.info(format!(
            "Scenario '{}' selected transition '{}' -> '{}': {reason}.",
            id.as_str(),
            previous.as_str(),
            next.as_str(),
        ));

        ScenarioCallbackInvocation::new(
            id.clone(),
            callback,
            fired_at,
            ScenarioCallbackTrigger::Stage {
                stage: next,
                previous_stage: Some(previous),
                reason,
                transition_trigger: Some(transition_trigger),
            },
        )
    }

    pub(crate) fn track_action(
        &mut self,
        id: &ScenarioId,
        run_id: ScenarioRunId,
        action_id: ProcessActionId,
    ) {
        if let Some(scenario) = self
            .scenarios
            .get_mut(id)
            .filter(|scenario| scenario.accepts_steps(run_id))
        {
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
        let unix_now = fired_at
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |duration| duration.as_secs_f64());
        let events = self.application_events.try_iter().collect::<Vec<_>>();
        let race_eligible = self
            .scenarios
            .iter()
            .filter(|(_, scenario)| {
                !scenario.is_busy()
                    && !scenario.status.is_terminal()
                    && scenario.finalization.is_none()
            })
            .map(|(id, _)| id.clone())
            .collect::<HashSet<_>>();
        let mut callbacks = Vec::new();
        let mut scheduled = HashSet::new();
        let mut measurements = Vec::new();
        let mut race_winners = Vec::new();
        let mut stage_transitions = Vec::new();

        for (id, scenario) in &mut self.scenarios {
            if scenario.is_busy()
                || scenario.status.is_terminal()
                || scenario.finalization.is_some()
            {
                continue;
            }

            scenario.timers.retain(|timer| {
                if timer.deadline <= now && !scheduled.contains(id) {
                    let ScenarioTriggerTarget::Callback(callback) = &timer.target else {
                        unreachable!("standalone scenario timer must target a callback");
                    };
                    callbacks.push(ScenarioCallbackInvocation::new(
                        id.clone(),
                        callback.clone(),
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
                                if scheduled.contains(id) {
                                    return true;
                                }
                                if condition.update(measurement, now, unix_now) {
                                    let ScenarioTriggerTarget::Callback(callback) =
                                        &condition.target
                                    else {
                                        unreachable!(
                                            "standalone scenario condition must target a callback"
                                        );
                                    };
                                    callbacks.push(ScenarioCallbackInvocation::new(
                                        id.clone(),
                                        callback.clone(),
                                        fired_at,
                                        ScenarioCallbackTrigger::Measurement {
                                            measurement: Some(ScenarioMeasurementContext {
                                                series: measurement.series_name.clone(),
                                                value: measurement.value,
                                                timestamp: measurement.timestamp,
                                            }),
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

            scenario.conditions.retain_mut(|condition| {
                if !condition.tick(now, unix_now) || scheduled.contains(id) {
                    return true;
                }
                let ScenarioTriggerTarget::Callback(callback) = &condition.target else {
                    unreachable!("standalone scenario condition must target a callback");
                };
                callbacks.push(ScenarioCallbackInvocation::new(
                    id.clone(),
                    callback.clone(),
                    fired_at,
                    ScenarioCallbackTrigger::Measurement {
                        measurement: None,
                        condition: condition.condition.clone(),
                    },
                ));
                scheduled.insert(id.clone());
                false
            });
        }

        for (id, scenario) in &mut self.scenarios {
            if !race_eligible.contains(id) || scenario.is_busy() || scheduled.contains(id) {
                continue;
            }

            let Some(winner) = scenario.poll_race(now, unix_now, &measurements) else {
                continue;
            };

            match winner.target {
                ScenarioTriggerTarget::Callback(callback) => {
                    race_winners.push((
                        id.clone(),
                        winner.alternative_index,
                        callback.clone(),
                        winner.trigger.reason(),
                    ));
                    callbacks.push(ScenarioCallbackInvocation::new(
                        id.clone(),
                        callback,
                        fired_at,
                        winner.trigger,
                    ));
                }
                ScenarioTriggerTarget::StageTransition { next, reason } => {
                    stage_transitions.push((id.clone(), next, reason, winner.trigger));
                }
            }
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

        for (id, next, reason, trigger) in stage_transitions {
            callbacks.push(self.prepare_stage_transition(&id, next, reason, trigger, fired_at));
        }

        for invocation in callbacks {
            self.invoke_callback(invocation);
        }
    }

    fn poll_callback_results(&mut self) {
        let results = self.callback_results.try_iter().collect::<Vec<_>>();

        for result in results {
            let id = result.invocation.scenario_id().clone();
            let callback = result.invocation.callback().to_owned();
            let run_id = result.invocation.run_id();

            if !self
                .scenarios
                .get(&id)
                .is_some_and(|scenario| scenario.accepts_steps(run_id))
            {
                continue;
            }

            if let Some(error) = result.error {
                let cleanup_in_flight = self
                    .scenarios
                    .get(&id)
                    .is_some_and(|scenario| scenario.cleanup_in_flight);
                if cleanup_in_flight {
                    self.cleanup_failed(&id, &callback, error);
                } else {
                    let already_failed = self.scenarios.get(&id).is_some_and(|scenario| {
                        scenario.finalization.as_ref().is_some_and(|finalization| {
                            finalization.intended_status == ScenarioStatus::Failed
                        })
                    });
                    if let Some(scenario) = self.scenarios.get_mut(&id) {
                        scenario.callback_in_flight = false;
                    }
                    if already_failed {
                        self.log.error(format!(
                            "Scenario '{}' callback '{callback}' also failed: {error}.",
                            id.as_str(),
                        ));
                        self.advance_finalization(&id);
                    } else {
                        let reason = format!("callback '{callback}' failed: {error}");
                        self.log.error(format!(
                            "Scenario '{}' {reason}; starting error finalization.",
                            id.as_str(),
                        ));
                        self.request_finalization(&id, ScenarioStatus::Failed, reason, true);
                    }
                }
            } else if let Some(scenario) = self.scenarios.get_mut(&id) {
                scenario.callback_in_flight = false;
                if scenario.pending_actions.is_empty() {
                    self.finish_callback(&id, &callback);
                } else {
                    scenario.waiting_callback = Some(callback);
                }
            }
        }
    }

    fn action_applied(&mut self, action_id: ProcessActionId) {
        let mut completed = None;

        for (id, scenario) in &mut self.scenarios {
            if !scenario.pending_actions.remove(&action_id) {
                continue;
            }

            if scenario.pending_actions.is_empty()
                && !scenario.callback_in_flight
                && let Some(callback) = scenario.waiting_callback.take()
            {
                completed = Some((id.clone(), callback));
            }

            break;
        }

        if let Some((id, callback)) = completed {
            self.finish_callback(&id, &callback);
        }
    }

    fn finish_callback(&mut self, id: &ScenarioId, callback: &str) {
        self.log.info(format!(
            "Scenario '{}' callback '{callback}' completed.",
            id.as_str(),
        ));

        let finalizing = self
            .scenarios
            .get(id)
            .is_some_and(|scenario| scenario.finalization.is_some());
        if finalizing {
            let cleanup_finished = self
                .scenarios
                .get(id)
                .is_some_and(|scenario| scenario.cleanup_in_flight);
            if cleanup_finished {
                if let Some(scenario) = self.scenarios.get_mut(id) {
                    scenario.cleanup_in_flight = false;
                }
                self.finalize_scenario(id);
            } else {
                self.advance_finalization(id);
            }
            return;
        }

        let activation = self
            .scenarios
            .get_mut(id)
            .map(|scenario| scenario.activate_pending_stage(Instant::now(), SystemTime::now()));

        match activation {
            Some(Ok(Some(stage))) => self.log.info(format!(
                "Scenario '{}' stage '{}' transitions activated.",
                id.as_str(),
                stage.as_str(),
            )),
            Some(Err(error)) => {
                self.scenarios.remove(id);
                self.log.error(format!(
                    "Scenario '{}' could not activate stage transitions; scenario stopped: {error}.",
                    id.as_str(),
                ));
            }
            Some(Ok(None)) | None => {}
        }
    }

    fn action_failed(&mut self, action_id: ProcessActionId, error: String) {
        let failed_scenario = self.scenarios.iter().find_map(|(id, scenario)| {
            scenario
                .pending_actions
                .contains(&action_id)
                .then(|| (id.clone(), scenario.cleanup_in_flight))
        });

        if let Some((id, cleanup_in_flight)) = failed_scenario {
            if cleanup_in_flight {
                let callback = self
                    .scenarios
                    .get(&id)
                    .and_then(|scenario| scenario.waiting_callback.clone())
                    .or_else(|| {
                        self.scenarios.get(&id).and_then(|scenario| {
                            scenario.finalization.as_ref().and_then(|finalization| {
                                finalization
                                    .handler
                                    .as_ref()
                                    .map(|(_, callback)| callback.clone())
                            })
                        })
                    })
                    .unwrap_or_else(|| "cleanup".to_owned());
                self.cleanup_failed(
                    &id,
                    &callback,
                    format!("action {} failed: {error}", action_id.value()),
                );
            } else {
                let reason = format!("action {} failed: {error}", action_id.value());
                self.log.error(format!(
                    "Scenario '{}' {reason}; starting error finalization.",
                    id.as_str(),
                ));
                self.request_finalization(&id, ScenarioStatus::Failed, reason, true);
            }
        }
    }

    fn invoke_callback(&mut self, mut invocation: ScenarioCallbackInvocation) {
        let id = invocation.scenario_id().clone();
        let callback = invocation.callback().to_owned();

        let Some(scenario) = self.scenarios.get_mut(&id) else {
            return;
        };
        if scenario.status.is_terminal() {
            return;
        }
        invocation.run_id = scenario.run_id;
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
        let cleanup_in_flight = self
            .scenarios
            .get(id)
            .is_some_and(|scenario| scenario.cleanup_in_flight);
        if cleanup_in_flight {
            self.cleanup_failed(id, callback, error.to_string());
        } else {
            if let Some(scenario) = self.scenarios.get_mut(id) {
                scenario.callback_in_flight = false;
            }
            let reason = format!("could not invoke callback '{callback}': {error}");
            self.log.error(format!(
                "Scenario '{}' {reason}; starting error finalization.",
                id.as_str(),
            ));
            self.request_finalization(id, ScenarioStatus::Failed, reason, true);
        }
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
        ScenarioCallbackTrigger, ScenarioCondition, ScenarioConditionKind, ScenarioRaceAlternative,
        ScenarioRaceState, ScenarioRaceTrigger, ScenarioRunId, ScenarioStageDefinition,
        ScenarioStageName, ScenarioStageTransition, ScenarioState, ScenarioTriggerTarget,
        ThresholdDirection,
    };
    use crate::{connection::ConnectionId, data::SeriesId, process_recorder::ProcessMeasurement};

    #[test]
    fn race_uses_declaration_order_and_cancels_losers() {
        let now = Instant::now();
        let condition = ScenarioCondition {
            kind: ScenarioConditionKind::Threshold {
                series: "temperature".to_owned(),
                direction: ThresholdDirection::Above,
                threshold: 150.0,
                hysteresis: 0.0,
            },
            hold: Duration::ZERO,
        };
        let mut scenario = ScenarioState::new(ScenarioRunId::new(1));
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

        let winner = scenario.poll_race(now, 10.0, &measurements).unwrap();

        assert_eq!(winner.alternative_index, 0);
        assert!(matches!(
            winner.target,
            ScenarioTriggerTarget::Callback(callback) if callback == "measurement_won"
        ));
        assert!(matches!(
            winner.trigger,
            ScenarioCallbackTrigger::Measurement { .. }
        ));
        assert!(scenario.races.is_empty());
        assert!(
            scenario
                .poll_race(now + Duration::from_secs(1), 11.0, &[])
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

    #[test]
    fn stage_transitions_start_only_when_entry_callback_is_complete() {
        let registration_time = Instant::now();
        let activation_time = registration_time + Duration::from_secs(60);
        let heating = ScenarioStageName::new("heating").unwrap();
        let holding = ScenarioStageName::new("holding").unwrap();
        let mut scenario = ScenarioState::new(ScenarioRunId::new(1));
        scenario.stages.insert(
            heating.clone(),
            ScenarioStageDefinition {
                enter: "start_heating".to_owned(),
                transitions: vec![ScenarioStageTransition {
                    trigger: ScenarioRaceTrigger::After {
                        delay: Duration::from_secs(10),
                    },
                    next: holding.clone(),
                    reason: Some("Heating interval complete".to_owned()),
                }],
            },
        );
        scenario.pending_stage_activation = Some(heating.clone());

        assert!(scenario.races.is_empty());
        assert!(scenario.poll_race(activation_time, 0.0, &[]).is_none());

        assert_eq!(
            scenario
                .activate_pending_stage(activation_time, SystemTime::now())
                .unwrap(),
            Some(heating)
        );
        assert!(
            scenario
                .poll_race(activation_time + Duration::from_secs(9), 9.0, &[])
                .is_none()
        );

        let winner = scenario
            .poll_race(activation_time + Duration::from_secs(10), 10.0, &[])
            .unwrap();
        assert!(matches!(
            winner.target,
            ScenarioTriggerTarget::StageTransition { next, reason }
                if next == holding && reason.as_deref() == Some("Heating interval complete")
        ));
    }
}
