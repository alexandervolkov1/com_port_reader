use self::metakon_5x3::{Metakon5x3, Metakon5x3Register, Metakon5x3Write};

pub mod metakon_5x3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterAccess {
    ReadOnly,
    ReadWrite,
}

impl ParameterAccess {
    pub const fn writable(self) -> bool {
        matches!(self, Self::ReadWrite)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::ReadWrite => "read_write",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterValueType {
    Boolean,
    Unsigned8,
    Signed8,
    Unsigned16,
    Signed16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParameterRange {
    pub minimum: i64,
    pub maximum: i64,
}

impl ParameterValueType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Unsigned8 => "u8",
            Self::Signed8 => "i8",
            Self::Unsigned16 => "u16",
            Self::Signed16 => "i16",
        }
    }
}

impl ParameterRange {
    pub const fn new(minimum: i64, maximum: i64) -> Self {
        Self { minimum, maximum }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstrumentWriteRequest {
    Metakon5x3 {
        instrument: Metakon5x3,
        parameter: Metakon5x3Write,
    },
}

impl InstrumentWriteRequest {
    pub fn metakon_5x3(
        instrument: Metakon5x3,
        parameter: Metakon5x3Write,
    ) -> Result<Self, metakon_5x3::Metakon5x3ValueError> {
        parameter.validate()?;

        Ok(Self::Metakon5x3 {
            instrument,
            parameter,
        })
    }
}

impl std::fmt::Display for InstrumentWriteRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Metakon5x3 {
                instrument,
                parameter,
            } => {
                write!(
                    formatter,
                    "Metakon 5X3 device {}, channel {}, \
                     parameter {} = {}",
                    instrument.device(),
                    instrument.channel(),
                    parameter.register(),
                    parameter.value(),
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
