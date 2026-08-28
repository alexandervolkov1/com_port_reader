use std::{error::Error, fmt};

use crossbeam_channel::Sender;

use crate::{
    acquisition::{InstrumentReadResult, InstrumentWriteResult, VirtualInstrumentDescribeResult},
    connection::ConnectionId,
    data::{NewControllerDiagnosticSeries, NewFilteredSeries, NewSeries, SeriesColor},
    instrument::{
        InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest, ParameterDescriptor,
    },
    process_control::{ControlOutputTarget, NewPidLoop},
    signal_processing::{ControllerRequestError, SignalFilterDefinition},
};

#[derive(Clone, Debug, PartialEq)]
pub enum SetControllerInputError {
    SeriesNotFound(String),
    Controller(ControllerRequestError),
}

impl fmt::Display for SetControllerInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SeriesNotFound(name) => {
                write!(formatter, "Controller input series '{name}' was not found",)
            }

            Self::Controller(error) => error.fmt(formatter),
        }
    }
}

impl Error for SetControllerInputError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SeriesNotFound(_) => None,
            Self::Controller(error) => Some(error),
        }
    }
}

impl From<ControllerRequestError> for SetControllerInputError {
    fn from(error: ControllerRequestError) -> Self {
        Self::Controller(error)
    }
}

#[derive(Debug)]
pub enum UserCommand {
    Add(NewSeries),
    AddFilter(NewFilteredSeries),
    AddControllerDiagnostic(NewControllerDiagnosticSeries),
    AddPidLoop(NewPidLoop<ControlOutputTarget>),

    ControllerParameters {
        name: String,
        response_sender: Sender<Result<Vec<ParameterDescriptor>, ControllerRequestError>>,
    },

    ReadControllerParameter {
        name: String,
        key: String,
        response_sender: Sender<Result<InstrumentValue, ControllerRequestError>>,
    },

    WriteControllerParameter {
        name: String,
        key: String,
        value: InstrumentValue,
        response_sender: Sender<Result<InstrumentValue, ControllerRequestError>>,
    },

    ConfigureController {
        name: String,
        updates: Vec<(String, InstrumentValue)>,
        response_sender: Sender<Result<(), ControllerRequestError>>,
    },

    SetControllerInput {
        name: String,
        input_name: String,
        response_sender: Sender<Result<(), SetControllerInputError>>,
    },

    ResetController {
        name: String,
        response_sender: Sender<Result<(), ControllerRequestError>>,
    },

    SetFilter {
        name: String,
        definition: SignalFilterDefinition,
    },

    Delete {
        name: String,
    },

    Rename {
        current_name: String,
        new_name: String,
    },

    SetSeriesColor {
        name: String,
        color: Option<SeriesColor>,
    },

    Retry {
        name: String,
    },

    RetryAll,

    Start,
    Stop,
    Clear,

    StartEmulator,
    StopEmulator,

    Log {
        message: String,
    },

    SendSerial {
        connection_id: ConnectionId,
        command: String,
    },

    ReadInstrument {
        connection_id: ConnectionId,
        request: InstrumentReadRequest,
        response_sender: Sender<InstrumentReadResult>,
    },

    WriteInstrument {
        connection_id: ConnectionId,
        request: InstrumentWriteRequest,
        response_sender: Sender<InstrumentWriteResult>,
    },

    DescribeVirtualInstruments {
        connection_id: ConnectionId,
        response_sender: Sender<VirtualInstrumentDescribeResult>,
    },
}

#[cfg(test)]
mod tests {
    use super::SetControllerInputError;

    #[test]
    fn describes_missing_controller_input_series() {
        assert_eq!(
            SetControllerInputError::SeriesNotFound("temperature_filtered".to_owned(),).to_string(),
            "Controller input series \
             'temperature_filtered' was not found",
        );
    }
}
