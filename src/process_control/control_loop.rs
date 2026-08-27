use std::{error::Error, fmt};

use super::{Controller, PidController, PidControllerError, PidGains, PidOutput, PidOutputLimits};

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

#[derive(Debug)]
pub struct ControlLoop<SignalId, OutputTarget> {
    name: String,
    input: SignalId,
    output_target: OutputTarget,
    controller: Controller,
}

impl<SignalId, OutputTarget> ControlLoop<SignalId, OutputTarget> {
    pub fn new(definition: ControlLoopDefinition<SignalId, OutputTarget>) -> Self {
        let (name, input, output_target, controller) = definition.into_parts();

        Self {
            name,
            input,
            output_target,
            controller,
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

    fn pid_controller(&self) -> &PidController {
        let Controller::Pid(controller) = &self.controller;

        controller
    }

    fn pid_controller_mut(&mut self) -> &mut PidController {
        let Controller::Pid(controller) = &mut self.controller;

        controller
    }

    pub fn setpoint(&self) -> f64 {
        self.pid_controller().setpoint()
    }

    pub fn gains(&self) -> PidGains {
        self.pid_controller().gains()
    }

    pub fn output_limits(&self) -> PidOutputLimits {
        self.pid_controller().output_limits()
    }

    pub fn integral(&self) -> f64 {
        self.pid_controller().integral()
    }

    pub fn set_setpoint(&mut self, setpoint: f64) -> Result<(), PidControllerError> {
        self.pid_controller_mut().set_setpoint(setpoint)
    }

    pub fn set_gains(&mut self, gains: PidGains) {
        self.pid_controller_mut().set_gains(gains);
    }

    pub fn set_output_limits(&mut self, output_limits: PidOutputLimits) {
        self.pid_controller_mut().set_output_limits(output_limits);
    }

    pub fn update(
        &mut self,
        timestamp: f64,
        measurement: f64,
    ) -> Result<PidOutput, PidControllerError> {
        self.pid_controller_mut().update(timestamp, measurement)
    }

    pub fn reset(&mut self) {
        self.pid_controller_mut().reset();
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
    use super::{ControlLoop, ControlLoopDefinition, ControlLoopDefinitionError};

    use crate::process_control::{PidController, PidControllerError, PidGains, PidOutputLimits};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TestOutputTarget {
        connection: u64,
        instrument: u16,
        parameter: u16,
    }

    #[test]
    fn creates_pid_loop_definition() {
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

        assert_close(second.integral(), 20.0);

        assert!(second.saturated(),);
    }

    #[test]
    fn changes_setpoint_without_derivative_kick() {
        let definition = definition_with(
            100.0,
            PidGains::new(0.0, 0.0, 4.0).unwrap(),
            PidOutputLimits::new(-100.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        control_loop.update(0.0, 20.0).unwrap();

        control_loop.set_setpoint(200.0).unwrap();

        let output = control_loop.update(1.0, 20.0).unwrap();

        assert_eq!(control_loop.setpoint(), 200.0,);

        assert_close(output.derivative(), 0.0);

        assert_close(output.value(), 0.0);
    }

    #[test]
    fn rejects_invalid_setpoint_without_changing_configuration() {
        let definition = definition_with(
            100.0,
            PidGains::new(1.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        assert_eq!(
            control_loop.set_setpoint(f64::NAN),
            Err(PidControllerError::NonFiniteSetpoint),
        );

        assert_eq!(control_loop.setpoint(), 100.0,);

        let output = control_loop.update(0.0, 90.0).unwrap();

        assert_close(output.value(), 10.0);
    }

    #[test]
    fn changes_gains_without_resetting_loop_integral() {
        let definition = definition_with(
            10.0,
            PidGains::new(0.0, 1.0, 0.0).unwrap(),
            PidOutputLimits::new(-100.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        control_loop.update(0.0, 8.0).unwrap();

        control_loop.update(1.0, 8.0).unwrap();

        assert_close(control_loop.integral(), 2.0);

        let gains = PidGains::new(2.0, 0.0, 0.0).unwrap();

        control_loop.set_gains(gains);

        let output = control_loop.update(2.0, 8.0).unwrap();

        assert_eq!(control_loop.gains(), gains,);

        assert_close(output.integral(), 2.0);

        assert_close(output.value(), 6.0);
    }

    #[test]
    fn changes_output_limits_for_running_loop() {
        let definition = definition_with(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 300.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        let initial = control_loop.update(0.0, 0.0).unwrap();

        assert_close(initial.value(), 200.0);

        let limits = PidOutputLimits::new(0.0, 100.0).unwrap();

        control_loop.set_output_limits(limits);

        let limited = control_loop.update(1.0, 0.0).unwrap();

        assert_eq!(control_loop.output_limits(), limits,);

        assert_close(limited.value(), 100.0);

        assert!(limited.saturated(),);
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

        control_loop.update(1.0, 90.0).unwrap();

        assert_close(control_loop.integral(), 10.0);

        control_loop.reset();

        assert_close(control_loop.integral(), 0.0);

        assert_eq!(control_loop.name(), "heater",);

        assert_eq!(control_loop.setpoint(), 100.0,);

        let restarted = control_loop.update(0.0, 90.0).unwrap();

        assert_close(restarted.integral(), 0.0);
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
}
