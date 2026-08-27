use std::{error::Error, fmt};

use super::{PidController, PidControllerError, PidGains, PidOutput, PidOutputLimits};

#[derive(Clone, Debug, PartialEq)]
pub struct PidLoopDefinition<SignalId, OutputTarget> {
    name: String,
    input: SignalId,
    output_target: OutputTarget,
    setpoint: f64,
    gains: PidGains,
    output_limits: PidOutputLimits,
}

impl<SignalId, OutputTarget> PidLoopDefinition<SignalId, OutputTarget> {
    pub fn new(
        name: impl Into<String>,
        input: SignalId,
        output_target: OutputTarget,
        setpoint: f64,
        gains: PidGains,
        output_limits: PidOutputLimits,
    ) -> Result<Self, PidLoopDefinitionError> {
        let name = name.into();

        let name = normalize_name(&name)?;

        validate_setpoint(setpoint)?;

        Ok(Self {
            name: name.to_owned(),
            input,
            output_target,
            setpoint,
            gains,
            output_limits,
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

    pub const fn setpoint(&self) -> f64 {
        self.setpoint
    }

    pub const fn gains(&self) -> PidGains {
        self.gains
    }

    pub const fn output_limits(&self) -> PidOutputLimits {
        self.output_limits
    }
}

#[derive(Debug)]
pub struct PidLoop<SignalId, OutputTarget> {
    name: String,
    input: SignalId,
    output_target: OutputTarget,
    setpoint: f64,
    controller: PidController,
}

impl<SignalId, OutputTarget> PidLoop<SignalId, OutputTarget> {
    pub fn new(definition: PidLoopDefinition<SignalId, OutputTarget>) -> Self {
        let PidLoopDefinition {
            name,
            input,
            output_target,
            setpoint,
            gains,
            output_limits,
        } = definition;

        let controller = PidController::with_output_limits(gains, output_limits);

        Self {
            name,
            input,
            output_target,
            setpoint,
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

    pub const fn setpoint(&self) -> f64 {
        self.setpoint
    }

    pub const fn gains(&self) -> PidGains {
        self.controller.gains()
    }

    pub const fn output_limits(&self) -> PidOutputLimits {
        self.controller.output_limits()
    }

    pub const fn integral(&self) -> f64 {
        self.controller.integral()
    }

    pub fn set_setpoint(&mut self, setpoint: f64) -> Result<(), PidLoopDefinitionError> {
        validate_setpoint(setpoint)?;

        self.setpoint = setpoint;

        Ok(())
    }

    pub fn set_gains(&mut self, gains: PidGains) {
        self.controller.set_gains(gains);
    }

    pub fn set_output_limits(&mut self, output_limits: PidOutputLimits) {
        self.controller.set_output_limits(output_limits);
    }

    pub fn update(
        &mut self,
        timestamp: f64,
        measurement: f64,
    ) -> Result<PidOutput, PidControllerError> {
        self.controller
            .update(timestamp, self.setpoint, measurement)
    }

    pub fn reset(&mut self) {
        self.controller.reset();
    }
}

fn normalize_name(name: &str) -> Result<&str, PidLoopDefinitionError> {
    let name = name.trim();

    if name.is_empty() {
        return Err(PidLoopDefinitionError::EmptyName);
    }

    if name.chars().any(char::is_whitespace) {
        return Err(PidLoopDefinitionError::NameContainsWhitespace);
    }

    Ok(name)
}

fn validate_setpoint(setpoint: f64) -> Result<(), PidLoopDefinitionError> {
    if !setpoint.is_finite() {
        return Err(PidLoopDefinitionError::NonFiniteSetpoint);
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PidLoopDefinitionError {
    EmptyName,
    NameContainsWhitespace,
    NonFiniteSetpoint,
}

impl fmt::Display for PidLoopDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("PID loop name cannot be empty"),

            Self::NameContainsWhitespace => formatter.write_str(
                "PID loop name cannot contain \
                     whitespace",
            ),

            Self::NonFiniteSetpoint => formatter.write_str("PID loop setpoint must be finite"),
        }
    }
}

impl Error for PidLoopDefinitionError {}

#[cfg(test)]
mod tests {
    use super::{PidLoop, PidLoopDefinition, PidLoopDefinitionError};

    use crate::process_control::{PidGains, PidOutputLimits};

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

        let definition =
            PidLoopDefinition::new("  heater  ", 11_u64, output_target, 200.0, gains, limits)
                .unwrap();

        assert_eq!(definition.name(), "heater",);

        assert_eq!(*definition.input(), 11,);

        assert_eq!(*definition.output_target(), output_target,);

        assert_eq!(definition.setpoint(), 200.0,);

        assert_eq!(definition.gains(), gains,);

        assert_eq!(definition.output_limits(), limits,);
    }

    #[test]
    fn rejects_invalid_pid_loop_names() {
        let gains = PidGains::new(1.0, 0.0, 0.0).unwrap();

        let limits = PidOutputLimits::new(0.0, 100.0).unwrap();

        let output_target = TestOutputTarget {
            connection: 1,
            instrument: 1,
            parameter: 1,
        };

        for name in ["", " ", "\t"] {
            assert_eq!(
                PidLoopDefinition::new(name, 1_u64, output_target, 100.0, gains, limits,),
                Err(PidLoopDefinitionError::EmptyName,),
            );
        }

        for name in ["heater one", "heater\tone", "heater\none"] {
            assert_eq!(
                PidLoopDefinition::new(name, 1_u64, output_target, 100.0, gains, limits,),
                Err(PidLoopDefinitionError::NameContainsWhitespace,),
            );
        }
    }

    #[test]
    fn rejects_non_finite_setpoints() {
        let gains = PidGains::new(1.0, 0.0, 0.0).unwrap();

        let limits = PidOutputLimits::new(0.0, 100.0).unwrap();

        let output_target = TestOutputTarget {
            connection: 1,
            instrument: 1,
            parameter: 1,
        };

        for setpoint in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                PidLoopDefinition::new("heater", 1_u64, output_target, setpoint, gains, limits,),
                Err(PidLoopDefinitionError::NonFiniteSetpoint,),
            );
        }
    }

    #[test]
    fn updates_loop_using_its_setpoint() {
        let definition = definition_with(
            80.0,
            PidGains::new(2.0, 1.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let mut control_loop = PidLoop::new(definition);

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

        let mut control_loop = PidLoop::new(definition);

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

        let mut control_loop = PidLoop::new(definition);

        assert_eq!(
            control_loop.set_setpoint(f64::NAN,),
            Err(PidLoopDefinitionError::NonFiniteSetpoint,),
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

        let mut control_loop = PidLoop::new(definition);

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

        let mut control_loop = PidLoop::new(definition);

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

        let mut control_loop = PidLoop::new(definition);

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
    ) -> PidLoopDefinition<u64, TestOutputTarget> {
        PidLoopDefinition::new(
            "heater",
            1_u64,
            TestOutputTarget {
                connection: 2,
                instrument: 7,
                parameter: 3,
            },
            setpoint,
            gains,
            output_limits,
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
