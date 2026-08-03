use self::metakon_5x3::{Metakon5x3, Metakon5x3Register, Metakon5x3Write};

pub mod metakon_5x3;
pub mod virtual_instrument;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterAccess {
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

impl ParameterAccess {
    pub const fn readable(self) -> bool {
        matches!(self, Self::ReadOnly | Self::ReadWrite)
    }

    pub const fn writable(self) -> bool {
        matches!(self, Self::WriteOnly | Self::ReadWrite)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::WriteOnly => "write_only",
            Self::ReadWrite => "read_write",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterValueType {
    Boolean,
    Integer,
    Number,
}

impl ParameterValueType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Integer => "integer",
            Self::Number => "number",
        }
    }

    pub fn scaled(self, scale: f64) -> Self {
        match self {
            Self::Boolean => Self::Boolean,

            Self::Integer if scale == 1.0 => Self::Integer,

            Self::Integer | Self::Number => Self::Number,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParameterRange {
    Integer { minimum: i64, maximum: i64 },

    Number { minimum: f64, maximum: f64 },
}

impl ParameterRange {
    pub const fn integer(minimum: i64, maximum: i64) -> Self {
        Self::Integer { minimum, maximum }
    }

    pub fn scaled(self, scale: f64) -> Self {
        match self {
            Self::Integer { minimum, maximum } if scale == 1.0 => {
                Self::Integer { minimum, maximum }
            }

            Self::Integer { minimum, maximum } => Self::Number {
                minimum: minimum as f64 * scale,
                maximum: maximum as f64 * scale,
            },

            Self::Number { minimum, maximum } => Self::Number {
                minimum: minimum * scale,
                maximum: maximum * scale,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParameterDescriptor {
    pub key: &'static str,
    pub name: &'static str,
    pub access: ParameterAccess,
    pub value_type: ParameterValueType,
    pub range: ParameterRange,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InstrumentReadRequest {
    Metakon5x3 {
        instrument: Metakon5x3,
        parameter: Metakon5x3Register,
        scale: f64,
    },
}

impl InstrumentReadRequest {
    pub const fn metakon_5x3(
        instrument: Metakon5x3,
        parameter: Metakon5x3Register,
        scale: f64,
    ) -> Self {
        Self::Metakon5x3 {
            instrument,
            parameter,
            scale,
        }
    }

    pub const fn scale(&self) -> f64 {
        match self {
            Self::Metakon5x3 { scale, .. } => *scale,
        }
    }

    pub(crate) const fn default_name_prefix(&self) -> &'static str {
        match self {
            Self::Metakon5x3 { .. } => "metakon",
        }
    }

    pub(crate) const fn kind_name(&self) -> &'static str {
        match self {
            Self::Metakon5x3 { .. } => "Metakon",
        }
    }
}

impl std::fmt::Display for InstrumentReadRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Metakon5x3 {
                instrument,
                parameter,
                scale,
            } => {
                write!(
                    formatter,
                    "Metakon 5X3: device {}, channel {}, \
                     parameter {}, scale {}",
                    instrument.device(),
                    instrument.channel(),
                    parameter,
                    scale,
                )
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InstrumentWriteRequest {
    Metakon5x3 {
        instrument: Metakon5x3,
        parameter: Metakon5x3Write,
        scale: f64,
    },
}

impl InstrumentWriteRequest {
    pub fn metakon_5x3(
        instrument: Metakon5x3,
        parameter: Metakon5x3Write,
        scale: f64,
    ) -> Result<Self, metakon_5x3::Metakon5x3ValueError> {
        parameter.validate()?;

        Ok(Self::Metakon5x3 {
            instrument,
            parameter,
            scale,
        })
    }
}

impl std::fmt::Display for InstrumentWriteRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Metakon5x3 {
                instrument,
                parameter,
                scale,
            } => {
                write!(
                    formatter,
                    "Metakon 5X3 device {}, channel {}, \
                     parameter {} = {}, scale {}",
                    instrument.device(),
                    instrument.channel(),
                    parameter.register(),
                    parameter.value(),
                    scale,
                )
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InstrumentValue {
    Boolean(bool),
    Integer(i64),
    Number(f64),
}

impl InstrumentValue {
    pub const fn as_f64(self) -> f64 {
        match self {
            Self::Boolean(value) => {
                if value {
                    1.0
                } else {
                    0.0
                }
            }

            Self::Integer(value) => value as f64,

            Self::Number(value) => value,
        }
    }
}

impl std::fmt::Display for InstrumentValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Boolean(value) => value.fmt(formatter),

            Self::Integer(value) => value.fmt(formatter),

            Self::Number(value) => value.fmt(formatter),
        }
    }
}
