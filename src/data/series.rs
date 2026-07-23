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

#[derive(Clone, Debug)]
pub enum SeriesSource {
    Generated(Signal),
}

impl std::fmt::Display for SeriesSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Generated(signal) => signal.fmt(formatter),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NewSeries {
    signal: Signal,
    name: Option<String>,
}

impl NewSeries {
    pub fn unnamed(signal: Signal) -> Self {
        Self { signal, name: None }
    }

    pub fn named(signal: Signal, name: impl Into<String>) -> Self {
        Self {
            signal,
            name: Some(name.into()),
        }
    }

    pub(crate) fn into_parts(self) -> (Signal, Option<String>) {
        (self.signal, self.name)
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
