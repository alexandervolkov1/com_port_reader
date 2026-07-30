use super::Sample;

pub const DEFAULT_METAKON_DEVICE: u8 = 1;
pub const DEFAULT_METAKON_CHANNEL: u8 = 0;
pub const DEFAULT_METAKON_REGISTER: u8 = 0x01;
pub const DEFAULT_METAKON_SCALE: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetakonValueType {
    Ubyte,
    Byte,
    Uint,
    Int,
}

impl std::fmt::Display for MetakonValueType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Ubyte => "Ubyte",
            Self::Byte => "Byte",
            Self::Uint => "Uint",
            Self::Int => "Int",
        };

        formatter.write_str(name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SeriesId(u64);

impl SeriesId {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for SeriesId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SeriesSource {
    SerialCommand {
        command: String,
    },

    Metakon {
        device: u8,
        channel: u8,
        register: u8,
        value_type: MetakonValueType,
        scale: f64,
    },
}

impl SeriesSource {
    pub(crate) fn default_name_prefix(&self) -> &str {
        match self {
            Self::SerialCommand { .. } => "serial",
            Self::Metakon { .. } => "metakon",
        }
    }
}

impl std::fmt::Display for SeriesSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SerialCommand { command } => {
                write!(formatter, "COM command: {command}")
            }

            Self::Metakon {
                device,
                channel,
                register,
                value_type,
                scale,
            } => {
                write!(
                    formatter,
                    "Metakon: device {device}, channel {channel}, \
                     register 0x{register:02X}, type {value_type}, \
                     scale {scale}",
                )
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct NewSeries {
    source: SeriesSource,
    name: Option<String>,
}

impl NewSeries {
    pub fn unnamed_serial_command(command: impl Into<String>) -> Self {
        Self {
            source: SeriesSource::SerialCommand {
                command: command.into(),
            },
            name: None,
        }
    }

    pub fn named_serial_command(command: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            source: SeriesSource::SerialCommand {
                command: command.into(),
            },
            name: Some(name.into()),
        }
    }

    pub fn unnamed_typed_metakon(
        device: u8,
        channel: u8,
        register: u8,
        value_type: MetakonValueType,
        scale: f64,
    ) -> Self {
        Self {
            source: SeriesSource::Metakon {
                device,
                channel,
                register,
                value_type,
                scale,
            },
            name: None,
        }
    }

    pub fn named_typed_metakon(
        device: u8,
        channel: u8,
        register: u8,
        value_type: MetakonValueType,
        scale: f64,
        name: impl Into<String>,
    ) -> Self {
        Self {
            source: SeriesSource::Metakon {
                device,
                channel,
                register,
                value_type,
                scale,
            },
            name: Some(name.into()),
        }
    }

    pub(crate) fn into_source_parts(self) -> (SeriesSource, Option<String>) {
        (self.source, self.name)
    }
}

#[derive(Clone)]
pub struct Series {
    pub id: SeriesId,
    pub name: String,
    pub source: SeriesSource,
    pub samples: Vec<Sample>,
    pub visible: bool,
}

impl Series {
    pub(crate) fn new(id: SeriesId, name: String, source: SeriesSource) -> Self {
        Self {
            id,
            name,
            source,
            samples: Vec::new(),
            visible: true,
        }
    }
}

#[derive(Clone)]
pub struct SeriesMetadata {
    pub id: SeriesId,
    pub name: String,
    pub source: SeriesSource,
    pub visible: bool,
}

impl From<&Series> for SeriesMetadata {
    fn from(series: &Series) -> Self {
        Self {
            id: series.id,
            name: series.name.clone(),
            source: series.source.clone(),
            visible: series.visible,
        }
    }
}
