use std::{error::Error, fmt};

use crate::instrument::{InstrumentValue, ParameterDescriptor};

use super::{Controller, ControllerError, ControllerOutput, ControllerParameterError};

#[derive(Debug)]
pub struct ControlLoopDefinition<SignalId, OutputTarget> {
    name: String,
    input: SignalId,
    output_target: OutputTarget,
    controller: Controller,
}

impl<SignalId, OutputTarget> ControlLoopDefinition<SignalId, OutputTarget> {
    pub fn new(
        name: impl Into<String>,
        input: SignalId,
        output_target: OutputTarget,
        controller: Controller,
    ) -> Result<Self, ControlLoopDefinitionError> {
        let name = name.into();
        let name = normalize_name(&name)?;

        Ok(Self {
            name: name.to_owned(),
            input,
            output_target,
            controller,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn input(&self) -> &SignalId {
        &self.input
    }

    pub const fn output_target(&self) -> &OutputTarget {
        &self.output_target
    }

    pub const fn controller(&self) -> &Controller {
        &self.controller
    }

    fn into_parts(self) -> (String, SignalId, OutputTarget, Controller) {
        (self.name, self.input, self.output_target, self.controller)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlLoopState {
    #[default]
    Running,
    Paused,
}

impl ControlLoopState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
        }
    }
}

#[derive(Debug)]
pub struct ControlLoop<SignalId, OutputTarget> {
    name: String,
    input: SignalId,
    output_target: OutputTarget,
    controller: Controller,
    state: ControlLoopState,
}

impl<SignalId, OutputTarget> ControlLoop<SignalId, OutputTarget> {
    pub fn new(definition: ControlLoopDefinition<SignalId, OutputTarget>) -> Self {
        let (name, input, output_target, controller) = definition.into_parts();

        Self {
            name,
            input,
            output_target,
            controller,
            state: ControlLoopState::Running,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn input(&self) -> &SignalId {
        &self.input
    }

    pub const fn output_target(&self) -> &OutputTarget {
        &self.output_target
    }

    pub const fn state(&self) -> ControlLoopState {
        self.state
    }

    pub const fn is_running(&self) -> bool {
        matches!(self.state, ControlLoopState::Running)
    }

    pub fn parameters(&self) -> Vec<ParameterDescriptor> {
        self.controller.parameters()
    }

    pub fn read_parameter(&self, key: &str) -> Result<InstrumentValue, ControllerParameterError> {
        self.controller.read(key)
    }

    pub fn write_parameter(
        &mut self,
        key: &str,
        value: InstrumentValue,
    ) -> Result<InstrumentValue, ControllerParameterError> {
        self.controller.write(key, value)
    }

    pub fn configure<I, K>(&mut self, updates: I) -> Result<(), ControllerParameterError>
    where
        I: IntoIterator<Item = (K, InstrumentValue)>,
        K: AsRef<str>,
    {
        self.controller.configure(updates)
    }

    pub fn update(
        &mut self,
        timestamp: f64,
        measurement: f64,
    ) -> Result<ControllerOutput, ControllerError> {
        self.controller.update(timestamp, measurement)
    }

    pub fn process(
        &mut self,
        timestamp: f64,
        measurement: f64,
    ) -> Result<Option<ControllerOutput>, ControllerError> {
        if !self.is_running() {
            return Ok(None);
        }

        self.update(timestamp, measurement).map(Some)
    }

    pub fn pause(&mut self) {
        self.state = ControlLoopState::Paused;
    }

    pub fn resume(&mut self) {
        if self.state == ControlLoopState::Paused {
            self.controller.resynchronize();
            self.state = ControlLoopState::Running;
        }
    }

    pub fn reset_integral(&mut self) {
        self.controller.reset_integral();
    }

    pub fn reset(&mut self) {
        self.controller.reset();
    }
}

fn normalize_name(name: &str) -> Result<&str, ControlLoopDefinitionError> {
    let name = name.trim();

    if name.is_empty() {
        return Err(ControlLoopDefinitionError::EmptyName);
    }

    if name.chars().any(char::is_whitespace) {
        return Err(ControlLoopDefinitionError::NameContainsWhitespace);
    }

    Ok(name)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlLoopDefinitionError {
    EmptyName,
    NameContainsWhitespace,
}

impl fmt::Display for ControlLoopDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("Control loop name cannot be empty"),

            Self::NameContainsWhitespace => {
                formatter.write_str("Control loop name cannot contain whitespace")
            }
        }
    }
}

impl Error for ControlLoopDefinitionError {}

#[cfg(test)]
mod tests {
    use super::{ControlLoop, ControlLoopDefinition, ControlLoopDefinitionError, ControlLoopState};

    use crate::instrument::InstrumentValue;
    use crate::process_control::{PidController, PidGains, PidOutputLimits};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TestOutputTarget {
        connection: u64,
        instrument: u16,
        parameter: u16,
    }

    #[test]
    fn creates_control_loop_definition() {
        let output_target = TestOutputTarget {
            connection: 2,
            instrument: 7,
            parameter: 3,
        };

        let gains = PidGains::new(2.0, 0.5, 1.0).unwrap();

        let limits = PidOutputLimits::new(0.0, 100.0).unwrap();

        let controller = PidController::with_output_limits(200.0, gains, limits)
            .unwrap()
            .into();

        let definition =
            ControlLoopDefinition::new("  heater  ", 11_u64, output_target, controller).unwrap();

        assert_eq!(definition.name(), "heater",);

        assert_eq!(*definition.input(), 11,);

        assert_eq!(*definition.output_target(), output_target,);
    }

    #[test]
    fn rejects_invalid_control_loop_names() {
        let gains = PidGains::new(1.0, 0.0, 0.0).unwrap();

        let limits = PidOutputLimits::new(0.0, 100.0).unwrap();

        let output_target = TestOutputTarget {
            connection: 1,
            instrument: 1,
            parameter: 1,
        };

        for name in ["", " ", "\t"] {
            let controller = PidController::with_output_limits(100.0, gains, limits)
                .unwrap()
                .into();

            let error =
                ControlLoopDefinition::new(name, 1_u64, output_target, controller).unwrap_err();

            assert_eq!(error, ControlLoopDefinitionError::EmptyName,);
        }

        for name in ["heater one", "heater\tone", "heater\none"] {
            let controller = PidController::with_output_limits(100.0, gains, limits)
                .unwrap()
                .into();

            let error =
                ControlLoopDefinition::new(name, 1_u64, output_target, controller).unwrap_err();

            assert_eq!(error, ControlLoopDefinitionError::NameContainsWhitespace,);
        }
    }

    #[test]
    fn updates_loop_using_its_setpoint() {
        let definition = definition_with(
            80.0,
            PidGains::new(2.0, 1.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        let first = control_loop.update(0.0, 50.0).unwrap();

        assert_close(first.value(), 60.0);

        let second = control_loop.update(1.0, 40.0).unwrap();

        assert_close(second.value(), 100.0);

        assert_close(second.integral().unwrap(), 20.0);

        assert_eq!(second.saturated(), Some(true),);
    }

    #[test]
    fn reset_preserves_loop_configuration() {
        let definition = definition_with(
            100.0,
            PidGains::new(0.0, 1.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        control_loop.update(0.0, 90.0).unwrap();

        let accumulated = control_loop.update(1.0, 90.0).unwrap();

        assert_eq!(accumulated.integral(), Some(10.0),);

        control_loop.reset();

        assert_eq!(control_loop.name(), "heater",);

        assert_eq!(
            control_loop.read_parameter("setpoint",),
            Ok(InstrumentValue::Number(100.0,),),
        );

        let restarted = control_loop.update(0.0, 90.0).unwrap();

        assert_eq!(restarted.integral(), Some(0.0),);
    }

    fn definition_with(
        setpoint: f64,
        gains: PidGains,
        output_limits: PidOutputLimits,
    ) -> ControlLoopDefinition<u64, TestOutputTarget> {
        let controller = PidController::with_output_limits(setpoint, gains, output_limits)
            .unwrap()
            .into();

        ControlLoopDefinition::new(
            "heater",
            1_u64,
            TestOutputTarget {
                connection: 2,
                instrument: 7,
                parameter: 3,
            },
            controller,
        )
        .unwrap()
    }

    fn assert_close(actual: f64, expected: f64) {
        let tolerance = expected.abs().max(1.0) * 1.0e-12;

        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}",
        );
    }

    #[test]
    fn reads_controller_parameter() {
        let definition = definition_with(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let control_loop = ControlLoop::new(definition);

        assert_eq!(
            control_loop.read_parameter("setpoint",),
            Ok(InstrumentValue::Number(100.0,),),
        );

        assert_eq!(
            control_loop.read_parameter("kp",),
            Ok(InstrumentValue::Number(2.0,),),
        );
    }

    #[test]
    fn writes_controller_parameter() {
        let definition = definition_with(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        assert_eq!(
            control_loop.write_parameter("setpoint", InstrumentValue::Number(120.0,),),
            Ok(InstrumentValue::Number(120.0,),),
        );

        assert_eq!(
            control_loop.read_parameter("setpoint",),
            Ok(InstrumentValue::Number(120.0,),),
        );
    }

    #[test]
    fn configures_controller_parameters_atomically() {
        let definition = definition_with(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        control_loop
            .configure([
                ("output_min", InstrumentValue::Number(200.0)),
                ("output_max", InstrumentValue::Number(300.0)),
            ])
            .unwrap();

        assert_eq!(
            control_loop.read_parameter("output_min",),
            Ok(InstrumentValue::Number(200.0,),),
        );

        assert_eq!(
            control_loop.read_parameter("output_max",),
            Ok(InstrumentValue::Number(300.0,),),
        );
    }

    #[test]
    fn exposes_controller_parameter_descriptors() {
        let control_loop = ControlLoop::new(definition_with(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        ));

        let keys = control_loop
            .parameters()
            .into_iter()
            .map(|parameter| parameter.key)
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec!["setpoint", "kp", "ki", "kd", "output_min", "output_max",],
        );
    }

    #[test]
    fn pauses_and_resumes_without_losing_integral() {
        let definition = definition_with(
            100.0,
            PidGains::new(0.0, 1.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        assert_eq!(control_loop.state(), ControlLoopState::Running,);

        control_loop.process(0.0, 90.0).unwrap();

        let accumulated = control_loop.process(1.0, 90.0).unwrap().unwrap();

        assert_eq!(accumulated.integral(), Some(10.0),);

        control_loop.pause();

        assert_eq!(control_loop.state(), ControlLoopState::Paused,);

        assert_eq!(control_loop.process(100.0, 90.0).unwrap(), None,);

        control_loop.resume();

        assert_eq!(control_loop.state(), ControlLoopState::Running,);

        let resumed = control_loop.process(101.0, 90.0).unwrap().unwrap();

        assert_eq!(resumed.integral(), Some(10.0),);
    }
}
