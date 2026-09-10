use std::{error::Error, fmt};

use crate::{
    process_control::{ControllerAccessError, ControllerRegistryError},
    signal_processing::{SignalProcessingGraphDefinitionError, SignalProcessingGraphUpdateError},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddSignalFilterError<SignalId> {
    Definition(SignalProcessingGraphDefinitionError<SignalId>),

    Disconnected,
}

impl<SignalId> fmt::Display for AddSignalFilterError<SignalId>
where
    SignalId: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Definition(error) => error.fmt(formatter),

            Self::Disconnected => formatter.write_str("Processing service is disconnected"),
        }
    }
}

impl<SignalId> Error for AddSignalFilterError<SignalId>
where
    SignalId: fmt::Debug + fmt::Display + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Definition(error) => Some(error),
            Self::Disconnected => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AddControlLoopError {
    Definition(ControllerRegistryError),

    Disconnected,
}

impl fmt::Display for AddControlLoopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Definition(error) => error.fmt(formatter),

            Self::Disconnected => formatter.write_str(
                "Processing \
                     service is disconnected",
            ),
        }
    }
}

impl Error for AddControlLoopError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Definition(error) => Some(error),

            Self::Disconnected => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AddControllerDiagnosticError<SignalId> {
    Controller(ControllerAccessError),
    DuplicateOutput { output: SignalId },
    Disconnected,
}

impl<SignalId> fmt::Display for AddControllerDiagnosticError<SignalId>
where
    SignalId: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Controller(error) => error.fmt(formatter),

            Self::DuplicateOutput { output } => {
                write!(
                    formatter,
                    "Processing output {output} \
                     is already registered",
                )
            }

            Self::Disconnected => formatter.write_str(
                "Processing service is \
                     disconnected",
            ),
        }
    }
}

impl<SignalId> Error for AddControllerDiagnosticError<SignalId>
where
    SignalId: fmt::Debug + fmt::Display + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Controller(error) => Some(error),

            Self::DuplicateOutput { .. } | Self::Disconnected => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControllerRequestError {
    Access(ControllerAccessError),

    Disconnected,
}

impl fmt::Display for ControllerRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Access(error) => error.fmt(formatter),

            Self::Disconnected => formatter.write_str(
                "Processing service is \
                     disconnected",
            ),
        }
    }
}

impl Error for ControllerRequestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Access(error) => Some(error),

            Self::Disconnected => None,
        }
    }
}

impl From<ControllerAccessError> for ControllerRequestError {
    fn from(error: ControllerAccessError) -> Self {
        Self::Access(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplaceSignalFilterError<SignalId> {
    Definition(SignalProcessingGraphUpdateError<SignalId>),

    Disconnected,
}

impl<SignalId> fmt::Display for ReplaceSignalFilterError<SignalId>
where
    SignalId: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Definition(error) => error.fmt(formatter),

            Self::Disconnected => formatter.write_str(
                "Processing service \
                     is disconnected",
            ),
        }
    }
}

impl<SignalId> Error for ReplaceSignalFilterError<SignalId>
where
    SignalId: fmt::Debug + fmt::Display + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Definition(error) => Some(error),
            Self::Disconnected => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessingServiceDisconnected;

impl fmt::Display for ProcessingServiceDisconnected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Processing service is disconnected")
    }
}

impl Error for ProcessingServiceDisconnected {}
