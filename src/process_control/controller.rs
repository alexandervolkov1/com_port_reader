use std::{error::Error, fmt};

use super::{
    ControllerDiagnostic, OnOffController, OnOffControllerError, OnOffOutput, PidController,
    PidControllerError, PidGains, PidGainsError, PidOutput, PidOutputLimits, PidOutputLimitsError,
};

use crate::instrument::{
    InstrumentValue, ParameterAccess, ParameterDescriptor, ParameterRange, ParameterValueType,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerKind {
    Pid,
    OnOff,
}

impl ControllerKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pid => "pid",
            Self::OnOff => "on_off",
        }
    }
}

impl fmt::Display for ControllerKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerOperation {
    ResetIntegral,
}

impl ControllerOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ResetIntegral => "reset_integral",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerOperationError {
    Unsupported {
        kind: ControllerKind,
        operation: ControllerOperation,
    },
}

impl fmt::Display for ControllerOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { kind, operation } => {
                write!(
                    formatter,
                    "Controller type '{kind}' does not \
                     support operation '{}'",
                    operation.as_str(),
                )
            }
        }
    }
}

impl Error for ControllerOperationError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerParameter {
    Setpoint,

    ProportionalGain,
    IntegralGain,
    DerivativeGain,

    Hysteresis,

    OutputMinimum,
    OutputMaximum,

    OutputOff,
    OutputOn,
}

impl ControllerParameter {
    pub const ALL: [Self; 9] = [
        Self::Setpoint,
        Self::ProportionalGain,
        Self::IntegralGain,
        Self::DerivativeGain,
        Self::Hysteresis,
        Self::OutputMinimum,
        Self::OutputMaximum,
        Self::OutputOff,
        Self::OutputOn,
    ];

    pub const fn key(self) -> &'static str {
        self.descriptor().key
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|parameter| parameter.key() == key)
    }

    pub const fn descriptor(self) -> ParameterDescriptor {
        match self {
            Self::Setpoint => ParameterDescriptor {
                key: "setpoint",
                name: "setpoint",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: -f64::MAX,
                    maximum: f64::MAX,
                },
            },

            Self::ProportionalGain => ParameterDescriptor {
                key: "kp",
                name: "proportional gain",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: 0.0,
                    maximum: f64::MAX,
                },
            },

            Self::IntegralGain => ParameterDescriptor {
                key: "ki",
                name: "integral gain",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: 0.0,
                    maximum: f64::MAX,
                },
            },

            Self::DerivativeGain => ParameterDescriptor {
                key: "kd",
                name: "derivative gain",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: 0.0,
                    maximum: f64::MAX,
                },
            },

            Self::OutputMinimum => ParameterDescriptor {
                key: "output_min",
                name: "output minimum",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: -f64::MAX,
                    maximum: f64::MAX,
                },
            },

            Self::OutputMaximum => ParameterDescriptor {
                key: "output_max",
                name: "output maximum",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: -f64::MAX,
                    maximum: f64::MAX,
                },
            },

            Self::Hysteresis => ParameterDescriptor {
                key: "hysteresis",
                name: "hysteresis",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: 0.0,
                    maximum: f64::MAX,
                },
            },

            Self::OutputOff => ParameterDescriptor {
                key: "output_off",
                name: "output off",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: -f64::MAX,
                    maximum: f64::MAX,
                },
            },

            Self::OutputOn => ParameterDescriptor {
                key: "output_on",
                name: "output on",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: -f64::MAX,
                    maximum: f64::MAX,
                },
            },
        }
    }
}

const PID_PARAMETERS: [ControllerParameter; 6] = [
    ControllerParameter::Setpoint,
    ControllerParameter::ProportionalGain,
    ControllerParameter::IntegralGain,
    ControllerParameter::DerivativeGain,
    ControllerParameter::OutputMinimum,
    ControllerParameter::OutputMaximum,
];

const ON_OFF_PARAMETERS: [ControllerParameter; 4] = [
    ControllerParameter::Setpoint,
    ControllerParameter::Hysteresis,
    ControllerParameter::OutputOff,
    ControllerParameter::OutputOn,
];

const PID_DIAGNOSTICS: &[ControllerDiagnostic] = &[
    ControllerDiagnostic::Setpoint,
    ControllerDiagnostic::Proportional,
    ControllerDiagnostic::Integral,
    ControllerDiagnostic::Derivative,
    ControllerDiagnostic::Output,
    ControllerDiagnostic::UnconstrainedOutput,
];

const ON_OFF_DIAGNOSTICS: &[ControllerDiagnostic] =
    &[ControllerDiagnostic::Setpoint, ControllerDiagnostic::Output];

#[derive(Debug)]
pub enum Controller {
    Pid(PidController),
    OnOff(OnOffController),
}

impl Controller {
    pub const fn kind(&self) -> ControllerKind {
        match self {
            Self::Pid(_) => ControllerKind::Pid,
            Self::OnOff(_) => ControllerKind::OnOff,
        }
    }

    fn supported_parameters(&self) -> &'static [ControllerParameter] {
        match self {
            Self::Pid(_) => &PID_PARAMETERS,
            Self::OnOff(_) => &ON_OFF_PARAMETERS,
        }
    }

    pub fn parameters(&self) -> Vec<ParameterDescriptor> {
        self.supported_parameters()
            .iter()
            .copied()
            .map(ControllerParameter::descriptor)
            .collect()
    }

    pub fn read(&self, key: &str) -> Result<InstrumentValue, ControllerParameterError> {
        let parameter = ControllerParameter::from_key(key)
            .ok_or_else(|| ControllerParameterError::UnknownParameter(key.to_owned()))?;

        if !self.supports_parameter(parameter) {
            return Err(ControllerParameterError::UnsupportedParameter {
                kind: self.kind(),
                parameter,
            });
        }

        let descriptor = parameter.descriptor();

        if !descriptor.access.readable() {
            return Err(ControllerParameterError::NotReadable(parameter));
        }

        match self {
            Self::Pid(controller) => Ok(read_pid_parameter(controller, parameter)),

            Self::OnOff(controller) => Ok(read_on_off_parameter(controller, parameter)),
        }
    }

    pub fn configure<I, K>(&mut self, updates: I) -> Result<(), ControllerParameterError>
    where
        I: IntoIterator<Item = (K, InstrumentValue)>,
        K: AsRef<str>,
    {
        let mut resolved = Vec::new();

        for (key, value) in updates {
            let key = key.as_ref();

            let parameter = ControllerParameter::from_key(key)
                .ok_or_else(|| ControllerParameterError::UnknownParameter(key.to_owned()))?;

            if !self.supports_parameter(parameter) {
                return Err(ControllerParameterError::UnsupportedParameter {
                    kind: self.kind(),
                    parameter,
                });
            }

            let descriptor = parameter.descriptor();

            if !descriptor.access.writable() {
                return Err(ControllerParameterError::NotWritable(parameter));
            }

            if resolved
                .iter()
                .any(|(existing_parameter, _)| *existing_parameter == parameter)
            {
                return Err(ControllerParameterError::DuplicateParameter(parameter));
            }

            let value = expect_number(parameter, value)?;

            resolved.push((parameter, value));
        }

        match self {
            Self::Pid(controller) => configure_pid(controller, &resolved),

            Self::OnOff(controller) => configure_on_off(controller, &resolved),
        }
    }

    pub fn write(
        &mut self,
        key: &str,
        value: InstrumentValue,
    ) -> Result<InstrumentValue, ControllerParameterError> {
        self.configure([(key, value)])?;

        self.read(key)
    }

    fn supports_parameter(&self, parameter: ControllerParameter) -> bool {
        self.supported_parameters().contains(&parameter)
    }

    pub fn output_range(&self) -> ParameterRange {
        match self {
            Self::Pid(controller) => {
                let limits = controller.output_limits();

                ParameterRange::Number {
                    minimum: limits.minimum(),
                    maximum: limits.maximum(),
                }
            }

            Self::OnOff(controller) => {
                let output_off = controller.output_off();

                let output_on = controller.output_on();

                ParameterRange::Number {
                    minimum: output_off.min(output_on),
                    maximum: output_off.max(output_on),
                }
            }
        }
    }

    pub fn update(
        &mut self,
        timestamp: f64,
        measurement: f64,
    ) -> Result<ControllerOutput, ControllerError> {
        match self {
            Self::Pid(controller) => {
                let setpoint = controller.setpoint();

                let output = controller
                    .update(timestamp, measurement)
                    .map_err(ControllerError::Pid)?;

                Ok(ControllerOutput::Pid { setpoint, output })
            }

            Self::OnOff(controller) => {
                let setpoint = controller.setpoint();

                let output = controller
                    .update(timestamp, measurement)
                    .map_err(ControllerError::OnOff)?;

                Ok(ControllerOutput::OnOff { setpoint, output })
            }
        }
    }

    pub fn reset_integral(&mut self) -> Result<(), ControllerOperationError> {
        match self {
            Self::Pid(controller) => {
                controller.reset_integral();
                Ok(())
            }

            Self::OnOff(_) => Err(ControllerOperationError::Unsupported {
                kind: ControllerKind::OnOff,
                operation: ControllerOperation::ResetIntegral,
            }),
        }
    }

    pub fn resynchronize(&mut self) {
        match self {
            Self::Pid(controller) => {
                controller.resynchronize();
            }

            Self::OnOff(controller) => {
                controller.resynchronize();
            }
        }
    }

    pub fn reset(&mut self) {
        match self {
            Self::Pid(controller) => {
                controller.reset();
            }

            Self::OnOff(controller) => {
                controller.reset();
            }
        }
    }

    pub fn diagnostics(&self) -> &'static [ControllerDiagnostic] {
        match self {
            Self::Pid(_) => PID_DIAGNOSTICS,
            Self::OnOff(_) => ON_OFF_DIAGNOSTICS,
        }
    }

    pub fn validate_diagnostic(
        &self,
        diagnostic: ControllerDiagnostic,
    ) -> Result<(), ControllerDiagnosticError> {
        if self.diagnostics().contains(&diagnostic) {
            return Ok(());
        }

        Err(ControllerDiagnosticError::Unsupported {
            kind: self.kind(),
            diagnostic,
        })
    }
}

fn read_pid_parameter(
    controller: &PidController,
    parameter: ControllerParameter,
) -> InstrumentValue {
    let value = match parameter {
        ControllerParameter::Setpoint => controller.setpoint(),

        ControllerParameter::ProportionalGain => controller.gains().proportional(),

        ControllerParameter::IntegralGain => controller.gains().integral(),

        ControllerParameter::DerivativeGain => controller.gains().derivative(),

        ControllerParameter::OutputMinimum => controller.output_limits().minimum(),

        ControllerParameter::OutputMaximum => controller.output_limits().maximum(),

        _ => {
            unreachable!("unsupported PID controller parameter")
        }
    };

    InstrumentValue::Number(value)
}

fn read_on_off_parameter(
    controller: &OnOffController,
    parameter: ControllerParameter,
) -> InstrumentValue {
    let value = match parameter {
        ControllerParameter::Setpoint => controller.setpoint(),

        ControllerParameter::Hysteresis => controller.hysteresis(),

        ControllerParameter::OutputOff => controller.output_off(),

        ControllerParameter::OutputOn => controller.output_on(),

        _ => {
            unreachable!("unsupported on/off controller parameter")
        }
    };

    InstrumentValue::Number(value)
}

fn configure_pid(
    controller: &mut PidController,
    updates: &[(ControllerParameter, f64)],
) -> Result<(), ControllerParameterError> {
    let mut setpoint = controller.setpoint();

    let current_gains = controller.gains();

    let mut proportional_gain = current_gains.proportional();

    let mut integral_gain = current_gains.integral();

    let mut derivative_gain = current_gains.derivative();

    let current_limits = controller.output_limits();

    let mut output_minimum = current_limits.minimum();

    let mut output_maximum = current_limits.maximum();

    for (parameter, value) in updates {
        match parameter {
            ControllerParameter::Setpoint => {
                setpoint = *value;
            }

            ControllerParameter::ProportionalGain => {
                proportional_gain = *value;
            }

            ControllerParameter::IntegralGain => {
                integral_gain = *value;
            }

            ControllerParameter::DerivativeGain => {
                derivative_gain = *value;
            }

            ControllerParameter::OutputMinimum => {
                output_minimum = *value;
            }

            ControllerParameter::OutputMaximum => {
                output_maximum = *value;
            }

            ControllerParameter::Hysteresis
            | ControllerParameter::OutputOff
            | ControllerParameter::OutputOn => {
                unreachable!("unsupported PID controller parameter");
            }
        }
    }

    let gains = PidGains::new(proportional_gain, integral_gain, derivative_gain)
        .map_err(ControllerParameterError::Gains)?;

    let output_limits = PidOutputLimits::new(output_minimum, output_maximum)
        .map_err(ControllerParameterError::OutputLimits)?;

    controller
        .configure(setpoint, gains, output_limits)
        .map_err(ControllerParameterError::Pid)
}

fn configure_on_off(
    controller: &mut OnOffController,
    updates: &[(ControllerParameter, f64)],
) -> Result<(), ControllerParameterError> {
    let mut setpoint = controller.setpoint();

    let mut hysteresis = controller.hysteresis();

    let mut output_off = controller.output_off();

    let mut output_on = controller.output_on();

    for (parameter, value) in updates {
        match parameter {
            ControllerParameter::Setpoint => {
                setpoint = *value;
            }

            ControllerParameter::Hysteresis => {
                hysteresis = *value;
            }

            ControllerParameter::OutputOff => {
                output_off = *value;
            }

            ControllerParameter::OutputOn => {
                output_on = *value;
            }

            _ => {
                unreachable!("unsupported on/off controller parameter");
            }
        }
    }

    controller
        .configure(setpoint, hysteresis, output_off, output_on)
        .map_err(ControllerParameterError::OnOff)
}

fn expect_number(
    parameter: ControllerParameter,
    value: InstrumentValue,
) -> Result<f64, ControllerParameterError> {
    match value {
        InstrumentValue::Number(value) => Ok(value),

        value => Err(ControllerParameterError::TypeMismatch {
            parameter,
            expected: ParameterValueType::Number,
            actual: instrument_value_type(value),
        }),
    }
}

fn instrument_value_type(value: InstrumentValue) -> ParameterValueType {
    match value {
        InstrumentValue::Boolean(_) => ParameterValueType::Boolean,

        InstrumentValue::Integer(_) => ParameterValueType::Integer,

        InstrumentValue::Number(_) => ParameterValueType::Number,
    }
}

impl From<PidController> for Controller {
    fn from(controller: PidController) -> Self {
        Self::Pid(controller)
    }
}

impl From<OnOffController> for Controller {
    fn from(controller: OnOffController) -> Self {
        Self::OnOff(controller)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControllerOutput {
    Pid { setpoint: f64, output: PidOutput },
    OnOff { setpoint: f64, output: OnOffOutput },
}

impl ControllerOutput {
    pub const fn kind(&self) -> ControllerKind {
        match self {
            Self::Pid { .. } => ControllerKind::Pid,

            Self::OnOff { .. } => ControllerKind::OnOff,
        }
    }

    pub const fn value(&self) -> f64 {
        match self {
            Self::Pid { output, .. } => output.value(),

            Self::OnOff { output, .. } => output.value(),
        }
    }

    pub const fn diagnostic(&self, diagnostic: ControllerDiagnostic) -> Option<f64> {
        match diagnostic {
            ControllerDiagnostic::Setpoint => self.setpoint(),

            ControllerDiagnostic::Proportional => self.proportional(),

            ControllerDiagnostic::Integral => self.integral(),

            ControllerDiagnostic::Derivative => self.derivative(),

            ControllerDiagnostic::Output => Some(self.value()),

            ControllerDiagnostic::UnconstrainedOutput => self.unconstrained_value(),
        }
    }

    pub const fn setpoint(&self) -> Option<f64> {
        match self {
            Self::Pid { setpoint, .. } | Self::OnOff { setpoint, .. } => Some(*setpoint),
        }
    }

    pub const fn unconstrained_value(&self) -> Option<f64> {
        match self {
            Self::Pid { output, .. } => Some(output.unconstrained_value()),

            Self::OnOff { .. } => None,
        }
    }

    pub const fn proportional(&self) -> Option<f64> {
        match self {
            Self::Pid { output, .. } => Some(output.proportional()),

            Self::OnOff { .. } => None,
        }
    }

    pub const fn integral(&self) -> Option<f64> {
        match self {
            Self::Pid { output, .. } => Some(output.integral()),

            Self::OnOff { .. } => None,
        }
    }

    pub const fn derivative(&self) -> Option<f64> {
        match self {
            Self::Pid { output, .. } => Some(output.derivative()),

            Self::OnOff { .. } => None,
        }
    }

    pub const fn saturated(&self) -> Option<bool> {
        match self {
            Self::Pid { output, .. } => Some(output.saturated()),

            Self::OnOff { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControllerParameterError {
    UnknownParameter(String),
    DuplicateParameter(ControllerParameter),
    UnsupportedParameter {
        kind: ControllerKind,
        parameter: ControllerParameter,
    },
    NotReadable(ControllerParameter),
    NotWritable(ControllerParameter),
    TypeMismatch {
        parameter: ControllerParameter,
        expected: ParameterValueType,
        actual: ParameterValueType,
    },
    Pid(PidControllerError),
    OnOff(OnOffControllerError),
    Gains(PidGainsError),
    OutputLimits(PidOutputLimitsError),
}

impl fmt::Display for ControllerParameterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownParameter(key) => {
                write!(
                    formatter,
                    "Unknown controller parameter \
                     '{key}'",
                )
            }

            Self::DuplicateParameter(parameter) => {
                write!(
                    formatter,
                    "Controller parameter '{}' was \
                     configured more than once",
                    parameter.key(),
                )
            }

            Self::UnsupportedParameter { kind, parameter } => {
                write!(
                    formatter,
                    "Controller type '{kind}' does \
                     not support parameter '{}'",
                    parameter.key(),
                )
            }

            Self::NotReadable(parameter) => {
                write!(
                    formatter,
                    "Controller parameter '{}' is \
                     not readable",
                    parameter.key(),
                )
            }

            Self::NotWritable(parameter) => {
                write!(
                    formatter,
                    "Controller parameter '{}' is \
                     not writable",
                    parameter.key(),
                )
            }

            Self::TypeMismatch {
                parameter,
                expected,
                actual,
            } => {
                write!(
                    formatter,
                    "Controller parameter '{}' \
                     expects {}, received {}",
                    parameter.key(),
                    expected.as_str(),
                    actual.as_str(),
                )
            }

            Self::Pid(error) => error.fmt(formatter),

            Self::OnOff(error) => error.fmt(formatter),

            Self::Gains(error) => error.fmt(formatter),

            Self::OutputLimits(error) => error.fmt(formatter),
        }
    }
}

impl Error for ControllerParameterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Pid(error) => Some(error),

            Self::OnOff(error) => Some(error),

            Self::Gains(error) => Some(error),

            Self::OutputLimits(error) => Some(error),

            Self::UnknownParameter(_)
            | Self::DuplicateParameter(_)
            | Self::UnsupportedParameter { .. }
            | Self::NotReadable(_)
            | Self::NotWritable(_)
            | Self::TypeMismatch { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControllerError {
    Pid(PidControllerError),
    OnOff(OnOffControllerError),
}

impl fmt::Display for ControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pid(error) => error.fmt(formatter),

            Self::OnOff(error) => error.fmt(formatter),
        }
    }
}

impl Error for ControllerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Pid(error) => Some(error),
            Self::OnOff(error) => Some(error),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerDiagnosticError {
    Unsupported {
        kind: ControllerKind,
        diagnostic: ControllerDiagnostic,
    },
}

impl fmt::Display for ControllerDiagnosticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { kind, diagnostic } => {
                write!(
                    formatter,
                    "Controller type '{kind}' does not \
                     support diagnostic '{diagnostic}'",
                )
            }
        }
    }
}

impl Error for ControllerDiagnosticError {}

#[cfg(test)]
mod tests {
    use super::{
        Controller, ControllerDiagnostic, ControllerDiagnosticError, ControllerError,
        ControllerKind, ControllerOperation, ControllerOperationError, ControllerOutput,
        ControllerParameter, ControllerParameterError,
    };

    use crate::{
        instrument::{InstrumentValue, ParameterAccess, ParameterRange, ParameterValueType},
        process_control::{
            OnOffController, OnOffControllerError, PidController, PidControllerError, PidGains,
            PidOutputLimits,
        },
    };

    fn controller() -> Controller {
        PidController::with_output_limits(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap()
        .into()
    }

    fn on_off_controller() -> Controller {
        OnOffController::new(100.0, 2.0, 0.0, 100.0).unwrap().into()
    }

    #[test]
    fn exposes_controller_kind() {
        let controller = controller();

        assert_eq!(controller.kind(), ControllerKind::Pid,);

        assert_eq!(controller.kind().as_str(), "pid",);
    }

    #[test]
    fn exposes_controller_output_range() {
        let controller = controller();

        assert_eq!(
            controller.output_range(),
            ParameterRange::Number {
                minimum: 0.0,
                maximum: 100.0,
            },
        );
    }

    #[test]
    fn updates_pid_through_controller_api() {
        let mut controller = controller();

        let output = controller.update(1_000.0, 80.0).unwrap();

        assert_eq!(output.kind(), ControllerKind::Pid,);

        assert_eq!(output.setpoint(), Some(100.0),);

        assert_eq!(output.value(), 40.0,);

        let ControllerOutput::Pid { setpoint, output } = output else {
            panic!("expected PID output");
        };

        assert_eq!(setpoint, 100.0,);

        assert_eq!(output.proportional(), 40.0,);
    }

    #[test]
    fn exposes_pid_diagnostics() {
        let mut controller = controller();

        let output = controller.update(1_000.0, 80.0).unwrap();

        assert_eq!(
            output.diagnostic(ControllerDiagnostic::Setpoint,),
            Some(100.0),
        );

        assert_eq!(
            output.diagnostic(ControllerDiagnostic::Proportional,),
            Some(40.0),
        );

        assert_eq!(
            output.diagnostic(ControllerDiagnostic::Integral,),
            Some(0.0),
        );

        assert_eq!(
            output.diagnostic(ControllerDiagnostic::Derivative,),
            Some(0.0),
        );

        assert_eq!(output.diagnostic(ControllerDiagnostic::Output,), Some(40.0),);

        assert_eq!(
            output.diagnostic(ControllerDiagnostic::UnconstrainedOutput,),
            Some(40.0),
        );
    }

    #[test]
    fn wraps_pid_controller_error() {
        let mut controller = controller();

        assert_eq!(
            controller.update(f64::NAN, 80.0,),
            Err(ControllerError::Pid(PidControllerError::NonFiniteTimestamp,),),
        );
    }

    #[test]
    fn resets_pid_through_controller_api() {
        let mut controller = PidController::with_output_limits(
            100.0,
            PidGains::new(0.0, 1.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap();

        controller.update(0.0, 90.0).unwrap();

        controller.update(1.0, 90.0).unwrap();

        assert_eq!(controller.integral(), 10.0,);

        let mut controller: Controller = controller.into();

        controller.reset();

        let output = controller.update(0.0, 90.0).unwrap();

        let ControllerOutput::Pid { output, .. } = output else {
            panic!("expected PID output");
        };

        assert_eq!(output.integral(), 0.0,);
    }

    #[test]
    fn describes_pid_parameters() {
        let controller = controller();

        let parameters = controller.parameters();

        let keys = parameters
            .iter()
            .map(|parameter| parameter.key)
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec!["setpoint", "kp", "ki", "kd", "output_min", "output_max",],
        );
    }

    #[test]
    fn finds_controller_parameter_by_key() {
        assert_eq!(
            ControllerParameter::from_key("setpoint",),
            Some(ControllerParameter::Setpoint,),
        );

        assert_eq!(
            ControllerParameter::from_key("kp",),
            Some(ControllerParameter::ProportionalGain,),
        );

        assert_eq!(ControllerParameter::from_key("missing",), None,);
    }

    #[test]
    fn describes_pid_gain_parameter() {
        let descriptor = ControllerParameter::ProportionalGain.descriptor();

        assert_eq!(descriptor.key, "kp",);

        assert_eq!(descriptor.access, ParameterAccess::ReadWrite,);

        assert_eq!(descriptor.value_type, ParameterValueType::Number,);

        assert_eq!(
            descriptor.range,
            ParameterRange::Number {
                minimum: 0.0,
                maximum: f64::MAX,
            },
        );
    }

    #[test]
    fn reads_pid_parameters() {
        let controller = controller();

        assert_eq!(
            controller.read("setpoint"),
            Ok(InstrumentValue::Number(100.0,),),
        );

        assert_eq!(controller.read("kp"), Ok(InstrumentValue::Number(2.0,),),);

        assert_eq!(controller.read("ki"), Ok(InstrumentValue::Number(0.0,),),);

        assert_eq!(controller.read("kd"), Ok(InstrumentValue::Number(0.0,),),);

        assert_eq!(
            controller.read("output_min"),
            Ok(InstrumentValue::Number(0.0,),),
        );

        assert_eq!(
            controller.read("output_max"),
            Ok(InstrumentValue::Number(100.0,),),
        );
    }

    #[test]
    fn writes_pid_setpoint() {
        let mut controller = controller();

        assert_eq!(
            controller.write("setpoint", InstrumentValue::Number(120.0,),),
            Ok(InstrumentValue::Number(120.0,),),
        );

        assert_eq!(
            controller.read("setpoint"),
            Ok(InstrumentValue::Number(120.0,),),
        );
    }

    #[test]
    fn writes_pid_gain() {
        let mut controller = controller();

        controller
            .write("ki", InstrumentValue::Number(0.25))
            .unwrap();

        assert_eq!(controller.read("kp"), Ok(InstrumentValue::Number(2.0,),),);

        assert_eq!(controller.read("ki"), Ok(InstrumentValue::Number(0.25,),),);

        assert_eq!(controller.read("kd"), Ok(InstrumentValue::Number(0.0,),),);
    }

    #[test]
    fn rejects_invalid_pid_gain_without_change() {
        let mut controller = controller();

        assert!(matches!(
            controller.write("kp", InstrumentValue::Number(-1.0,),),
            Err(ControllerParameterError::Gains(_)),
        ));

        assert_eq!(controller.read("kp"), Ok(InstrumentValue::Number(2.0,),),);
    }

    #[test]
    fn rejects_invalid_output_limits_without_change() {
        let mut controller = controller();

        assert!(matches!(
            controller.write("output_min", InstrumentValue::Number(150.0,),),
            Err(ControllerParameterError::OutputLimits(_)),
        ));

        assert_eq!(
            controller.read("output_min"),
            Ok(InstrumentValue::Number(0.0,),),
        );

        assert_eq!(
            controller.read("output_max"),
            Ok(InstrumentValue::Number(100.0,),),
        );
    }

    #[test]
    fn rejects_controller_parameter_type_mismatch() {
        let mut controller = controller();

        assert_eq!(
            controller.write("setpoint", InstrumentValue::Integer(120,),),
            Err(ControllerParameterError::TypeMismatch {
                parameter: ControllerParameter::Setpoint,
                expected: ParameterValueType::Number,
                actual: ParameterValueType::Integer,
            },),
        );

        assert_eq!(
            controller.read("setpoint"),
            Ok(InstrumentValue::Number(100.0,),),
        );
    }

    #[test]
    fn rejects_unknown_controller_parameter() {
        let controller = controller();

        assert_eq!(
            controller.read("banana"),
            Err(ControllerParameterError::UnknownParameter(
                "banana".to_owned(),
            ),),
        );
    }

    #[test]
    fn configures_output_limits_atomically() {
        let mut controller = controller();

        controller
            .configure([
                ("output_min", InstrumentValue::Number(200.0)),
                ("output_max", InstrumentValue::Number(300.0)),
            ])
            .unwrap();

        assert_eq!(
            controller.read("output_min",),
            Ok(InstrumentValue::Number(200.0,),),
        );

        assert_eq!(
            controller.read("output_max",),
            Ok(InstrumentValue::Number(300.0,),),
        );
    }

    #[test]
    fn configures_multiple_pid_parameters() {
        let mut controller = controller();

        controller
            .configure([
                ("setpoint", InstrumentValue::Number(150.0)),
                ("kp", InstrumentValue::Number(3.0)),
                ("ki", InstrumentValue::Number(0.25)),
                ("kd", InstrumentValue::Number(0.5)),
            ])
            .unwrap();

        assert_eq!(
            controller.read("setpoint"),
            Ok(InstrumentValue::Number(150.0,),),
        );

        assert_eq!(controller.read("kp"), Ok(InstrumentValue::Number(3.0,),),);

        assert_eq!(controller.read("ki"), Ok(InstrumentValue::Number(0.25,),),);

        assert_eq!(controller.read("kd"), Ok(InstrumentValue::Number(0.5,),),);
    }

    #[test]
    fn failed_configure_does_not_change_any_parameter() {
        let mut controller = controller();

        assert!(matches!(
            controller.configure([
                ("setpoint", InstrumentValue::Number(150.0,),),
                ("kp", InstrumentValue::Number(-1.0,),),
                ("output_max", InstrumentValue::Number(200.0,),),
            ]),
            Err(ControllerParameterError::Gains(_)),
        ));

        assert_eq!(
            controller.read("setpoint"),
            Ok(InstrumentValue::Number(100.0,),),
        );

        assert_eq!(controller.read("kp"), Ok(InstrumentValue::Number(2.0,),),);

        assert_eq!(
            controller.read("output_max",),
            Ok(InstrumentValue::Number(100.0,),),
        );
    }

    #[test]
    fn rejects_duplicate_parameter_in_configuration() {
        let mut controller = controller();

        assert_eq!(
            controller.configure([
                ("setpoint", InstrumentValue::Number(120.0,),),
                ("setpoint", InstrumentValue::Number(130.0,),),
            ]),
            Err(ControllerParameterError::DuplicateParameter(
                ControllerParameter::Setpoint,
            ),),
        );

        assert_eq!(
            controller.read("setpoint"),
            Ok(InstrumentValue::Number(100.0,),),
        );
    }

    #[test]
    fn describes_on_off_controller() {
        let controller = on_off_controller();

        assert_eq!(controller.kind(), ControllerKind::OnOff,);

        assert_eq!(controller.kind().as_str(), "on_off",);

        let keys = controller
            .parameters()
            .into_iter()
            .map(|parameter| parameter.key)
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec!["setpoint", "hysteresis", "output_off", "output_on",],
        );
    }

    #[test]
    fn exposes_on_off_output_range() {
        let controller = on_off_controller();

        assert_eq!(
            controller.output_range(),
            ParameterRange::Number {
                minimum: 0.0,
                maximum: 100.0,
            },
        );
    }

    #[test]
    fn updates_on_off_through_controller_api() {
        let mut controller = on_off_controller();

        let on = controller.update(0.0, 97.0).unwrap();

        assert_eq!(on.kind(), ControllerKind::OnOff,);

        assert_eq!(on.setpoint(), Some(100.0),);

        assert_eq!(on.value(), 100.0,);

        let inside = controller.update(1.0, 100.0).unwrap();

        assert_eq!(inside.value(), 100.0,);

        let off = controller.update(2.0, 103.0).unwrap();

        assert_eq!(off.value(), 0.0,);
    }

    #[test]
    fn exposes_on_off_diagnostics() {
        let mut controller = on_off_controller();

        let output = controller.update(0.0, 97.0).unwrap();

        assert_eq!(
            output.diagnostic(ControllerDiagnostic::Setpoint,),
            Some(100.0),
        );

        assert_eq!(
            output.diagnostic(ControllerDiagnostic::Output,),
            Some(100.0),
        );

        assert_eq!(output.diagnostic(ControllerDiagnostic::Proportional,), None,);

        assert_eq!(output.diagnostic(ControllerDiagnostic::Integral,), None,);

        assert_eq!(output.diagnostic(ControllerDiagnostic::Derivative,), None,);

        assert_eq!(
            output.diagnostic(ControllerDiagnostic::UnconstrainedOutput,),
            None,
        );
    }

    #[test]
    fn configures_on_off_parameters() {
        let mut controller = on_off_controller();

        controller
            .configure([
                ("setpoint", InstrumentValue::Number(150.0)),
                ("hysteresis", InstrumentValue::Number(5.0)),
                ("output_off", InstrumentValue::Number(10.0)),
                ("output_on", InstrumentValue::Number(80.0)),
            ])
            .unwrap();

        assert_eq!(
            controller.read("setpoint"),
            Ok(InstrumentValue::Number(150.0)),
        );

        assert_eq!(
            controller.read("hysteresis"),
            Ok(InstrumentValue::Number(5.0)),
        );

        assert_eq!(
            controller.read("output_off"),
            Ok(InstrumentValue::Number(10.0)),
        );

        assert_eq!(
            controller.read("output_on"),
            Ok(InstrumentValue::Number(80.0)),
        );
    }

    #[test]
    fn rejects_pid_parameter_for_on_off_controller() {
        let controller = on_off_controller();

        assert_eq!(
            controller.read("kp"),
            Err(ControllerParameterError::UnsupportedParameter {
                kind: ControllerKind::OnOff,
                parameter: ControllerParameter::ProportionalGain,
            },),
        );
    }

    #[test]
    fn rejects_invalid_on_off_configuration_atomically() {
        let mut controller = on_off_controller();

        assert_eq!(
            controller.configure([
                ("setpoint", InstrumentValue::Number(150.0),),
                ("hysteresis", InstrumentValue::Number(-1.0),),
            ]),
            Err(ControllerParameterError::OnOff(
                OnOffControllerError::NegativeHysteresis,
            ),),
        );

        assert_eq!(
            controller.read("setpoint"),
            Ok(InstrumentValue::Number(100.0)),
        );

        assert_eq!(
            controller.read("hysteresis"),
            Ok(InstrumentValue::Number(2.0)),
        );
    }

    #[test]
    fn rejects_integral_reset_for_on_off_controller() {
        let mut controller = on_off_controller();

        assert_eq!(
            controller.reset_integral(),
            Err(ControllerOperationError::Unsupported {
                kind: ControllerKind::OnOff,
                operation: ControllerOperation::ResetIntegral,
            },),
        );
    }

    #[test]
    fn resets_pid_integral_through_controller_api() {
        let mut pid = PidController::with_output_limits(
            100.0,
            PidGains::new(0.0, 1.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap();

        pid.update(0.0, 90.0).unwrap();
        pid.update(1.0, 90.0).unwrap();

        assert_eq!(pid.integral(), 10.0);

        let mut controller: Controller = pid.into();

        assert_eq!(controller.reset_integral(), Ok(()),);

        let Controller::Pid(pid) = controller else {
            panic!("expected PID controller");
        };

        assert_eq!(pid.integral(), 0.0);
    }

    #[test]
    fn describes_on_off_diagnostics() {
        let controller = on_off_controller();

        assert_eq!(
            controller.diagnostics(),
            &[ControllerDiagnostic::Setpoint, ControllerDiagnostic::Output,],
        );
    }

    #[test]
    fn rejects_unsupported_on_off_diagnostic() {
        let controller = on_off_controller();

        assert_eq!(
            controller.validate_diagnostic(ControllerDiagnostic::Integral,),
            Err(ControllerDiagnosticError::Unsupported {
                kind: ControllerKind::OnOff,
                diagnostic: ControllerDiagnostic::Integral,
            },),
        );
    }
}
