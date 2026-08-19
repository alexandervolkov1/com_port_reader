use crate::{
    connection::ConnectionId, instrument::InstrumentReadRequest,
    signal_processing::SignalFilterDefinition,
};

use super::{Sample, SamplingInterval, SeriesColor};

pub const DEFAULT_METAKON_DEVICE: u8 = 1;
pub const DEFAULT_METAKON_CHANNEL: u8 = 0;
pub const DEFAULT_METAKON_SCALE: f64 = 1.0;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SeriesPollingState {
    #[default]
    Enabled,
    Suspended,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SeriesSource {
    SerialCommand {
        command: String,
    },
    Instrument(InstrumentReadRequest),
    Filtered {
        input: SeriesId,
        definition: SignalFilterDefinition,
    },
}

impl SeriesSource {
    pub(crate) fn default_name_prefix(&self) -> &str {
        match self {
            Self::SerialCommand { .. } => "serial",

            Self::Instrument(request) => request.default_name_prefix(),

            Self::Filtered { .. } => "filtered",
        }
    }

    pub(crate) const fn is_polled(&self) -> bool {
        !matches!(self, Self::Filtered { .. })
    }
}

impl std::fmt::Display for SeriesSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SerialCommand { command } => {
                write!(formatter, "COM command: {command}",)
            }

            Self::Instrument(request) => request.fmt(formatter),

            Self::Filtered { input, definition } => {
                write!(formatter, "Filtered series {input}: {definition}",)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct NewSeries {
    source: SeriesSource,
    name: Option<String>,
    sampling_interval: Option<SamplingInterval>,
    connection_id: ConnectionId,
    color: Option<SeriesColor>,
}

impl NewSeries {
    pub fn unnamed_serial_command(command: impl Into<String>) -> Self {
        Self {
            source: SeriesSource::SerialCommand {
                command: command.into(),
            },
            name: None,
            sampling_interval: None,
            connection_id: ConnectionId::PRIMARY,
            color: None,
        }
    }

    pub fn named_serial_command(command: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            source: SeriesSource::SerialCommand {
                command: command.into(),
            },
            name: Some(name.into()),
            sampling_interval: None,
            connection_id: ConnectionId::PRIMARY,
            color: None,
        }
    }

    pub fn unnamed_instrument(request: InstrumentReadRequest) -> Self {
        Self {
            source: SeriesSource::Instrument(request),
            name: None,
            sampling_interval: None,
            connection_id: ConnectionId::PRIMARY,
            color: None,
        }
    }

    pub fn named_instrument(request: InstrumentReadRequest, name: impl Into<String>) -> Self {
        Self {
            source: SeriesSource::Instrument(request),
            name: Some(name.into()),
            sampling_interval: None,
            connection_id: ConnectionId::PRIMARY,
            color: None,
        }
    }

    pub(crate) fn named_filtered(
        input: SeriesId,
        definition: SignalFilterDefinition,
        name: impl Into<String>,
    ) -> Self {
        Self {
            source: SeriesSource::Filtered { input, definition },
            name: Some(name.into()),
            sampling_interval: None,
            connection_id: ConnectionId::PRIMARY,
            color: None,
        }
    }

    pub fn with_sampling_interval(mut self, interval: SamplingInterval) -> Self {
        self.sampling_interval = Some(interval);
        self
    }

    pub fn with_color(mut self, color: SeriesColor) -> Self {
        self.color = Some(color);
        self
    }

    pub(crate) const fn source(&self) -> &SeriesSource {
        &self.source
    }

    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub(crate) const fn sampling_interval(&self) -> Option<SamplingInterval> {
        self.sampling_interval
    }

    pub(crate) fn into_parts(self) -> (SeriesSource, Option<String>, Option<SamplingInterval>) {
        (self.source, self.name, self.sampling_interval)
    }

    pub(crate) const fn color(&self) -> Option<SeriesColor> {
        self.color
    }

    pub fn with_connection(mut self, connection_id: ConnectionId) -> Self {
        self.connection_id = connection_id;
        self
    }

    pub(crate) const fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewFilteredSeries {
    input_name: String,
    name: String,
    definition: SignalFilterDefinition,
    color: Option<SeriesColor>,
}

impl NewFilteredSeries {
    pub fn new(
        input_name: impl Into<String>,
        name: impl Into<String>,
        definition: SignalFilterDefinition,
    ) -> Self {
        Self {
            input_name: input_name.into(),
            name: name.into(),
            definition,
            color: None,
        }
    }

    pub fn with_color(mut self, color: SeriesColor) -> Self {
        self.color = Some(color);
        self
    }

    pub(crate) fn input_name(&self) -> &str {
        &self.input_name
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) const fn definition(&self) -> SignalFilterDefinition {
        self.definition
    }

    pub(crate) const fn color(&self) -> Option<SeriesColor> {
        self.color
    }

    pub(crate) fn into_parts(
        self,
    ) -> (String, String, SignalFilterDefinition, Option<SeriesColor>) {
        (self.input_name, self.name, self.definition, self.color)
    }
}

#[derive(Clone)]
pub struct Series {
    pub id: SeriesId,
    pub connection_id: ConnectionId,
    pub name: String,
    pub source: SeriesSource,
    pub samples: Vec<Sample>,
    pub visible: bool,
    pub sampling_interval: Option<SamplingInterval>,
    pub polling_state: SeriesPollingState,
    pub color: Option<SeriesColor>,
}

impl Series {
    pub(crate) fn new(
        id: SeriesId,
        name: String,
        source: SeriesSource,
        sampling_interval: Option<SamplingInterval>,
        connection_id: ConnectionId,
        color: Option<SeriesColor>,
    ) -> Self {
        Self {
            id,
            connection_id,
            name,
            source,
            samples: Vec::new(),
            visible: true,
            sampling_interval,
            polling_state: SeriesPollingState::Enabled,
            color,
        }
    }
}

#[derive(Clone)]
pub struct SeriesMetadata {
    pub id: SeriesId,
    pub connection_id: ConnectionId,
    pub name: String,
    pub source: SeriesSource,
    pub visible: bool,
    pub sampling_interval: Option<SamplingInterval>,
    pub polling_state: SeriesPollingState,
}

impl From<&Series> for SeriesMetadata {
    fn from(series: &Series) -> Self {
        Self {
            id: series.id,
            connection_id: series.connection_id,
            name: series.name.clone(),
            source: series.source.clone(),
            visible: series.visible,
            sampling_interval: series.sampling_interval,
            polling_state: series.polling_state,
        }
    }
}
