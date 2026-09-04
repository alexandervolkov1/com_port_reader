use std::{error::Error, fmt};

use crossbeam_channel::Sender;

use crate::{
    acquisition::{InstrumentReadResult, InstrumentWriteResult, VirtualInstrumentDescribeResult},
    connection::ConnectionId,
    data::{NewControllerDiagnosticSeries, NewFilteredSeries, NewSeries, SeriesColor},
    instrument::{
        InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest, ParameterDescriptor,
    },
    output_control::OutputRequestError,
    process_control::{
        ControlLoopState, ControlOutputTarget, ControllerDiagnostic, NewOnOffLoop, NewPidLoop,
        ReferenceKind, ReferenceSource,
    },
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

#[derive(Clone, Debug, PartialEq)]
pub enum PauseControllerError {
    Output(OutputRequestError),

    ControllerAfterSafeOutput(ControllerRequestError),
}

impl fmt::Display for PauseControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Output(error) => {
                write!(
                    formatter,
                    "Safe controller output \
                     failed: {error}",
                )
            }

            Self::ControllerAfterSafeOutput(error) => {
                write!(
                    formatter,
                    "Safe output was requested, \
                     but controller pause failed: \
                     {error}",
                )
            }
        }
    }
}

impl Error for PauseControllerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Output(error) => Some(error),

            Self::ControllerAfterSafeOutput(error) => Some(error),
        }
    }
}

impl From<OutputRequestError> for PauseControllerError {
    fn from(error: OutputRequestError) -> Self {
        Self::Output(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResumeControllerError {
    Controller(ControllerRequestError),
    Output(OutputRequestError),

    Rollback {
        output: OutputRequestError,
        rollback: ControllerRequestError,
    },
}

impl fmt::Display for ResumeControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Controller(error) => error.fmt(formatter),

            Self::Output(error) => {
                write!(
                    formatter,
                    "Automatic output takeover \
                     failed: {error}",
                )
            }

            Self::Rollback { output, rollback } => {
                write!(
                    formatter,
                    "Automatic output takeover \
                     failed: {output}; controller \
                     rollback also failed: \
                     {rollback}",
                )
            }
        }
    }
}

impl Error for ResumeControllerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Controller(error) => Some(error),

            Self::Output(error) => Some(error),

            Self::Rollback { output, .. } => Some(output),
        }
    }
}

impl From<ControllerRequestError> for ResumeControllerError {
    fn from(error: ControllerRequestError) -> Self {
        Self::Controller(error)
    }
}

impl From<OutputRequestError> for ResumeControllerError {
    fn from(error: OutputRequestError) -> Self {
        Self::Output(error)
    }
}

#[derive(Debug)]
pub enum UserCommand {
    Add(NewSeries),
    AddFilter(NewFilteredSeries),
    AddControllerDiagnostic(NewControllerDiagnosticSeries),
    AddPidLoop(NewPidLoop<ControlOutputTarget>),
    AddOnOffLoop(NewOnOffLoop<ControlOutputTarget>),

    ControllerParameters {
        name: String,
        response_sender: Sender<Result<Vec<ParameterDescriptor>, ControllerRequestError>>,
    },

    ControllerDiagnostics {
        name: String,
        response_sender: Sender<Result<Vec<ControllerDiagnostic>, ControllerRequestError>>,
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

    ControllerReferenceKind {
        name: String,
        response_sender: Sender<Result<Option<ReferenceKind>, ControllerRequestError>>,
    },

    ControllerReferenceParameters {
        name: String,
        response_sender: Sender<Result<Vec<ParameterDescriptor>, ControllerRequestError>>,
    },

    ReadControllerReferenceParameter {
        name: String,
        key: String,
        response_sender: Sender<Result<InstrumentValue, ControllerRequestError>>,
    },

    WriteControllerReferenceParameter {
        name: String,
        key: String,
        value: InstrumentValue,
        response_sender: Sender<Result<InstrumentValue, ControllerRequestError>>,
    },

    ConfigureControllerReference {
        name: String,
        updates: Vec<(String, InstrumentValue)>,
        response_sender: Sender<Result<(), ControllerRequestError>>,
    },

    SetControllerReference {
        name: String,
        source: ReferenceSource,
        response_sender: Sender<Result<(), ControllerRequestError>>,
    },

    SetControllerInput {
        name: String,
        input_name: String,
        response_sender: Sender<Result<(), SetControllerInputError>>,
    },

    ControllerState {
        name: String,
        response_sender: Sender<Result<ControlLoopState, ControllerRequestError>>,
    },

    PauseController {
        name: String,
        response_sender: Sender<Result<(), PauseControllerError>>,
    },

    ResumeController {
        name: String,
        response_sender: Sender<Result<(), ResumeControllerError>>,
    },

    ResetControllerIntegral {
        name: String,
        response_sender: Sender<Result<(), ControllerRequestError>>,
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
