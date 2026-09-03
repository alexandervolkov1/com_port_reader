use std::{error::Error, fmt};

use crate::instrument::{
    InstrumentValue, ParameterAccess, ParameterDescriptor, ParameterRange, ParameterValueType,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceParameter {
    Setpoint,
}

impl ReferenceParameter {
    pub const ALL: [Self; 1] = [Self::Setpoint];

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
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceSourceParameter {
    Value,
    Start,
    Target,
    Rate,
}

impl ReferenceSourceParameter {
    pub const ALL: [Self; 4] = [Self::Value, Self::Start, Self::Target, Self::Rate];

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
            Self::Value => ParameterDescriptor {
                key: "value",
                name: "value",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: -f64::MAX,
                    maximum: f64::MAX,
                },
            },

            Self::Start => ParameterDescriptor {
                key: "start",
                name: "start",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: -f64::MAX,
                    maximum: f64::MAX,
                },
            },

            Self::Target => ParameterDescriptor {
                key: "target",
                name: "target",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: -f64::MAX,
                    maximum: f64::MAX,
                },
            },

            Self::Rate => ParameterDescriptor {
                key: "rate",
                name: "rate",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: 0.0,
                    maximum: f64::MAX,
                },
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    Fixed,
    Ramp,
}

impl ReferenceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Ramp => "ramp",
        }
    }
}

impl fmt::Display for ReferenceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedReference {
    value: f64,
}

impl FixedReference {
    pub fn new(value: f64) -> Result<Self, ReferenceSourceError> {
        if !value.is_finite() {
            return Err(ReferenceSourceError::NonFiniteFixedValue);
        }

        Ok(Self { value })
    }

    pub const fn value(self) -> f64 {
        self.value
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RampReference {
    start: f64,
    target: f64,
    rate: f64,
}

impl RampReference {
    pub fn new(start: f64, target: f64, rate: f64) -> Result<Self, ReferenceSourceError> {
        if !start.is_finite() {
            return Err(ReferenceSourceError::NonFiniteRampStart);
        }

        if !target.is_finite() {
            return Err(ReferenceSourceError::NonFiniteRampTarget);
        }

        if !rate.is_finite() {
            return Err(ReferenceSourceError::NonFiniteRampRate);
        }

        if rate <= 0.0 {
            return Err(ReferenceSourceError::NonPositiveRampRate);
        }

        let span = target - start;

        if !span.is_finite() {
            return Err(ReferenceSourceError::NonFiniteRampSpan);
        }

        Ok(Self {
            start,
            target,
            rate,
        })
    }

    pub const fn start(self) -> f64 {
        self.start
    }

    pub const fn target(self) -> f64 {
        self.target
    }

    pub const fn rate(self) -> f64 {
        self.rate
    }

    pub fn value_at_elapsed(self, elapsed_seconds: f64) -> Result<f64, ReferenceSourceError> {
        if !elapsed_seconds.is_finite() {
            return Err(ReferenceSourceError::NonFiniteElapsedTime);
        }

        if elapsed_seconds < 0.0 {
            return Err(ReferenceSourceError::NegativeElapsedTime);
        }

        let span = self.target - self.start;

        if span == 0.0 {
            return Ok(self.target);
        }

        let distance = span.abs();

        let duration = distance / self.rate;

        if elapsed_seconds >= duration {
            return Ok(self.target);
        }

        let direction = span.signum();

        Ok(self.start + direction * self.rate * elapsed_seconds)
    }
}

const FIXED_PARAMETERS: [ReferenceSourceParameter; 1] = [ReferenceSourceParameter::Value];

const RAMP_PARAMETERS: [ReferenceSourceParameter; 3] = [
    ReferenceSourceParameter::Start,
    ReferenceSourceParameter::Target,
    ReferenceSourceParameter::Rate,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReferenceSource {
    Fixed(FixedReference),
    Ramp(RampReference),
}

impl ReferenceSource {
    fn supported_parameters(&self) -> &'static [ReferenceSourceParameter] {
        match self {
            Self::Fixed(_) => &FIXED_PARAMETERS,

            Self::Ramp(_) => &RAMP_PARAMETERS,
        }
    }

    pub fn parameters(&self) -> Vec<ParameterDescriptor> {
        self.supported_parameters()
            .iter()
            .copied()
            .map(ReferenceSourceParameter::descriptor)
            .collect()
    }

    pub fn read(&self, key: &str) -> Result<InstrumentValue, ReferenceParameterError> {
        let parameter = ReferenceSourceParameter::from_key(key)
            .ok_or_else(|| ReferenceParameterError::UnknownParameter(key.to_owned()))?;

        if !self.supported_parameters().contains(&parameter) {
            return Err(ReferenceParameterError::UnsupportedParameter {
                kind: self.kind(),
                parameter,
            });
        }

        let descriptor = parameter.descriptor();

        if !descriptor.access.readable() {
            return Err(ReferenceParameterError::NotReadable(parameter));
        }

        let value = match (*self, parameter) {
            (Self::Fixed(reference), ReferenceSourceParameter::Value) => reference.value(),

            (Self::Ramp(reference), ReferenceSourceParameter::Start) => reference.start(),

            (Self::Ramp(reference), ReferenceSourceParameter::Target) => reference.target(),

            (Self::Ramp(reference), ReferenceSourceParameter::Rate) => reference.rate(),

            _ => {
                unreachable!("unsupported reference parameter")
            }
        };

        Ok(InstrumentValue::Number(value))
    }

    pub fn configure<I, K>(&mut self, updates: I) -> Result<(), ReferenceParameterError>
    where
        I: IntoIterator<Item = (K, InstrumentValue)>,
        K: AsRef<str>,
    {
        let mut resolved = Vec::new();

        for (key, value) in updates {
            let key = key.as_ref();

            let parameter = ReferenceSourceParameter::from_key(key)
                .ok_or_else(|| ReferenceParameterError::UnknownParameter(key.to_owned()))?;

            if !self.supported_parameters().contains(&parameter) {
                return Err(ReferenceParameterError::UnsupportedParameter {
                    kind: self.kind(),
                    parameter,
                });
            }

            let descriptor = parameter.descriptor();

            if !descriptor.access.writable() {
                return Err(ReferenceParameterError::NotWritable(parameter));
            }

            if resolved
                .iter()
                .any(|(existing_parameter, _)| *existing_parameter == parameter)
            {
                return Err(ReferenceParameterError::DuplicateParameter(parameter));
            }

            let value = expect_reference_number(parameter, value)?;

            resolved.push((parameter, value));
        }

        let updated = match *self {
            Self::Fixed(reference) => {
                let mut value = reference.value();

                for (parameter, candidate) in &resolved {
                    match parameter {
                        ReferenceSourceParameter::Value => {
                            value = *candidate;
                        }

                        _ => {
                            unreachable!(
                                "unsupported fixed \
                                 reference parameter"
                            )
                        }
                    }
                }

                Self::Fixed(FixedReference::new(value).map_err(ReferenceParameterError::Source)?)
            }

            Self::Ramp(reference) => {
                let mut start = reference.start();

                let mut target = reference.target();

                let mut rate = reference.rate();

                for (parameter, candidate) in &resolved {
                    match parameter {
                        ReferenceSourceParameter::Start => {
                            start = *candidate;
                        }

                        ReferenceSourceParameter::Target => {
                            target = *candidate;
                        }

                        ReferenceSourceParameter::Rate => {
                            rate = *candidate;
                        }

                        _ => {
                            unreachable!(
                                "unsupported ramp \
                                 reference parameter"
                            )
                        }
                    }
                }

                Self::Ramp(
                    RampReference::new(start, target, rate)
                        .map_err(ReferenceParameterError::Source)?,
                )
            }
        };

        *self = updated;

        Ok(())
    }

    pub fn write(
        &mut self,
        key: &str,
        value: InstrumentValue,
    ) -> Result<InstrumentValue, ReferenceParameterError> {
        self.configure([(key, value)])?;

        self.read(key)
    }

    pub fn fixed(value: f64) -> Result<Self, ReferenceSourceError> {
        FixedReference::new(value).map(Self::Fixed)
    }

    pub fn ramp(start: f64, target: f64, rate: f64) -> Result<Self, ReferenceSourceError> {
        RampReference::new(start, target, rate).map(Self::Ramp)
    }

    pub const fn kind(self) -> ReferenceKind {
        match self {
            Self::Fixed(_) => ReferenceKind::Fixed,

            Self::Ramp(_) => ReferenceKind::Ramp,
        }
    }

    pub fn value_at_elapsed(self, elapsed_seconds: f64) -> Result<f64, ReferenceSourceError> {
        match self {
            Self::Fixed(reference) => Ok(reference.value()),

            Self::Ramp(reference) => reference.value_at_elapsed(elapsed_seconds),
        }
    }
}

impl From<FixedReference> for ReferenceSource {
    fn from(reference: FixedReference) -> Self {
        Self::Fixed(reference)
    }
}

impl From<RampReference> for ReferenceSource {
    fn from(reference: RampReference) -> Self {
        Self::Ramp(reference)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReferenceParameterError {
    UnknownParameter(String),

    DuplicateParameter(ReferenceSourceParameter),

    UnsupportedParameter {
        kind: ReferenceKind,
        parameter: ReferenceSourceParameter,
    },

    NotReadable(ReferenceSourceParameter),

    NotWritable(ReferenceSourceParameter),

    TypeMismatch {
        parameter: ReferenceSourceParameter,
        expected: ParameterValueType,
        actual: ParameterValueType,
    },

    Source(ReferenceSourceError),
}

impl fmt::Display for ReferenceParameterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownParameter(key) => {
                write!(
                    formatter,
                    "Unknown reference \
                     parameter '{key}'",
                )
            }

            Self::DuplicateParameter(parameter) => {
                write!(
                    formatter,
                    "Reference parameter '{}' \
                     is configured more than once",
                    parameter.key(),
                )
            }

            Self::UnsupportedParameter { kind, parameter } => {
                write!(
                    formatter,
                    "Reference type '{kind}' \
                     does not support \
                     parameter '{}'",
                    parameter.key(),
                )
            }

            Self::NotReadable(parameter) => {
                write!(
                    formatter,
                    "Reference parameter '{}' \
                     is not readable",
                    parameter.key(),
                )
            }

            Self::NotWritable(parameter) => {
                write!(
                    formatter,
                    "Reference parameter '{}' \
                     is not writable",
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
                    "Reference parameter '{}' \
                     expects {}, got {}",
                    parameter.key(),
                    expected.as_str(),
                    actual.as_str(),
                )
            }

            Self::Source(error) => error.fmt(formatter),
        }
    }
}

impl Error for ReferenceParameterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),

            Self::UnknownParameter(_)
            | Self::DuplicateParameter(_)
            | Self::UnsupportedParameter { .. }
            | Self::NotReadable(_)
            | Self::NotWritable(_)
            | Self::TypeMismatch { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceSourceError {
    NonFiniteFixedValue,
    NonFiniteRampStart,
    NonFiniteRampTarget,
    NonFiniteRampRate,
    NonPositiveRampRate,
    NonFiniteRampSpan,
    NonFiniteElapsedTime,
    NegativeElapsedTime,
}

impl fmt::Display for ReferenceSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteFixedValue => formatter.write_str(
                "Fixed reference value \
                     must be finite",
            ),

            Self::NonFiniteRampStart => formatter.write_str(
                "Ramp start value \
                     must be finite",
            ),

            Self::NonFiniteRampTarget => formatter.write_str(
                "Ramp target value \
                     must be finite",
            ),

            Self::NonFiniteRampRate => formatter.write_str("Ramp rate must be finite"),

            Self::NonPositiveRampRate => formatter.write_str(
                "Ramp rate must be \
                     greater than zero",
            ),

            Self::NonFiniteRampSpan => formatter.write_str("Ramp span must be finite"),

            Self::NonFiniteElapsedTime => formatter.write_str(
                "Reference elapsed time \
                     must be finite",
            ),

            Self::NegativeElapsedTime => formatter.write_str(
                "Reference elapsed time \
                     must not be negative",
            ),
        }
    }
}

impl Error for ReferenceSourceError {}

fn expect_reference_number(
    parameter: ReferenceSourceParameter,
    value: InstrumentValue,
) -> Result<f64, ReferenceParameterError> {
    match value {
        InstrumentValue::Number(value) => Ok(value),

        value => Err(ReferenceParameterError::TypeMismatch {
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

#[cfg(test)]
mod tests {
    use super::{
        FixedReference, RampReference, ReferenceKind, ReferenceParameterError, ReferenceSource,
        ReferenceSourceError, ReferenceSourceParameter,
    };

    use crate::instrument::{InstrumentValue, ParameterValueType};

    #[test]
    fn fixed_reference_returns_constant_value() {
        let reference = FixedReference::new(150.0).unwrap();

        assert_eq!(reference.value(), 150.0,);
    }

    #[test]
    fn upward_ramp_reaches_and_holds_target() {
        let reference = RampReference::new(20.0, 100.0, 10.0).unwrap();

        assert_eq!(reference.value_at_elapsed(0.0).unwrap(), 20.0,);

        assert_eq!(reference.value_at_elapsed(3.0).unwrap(), 50.0,);

        assert_eq!(reference.value_at_elapsed(8.0).unwrap(), 100.0,);

        assert_eq!(reference.value_at_elapsed(20.0).unwrap(), 100.0,);
    }

    #[test]
    fn downward_ramp_reaches_and_holds_target() {
        let reference = RampReference::new(100.0, 20.0, 10.0).unwrap();

        assert_eq!(reference.value_at_elapsed(0.0).unwrap(), 100.0,);

        assert_eq!(reference.value_at_elapsed(3.0).unwrap(), 70.0,);

        assert_eq!(reference.value_at_elapsed(8.0).unwrap(), 20.0,);

        assert_eq!(reference.value_at_elapsed(30.0).unwrap(), 20.0,);
    }

    #[test]
    fn zero_length_ramp_holds_target() {
        let reference = RampReference::new(100.0, 100.0, 10.0).unwrap();

        assert_eq!(reference.value_at_elapsed(0.0).unwrap(), 100.0,);

        assert_eq!(reference.value_at_elapsed(1_000.0).unwrap(), 100.0,);
    }

    #[test]
    fn reference_source_dispatches_by_kind() {
        let fixed = ReferenceSource::fixed(150.0).unwrap();

        assert_eq!(fixed.kind(), ReferenceKind::Fixed,);

        assert_eq!(fixed.value_at_elapsed(10.0).unwrap(), 150.0,);

        let ramp = ReferenceSource::ramp(20.0, 100.0, 10.0).unwrap();

        assert_eq!(ramp.kind(), ReferenceKind::Ramp,);

        assert_eq!(ramp.value_at_elapsed(2.0).unwrap(), 40.0,);
    }

    #[test]
    fn rejects_invalid_reference_definitions() {
        assert_eq!(
            FixedReference::new(f64::NAN,),
            Err(ReferenceSourceError::NonFiniteFixedValue,),
        );

        assert_eq!(
            RampReference::new(f64::NAN, 100.0, 1.0,),
            Err(ReferenceSourceError::NonFiniteRampStart,),
        );

        assert_eq!(
            RampReference::new(20.0, f64::INFINITY, 1.0,),
            Err(ReferenceSourceError::NonFiniteRampTarget,),
        );

        assert_eq!(
            RampReference::new(20.0, 100.0, f64::NAN,),
            Err(ReferenceSourceError::NonFiniteRampRate,),
        );

        assert_eq!(
            RampReference::new(20.0, 100.0, 0.0,),
            Err(ReferenceSourceError::NonPositiveRampRate,),
        );

        assert_eq!(
            RampReference::new(20.0, 100.0, -1.0,),
            Err(ReferenceSourceError::NonPositiveRampRate,),
        );

        assert_eq!(
            RampReference::new(-f64::MAX, f64::MAX, 1.0,),
            Err(ReferenceSourceError::NonFiniteRampSpan,),
        );
    }

    #[test]
    fn rejects_invalid_ramp_elapsed_time() {
        let reference = RampReference::new(20.0, 100.0, 10.0).unwrap();

        assert_eq!(
            reference.value_at_elapsed(f64::NAN,),
            Err(ReferenceSourceError::NonFiniteElapsedTime,),
        );

        assert_eq!(
            reference.value_at_elapsed(-1.0),
            Err(ReferenceSourceError::NegativeElapsedTime,),
        );
    }

    #[test]
    fn exposes_fixed_reference_parameters() {
        let reference = ReferenceSource::fixed(150.0).unwrap();

        let keys = reference
            .parameters()
            .into_iter()
            .map(|parameter| parameter.key)
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["value"],);

        assert_eq!(
            reference.read("value"),
            Ok(InstrumentValue::Number(150.0,),),
        );
    }

    #[test]
    fn exposes_ramp_reference_parameters() {
        let reference = ReferenceSource::ramp(20.0, 150.0, 2.0).unwrap();

        let keys = reference
            .parameters()
            .into_iter()
            .map(|parameter| parameter.key)
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["start", "target", "rate",],);

        assert_eq!(reference.read("start"), Ok(InstrumentValue::Number(20.0,),),);

        assert_eq!(
            reference.read("target"),
            Ok(InstrumentValue::Number(150.0,),),
        );

        assert_eq!(reference.read("rate"), Ok(InstrumentValue::Number(2.0,),),);
    }

    #[test]
    fn writes_fixed_reference_value() {
        let mut reference = ReferenceSource::fixed(150.0).unwrap();

        assert_eq!(
            reference.write("value", InstrumentValue::Number(175.0,),),
            Ok(InstrumentValue::Number(175.0,),),
        );

        assert_eq!(reference.value_at_elapsed(1_000.0,), Ok(175.0),);
    }

    #[test]
    fn configures_ramp_reference_atomically() {
        let mut reference = ReferenceSource::ramp(20.0, 100.0, 10.0).unwrap();

        reference
            .configure([
                ("target", InstrumentValue::Number(200.0)),
                ("rate", InstrumentValue::Number(20.0)),
            ])
            .unwrap();

        assert_eq!(
            reference.read("target"),
            Ok(InstrumentValue::Number(200.0,),),
        );

        assert_eq!(reference.read("rate"), Ok(InstrumentValue::Number(20.0,),),);

        assert_eq!(reference.value_at_elapsed(5.0), Ok(120.0),);

        let result = reference.configure([
            ("target", InstrumentValue::Number(300.0)),
            ("rate", InstrumentValue::Number(0.0)),
        ]);

        assert_eq!(
            result,
            Err(ReferenceParameterError::Source(
                ReferenceSourceError::NonPositiveRampRate,
            ),),
        );

        assert_eq!(
            reference.read("target"),
            Ok(InstrumentValue::Number(200.0,),),
        );

        assert_eq!(reference.read("rate"), Ok(InstrumentValue::Number(20.0,),),);
    }

    #[test]
    fn rejects_parameter_for_wrong_reference_kind() {
        let reference = ReferenceSource::fixed(150.0).unwrap();

        assert_eq!(
            reference.read("target"),
            Err(ReferenceParameterError::UnsupportedParameter {
                kind: ReferenceKind::Fixed,
                parameter: ReferenceSourceParameter::Target,
            },),
        );
    }

    #[test]
    fn rejects_reference_parameter_type_mismatch() {
        let mut reference = ReferenceSource::fixed(150.0).unwrap();

        assert_eq!(
            reference.write("value", InstrumentValue::Integer(175,),),
            Err(ReferenceParameterError::TypeMismatch {
                parameter: ReferenceSourceParameter::Value,
                expected: ParameterValueType::Number,
                actual: ParameterValueType::Integer,
            },),
        );
    }
}
