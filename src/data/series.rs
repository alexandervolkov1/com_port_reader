use super::{Sample, Signal};

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
    Generated(Signal),

    SerialCommand { command: String },
}

impl SeriesSource {
    pub fn generated_signal(&self) -> Option<&Signal> {
        match self {
            Self::Generated(signal) => Some(signal),
            Self::SerialCommand { .. } => None,
        }
    }

    pub(crate) fn default_name_prefix(&self) -> &str {
        match self {
            Self::Generated(signal) => signal.kind_name(),
            Self::SerialCommand { .. } => "serial",
        }
    }
}

impl std::fmt::Display for SeriesSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Generated(signal) => signal.fmt(formatter),

            Self::SerialCommand { command } => {
                write!(formatter, "COM command: {command}")
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
    pub fn unnamed(signal: Signal) -> Self {
        Self {
            source: SeriesSource::Generated(signal),
            name: None,
        }
    }

    pub fn named(signal: Signal, name: impl Into<String>) -> Self {
        Self {
            source: SeriesSource::Generated(signal),
            name: Some(name.into()),
        }
    }

    pub fn serial_command(command: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            source: SeriesSource::SerialCommand {
                command: command.into(),
            },
            name: Some(name.into()),
        }
    }

    pub(crate) fn into_source_parts(self) -> (SeriesSource, Option<String>) {
        (self.source, self.name)
    }

    // Сохраняет совместимость существующих тестов
    // генератора и DSL.
    #[cfg(test)]
    pub(crate) fn into_parts(self) -> (Signal, Option<String>) {
        let (source, name) = self.into_source_parts();

        match source {
            SeriesSource::Generated(signal) => (signal, name),

            SeriesSource::SerialCommand { .. } => {
                panic!(
                    "expected generated series, \
                     found serial series",
                )
            }
        }
    }
}

#[derive(Clone)]
pub struct SignalSeries {
    pub id: SeriesId,
    pub name: String,
    pub source: SeriesSource,
    pub samples: Vec<Sample>,
    pub visible: bool,
}

impl SignalSeries {
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

impl From<&SignalSeries> for SeriesMetadata {
    fn from(series: &SignalSeries) -> Self {
        Self {
            id: series.id,
            name: series.name.clone(),
            source: series.source.clone(),
            visible: series.visible,
        }
    }
}
