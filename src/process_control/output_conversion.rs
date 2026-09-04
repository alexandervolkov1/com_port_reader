use std::fmt;

use crate::instrument::{
    InstrumentValue, InstrumentWriteRequest, ParameterRange, ParameterValueType,
    metakon_5x3::{Metakon5x3, Metakon5x3Register, Metakon5x3ValueError, Metakon5x3Write},
    virtual_instrument::{VirtualInstrumentId, VirtualParameterId},
};

use super::{ControlOutputParameter, ControlOutputTarget};

impl ControlOutputTarget {
    pub fn write_request(
        &self,
        value: f64,
    ) -> Result<InstrumentWriteRequest, ControlOutputConversionError> {
        validate_output_value(value, self.range())?;

        match self.parameter() {
            ControlOutputParameter::Metakon5x3 {
                instrument,
                parameter,
                scale,
            } => metakon_write_request(instrument, parameter, scale, value),

            ControlOutputParameter::VirtualInstrument {
                instrument,
                parameter,
                value_type,
                ..
            } => virtual_write_request(instrument, parameter, value_type, value),
        }
    }

    pub fn safe_write_request(
        &self,
    ) -> Result<Option<InstrumentWriteRequest>, ControlOutputConversionError> {
        self.safe_value()
            .map(|value| self.write_request(value))
            .transpose()
    }
}

fn validate_output_value(
    value: f64,
    range: Option<ParameterRange>,
) -> Result<(), ControlOutputConversionError> {
    if !value.is_finite() {
        return Err(ControlOutputConversionError::NonFiniteValue);
    }

    let Some(range) = range else {
        return Ok(());
    };

    let (minimum, maximum) = match range {
        ParameterRange::Integer { minimum, maximum } => (minimum as f64, maximum as f64),

        ParameterRange::Number { minimum, maximum } => (minimum, maximum),
    };

    if !(minimum..=maximum).contains(&value) {
        return Err(ControlOutputConversionError::ValueOutOfRange {
            value,
            minimum,
            maximum,
        });
    }

    Ok(())
}

fn metakon_write_request(
    instrument: Metakon5x3,
    parameter: Metakon5x3Register,
    scale: f64,
    value: f64,
) -> Result<InstrumentWriteRequest, ControlOutputConversionError> {
    let raw_value = rounded_integer(value / scale)?;

    let parameter = metakon_write_parameter(parameter, raw_value)?;

    InstrumentWriteRequest::metakon_5x3(instrument, parameter, scale)
        .map_err(ControlOutputConversionError::InvalidMetakonValue)
}

fn metakon_write_parameter(
    parameter: Metakon5x3Register,
    value: i64,
) -> Result<Metakon5x3Write, ControlOutputConversionError> {
    match parameter {
        Metakon5x3Register::Setpoint => Ok(Metakon5x3Write::Setpoint(metakon_raw_value(
            parameter, value,
        )?)),

        Metakon5x3Register::ProportionalBand => Ok(Metakon5x3Write::ProportionalBand(
            metakon_raw_value(parameter, value)?,
        )),

        Metakon5x3Register::IntegralTime => Ok(Metakon5x3Write::IntegralTime(metakon_raw_value(
            parameter, value,
        )?)),

        Metakon5x3Register::DerivativeTime => Ok(Metakon5x3Write::DerivativeTime(
            metakon_raw_value(parameter, value)?,
        )),

        Metakon5x3Register::OutputPower => Ok(Metakon5x3Write::OutputPower(metakon_raw_value(
            parameter, value,
        )?)),

        Metakon5x3Register::UpperSetpoint => Ok(Metakon5x3Write::UpperSetpoint(metakon_raw_value(
            parameter, value,
        )?)),

        Metakon5x3Register::UpperHysteresis => Ok(Metakon5x3Write::UpperHysteresis(
            metakon_raw_value(parameter, value)?,
        )),

        Metakon5x3Register::LowerSetpoint => Ok(Metakon5x3Write::LowerSetpoint(metakon_raw_value(
            parameter, value,
        )?)),

        Metakon5x3Register::LowerHysteresis => Ok(Metakon5x3Write::LowerHysteresis(
            metakon_raw_value(parameter, value)?,
        )),

        Metakon5x3Register::ChannelType
        | Metakon5x3Register::Measurement
        | Metakon5x3Register::PwmPositive
        | Metakon5x3Register::PwmNegative
        | Metakon5x3Register::UpperOutput
        | Metakon5x3Register::LowerOutput => Err(
            ControlOutputConversionError::UnsupportedMetakonParameter(parameter),
        ),
    }
}

fn metakon_raw_value<T>(
    parameter: Metakon5x3Register,
    value: i64,
) -> Result<T, ControlOutputConversionError>
where
    T: TryFrom<i64>,
{
    T::try_from(value)
        .map_err(|_| ControlOutputConversionError::MetakonRawValueOutOfRange { parameter, value })
}

fn virtual_write_request(
    instrument: VirtualInstrumentId,
    parameter: VirtualParameterId,
    value_type: ParameterValueType,
    value: f64,
) -> Result<InstrumentWriteRequest, ControlOutputConversionError> {
    let value = match value_type {
        ParameterValueType::Integer => InstrumentValue::Integer(rounded_integer(value)?),

        ParameterValueType::Number => InstrumentValue::Number(value),

        ParameterValueType::Boolean => {
            return Err(ControlOutputConversionError::UnsupportedParameterType(
                value_type,
            ));
        }
    };

    Ok(InstrumentWriteRequest::virtual_instrument(
        instrument, parameter, value,
    ))
}

fn rounded_integer(value: f64) -> Result<i64, ControlOutputConversionError> {
    let rounded = value.round();

    let minimum = i64::MIN as f64;

    let maximum_exclusive = -(i64::MIN as f64);

    if !(minimum..maximum_exclusive).contains(&rounded) {
        return Err(ControlOutputConversionError::IntegerOutOfRange { value });
    }

    Ok(rounded as i64)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControlOutputConversionError {
    NonFiniteValue,

    ValueOutOfRange {
        value: f64,
        minimum: f64,
        maximum: f64,
    },

    IntegerOutOfRange {
        value: f64,
    },

    UnsupportedParameterType(ParameterValueType),

    UnsupportedMetakonParameter(Metakon5x3Register),

    MetakonRawValueOutOfRange {
        parameter: Metakon5x3Register,
        value: i64,
    },

    InvalidMetakonValue(Metakon5x3ValueError),
}

impl fmt::Display for ControlOutputConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteValue => formatter.write_str("Control output must be finite"),

            Self::ValueOutOfRange {
                value,
                minimum,
                maximum,
            } => {
                write!(
                    formatter,
                    "Control output must be between \
                     {minimum} and {maximum}, \
                     received {value}",
                )
            }

            Self::IntegerOutOfRange { value } => {
                write!(
                    formatter,
                    "Control output {value} cannot \
                     be represented as an integer",
                )
            }

            Self::UnsupportedParameterType(value_type) => {
                write!(
                    formatter,
                    "Control output does not support \
                     parameter type '{}'",
                    value_type.as_str(),
                )
            }

            Self::UnsupportedMetakonParameter(parameter) => {
                write!(
                    formatter,
                    "Metakon parameter '{}' cannot \
                     receive numeric control output",
                    parameter.descriptor().key,
                )
            }

            Self::MetakonRawValueOutOfRange { parameter, value } => {
                write!(
                    formatter,
                    "Raw control value {value} does \
                     not fit Metakon parameter '{}'",
                    parameter.descriptor().key,
                )
            }

            Self::InvalidMetakonValue(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ControlOutputConversionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidMetakonValue(error) => Some(error),

            Self::NonFiniteValue
            | Self::ValueOutOfRange { .. }
            | Self::IntegerOutOfRange { .. }
            | Self::UnsupportedParameterType(_)
            | Self::UnsupportedMetakonParameter(_)
            | Self::MetakonRawValueOutOfRange { .. } => None,
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
            metakon_5x3::{Metakon5x3, Metakon5x3Register, Metakon5x3Write},
            virtual_instrument::{
                VirtualInstrumentId, VirtualParameterDescriptor, VirtualParameterId,
            },
        },
        process_control::ControlOutputTarget,
    };

    use super::ControlOutputConversionError;

    fn metakon_target(parameter: Metakon5x3Register, scale: f64) -> ControlOutputTarget {
        ControlOutputTarget::metakon_5x3(
            ConnectionId::PRIMARY,
            Metakon5x3::new(3, 0),
            parameter,
            scale,
        )
        .unwrap()
    }

    fn virtual_target(
        value_type: ParameterValueType,
        range: Option<ParameterRange>,
    ) -> ControlOutputTarget {
        let mut parameter = VirtualParameterDescriptor::new(
            VirtualParameterId::new(4),
            "power",
            "Power",
            ParameterAccess::ReadWrite,
            value_type,
        );

        if let Some(range) = range {
            parameter = parameter.with_range(range);
        }

        ControlOutputTarget::virtual_instrument(
            ConnectionId::new(2),
            VirtualInstrumentId::new(7),
            &parameter,
        )
        .unwrap()
    }

    #[test]
    fn converts_metakon_output_power() {
        let target = metakon_target(Metakon5x3Register::OutputPower, 1.0);

        let request = target.write_request(42.7).unwrap();

        assert_eq!(
            request,
            InstrumentWriteRequest::Metakon5x3 {
                instrument: Metakon5x3::new(3, 0),

                parameter: Metakon5x3Write::OutputPower(43),

                scale: 1.0,
            },
        );
    }

    #[test]
    fn rounds_negative_metakon_output_power() {
        let target = metakon_target(Metakon5x3Register::OutputPower, 1.0);

        let request = target.write_request(-42.6).unwrap();

        assert_eq!(
            request,
            InstrumentWriteRequest::Metakon5x3 {
                instrument: Metakon5x3::new(3, 0),

                parameter: Metakon5x3Write::OutputPower(-43),

                scale: 1.0,
            },
        );
    }

    #[test]
    fn converts_scaled_metakon_setpoint() {
        let target = metakon_target(Metakon5x3Register::Setpoint, 0.1);

        let request = target.write_request(12.36).unwrap();

        assert_eq!(
            request,
            InstrumentWriteRequest::Metakon5x3 {
                instrument: Metakon5x3::new(3, 0),

                parameter: Metakon5x3Write::Setpoint(124),

                scale: 0.1,
            },
        );
    }

    #[test]
    fn converts_metakon_integral_time() {
        let scale = 1.0 / 60.0;

        let target = metakon_target(Metakon5x3Register::IntegralTime, scale);

        let request = target.write_request(10.0).unwrap();

        assert_eq!(
            request,
            InstrumentWriteRequest::Metakon5x3 {
                instrument: Metakon5x3::new(3, 0),

                parameter: Metakon5x3Write::IntegralTime(600),

                scale,
            },
        );
    }

    #[test]
    fn converts_other_numeric_metakon_parameters() {
        let parameters = [
            (
                Metakon5x3Register::ProportionalBand,
                25.0,
                Metakon5x3Write::ProportionalBand(25),
            ),
            (
                Metakon5x3Register::DerivativeTime,
                12.0,
                Metakon5x3Write::DerivativeTime(12),
            ),
            (
                Metakon5x3Register::UpperSetpoint,
                150.0,
                Metakon5x3Write::UpperSetpoint(150),
            ),
            (
                Metakon5x3Register::UpperHysteresis,
                8.0,
                Metakon5x3Write::UpperHysteresis(8),
            ),
            (
                Metakon5x3Register::LowerSetpoint,
                -25.0,
                Metakon5x3Write::LowerSetpoint(-25),
            ),
            (
                Metakon5x3Register::LowerHysteresis,
                5.0,
                Metakon5x3Write::LowerHysteresis(5),
            ),
        ];

        for (register, value, expected_parameter) in parameters {
            let target = metakon_target(register, 1.0);

            let request = target.write_request(value).unwrap();

            assert_eq!(
                request,
                InstrumentWriteRequest::Metakon5x3 {
                    instrument: Metakon5x3::new(3, 0),

                    parameter: expected_parameter,

                    scale: 1.0,
                },
            );
        }
    }

    #[test]
    fn converts_virtual_numeric_output() {
        let target = virtual_target(
            ParameterValueType::Number,
            Some(ParameterRange::Number {
                minimum: 0.0,
                maximum: 100.0,
            }),
        );

        let request = target.write_request(42.75).unwrap();

        assert_eq!(
            request,
            InstrumentWriteRequest::VirtualInstrument {
                instrument: VirtualInstrumentId::new(7),

                parameter: VirtualParameterId::new(4),

                value: InstrumentValue::Number(42.75),
            },
        );
    }

    #[test]
    fn rounds_virtual_integer_output() {
        let target = virtual_target(
            ParameterValueType::Integer,
            Some(ParameterRange::Integer {
                minimum: -100,
                maximum: 100,
            }),
        );

        let request = target.write_request(42.7).unwrap();

        assert_eq!(
            request,
            InstrumentWriteRequest::VirtualInstrument {
                instrument: VirtualInstrumentId::new(7),

                parameter: VirtualParameterId::new(4),

                value: InstrumentValue::Integer(43),
            },
        );

        let request = target.write_request(-42.7).unwrap();

        assert_eq!(
            request,
            InstrumentWriteRequest::VirtualInstrument {
                instrument: VirtualInstrumentId::new(7),

                parameter: VirtualParameterId::new(4),

                value: InstrumentValue::Integer(-43),
            },
        );
    }

    #[test]
    fn accepts_output_range_boundaries() {
        let target = virtual_target(
            ParameterValueType::Number,
            Some(ParameterRange::Number {
                minimum: -10.0,
                maximum: 10.0,
            }),
        );

        assert!(target.write_request(-10.0).is_ok(),);

        assert!(target.write_request(10.0).is_ok(),);
    }

    #[test]
    fn rejects_output_outside_parameter_range() {
        let target = metakon_target(Metakon5x3Register::OutputPower, 1.0);

        let result = target.write_request(101.0);

        assert_eq!(
            result,
            Err(ControlOutputConversionError::ValueOutOfRange {
                value: 101.0,
                minimum: -100.0,
                maximum: 100.0,
            },),
        );
    }

    #[test]
    fn rejects_virtual_output_outside_parameter_range() {
        let target = virtual_target(
            ParameterValueType::Number,
            Some(ParameterRange::Number {
                minimum: 0.0,
                maximum: 10.0,
            }),
        );

        let result = target.write_request(-0.1);

        assert_eq!(
            result,
            Err(ControlOutputConversionError::ValueOutOfRange {
                value: -0.1,
                minimum: 0.0,
                maximum: 10.0,
            },),
        );
    }

    #[test]
    fn rejects_non_finite_output() {
        let target = virtual_target(ParameterValueType::Number, None);

        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let result = target.write_request(value);

            assert_eq!(result, Err(ControlOutputConversionError::NonFiniteValue,),);
        }
    }

    #[test]
    fn rejects_integer_output_outside_i64_range() {
        let target = virtual_target(ParameterValueType::Integer, None);

        let value = -(i64::MIN as f64);

        let result = target.write_request(value);

        assert_eq!(
            result,
            Err(ControlOutputConversionError::IntegerOutOfRange { value },),
        );
    }

    #[test]
    fn accepts_minimum_i64_output() {
        let target = virtual_target(ParameterValueType::Integer, None);

        let request = target.write_request(i64::MIN as f64).unwrap();

        assert_eq!(
            request,
            InstrumentWriteRequest::VirtualInstrument {
                instrument: VirtualInstrumentId::new(7),

                parameter: VirtualParameterId::new(4),

                value: InstrumentValue::Integer(i64::MIN),
            },
        );
    }

    #[test]
    fn describes_conversion_errors() {
        assert_eq!(
            ControlOutputConversionError::NonFiniteValue.to_string(),
            "Control output must be finite",
        );

        assert_eq!(
            ControlOutputConversionError::ValueOutOfRange {
                value: 150.0,
                minimum: 0.0,
                maximum: 100.0,
            }
            .to_string(),
            "Control output must be between \
             0 and 100, received 150",
        );

        assert_eq!(
            ControlOutputConversionError::UnsupportedParameterType(ParameterValueType::Boolean,)
                .to_string(),
            "Control output does not support \
             parameter type 'boolean'",
        );
    }

    #[test]
    fn converts_safe_virtual_output() {
        let target = virtual_target(
            ParameterValueType::Number,
            Some(ParameterRange::Number {
                minimum: 0.0,
                maximum: 100.0,
            }),
        )
        .with_safe_value(0.0)
        .unwrap();

        assert_eq!(
            target.safe_write_request(),
            Ok(Some(InstrumentWriteRequest::VirtualInstrument {
                instrument: VirtualInstrumentId::new(7),
                parameter: VirtualParameterId::new(4),
                value: InstrumentValue::Number(0.0,),
            },)),
        );
    }
}
