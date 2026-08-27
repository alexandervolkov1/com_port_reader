use std::{error::Error, fmt};

use super::{
    PidController, PidControllerError, PidGains, PidGainsError, PidOutput, PidOutputLimits,
    PidOutputLimitsError,
};
use crate::instrument::{
    InstrumentValue, ParameterAccess, ParameterDescriptor, ParameterRange, ParameterValueType,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerKind {
    Pid,
}

impl ControllerKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pid => "pid",
        }
    }
}

impl fmt::Display for ControllerKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerParameter {
    Setpoint,
    ProportionalGain,
    IntegralGain,
    DerivativeGain,
    OutputMinimum,
    OutputMaximum,
}

impl ControllerParameter {
    pub const ALL: [Self; 6] = [
        Self::Setpoint,
        Self::ProportionalGain,
        Self::IntegralGain,
        Self::DerivativeGain,
        Self::OutputMinimum,
        Self::OutputMaximum,
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
        }
    }
}

#[derive(Debug)]
pub enum Controller {
    Pid(PidController),
}

impl Controller {
    pub const fn kind(&self) -> ControllerKind {
        match self {
            Self::Pid(_) => ControllerKind::Pid,
        }
    }

    pub fn parameters(&self) -> Vec<ParameterDescriptor> {
        match self {
            Self::Pid(_) => ControllerParameter::ALL
                .into_iter()
                .map(ControllerParameter::descriptor)
                .collect(),
        }
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
        }
    }

    pub fn write(
        &mut self,
        key: &str,
        value: InstrumentValue,
    ) -> Result<InstrumentValue, ControllerParameterError> {
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

        let number = expect_number(parameter, value)?;

        match self {
            Self::Pid(controller) => {
                write_pid_parameter(controller, parameter, number)?;
            }
        }

        self.read(key)
    }

    fn supports_parameter(&self, parameter: ControllerParameter) -> bool {
        match self {
            Self::Pid(_) => matches!(
                parameter,
                ControllerParameter::Setpoint
                    | ControllerParameter::ProportionalGain
                    | ControllerParameter::IntegralGain
                    | ControllerParameter::DerivativeGain
                    | ControllerParameter::OutputMinimum
                    | ControllerParameter::OutputMaximum
            ),
        }
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
        }
    }

    pub fn reset(&mut self) {
        match self {
            Self::Pid(controller) => {
                controller.reset();
            }
        }
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
    };

    InstrumentValue::Number(value)
}

fn write_pid_parameter(
    controller: &mut PidController,
    parameter: ControllerParameter,
    value: f64,
) -> Result<(), ControllerParameterError> {
    match parameter {
        ControllerParameter::Setpoint => {
            controller
                .set_setpoint(value)
                .map_err(ControllerParameterError::Controller)?;
        }

        ControllerParameter::ProportionalGain => {
            let current = controller.gains();

            let gains = PidGains::new(value, current.integral(), current.derivative())
                .map_err(ControllerParameterError::Gains)?;

            controller.set_gains(gains);
        }

        ControllerParameter::IntegralGain => {
            let current = controller.gains();

            let gains = PidGains::new(current.proportional(), value, current.derivative())
                .map_err(ControllerParameterError::Gains)?;

            controller.set_gains(gains);
        }

        ControllerParameter::DerivativeGain => {
            let current = controller.gains();

            let gains = PidGains::new(current.proportional(), current.integral(), value)
                .map_err(ControllerParameterError::Gains)?;

            controller.set_gains(gains);
        }

        ControllerParameter::OutputMinimum => {
            let current = controller.output_limits();

            let limits = PidOutputLimits::new(value, current.maximum())
                .map_err(ControllerParameterError::OutputLimits)?;

            controller.set_output_limits(limits);
        }

        ControllerParameter::OutputMaximum => {
            let current = controller.output_limits();

            let limits = PidOutputLimits::new(current.minimum(), value)
                .map_err(ControllerParameterError::OutputLimits)?;

            controller.set_output_limits(limits);
        }
    }

    Ok(())
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControllerOutput {
    Pid { setpoint: f64, output: PidOutput },
}

impl ControllerOutput {
    pub const fn kind(&self) -> ControllerKind {
        match self {
            Self::Pid { .. } => ControllerKind::Pid,
        }
    }

    pub const fn value(&self) -> f64 {
        match self {
            Self::Pid { output, .. } => output.value(),
        }
    }

    pub const fn setpoint(&self) -> Option<f64> {
        match self {
            Self::Pid { setpoint, .. } => Some(*setpoint),
        }
    }

    pub const fn unconstrained_value(&self) -> Option<f64> {
        match self {
            Self::Pid { output, .. } => Some(output.unconstrained_value()),
        }
    }

    pub const fn proportional(&self) -> Option<f64> {
        match self {
            Self::Pid { output, .. } => Some(output.proportional()),
        }
    }

    pub const fn integral(&self) -> Option<f64> {
        match self {
            Self::Pid { output, .. } => Some(output.integral()),
        }
    }

    pub const fn derivative(&self) -> Option<f64> {
        match self {
            Self::Pid { output, .. } => Some(output.derivative()),
        }
    }

    pub const fn saturated(&self) -> Option<bool> {
        match self {
            Self::Pid { output, .. } => Some(output.saturated()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControllerParameterError {
    UnknownParameter(String),

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

    Controller(PidControllerError),

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

            Self::Controller(error) => error.fmt(formatter),

            Self::Gains(error) => error.fmt(formatter),

            Self::OutputLimits(error) => error.fmt(formatter),
        }
    }
}

impl Error for ControllerParameterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Controller(error) => Some(error),

            Self::Gains(error) => Some(error),

            Self::OutputLimits(error) => Some(error),

            Self::UnknownParameter(_)
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
}

impl fmt::Display for ControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pid(error) => error.fmt(formatter),
        }
    }
}

impl Error for ControllerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Pid(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Controller, ControllerError, ControllerKind, ControllerOutput, ControllerParameter,
        ControllerParameterError,
    };

    use crate::{
        instrument::{InstrumentValue, ParameterAccess, ParameterRange, ParameterValueType},
        process_control::{PidController, PidControllerError, PidGains, PidOutputLimits},
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

        let ControllerOutput::Pid { setpoint, output } = output;

        assert_eq!(setpoint, 100.0,);

        assert_eq!(output.proportional(), 40.0,);
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

        let ControllerOutput::Pid { output, .. } = output;

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
}
