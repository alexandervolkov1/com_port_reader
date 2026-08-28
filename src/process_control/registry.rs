use std::fmt;

use crate::{
    connection::ConnectionId,
    instrument::{InstrumentValue, InstrumentWriteRequest, ParameterDescriptor, ParameterRange},
};

use super::{
    ControlLoop, ControlLoopDefinition, ControlLoopState, ControlOutputConversionError,
    ControlOutputParameter, ControlOutputTarget, Controller, ControllerError, ControllerOutput,
    ControllerParameterError,
};

pub struct ControllerRegistry<SignalId> {
    loops: Vec<ControlLoop<SignalId, ControlOutputTarget>>,
}

impl<SignalId> Default for ControllerRegistry<SignalId> {
    fn default() -> Self {
        Self { loops: Vec::new() }
    }
}

impl<SignalId> ControllerRegistry<SignalId>
where
    SignalId: Copy + Eq,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(
        &mut self,
        definition: ControlLoopDefinition<SignalId, ControlOutputTarget>,
    ) -> Result<(), ControllerRegistryError> {
        let name = definition.name();

        if self.contains(name) {
            return Err(ControllerRegistryError::DuplicateName(name.to_owned()));
        }

        let target = definition.output_target();

        validate_controller_output(definition.controller(), target)?;

        for existing in &self.loops {
            if output_targets_overlap(existing.output_target(), target) {
                return Err(ControllerRegistryError::OutputAlreadyControlled {
                    existing_loop: existing.name().to_owned(),

                    target: *target,
                });
            }
        }

        self.loops.push(ControlLoop::new(definition));

        Ok(())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.loops
            .iter()
            .any(|control_loop| control_loop.name() == name)
    }

    fn control_loop(
        &self,
        name: &str,
    ) -> Result<&ControlLoop<SignalId, ControlOutputTarget>, ControllerAccessError> {
        self.loops
            .iter()
            .find(|control_loop| control_loop.name() == name)
            .ok_or_else(|| ControllerAccessError::ControlLoopNotFound(name.to_owned()))
    }

    fn control_loop_mut(
        &mut self,
        name: &str,
    ) -> Result<&mut ControlLoop<SignalId, ControlOutputTarget>, ControllerAccessError> {
        self.loops
            .iter_mut()
            .find(|control_loop| control_loop.name() == name)
            .ok_or_else(|| ControllerAccessError::ControlLoopNotFound(name.to_owned()))
    }

    pub fn parameters(
        &self,
        name: &str,
    ) -> Result<Vec<ParameterDescriptor>, ControllerAccessError> {
        Ok(self.control_loop(name)?.parameters())
    }

    pub fn read_parameter(
        &self,
        name: &str,
        key: &str,
    ) -> Result<InstrumentValue, ControllerAccessError> {
        self.control_loop(name)?
            .read_parameter(key)
            .map_err(Into::into)
    }

    pub fn write_parameter(
        &mut self,
        name: &str,
        key: &str,
        value: InstrumentValue,
    ) -> Result<InstrumentValue, ControllerAccessError> {
        self.control_loop_mut(name)?
            .write_parameter(key, value)
            .map_err(Into::into)
    }

    pub fn configure<I, K>(&mut self, name: &str, updates: I) -> Result<(), ControllerAccessError>
    where
        I: IntoIterator<Item = (K, InstrumentValue)>,
        K: AsRef<str>,
    {
        self.control_loop_mut(name)?
            .configure(updates)
            .map_err(Into::into)
    }

    pub fn set_input(&mut self, name: &str, input: SignalId) -> Result<(), ControllerAccessError> {
        self.control_loop_mut(name)?.set_input(input);

        Ok(())
    }

    pub fn state(&self, name: &str) -> Result<ControlLoopState, ControllerAccessError> {
        Ok(self.control_loop(name)?.state())
    }

    pub fn pause(&mut self, name: &str) -> Result<(), ControllerAccessError> {
        self.control_loop_mut(name)?.pause();

        Ok(())
    }

    pub fn resume(&mut self, name: &str) -> Result<(), ControllerAccessError> {
        self.control_loop_mut(name)?.resume();

        Ok(())
    }

    pub fn reset_integral(&mut self, name: &str) -> Result<(), ControllerAccessError> {
        self.control_loop_mut(name)?.reset_integral();

        Ok(())
    }

    pub fn reset(&mut self, name: &str) -> Result<(), ControllerAccessError> {
        self.control_loop_mut(name)?.reset();

        Ok(())
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
            .position(|control_loop| control_loop.name() == name)
        else {
            return false;
        };

        self.loops.remove(index);

        true
    }

    pub fn remove_from(&mut self, signal_id: SignalId) -> Vec<String> {
        let mut removed = Vec::new();

        self.loops.retain(|control_loop| {
            if *control_loop.input() == signal_id {
                removed.push(control_loop.name().to_owned());

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
            if *control_loop.input() == signal_id {
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
    ) -> Vec<ControlEvent<SignalId>> {
        let mut events = Vec::new();

        for control_loop in &mut self.loops {
            if *control_loop.input() != signal_id {
                continue;
            }

            let loop_name = control_loop.name().to_owned();

            let target = *control_loop.output_target();

            let output = match control_loop.process(timestamp, measurement) {
                Ok(Some(output)) => output,

                Ok(None) => {
                    continue;
                }

                Err(source) => {
                    events.push(ControlEvent::Error(ControlExecutionError::Controller {
                        loop_name,
                        source,
                    }));

                    continue;
                }
            };

            let request = match target.write_request(output.value()) {
                Ok(request) => request,

                Err(source) => {
                    events.push(ControlEvent::Error(ControlExecutionError::Output {
                        loop_name,
                        source,
                    }));

                    continue;
                }
            };

            events.push(ControlEvent::Output(ControlOutput {
                loop_name,
                input: signal_id,
                timestamp,
                measurement,
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

fn validate_controller_output(
    controller: &Controller,
    target: &ControlOutputTarget,
) -> Result<(), ControllerRegistryError> {
    let Some(target_range) = target.range() else {
        return Ok(());
    };

    let (minimum, maximum) = numeric_range_bounds(controller.output_range());

    let (target_minimum, target_maximum) = numeric_range_bounds(target_range);

    if minimum < target_minimum || maximum > target_maximum {
        return Err(ControllerRegistryError::OutputRangeOutsideTargetRange {
            minimum,
            maximum,
            target_minimum,
            target_maximum,
        });
    }

    Ok(())
}

fn numeric_range_bounds(range: ParameterRange) -> (f64, f64) {
    match range {
        ParameterRange::Integer { minimum, maximum } => (minimum as f64, maximum as f64),

        ParameterRange::Number { minimum, maximum } => (minimum, maximum),
    }
}

#[derive(Debug)]
pub enum ControlEvent<SignalId> {
    Output(ControlOutput<SignalId>),
    Error(ControlExecutionError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlOutput<SignalId> {
    pub loop_name: String,
    pub input: SignalId,
    pub timestamp: f64,
    pub measurement: f64,
    pub output: ControllerOutput,
    pub connection_id: ConnectionId,
    pub request: InstrumentWriteRequest,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControllerRegistryError {
    DuplicateName(String),

    OutputAlreadyControlled {
        existing_loop: String,
        target: ControlOutputTarget,
    },

    OutputRangeOutsideTargetRange {
        minimum: f64,
        maximum: f64,
        target_minimum: f64,
        target_maximum: f64,
    },
}

impl fmt::Display for ControllerRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateName(name) => {
                write!(formatter, "Control loop '{name}' already exists")
            }

            Self::OutputAlreadyControlled {
                existing_loop,

                target,
            } => {
                write!(
                    formatter,
                    "Output target {target} \
                    is already controlled by control loop \
                     '{existing_loop}'",
                )
            }

            Self::OutputRangeOutsideTargetRange {
                minimum,
                maximum,
                target_minimum,
                target_maximum,
            } => {
                write!(
                    formatter,
                    "Controller output range \
                     {minimum}..={maximum} \
                     exceeds target range \
                     {target_minimum}..=\
                     {target_maximum}",
                )
            }
        }
    }
}

impl std::error::Error for ControllerRegistryError {}

#[derive(Clone, Debug, PartialEq)]
pub enum ControllerAccessError {
    ControlLoopNotFound(String),

    Parameter(ControllerParameterError),
}

impl fmt::Display for ControllerAccessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ControlLoopNotFound(name) => {
                write!(
                    formatter,
                    "Control loop '{name}' \
                     was not found",
                )
            }

            Self::Parameter(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ControllerAccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ControlLoopNotFound(_) => None,

            Self::Parameter(error) => Some(error),
        }
    }
}

impl From<ControllerParameterError> for ControllerAccessError {
    fn from(error: ControllerParameterError) -> Self {
        Self::Parameter(error)
    }
}

#[derive(Debug)]
pub enum ControlExecutionError {
    Controller {
        loop_name: String,
        source: ControllerError,
    },

    Output {
        loop_name: String,
        source: ControlOutputConversionError,
    },
}

impl fmt::Display for ControlExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Controller { loop_name, source } => {
                write!(formatter, "Control loop '{loop_name}' failed: {source}")
            }

            Self::Output { loop_name, source } => {
                write!(
                    formatter,
                    "Control loop '{loop_name}' output conversion failed: {source}"
                )
            }
        }
    }
}

impl std::error::Error for ControlExecutionError {
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
        process_control::{
            ControlLoopDefinition, ControlOutputTarget, ControllerError, PidController,
            PidControllerError, PidGains, PidOutputLimits,
        },
    };

    use super::{
        ControlEvent, ControlExecutionError, ControllerAccessError, ControllerRegistry,
        ControllerRegistryError,
    };

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
    ) -> ControlLoopDefinition<u64, ControlOutputTarget> {
        let controller = PidController::with_output_limits(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap()
        .into();

        ControlLoopDefinition::new(name, input, target, controller).unwrap()
    }

    #[test]
    fn starts_empty() {
        let registry = ControllerRegistry::<u64>::new();

        assert!(registry.is_empty());

        assert_eq!(registry.len(), 0,);

        assert!(!registry.contains("heater"),);
    }

    #[test]
    fn registers_pid_loop() {
        let mut registry = ControllerRegistry::new();

        registry
            .add(definition("heater", 1, virtual_target(1, 1, 1)))
            .unwrap();

        assert_eq!(registry.len(), 1,);

        assert!(registry.contains("heater"),);

        assert!(!registry.is_empty(),);
    }

    #[test]
    fn rejects_duplicate_loop_name() {
        let mut registry = ControllerRegistry::new();

        registry
            .add(definition("heater", 1, virtual_target(1, 1, 1)))
            .unwrap();

        let result = registry.add(definition("heater", 2, virtual_target(1, 1, 2)));

        assert_eq!(
            result,
            Err(ControllerRegistryError::DuplicateName("heater".to_owned(),),),
        );

        assert_eq!(registry.len(), 1,);
    }

    #[test]
    fn rejects_duplicate_virtual_output() {
        let mut registry = ControllerRegistry::new();

        let target = virtual_target(1, 1, 1);

        registry.add(definition("heater_one", 1, target)).unwrap();

        let result = registry.add(definition("heater_two", 2, target));

        assert_eq!(
            result,
            Err(ControllerRegistryError::OutputAlreadyControlled {
                existing_loop: "heater_one".to_owned(),

                target,
            },),
        );
    }

    #[test]
    fn rejects_same_metakon_parameter_with_different_scales() {
        let mut registry = ControllerRegistry::new();

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
            Err(ControllerRegistryError::OutputAlreadyControlled {
                existing_loop: "heater_one".to_owned(),

                target: second,
            },),
        );
    }

    #[test]
    fn allows_same_parameter_on_different_connections() {
        let mut registry = ControllerRegistry::new();

        registry
            .add(definition("heater_one", 1, virtual_target(1, 1, 1)))
            .unwrap();

        registry
            .add(definition("heater_two", 2, virtual_target(2, 1, 1)))
            .unwrap();

        assert_eq!(registry.len(), 2,);
    }

    #[test]
    fn rejects_controller_output_range_outside_target_range() {
        let target = virtual_target(1, 1, 1);

        let controller = PidController::with_output_limits(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(-1.0, 100.0).unwrap(),
        )
        .unwrap()
        .into();

        let definition = ControlLoopDefinition::new("heater", 1_u64, target, controller).unwrap();

        let mut registry = ControllerRegistry::new();

        let result = registry.add(definition);

        assert_eq!(
            result,
            Err(ControllerRegistryError::OutputRangeOutsideTargetRange {
                minimum: -1.0,
                maximum: 100.0,
                target_minimum: 0.0,
                target_maximum: 100.0,
            },),
        );

        assert!(registry.is_empty());
    }

    #[test]
    fn creates_output_for_matching_input() {
        let mut registry = ControllerRegistry::new();

        registry
            .add(definition("heater", 5, virtual_target(2, 7, 4)))
            .unwrap();

        let events = registry.process(5, 1_000.0, 80.0);

        assert_eq!(events.len(), 1,);

        let event = events.into_iter().next().unwrap();

        let ControlEvent::Output(output) = event else {
            panic!("expected PID output event",);
        };

        assert_eq!(output.loop_name, "heater",);

        assert_eq!(output.input, 5,);

        assert_eq!(output.timestamp, 1_000.0,);

        assert_eq!(output.output.setpoint(), Some(100.0),);

        assert_eq!(output.measurement, 80.0,);

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
        let mut registry = ControllerRegistry::new();

        registry
            .add(definition("heater", 5, virtual_target(1, 1, 1)))
            .unwrap();

        let events = registry.process(6, 1_000.0, 80.0);

        assert!(events.is_empty(),);
    }

    #[test]
    fn processes_multiple_loops_from_same_input() {
        let mut registry = ControllerRegistry::new();

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

            ControlEvent::Output(
                output,
            ) if output.loop_name
                == "heater_one"
        ),);

        assert!(matches!(
            &events[1],

            ControlEvent::Output(
                output,
            ) if output.loop_name
                == "heater_two"
        ),);
    }

    #[test]
    fn reports_controller_errors() {
        let mut registry = ControllerRegistry::new();

        registry
            .add(definition("heater", 5, virtual_target(1, 1, 1)))
            .unwrap();

        let events = registry.process(5, f64::NAN, 80.0);

        assert_eq!(events.len(), 1,);

        assert!(matches!(
            &events[0],
            ControlEvent::Error(
                ControlExecutionError::Controller {
                    loop_name,
                    source:
                        ControllerError::Pid(
                            PidControllerError::
                                NonFiniteTimestamp
                        ),
                },
            ) if loop_name == "heater"
        ));
    }

    #[test]
    fn removes_loop_by_name() {
        let mut registry = ControllerRegistry::new();

        registry
            .add(definition("heater", 1, virtual_target(1, 1, 1)))
            .unwrap();

        assert!(registry.remove("heater"),);

        assert!(registry.is_empty(),);

        assert!(!registry.remove("heater"),);
    }

    #[test]
    fn removes_loops_for_deleted_input() {
        let mut registry = ControllerRegistry::new();

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
        let mut registry = ControllerRegistry::new();

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
        let mut registry = ControllerRegistry::new();

        registry
            .add(definition("heater", 1, virtual_target(1, 1, 1)))
            .unwrap();

        registry.clear();

        assert!(registry.is_empty(),);
    }

    #[test]
    fn describes_registry_errors() {
        assert_eq!(
            ControllerRegistryError::DuplicateName("heater".to_owned(),).to_string(),
            "Control loop 'heater' already exists",
        );

        assert_eq!(
            ControllerRegistryError::OutputRangeOutsideTargetRange {
                minimum: -10.0,
                maximum: 100.0,
                target_minimum: 0.0,
                target_maximum: 100.0,
            }
            .to_string(),
            "Controller output range \
             -10..=100 exceeds target range \
             0..=100",
        );
    }

    #[test]
    fn reads_registered_controller_parameter() {
        let mut registry = ControllerRegistry::new();

        registry
            .add(definition("heater", 1, virtual_target(1, 1, 1)))
            .unwrap();

        assert_eq!(
            registry.read_parameter("heater", "setpoint",),
            Ok(InstrumentValue::Number(100.0,),),
        );

        assert_eq!(
            registry.read_parameter("heater", "kp",),
            Ok(InstrumentValue::Number(2.0,),),
        );
    }

    #[test]
    fn writes_registered_controller_parameter() {
        let mut registry = ControllerRegistry::new();

        registry
            .add(definition("heater", 1, virtual_target(1, 1, 1)))
            .unwrap();

        assert_eq!(
            registry.write_parameter("heater", "setpoint", InstrumentValue::Number(120.0,),),
            Ok(InstrumentValue::Number(120.0,),),
        );

        assert_eq!(
            registry.read_parameter("heater", "setpoint",),
            Ok(InstrumentValue::Number(120.0,),),
        );
    }

    #[test]
    fn configures_registered_controller_atomically() {
        let mut registry = ControllerRegistry::new();

        registry
            .add(definition("heater", 1, virtual_target(1, 1, 1)))
            .unwrap();

        registry
            .configure(
                "heater",
                [
                    ("output_min", InstrumentValue::Number(200.0)),
                    ("output_max", InstrumentValue::Number(300.0)),
                ],
            )
            .unwrap();

        assert_eq!(
            registry.read_parameter("heater", "output_min",),
            Ok(InstrumentValue::Number(200.0,),),
        );

        assert_eq!(
            registry.read_parameter("heater", "output_max",),
            Ok(InstrumentValue::Number(300.0,),),
        );
    }

    #[test]
    fn reports_missing_controller_parameter_target() {
        let registry = ControllerRegistry::<u64>::new();

        assert_eq!(
            registry.read_parameter("missing", "setpoint",),
            Err(ControllerAccessError::ControlLoopNotFound(
                "missing".to_owned(),
            ),),
        );
    }

    #[test]
    fn resets_registered_controller_by_name() {
        let mut registry = ControllerRegistry::new();

        registry
            .add(definition("heater", 1, virtual_target(1, 1, 1)))
            .unwrap();

        registry.process(1, 0.0, 90.0);

        registry.process(1, 1.0, 90.0);

        registry.reset("heater").unwrap();

        let events = registry.process(1, 0.0, 90.0);

        let [ControlEvent::Output(output)] = events.as_slice() else {
            panic!("expected one control output");
        };

        assert_eq!(output.output.integral(), Some(0.0),);
    }

    #[test]
    fn changes_registered_controller_input() {
        let mut registry = ControllerRegistry::new();

        registry
            .add(definition("heater", 1, virtual_target(1, 1, 1)))
            .unwrap();

        let first = registry.process(1, 0.0, 80.0);

        assert_eq!(first.len(), 1);

        registry.set_input("heater", 2).unwrap();

        assert!(registry.process(1, 1.0, 80.0).is_empty());

        let second = registry.process(2, 100.0, 80.0);

        let [ControlEvent::Output(output)] = second.as_slice() else {
            panic!("expected one control output");
        };

        assert_eq!(output.loop_name, "heater",);

        assert_eq!(output.input, 2,);
    }
}
