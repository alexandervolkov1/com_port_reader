use super::{Sample, SamplingInterval, SeriesColor};
use crate::{
    connection::ConnectionId, instrument::InstrumentReadRequest, presentation::PlotPaneKey,
    process_control::ControllerDiagnostic, signal_processing::SignalFilterDefinition,
};

pub const DEFAULT_METAKON_DEVICE: u8 = 1;
pub const DEFAULT_METAKON_CHANNEL: u8 = 0;
pub const DEFAULT_METAKON_SCALE: f64 = 1.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeriesPresentation {
    pub visible: bool,
    pub color: Option<SeriesColor>,
    pub pane: Option<PlotPaneKey>,
}

impl Default for SeriesPresentation {
    fn default() -> Self {
        Self {
            visible: true,
            color: None,
            pane: None,
        }
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

    ControllerDiagnostic {
        controller: String,
        diagnostic: ControllerDiagnostic,
    },
}

impl SeriesSource {
    pub(crate) fn default_name_prefix(&self) -> &str {
        match self {
            Self::SerialCommand { .. } => "serial",

            Self::Instrument(request) => request.default_name_prefix(),

            Self::Filtered { .. } => "filtered",

            Self::ControllerDiagnostic { .. } => "controller",
        }
    }

    pub(crate) const fn is_polled(&self) -> bool {
        match self {
            Self::SerialCommand { .. } | Self::Instrument(_) => true,

            Self::Filtered { .. } | Self::ControllerDiagnostic { .. } => false,
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

            Self::Filtered { input, definition } => {
                write!(formatter, "Filtered series {input}: {definition}",)
            }

            Self::ControllerDiagnostic {
                controller,
                diagnostic,
            } => {
                write!(
                    formatter,
                    "Controller '{controller}' \
                     diagnostic: {diagnostic}",
                )
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
    presentation: SeriesPresentation,
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
            presentation: SeriesPresentation::default(),
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
            presentation: SeriesPresentation::default(),
        }
    }

    pub fn unnamed_instrument(request: InstrumentReadRequest) -> Self {
        Self {
            source: SeriesSource::Instrument(request),
            name: None,
            sampling_interval: None,
            connection_id: ConnectionId::PRIMARY,
            presentation: SeriesPresentation::default(),
        }
    }

    pub fn named_instrument(request: InstrumentReadRequest, name: impl Into<String>) -> Self {
        Self {
            source: SeriesSource::Instrument(request),
            name: Some(name.into()),
            sampling_interval: None,
            connection_id: ConnectionId::PRIMARY,
            presentation: SeriesPresentation::default(),
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
            presentation: SeriesPresentation::default(),
        }
    }

    pub fn named_controller_diagnostic(
        controller: impl Into<String>,
        diagnostic: ControllerDiagnostic,
        name: impl Into<String>,
    ) -> Self {
        Self {
            source: SeriesSource::ControllerDiagnostic {
                controller: controller.into(),
                diagnostic,
            },

            name: Some(name.into()),

            sampling_interval: None,

            connection_id: ConnectionId::PRIMARY,

            presentation: SeriesPresentation::default(),
        }
    }

    pub fn with_sampling_interval(mut self, interval: SamplingInterval) -> Self {
        self.sampling_interval = Some(interval);
        self
    }

    pub fn with_color(mut self, color: SeriesColor) -> Self {
        self.presentation.color = Some(color);
        self
    }

    pub fn with_visibility(mut self, visible: bool) -> Self {
        self.presentation.visible = visible;
        self
    }

    pub fn with_pane(mut self, pane: PlotPaneKey) -> Self {
        self.presentation.pane = Some(pane);
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

    pub(crate) fn into_parts(
        self,
    ) -> (
        SeriesSource,
        Option<String>,
        Option<SamplingInterval>,
        SeriesPresentation,
    ) {
        (
            self.source,
            self.name,
            self.sampling_interval,
            self.presentation,
        )
    }

    pub(crate) const fn color(&self) -> Option<SeriesColor> {
        self.presentation.color
    }

    pub(crate) fn pane(&self) -> Option<&PlotPaneKey> {
        self.presentation.pane.as_ref()
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
    presentation: SeriesPresentation,
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
            presentation: SeriesPresentation::default(),
        }
    }

    pub fn with_color(mut self, color: SeriesColor) -> Self {
        self.presentation.color = Some(color);
        self
    }

    pub fn with_visibility(mut self, visible: bool) -> Self {
        self.presentation.visible = visible;
        self
    }

    pub fn with_pane(mut self, pane: PlotPaneKey) -> Self {
        self.presentation.pane = Some(pane);
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
        self.presentation.color
    }

    pub(crate) fn pane(&self) -> Option<&PlotPaneKey> {
        self.presentation.pane.as_ref()
    }

    pub(crate) fn into_parts(self) -> (String, String, SignalFilterDefinition, SeriesPresentation) {
        (
            self.input_name,
            self.name,
            self.definition,
            self.presentation,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewControllerDiagnosticSeries {
    controller: String,
    diagnostic: ControllerDiagnostic,
    name: String,
    connection_id: ConnectionId,
    presentation: SeriesPresentation,
}

impl NewControllerDiagnosticSeries {
    pub fn new(
        controller: impl Into<String>,
        diagnostic: ControllerDiagnostic,
        name: impl Into<String>,
    ) -> Self {
        Self {
            controller: controller.into(),
            diagnostic,
            name: name.into(),
            connection_id: ConnectionId::PRIMARY,
            presentation: SeriesPresentation::default(),
        }
    }

    pub fn with_connection(mut self, connection_id: ConnectionId) -> Self {
        self.connection_id = connection_id;
        self
    }

    pub fn with_color(mut self, color: SeriesColor) -> Self {
        self.presentation.color = Some(color);
        self
    }

    pub fn with_visibility(mut self, visible: bool) -> Self {
        self.presentation.visible = visible;
        self
    }

    pub fn with_pane(mut self, pane: PlotPaneKey) -> Self {
        self.presentation.pane = Some(pane);
        self
    }

    pub(crate) fn pane(&self) -> Option<&PlotPaneKey> {
        self.presentation.pane.as_ref()
    }

    pub fn into_parts(
        self,
    ) -> (
        String,
        ControllerDiagnostic,
        String,
        ConnectionId,
        SeriesPresentation,
    ) {
        (
            self.controller,
            self.diagnostic,
            self.name,
            self.connection_id,
            self.presentation,
        )
    }
}

#[derive(Clone)]
pub struct Series {
    pub id: SeriesId,
    pub connection_id: ConnectionId,
    pub name: String,
    pub source: SeriesSource,
    pub samples: Vec<Sample>,
    pub presentation: SeriesPresentation,
    pub sampling_interval: Option<SamplingInterval>,
    pub polling_state: SeriesPollingState,
}

impl Series {
    pub(crate) fn new(
        id: SeriesId,
        name: String,
        source: SeriesSource,
        sampling_interval: Option<SamplingInterval>,
        connection_id: ConnectionId,
        presentation: SeriesPresentation,
    ) -> Self {
        Self {
            id,
            connection_id,
            name,
            source,
            samples: Vec::new(),
            presentation,
            sampling_interval,
            polling_state: SeriesPollingState::Enabled,
        }
    }
}

#[derive(Clone)]
pub struct SeriesMetadata {
    pub id: SeriesId,
    pub connection_id: ConnectionId,
    pub name: String,
    pub source: SeriesSource,
    pub presentation: SeriesPresentation,
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
            presentation: series.presentation.clone(),
            sampling_interval: series.sampling_interval,
            polling_state: series.polling_state,
        }
    }
}
