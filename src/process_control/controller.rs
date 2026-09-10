use std::{error::Error, fmt};

use super::{
    ControllerDiagnostic, FurnaceController, FurnaceControllerError, FurnaceOutput,
    OnOffController, OnOffControllerError, OnOffOutput, PidController, PidControllerError,
    PidGainsError, PidOutput, PidOutputLimitsError,
};

use crate::instrument::{InstrumentValue, ParameterDescriptor, ParameterRange, ParameterValueType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerKind {
    Pid,
    OnOff,
    Furnace,
}

impl ControllerKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pid => "pid",
            Self::OnOff => "on_off",
            Self::Furnace => "furnace",
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

const FURNACE_DIAGNOSTICS: &[ControllerDiagnostic] = &[
    ControllerDiagnostic::Setpoint,
    ControllerDiagnostic::Proportional,
    ControllerDiagnostic::Integral,
    ControllerDiagnostic::Output,
    ControllerDiagnostic::UnconstrainedOutput,
];

#[derive(Debug)]
pub enum Controller {
    Pid(PidController),
    OnOff(OnOffController),
    Furnace(FurnaceController),
}

impl Controller {
    pub const fn kind(&self) -> ControllerKind {
        match self {
            Self::Pid(_) => ControllerKind::Pid,
            Self::OnOff(_) => ControllerKind::OnOff,
            Self::Furnace(_) => ControllerKind::Furnace,
        }
    }

    pub fn parameters(&self) -> Vec<ParameterDescriptor> {
        match self {
            Self::Pid(controller) => controller.parameters(),
            Self::OnOff(controller) => controller.parameters(),
            Self::Furnace(controller) => controller.parameters(),
        }
    }

    pub fn parameter_values(
        &self,
    ) -> Result<Vec<(String, InstrumentValue)>, ControllerParameterError> {
        match self {
            Self::Pid(controller) => controller.parameter_values(),
            Self::OnOff(controller) => controller.parameter_values(),
            Self::Furnace(controller) => controller.parameter_values(),
        }
    }

    pub fn read(&self, key: &str) -> Result<InstrumentValue, ControllerParameterError> {
        match self {
            Self::Pid(controller) => controller.read_parameter(key),
            Self::OnOff(controller) => controller.read_parameter(key),
            Self::Furnace(controller) => controller.read_parameter(key),
        }
    }

    pub fn configure<I, K>(&mut self, updates: I) -> Result<(), ControllerParameterError>
    where
        I: IntoIterator<Item = (K, InstrumentValue)>,
        K: AsRef<str>,
    {
        match self {
            Self::Pid(controller) => controller.configure_parameters(updates),
            Self::OnOff(controller) => controller.configure_parameters(updates),
            Self::Furnace(controller) => controller.configure_parameters(updates),
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

    pub(super) fn apply_reference(
        &mut self,
        setpoint: f64,
    ) -> Result<(), ControllerParameterError> {
        self.configure([("setpoint", InstrumentValue::Number(setpoint))])
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

            Self::Furnace(controller) => {
                let limits = controller.output_limits();

                ParameterRange::Number {
                    minimum: limits.minimum(),
                    maximum: limits.maximum(),
                }
            }
        }
    }

    pub(crate) fn output_range_after_configuration<I, K>(
        &self,
        updates: I,
    ) -> Result<ParameterRange, ControllerParameterError>
    where
        I: IntoIterator<Item = (K, InstrumentValue)>,
        K: AsRef<str>,
    {
        let mut candidate = match self {
            Self::Pid(controller) => {
                let controller = PidController::with_output_limits(
                    controller.setpoint(),
                    controller.gains(),
                    controller.output_limits(),
                )
                .map_err(ControllerParameterError::Pid)?;

                Self::Pid(controller)
            }

            Self::OnOff(controller) => {
                let controller = OnOffController::new(
                    controller.setpoint(),
                    controller.hysteresis(),
                    controller.output_off(),
                    controller.output_on(),
                )
                .map_err(ControllerParameterError::OnOff)?;

                Self::OnOff(controller)
            }

            Self::Furnace(controller) => {
                let controller = FurnaceController::new(
                    controller.setpoint(),
                    controller.gains(),
                    controller.model(),
                    controller.output_limits(),
                )
                .map_err(ControllerParameterError::Furnace)?;

                Self::Furnace(controller)
            }
        };

        candidate.configure(updates)?;

        Ok(candidate.output_range())
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

            Self::Furnace(controller) => {
                let setpoint = controller.setpoint();

                let output = controller
                    .update(timestamp, measurement)
                    .map_err(ControllerError::Furnace)?;

                Ok(ControllerOutput::Furnace { setpoint, output })
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

            Self::Furnace(controller) => {
                controller.reset_integral();
                Ok(())
            }
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

            Self::Furnace(controller) => {
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

            Self::Furnace(controller) => {
                controller.reset();
            }
        }
    }

    pub fn diagnostics(&self) -> &'static [ControllerDiagnostic] {
        match self {
            Self::Pid(_) => PID_DIAGNOSTICS,
            Self::OnOff(_) => ON_OFF_DIAGNOSTICS,
            Self::Furnace(_) => FURNACE_DIAGNOSTICS,
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

// Preserve the distinction between unknown keys and keys of another controller
// without keeping a shared catalog of implementation-specific parameters.
pub(super) fn unknown_parameter(kind: ControllerKind, key: &str) -> ControllerParameterError {
    if super::pid::PidParameter::from_key(key).is_some()
        || super::on_off::OnOffParameter::from_key(key).is_some()
        || super::furnace::FurnaceParameter::from_key(key).is_some()
    {
        ControllerParameterError::UnsupportedParameter {
            kind,
            key: key.to_owned(),
        }
    } else {
        ControllerParameterError::UnknownParameter(key.to_owned())
    }
}

pub(super) fn expect_number(
    key: &str,
    value: InstrumentValue,
) -> Result<f64, ControllerParameterError> {
    match value {
        InstrumentValue::Number(value) => Ok(value),

        value => Err(ControllerParameterError::TypeMismatch {
            key: key.to_owned(),
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

impl From<FurnaceController> for Controller {
    fn from(controller: FurnaceController) -> Self {
        Self::Furnace(controller)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControllerOutput {
    Pid {
        setpoint: f64,
        output: PidOutput,
    },
    OnOff {
        setpoint: f64,
        output: OnOffOutput,
    },
    Furnace {
        setpoint: f64,
        output: FurnaceOutput,
    },
}

impl ControllerOutput {
    pub const fn kind(&self) -> ControllerKind {
        match self {
            Self::Pid { .. } => ControllerKind::Pid,

            Self::OnOff { .. } => ControllerKind::OnOff,

            Self::Furnace { .. } => ControllerKind::Furnace,
        }
    }

    pub const fn value(&self) -> f64 {
        match self {
            Self::Pid { output, .. } => output.value(),

            Self::OnOff { output, .. } => output.value(),

            Self::Furnace { output, .. } => output.value(),
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
            Self::Pid { setpoint, .. }
            | Self::OnOff { setpoint, .. }
            | Self::Furnace { setpoint, .. } => Some(*setpoint),
        }
    }

    pub const fn unconstrained_value(&self) -> Option<f64> {
        match self {
            Self::Pid { output, .. } => Some(output.unconstrained_value()),

            Self::OnOff { .. } => None,

            Self::Furnace { output, .. } => Some(output.unconstrained_value()),
        }
    }

    pub const fn proportional(&self) -> Option<f64> {
        match self {
            Self::Pid { output, .. } => Some(output.proportional()),

            Self::OnOff { .. } => None,

            Self::Furnace { output, .. } => Some(output.proportional()),
        }
    }

    pub const fn integral(&self) -> Option<f64> {
        match self {
            Self::Pid { output, .. } => Some(output.integral()),

            Self::OnOff { .. } => None,

            Self::Furnace { output, .. } => Some(output.integral()),
        }
    }

    pub const fn derivative(&self) -> Option<f64> {
        match self {
            Self::Pid { output, .. } => Some(output.derivative()),

            Self::Furnace { .. } | Self::OnOff { .. } => None,
        }
    }

    pub const fn saturated(&self) -> Option<bool> {
        match self {
            Self::Pid { output, .. } => Some(output.saturated()),

            Self::OnOff { .. } => None,

            Self::Furnace { output, .. } => Some(output.saturated()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControllerParameterError {
    UnknownParameter(String),
    DuplicateParameter(String),
    UnsupportedParameter {
        kind: ControllerKind,
        key: String,
    },
    NotReadable(String),
    NotWritable(String),
    TypeMismatch {
        key: String,
        expected: ParameterValueType,
        actual: ParameterValueType,
    },
    Pid(PidControllerError),
    OnOff(OnOffControllerError),
    Gains(PidGainsError),
    OutputLimits(PidOutputLimitsError),
    Furnace(FurnaceControllerError),
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
                    parameter,
                )
            }

            Self::UnsupportedParameter { kind, key } => {
                write!(
                    formatter,
                    "Controller type '{kind}' does \
                     not support parameter '{}'",
                    key,
                )
            }

            Self::NotReadable(parameter) => {
                write!(
                    formatter,
                    "Controller parameter '{}' is \
                     not readable",
                    parameter,
                )
            }

            Self::NotWritable(parameter) => {
                write!(
                    formatter,
                    "Controller parameter '{}' is \
                     not writable",
                    parameter,
                )
            }

            Self::TypeMismatch {
                key,
                expected,
                actual,
            } => {
                write!(
                    formatter,
                    "Controller parameter '{}' \
                     expects {}, received {}",
                    key,
                    expected.as_str(),
                    actual.as_str(),
                )
            }

            Self::Pid(error) => error.fmt(formatter),

            Self::OnOff(error) => error.fmt(formatter),

            Self::Gains(error) => error.fmt(formatter),

            Self::OutputLimits(error) => error.fmt(formatter),

            Self::Furnace(error) => error.fmt(formatter),
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

            Self::Furnace(error) => Some(error),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControllerError {
    Pid(PidControllerError),
    OnOff(OnOffControllerError),
    Furnace(FurnaceControllerError),
}

impl fmt::Display for ControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pid(error) => error.fmt(formatter),

            Self::OnOff(error) => error.fmt(formatter),

            Self::Furnace(error) => error.fmt(formatter),
        }
    }
}

impl Error for ControllerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Pid(error) => Some(error),
            Self::OnOff(error) => Some(error),
            Self::Furnace(error) => Some(error),
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
        ControllerParameterError, FurnaceController,
    };

    use crate::{
        instrument::{InstrumentValue, ParameterAccess, ParameterRange, ParameterValueType},
        process_control::{
            FurnaceGains, FurnaceModel, FurnaceOutputLimits, OnOffController, OnOffControllerError,
            PidController, PidControllerError, PidGains, PidOutputLimits,
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

        assert_eq!(keys, vec!["kp", "ki", "kd", "output_min", "output_max",],);
    }

    #[test]
    fn describes_pid_gain_parameter() {
        let descriptor = controller()
            .parameters()
            .into_iter()
            .find(|parameter| parameter.key == "kp")
            .unwrap();

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
    fn exposes_pid_parameter_values() {
        let controller = controller();

        assert_eq!(
            controller.parameter_values(),
            Ok(vec![
                ("setpoint".to_owned(), InstrumentValue::Number(100.0,),),
                ("kp".to_owned(), InstrumentValue::Number(2.0,),),
                ("ki".to_owned(), InstrumentValue::Number(0.0,),),
                ("kd".to_owned(), InstrumentValue::Number(0.0,),),
                ("output_min".to_owned(), InstrumentValue::Number(0.0,),),
                ("output_max".to_owned(), InstrumentValue::Number(100.0,),),
            ]),
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
                key: "setpoint".to_owned(),
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
                "setpoint".to_owned(),
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

        assert_eq!(keys, vec!["hysteresis", "output_off", "output_on",],);
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
                key: "kp".to_owned(),
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

    #[test]
    fn parameter_configuration_preserves_state_and_rejects_invalid_batches() {
        let furnace = || {
            FurnaceController::new(
                500.0,
                FurnaceGains::new(0.1, 0.0005).unwrap(),
                FurnaceModel::new(20.0, 2500.0, 90.0, 0.35, 1200.0).unwrap(),
                FurnaceOutputLimits::new(0.0, 100.0).unwrap(),
            )
            .unwrap()
            .into()
        };

        for (mut configured, mut unchanged, invalid_key, foreign_key) in [
            (controller(), controller(), "kp", "hysteresis"),
            (on_off_controller(), on_off_controller(), "hysteresis", "kp"),
            (furnace(), furnace(), "max_power", "kd"),
        ] {
            for (timestamp, measurement) in [(0.0, 80.0), (1.0, 85.0)] {
                configured.update(timestamp, measurement).unwrap();
                unchanged.update(timestamp, measurement).unwrap();
            }

            let before = configured.parameter_values().unwrap();
            // Reapplying the current configuration must retain integral,
            // measurement history and on/off state.
            configured.configure(before.clone()).unwrap();

            for updates in [
                vec![
                    ("setpoint", InstrumentValue::Number(200.0)),
                    (invalid_key, InstrumentValue::Number(-1.0)),
                ],
                vec![
                    ("setpoint", InstrumentValue::Number(200.0)),
                    ("setpoint", InstrumentValue::Number(300.0)),
                ],
                vec![
                    ("setpoint", InstrumentValue::Number(200.0)),
                    (invalid_key, InstrumentValue::Integer(1)),
                ],
                vec![
                    ("setpoint", InstrumentValue::Number(200.0)),
                    ("missing", InstrumentValue::Number(1.0)),
                ],
                vec![
                    ("setpoint", InstrumentValue::Number(200.0)),
                    (foreign_key, InstrumentValue::Number(1.0)),
                ],
            ] {
                assert!(configured.configure(updates).is_err());
                assert_eq!(configured.parameter_values().unwrap(), before);
            }

            assert_eq!(
                configured.read(foreign_key),
                Err(ControllerParameterError::UnsupportedParameter {
                    kind: configured.kind(),
                    key: foreign_key.to_owned(),
                }),
            );
            assert_eq!(
                configured.read("missing"),
                Err(ControllerParameterError::UnknownParameter(
                    "missing".to_owned()
                )),
            );
            assert_eq!(configured.update(2.0, 90.0), unchanged.update(2.0, 90.0));
        }
    }

    #[test]
    fn configures_all_furnace_parameters() {
        let mut controller: Controller = FurnaceController::new(
            500.0,
            FurnaceGains::new(0.1, 0.0005).unwrap(),
            FurnaceModel::new(20.0, 2500.0, 90.0, 0.35, 1200.0).unwrap(),
            FurnaceOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap()
        .into();
        let updates = [
            ("setpoint", 600.0),
            ("kp", 0.2),
            ("ki", 0.001),
            ("output_min", 110.0),
            ("output_max", 120.0),
            ("ambient_temperature", 25.0),
            ("max_power", 3000.0),
            ("heater_lag", 100.0),
            ("linear_loss", 0.5),
            ("radiation_loss_1000c", 1300.0),
        ]
        .map(|(key, value)| (key, InstrumentValue::Number(value)));

        controller.configure(updates).unwrap();
        assert_eq!(
            controller.parameter_values().unwrap(),
            updates.map(|(key, value)| (key.to_owned(), value)).to_vec(),
        );
        assert_eq!(
            controller
                .parameters()
                .iter()
                .map(|descriptor| descriptor.key)
                .collect::<Vec<_>>(),
            updates
                .iter()
                .skip(1)
                .map(|(key, _)| *key)
                .collect::<Vec<_>>(),
        );
        for (key, value) in updates {
            assert_eq!(controller.read(key), Ok(value));
        }
    }

    #[test]
    fn updates_furnace_through_controller_api() {
        let furnace = FurnaceController::new(
            500.0,
            FurnaceGains::new(0.1, 0.0005).unwrap(),
            FurnaceModel::new(20.0, 2500.0, 90.0, 0.35, 1200.0).unwrap(),
            FurnaceOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap();

        let mut controller: Controller = furnace.into();

        assert_eq!(controller.kind(), ControllerKind::Furnace,);

        let output = controller.update(0.0, 20.0).unwrap();

        assert_eq!(output.kind(), ControllerKind::Furnace,);

        assert_eq!(output.setpoint(), Some(500.0),);

        assert!(output.value() > 0.0);

        assert_eq!(
            controller.read("max_power"),
            Ok(InstrumentValue::Number(2500.0)),
        );

        controller
            .write("heater_lag", InstrumentValue::Number(120.0))
            .unwrap();

        assert_eq!(
            controller.read("heater_lag"),
            Ok(InstrumentValue::Number(120.0)),
        );
    }
}
