use std::fmt;

use crate::{
    connection::ConnectionId,
    instrument::{
        ParameterAccess, ParameterRange, ParameterValueType,
        metakon_5x3::{Metakon5x3, Metakon5x3Register},
        virtual_instrument::{VirtualInstrumentId, VirtualParameterDescriptor, VirtualParameterId},
    },
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlOutputTarget {
    connection_id: ConnectionId,
    parameter: ControlOutputParameter,
}

impl ControlOutputTarget {
    pub fn metakon_5x3(
        connection_id: ConnectionId,
        instrument: Metakon5x3,
        parameter: Metakon5x3Register,
        scale: f64,
    ) -> Result<Self, ControlOutputTargetError> {
        let descriptor = parameter.descriptor();

        validate_numeric_output(descriptor.key, descriptor.access, descriptor.value_type)?;

        if !scale.is_finite() || scale <= 0.0 {
            return Err(ControlOutputTargetError::InvalidScale);
        }

        Ok(Self {
            connection_id,

            parameter: ControlOutputParameter::Metakon5x3 {
                instrument,
                parameter,
                scale,
            },
        })
    }

    pub fn virtual_instrument(
        connection_id: ConnectionId,
        instrument: VirtualInstrumentId,
        parameter: &VirtualParameterDescriptor,
    ) -> Result<Self, ControlOutputTargetError> {
        validate_numeric_output(parameter.key(), parameter.access(), parameter.value_type())?;

        Ok(Self {
            connection_id,

            parameter: ControlOutputParameter::VirtualInstrument {
                instrument,
                parameter: parameter.id(),
                value_type: parameter.value_type(),
                range: parameter.range(),
            },
        })
    }

    pub const fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }

    pub const fn parameter(&self) -> ControlOutputParameter {
        self.parameter
    }

    pub fn value_type(&self) -> ParameterValueType {
        match self.parameter {
            ControlOutputParameter::Metakon5x3 {
                parameter, scale, ..
            } => parameter.descriptor().value_type.scaled(scale),

            ControlOutputParameter::VirtualInstrument { value_type, .. } => value_type,
        }
    }

    pub fn range(&self) -> Option<ParameterRange> {
        match self.parameter {
            ControlOutputParameter::Metakon5x3 {
                parameter, scale, ..
            } => Some(parameter.descriptor().range.scaled(scale)),

            ControlOutputParameter::VirtualInstrument { range, .. } => range,
        }
    }
}

impl fmt::Display for ControlOutputTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.parameter {
            ControlOutputParameter::Metakon5x3 {
                instrument,
                parameter,
                ..
            } => {
                write!(
                    formatter,
                    "connection {}, Metakon 5X3 \
                     device {}, channel {}, parameter {}",
                    self.connection_id,
                    instrument.device(),
                    instrument.channel(),
                    parameter.descriptor().key,
                )
            }

            ControlOutputParameter::VirtualInstrument {
                instrument,
                parameter,
                ..
            } => {
                write!(
                    formatter,
                    "connection {}, virtual instrument {}, \
                     parameter {}",
                    self.connection_id, instrument, parameter,
                )
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControlOutputParameter {
    Metakon5x3 {
        instrument: Metakon5x3,
        parameter: Metakon5x3Register,
        scale: f64,
    },

    VirtualInstrument {
        instrument: VirtualInstrumentId,
        parameter: VirtualParameterId,
        value_type: ParameterValueType,
        range: Option<ParameterRange>,
    },
}

fn validate_numeric_output(
    key: &str,
    access: ParameterAccess,
    value_type: ParameterValueType,
) -> Result<(), ControlOutputTargetError> {
    if !access.writable() {
        return Err(ControlOutputTargetError::ReadOnlyParameter(key.to_owned()));
    }

    if value_type == ParameterValueType::Boolean {
        return Err(ControlOutputTargetError::NonNumericParameter(
            key.to_owned(),
        ));
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlOutputTargetError {
    ReadOnlyParameter(String),

    NonNumericParameter(String),

    InvalidScale,
}

impl fmt::Display for ControlOutputTargetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadOnlyParameter(parameter) => {
                write!(
                    formatter,
                    "Output parameter '{parameter}' \
                     is read-only",
                )
            }

            Self::NonNumericParameter(parameter) => {
                write!(
                    formatter,
                    "Output parameter '{parameter}' \
                     must be numeric",
                )
            }

            Self::InvalidScale => formatter.write_str(
                "Metakon output scale must be finite \
                     and greater than zero",
            ),
        }
    }
}

impl std::error::Error for ControlOutputTargetError {}

#[cfg(test)]
mod tests {
    use crate::{
        connection::ConnectionId,
        instrument::{
            ParameterAccess, ParameterRange, ParameterValueType,
            metakon_5x3::{Metakon5x3, Metakon5x3Register},
            virtual_instrument::{
                VirtualInstrumentId, VirtualParameterDescriptor, VirtualParameterId,
            },
        },
    };

    use super::{ControlOutputParameter, ControlOutputTarget, ControlOutputTargetError};

    #[test]
    fn creates_metakon_output_target() {
        let target = ControlOutputTarget::metakon_5x3(
            ConnectionId::new(7),
            Metakon5x3::new(3, 0),
            Metakon5x3Register::OutputPower,
            1.0,
        )
        .unwrap();

        assert_eq!(target.connection_id(), ConnectionId::new(7),);

        assert_eq!(
            target.parameter(),
            ControlOutputParameter::Metakon5x3 {
                instrument: Metakon5x3::new(3, 0),
                parameter: Metakon5x3Register::OutputPower,
                scale: 1.0,
            },
        );

        assert_eq!(target.value_type(), ParameterValueType::Integer,);

        assert_eq!(
            target.range(),
            Some(ParameterRange::Integer {
                minimum: -100,
                maximum: 100,
            }),
        );

        assert_eq!(
            target.to_string(),
            "connection 7, Metakon 5X3 device 3, \
             channel 0, parameter output_power",
        );
    }

    #[test]
    fn scales_metakon_output_parameter() {
        let target = ControlOutputTarget::metakon_5x3(
            ConnectionId::PRIMARY,
            Metakon5x3::new(1, 0),
            Metakon5x3Register::Setpoint,
            0.1,
        )
        .unwrap();

        assert_eq!(target.value_type(), ParameterValueType::Number,);

        let Some(ParameterRange::Number { minimum, maximum }) = target.range() else {
            panic!(
                "scaled Metakon parameter \
                 must expose numeric range"
            );
        };

        assert!((minimum - -99.9).abs() < 1.0e-12);

        assert!((maximum - 999.9).abs() < 1.0e-12);
    }

    #[test]
    fn rejects_read_only_metakon_parameter() {
        let result = ControlOutputTarget::metakon_5x3(
            ConnectionId::PRIMARY,
            Metakon5x3::new(1, 0),
            Metakon5x3Register::Measurement,
            1.0,
        );

        assert_eq!(
            result,
            Err(ControlOutputTargetError::ReadOnlyParameter(
                "measurement".to_owned(),
            ),),
        );
    }

    #[test]
    fn rejects_boolean_metakon_parameter() {
        let result = ControlOutputTarget::metakon_5x3(
            ConnectionId::PRIMARY,
            Metakon5x3::new(1, 0),
            Metakon5x3Register::UpperOutput,
            1.0,
        );

        assert_eq!(
            result,
            Err(ControlOutputTargetError::NonNumericParameter(
                "upper_output".to_owned(),
            ),),
        );
    }

    #[test]
    fn rejects_invalid_metakon_output_scale() {
        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let result = ControlOutputTarget::metakon_5x3(
                ConnectionId::PRIMARY,
                Metakon5x3::new(1, 0),
                Metakon5x3Register::OutputPower,
                scale,
            );

            assert_eq!(result, Err(ControlOutputTargetError::InvalidScale),);
        }
    }

    #[test]
    fn creates_virtual_instrument_output_target() {
        let parameter = VirtualParameterDescriptor::new(
            VirtualParameterId::new(4),
            "heater_power",
            "Heater power",
            ParameterAccess::ReadWrite,
            ParameterValueType::Number,
        )
        .with_range(ParameterRange::Number {
            minimum: 0.0,
            maximum: 100.0,
        });

        let target = ControlOutputTarget::virtual_instrument(
            ConnectionId::new(2),
            VirtualInstrumentId::new(9),
            &parameter,
        )
        .unwrap();

        assert_eq!(target.connection_id(), ConnectionId::new(2),);

        assert_eq!(
            target.parameter(),
            ControlOutputParameter::VirtualInstrument {
                instrument: VirtualInstrumentId::new(9),
                parameter: VirtualParameterId::new(4),
                value_type: ParameterValueType::Number,

                range: Some(ParameterRange::Number {
                    minimum: 0.0,
                    maximum: 100.0,
                }),
            },
        );

        assert_eq!(target.value_type(), ParameterValueType::Number,);

        assert_eq!(
            target.range(),
            Some(ParameterRange::Number {
                minimum: 0.0,
                maximum: 100.0,
            }),
        );

        assert_eq!(
            target.to_string(),
            "connection 2, virtual instrument 9, \
             parameter 4",
        );
    }

    #[test]
    fn accepts_write_only_virtual_parameter() {
        let parameter = VirtualParameterDescriptor::new(
            VirtualParameterId::new(5),
            "power",
            "Power",
            ParameterAccess::WriteOnly,
            ParameterValueType::Integer,
        );

        let target = ControlOutputTarget::virtual_instrument(
            ConnectionId::PRIMARY,
            VirtualInstrumentId::new(1),
            &parameter,
        )
        .unwrap();

        assert_eq!(target.value_type(), ParameterValueType::Integer,);

        assert_eq!(target.range(), None);
    }

    #[test]
    fn rejects_read_only_virtual_parameter() {
        let parameter = VirtualParameterDescriptor::new(
            VirtualParameterId::new(1),
            "temperature",
            "Temperature",
            ParameterAccess::ReadOnly,
            ParameterValueType::Number,
        );

        let result = ControlOutputTarget::virtual_instrument(
            ConnectionId::PRIMARY,
            VirtualInstrumentId::new(1),
            &parameter,
        );

        assert_eq!(
            result,
            Err(ControlOutputTargetError::ReadOnlyParameter(
                "temperature".to_owned(),
            ),),
        );
    }

    #[test]
    fn rejects_boolean_virtual_parameter() {
        let parameter = VirtualParameterDescriptor::new(
            VirtualParameterId::new(2),
            "enabled",
            "Enabled",
            ParameterAccess::ReadWrite,
            ParameterValueType::Boolean,
        );

        let result = ControlOutputTarget::virtual_instrument(
            ConnectionId::PRIMARY,
            VirtualInstrumentId::new(1),
            &parameter,
        );

        assert_eq!(
            result,
            Err(ControlOutputTargetError::NonNumericParameter(
                "enabled".to_owned(),
            ),),
        );
    }

    #[test]
    fn describes_output_target_errors() {
        assert_eq!(
            ControlOutputTargetError::ReadOnlyParameter("temperature".to_owned(),).to_string(),
            "Output parameter 'temperature' is read-only",
        );

        assert_eq!(
            ControlOutputTargetError::NonNumericParameter("enabled".to_owned(),).to_string(),
            "Output parameter 'enabled' must be numeric",
        );

        assert_eq!(
            ControlOutputTargetError::InvalidScale.to_string(),
            "Metakon output scale must be finite \
             and greater than zero",
        );
    }
}
