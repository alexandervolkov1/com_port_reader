use std::fmt;

use crate::{
    connection::ConnectionId,
    instrument::{InstrumentWriteRequest, ParameterRange},
};

use super::{
    ControlOutputConversionError, ControlOutputParameter, ControlOutputTarget, PidControllerError,
    PidLoop, PidLoopDefinition, PidOutput, PidOutputLimits,
};

pub struct PidLoopRegistry<SignalId> {
    loops: Vec<PidLoop<SignalId, ControlOutputTarget>>,
}

impl<SignalId> Default for PidLoopRegistry<SignalId> {
    fn default() -> Self {
        Self { loops: Vec::new() }
    }
}

impl<SignalId> PidLoopRegistry<SignalId>
where
    SignalId: Copy + Eq,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(
        &mut self,
        definition: PidLoopDefinition<SignalId, ControlOutputTarget>,
    ) -> Result<(), PidLoopRegistryError> {
        let name = definition.name();

        if self.contains(name) {
            return Err(PidLoopRegistryError::DuplicateName(name.to_owned()));
        }

        let target = definition.output_target();

        for existing in &self.loops {
            let existing_definition = existing.definition();

            if output_targets_overlap(existing_definition.output_target(), target) {
                return Err(PidLoopRegistryError::OutputAlreadyControlled {
                    existing_loop: existing_definition.name().to_owned(),

                    target: *target,
                });
            }
        }

        validate_output_limits(definition.output_limits(), target)?;

        self.loops.push(PidLoop::new(definition));

        Ok(())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.loops
            .iter()
            .any(|control_loop| control_loop.definition().name() == name)
    }

    pub fn len(&self) -> usize {
        self.loops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.loops.is_empty()
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let Some(index) = self
            .loops
            .iter()
            .position(|control_loop| control_loop.definition().name() == name)
        else {
            return false;
        };

        self.loops.remove(index);

        true
    }

    pub fn remove_from(&mut self, signal_id: SignalId) -> Vec<String> {
        let mut removed = Vec::new();

        self.loops.retain(|control_loop| {
            let definition = control_loop.definition();

            if *definition.input() == signal_id {
                removed.push(definition.name().to_owned());

                false
            } else {
                true
            }
        });

        removed
    }

    pub fn reset_from(&mut self, signal_id: SignalId) -> usize {
        let mut reset_count = 0;

        for control_loop in &mut self.loops {
            if *control_loop.definition().input() == signal_id {
                control_loop.reset();

                reset_count += 1;
            }
        }

        reset_count
    }

    pub fn clear(&mut self) {
        self.loops.clear();
    }

    pub fn process(
        &mut self,
        signal_id: SignalId,
        timestamp: f64,
        measurement: f64,
    ) -> Vec<PidLoopEvent<SignalId>> {
        let mut events = Vec::new();

        for control_loop in &mut self.loops {
            if *control_loop.definition().input() != signal_id {
                continue;
            }

            let loop_name = control_loop.definition().name().to_owned();

            let target = *control_loop.definition().output_target();

            let output = match control_loop.update(timestamp, measurement) {
                Ok(output) => output,

                Err(source) => {
                    events.push(PidLoopEvent::Error(PidLoopExecutionError::Controller {
                        loop_name,
                        source,
                    }));

                    continue;
                }
            };

            let request = match target.write_request(output.value()) {
                Ok(request) => request,

                Err(source) => {
                    events.push(PidLoopEvent::Error(PidLoopExecutionError::Output {
                        loop_name,
                        source,
                    }));

                    continue;
                }
            };

            events.push(PidLoopEvent::Output(PidLoopOutput {
                loop_name,

                input: signal_id,

                timestamp,

                output,

                connection_id: target.connection_id(),

                request,
            }));
        }

        events
    }
}

fn output_targets_overlap(left: &ControlOutputTarget, right: &ControlOutputTarget) -> bool {
    if left.connection_id() != right.connection_id() {
        return false;
    }

    match (left.parameter(), right.parameter()) {
        (
            ControlOutputParameter::Metakon5x3 {
                instrument: left_instrument,

                parameter: left_parameter,
                ..
            },
            ControlOutputParameter::Metakon5x3 {
                instrument: right_instrument,

                parameter: right_parameter,
                ..
            },
        ) => left_instrument == right_instrument && left_parameter == right_parameter,

        (
            ControlOutputParameter::VirtualInstrument {
                instrument: left_instrument,

                parameter: left_parameter,
                ..
            },
            ControlOutputParameter::VirtualInstrument {
                instrument: right_instrument,

                parameter: right_parameter,
                ..
            },
        ) => left_instrument == right_instrument && left_parameter == right_parameter,

        _ => false,
    }
}

fn validate_output_limits(
    limits: PidOutputLimits,
    target: &ControlOutputTarget,
) -> Result<(), PidLoopRegistryError> {
    let Some(range) = target.range() else {
        return Ok(());
    };

    let (target_minimum, target_maximum) = match range {
        ParameterRange::Integer { minimum, maximum } => (minimum as f64, maximum as f64),

        ParameterRange::Number { minimum, maximum } => (minimum, maximum),
    };

    if limits.minimum() < target_minimum || limits.maximum() > target_maximum {
        return Err(PidLoopRegistryError::OutputLimitsOutsideRange {
            minimum: limits.minimum(),

            maximum: limits.maximum(),

            target_minimum,

            target_maximum,
        });
    }

    Ok(())
}

#[derive(Debug)]
pub enum PidLoopEvent<SignalId> {
    Output(PidLoopOutput<SignalId>),

    Error(PidLoopExecutionError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PidLoopOutput<SignalId> {
    pub loop_name: String,

    pub input: SignalId,

    pub timestamp: f64,

    pub output: PidOutput,

    pub connection_id: ConnectionId,

    pub request: InstrumentWriteRequest,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PidLoopRegistryError {
    DuplicateName(String),

    OutputAlreadyControlled {
        existing_loop: String,

        target: ControlOutputTarget,
    },

    OutputLimitsOutsideRange {
        minimum: f64,

        maximum: f64,

        target_minimum: f64,

        target_maximum: f64,
    },
}

impl fmt::Display for PidLoopRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateName(name) => {
                write!(
                    formatter,
                    "PID loop '{name}' \
                     already exists",
                )
            }

            Self::OutputAlreadyControlled {
                existing_loop,

                target,
            } => {
                write!(
                    formatter,
                    "Output target {target} \
                     is already controlled \
                     by PID loop \
                     '{existing_loop}'",
                )
            }

            Self::OutputLimitsOutsideRange {
                minimum,

                maximum,

                target_minimum,

                target_maximum,
            } => {
                write!(
                    formatter,
                    "PID output limits \
                     {minimum}..={maximum} \
                     exceed target range \
                     {target_minimum}..=\
                     {target_maximum}",
                )
            }
        }
    }
}

impl std::error::Error for PidLoopRegistryError {}

#[derive(Debug)]
pub enum PidLoopExecutionError {
    Controller {
        loop_name: String,

        source: PidControllerError,
    },

    Output {
        loop_name: String,

        source: ControlOutputConversionError,
    },
}

impl fmt::Display for PidLoopExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Controller { loop_name, source } => {
                write!(
                    formatter,
                    "PID loop '{loop_name}' \
                     failed: {source}",
                )
            }

            Self::Output { loop_name, source } => {
                write!(
                    formatter,
                    "PID loop '{loop_name}' \
                     output conversion \
                     failed: {source}",
                )
            }
        }
    }
}

impl std::error::Error for PidLoopExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Controller { source, .. } => Some(source),

            Self::Output { source, .. } => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        connection::ConnectionId,
        instrument::{
            InstrumentValue, InstrumentWriteRequest, ParameterAccess, ParameterRange,
            ParameterValueType,
            metakon_5x3::{Metakon5x3, Metakon5x3Register},
            virtual_instrument::{
                VirtualInstrumentId, VirtualParameterDescriptor, VirtualParameterId,
            },
        },
        process_control::{ControlOutputTarget, PidGains, PidLoopDefinition, PidOutputLimits},
    };

    use super::{PidLoopEvent, PidLoopExecutionError, PidLoopRegistry, PidLoopRegistryError};

    fn virtual_target(connection: u64, instrument: u16, parameter: u16) -> ControlOutputTarget {
        let descriptor = VirtualParameterDescriptor::new(
            VirtualParameterId::new(parameter),
            format!("power_{parameter}",),
            "Power",
            ParameterAccess::ReadWrite,
            ParameterValueType::Number,
        )
        .with_range(ParameterRange::Number {
            minimum: 0.0,

            maximum: 100.0,
        });

        ControlOutputTarget::virtual_instrument(
            ConnectionId::new(connection),
            VirtualInstrumentId::new(instrument),
            &descriptor,
        )
        .unwrap()
    }

    fn definition(
        name: &str,
        input: u64,
        target: ControlOutputTarget,
    ) -> PidLoopDefinition<u64, ControlOutputTarget> {
        PidLoopDefinition::new(
            name,
            input,
            target,
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn starts_empty() {
        let registry = PidLoopRegistry::<u64>::new();

        assert!(registry.is_empty());

        assert_eq!(registry.len(), 0,);

        assert!(!registry.contains("heater"),);
    }

    #[test]
    fn registers_pid_loop() {
        let mut registry = PidLoopRegistry::new();

        registry
            .add(definition("heater", 1, virtual_target(1, 1, 1)))
            .unwrap();

        assert_eq!(registry.len(), 1,);

        assert!(registry.contains("heater"),);

        assert!(!registry.is_empty(),);
    }

    #[test]
    fn rejects_duplicate_loop_name() {
        let mut registry = PidLoopRegistry::new();

        registry
            .add(definition("heater", 1, virtual_target(1, 1, 1)))
            .unwrap();

        let result = registry.add(definition("heater", 2, virtual_target(1, 1, 2)));

        assert_eq!(
            result,
            Err(PidLoopRegistryError::DuplicateName("heater".to_owned(),),),
        );

        assert_eq!(registry.len(), 1,);
    }

    #[test]
    fn rejects_duplicate_virtual_output() {
        let mut registry = PidLoopRegistry::new();

        let target = virtual_target(1, 1, 1);

        registry.add(definition("heater_one", 1, target)).unwrap();

        let result = registry.add(definition("heater_two", 2, target));

        assert_eq!(
            result,
            Err(PidLoopRegistryError::OutputAlreadyControlled {
                existing_loop: "heater_one".to_owned(),

                target,
            },),
        );
    }

    #[test]
    fn rejects_same_metakon_parameter_with_different_scales() {
        let mut registry = PidLoopRegistry::new();

        let first = ControlOutputTarget::metakon_5x3(
            ConnectionId::PRIMARY,
            Metakon5x3::new(3, 0),
            Metakon5x3Register::Setpoint,
            1.0,
        )
        .unwrap();

        let second = ControlOutputTarget::metakon_5x3(
            ConnectionId::PRIMARY,
            Metakon5x3::new(3, 0),
            Metakon5x3Register::Setpoint,
            0.1,
        )
        .unwrap();

        registry.add(definition("heater_one", 1, first)).unwrap();

        let result = registry.add(definition("heater_two", 2, second));

        assert_eq!(
            result,
            Err(PidLoopRegistryError::OutputAlreadyControlled {
                existing_loop: "heater_one".to_owned(),

                target: second,
            },),
        );
    }

    #[test]
    fn allows_same_parameter_on_different_connections() {
        let mut registry = PidLoopRegistry::new();

        registry
            .add(definition("heater_one", 1, virtual_target(1, 1, 1)))
            .unwrap();

        registry
            .add(definition("heater_two", 2, virtual_target(2, 1, 1)))
            .unwrap();

        assert_eq!(registry.len(), 2,);
    }

    #[test]
    fn rejects_limits_outside_target_range() {
        let target = virtual_target(1, 1, 1);

        let definition = PidLoopDefinition::new(
            "heater",
            1_u64,
            target,
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(-1.0, 100.0).unwrap(),
        )
        .unwrap();

        let mut registry = PidLoopRegistry::new();

        let result = registry.add(definition);

        assert_eq!(
            result,
            Err(PidLoopRegistryError::OutputLimitsOutsideRange {
                minimum: -1.0,

                maximum: 100.0,

                target_minimum: 0.0,

                target_maximum: 100.0,
            },),
        );

        assert!(registry.is_empty(),);
    }

    #[test]
    fn creates_output_for_matching_input() {
        let mut registry = PidLoopRegistry::new();

        registry
            .add(definition("heater", 5, virtual_target(2, 7, 4)))
            .unwrap();

        let events = registry.process(5, 1_000.0, 80.0);

        assert_eq!(events.len(), 1,);

        let event = events.into_iter().next().unwrap();

        let PidLoopEvent::Output(output) = event else {
            panic!("expected PID output event",);
        };

        assert_eq!(output.loop_name, "heater",);

        assert_eq!(output.input, 5,);

        assert_eq!(output.timestamp, 1_000.0,);

        assert_eq!(output.output.value(), 40.0,);

        assert_eq!(output.connection_id, ConnectionId::new(2),);

        assert_eq!(
            output.request,
            InstrumentWriteRequest::VirtualInstrument {
                instrument: VirtualInstrumentId::new(7,),

                parameter: VirtualParameterId::new(4,),

                value: InstrumentValue::Number(40.0,),
            },
        );
    }

    #[test]
    fn ignores_unrelated_input() {
        let mut registry = PidLoopRegistry::new();

        registry
            .add(definition("heater", 5, virtual_target(1, 1, 1)))
            .unwrap();

        let events = registry.process(6, 1_000.0, 80.0);

        assert!(events.is_empty(),);
    }

    #[test]
    fn processes_multiple_loops_from_same_input() {
        let mut registry = PidLoopRegistry::new();

        registry
            .add(definition("heater_one", 5, virtual_target(1, 1, 1)))
            .unwrap();

        registry
            .add(definition("heater_two", 5, virtual_target(2, 1, 1)))
            .unwrap();

        let events = registry.process(5, 1_000.0, 80.0);

        assert_eq!(events.len(), 2,);

        assert!(matches!(
            &events[0],

            PidLoopEvent::Output(
                output,
            ) if output.loop_name
                == "heater_one"
        ),);

        assert!(matches!(
            &events[1],

            PidLoopEvent::Output(
                output,
            ) if output.loop_name
                == "heater_two"
        ),);
    }

    #[test]
    fn reports_controller_errors() {
        let mut registry = PidLoopRegistry::new();

        registry
            .add(definition("heater", 5, virtual_target(1, 1, 1)))
            .unwrap();

        let events = registry.process(5, f64::NAN, 80.0);

        assert_eq!(events.len(), 1,);

        assert!(matches!(
            &events[0],

            PidLoopEvent::Error(
                PidLoopExecutionError::
                    Controller {
                        loop_name,
                        ..
                    },
            ) if loop_name == "heater"
        ),);
    }

    #[test]
    fn removes_loop_by_name() {
        let mut registry = PidLoopRegistry::new();

        registry
            .add(definition("heater", 1, virtual_target(1, 1, 1)))
            .unwrap();

        assert!(registry.remove("heater"),);

        assert!(registry.is_empty(),);

        assert!(!registry.remove("heater"),);
    }

    #[test]
    fn removes_loops_for_deleted_input() {
        let mut registry = PidLoopRegistry::new();

        registry
            .add(definition("first", 1, virtual_target(1, 1, 1)))
            .unwrap();

        registry
            .add(definition("second", 1, virtual_target(1, 1, 2)))
            .unwrap();

        registry
            .add(definition("third", 2, virtual_target(1, 1, 3)))
            .unwrap();

        let removed = registry.remove_from(1);

        assert_eq!(removed, vec!["first".to_owned(), "second".to_owned(),],);

        assert_eq!(registry.len(), 1,);

        assert!(registry.contains("third"),);
    }

    #[test]
    fn resets_only_matching_loops() {
        let mut registry = PidLoopRegistry::new();

        registry
            .add(definition("first", 1, virtual_target(1, 1, 1)))
            .unwrap();

        registry
            .add(definition("second", 1, virtual_target(1, 1, 2)))
            .unwrap();

        registry
            .add(definition("third", 2, virtual_target(1, 1, 3)))
            .unwrap();

        assert_eq!(registry.reset_from(1), 2,);

        assert_eq!(registry.reset_from(3), 0,);
    }

    #[test]
    fn clears_all_loops() {
        let mut registry = PidLoopRegistry::new();

        registry
            .add(definition("heater", 1, virtual_target(1, 1, 1)))
            .unwrap();

        registry.clear();

        assert!(registry.is_empty(),);
    }

    #[test]
    fn describes_registry_errors() {
        assert_eq!(
            PidLoopRegistryError::DuplicateName("heater".to_owned(),).to_string(),
            "PID loop 'heater' already exists",
        );

        assert_eq!(
            PidLoopRegistryError::OutputLimitsOutsideRange {
                minimum: -10.0,

                maximum: 100.0,

                target_minimum: 0.0,

                target_maximum: 100.0,
            }
            .to_string(),
            "PID output limits -10..=100 \
             exceed target range 0..=100",
        );
    }
}
