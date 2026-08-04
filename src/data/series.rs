use crate::instrument::InstrumentReadRequest;

use super::{Sample, SamplingInterval};

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

#[derive(Clone, Debug, PartialEq)]
pub enum SeriesSource {
    SerialCommand { command: String },

    Instrument(InstrumentReadRequest),
}

impl SeriesSource {
    pub(crate) fn default_name_prefix(&self) -> &str {
        match self {
            Self::SerialCommand { .. } => "serial",

            Self::Instrument(request) => request.default_name_prefix(),
        }
    }
}

impl std::fmt::Display for SeriesSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SerialCommand { command } => {
                write!(formatter, "COM command: {command}",)
            }

            Self::Instrument(request) => request.fmt(formatter),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NewSeries {
    source: SeriesSource,
    name: Option<String>,
    sampling_interval: Option<SamplingInterval>,
}

impl NewSeries {
    pub fn unnamed_serial_command(command: impl Into<String>) -> Self {
        Self {
            source: SeriesSource::SerialCommand {
                command: command.into(),
            },
            name: None,
            sampling_interval: None,
        }
    }

    pub fn named_serial_command(command: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            source: SeriesSource::SerialCommand {
                command: command.into(),
            },
            name: Some(name.into()),
            sampling_interval: None,
        }
    }

    pub fn unnamed_instrument(request: InstrumentReadRequest) -> Self {
        Self {
            source: SeriesSource::Instrument(request),
            name: None,
            sampling_interval: None,
        }
    }

    pub fn named_instrument(request: InstrumentReadRequest, name: impl Into<String>) -> Self {
        Self {
            source: SeriesSource::Instrument(request),
            name: Some(name.into()),
            sampling_interval: None,
        }
    }

    pub fn with_sampling_interval(mut self, interval: SamplingInterval) -> Self {
        self.sampling_interval = Some(interval);
        self
    }

    pub(crate) fn into_parts(self) -> (SeriesSource, Option<String>, Option<SamplingInterval>) {
        (self.source, self.name, self.sampling_interval)
    }
}

#[derive(Clone)]
pub struct Series {
    pub id: SeriesId,
    pub name: String,
    pub source: SeriesSource,
    pub sampling_interval: Option<SamplingInterval>,
    pub samples: Vec<Sample>,
    pub visible: bool,
}

impl Series {
    pub(crate) fn new(
        id: SeriesId,
        name: String,
        source: SeriesSource,
        sampling_interval: Option<SamplingInterval>,
    ) -> Self {
        Self {
            id,
            name,
            source,
            sampling_interval,
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
    pub sampling_interval: Option<SamplingInterval>,
    pub visible: bool,
}

impl From<&Series> for SeriesMetadata {
    fn from(series: &Series) -> Self {
        Self {
            id: series.id,
            name: series.name.clone(),
            source: series.source.clone(),
            sampling_interval: series.sampling_interval,
            visible: series.visible,
        }
    }
}
