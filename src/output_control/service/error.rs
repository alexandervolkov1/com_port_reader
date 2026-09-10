use std::{error::Error, fmt};

use crate::{
    acquisition::AcquisitionError, instrument::InstrumentParameterAddress,
    output_control::OutputArbiterError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OutputWriteError {
    Instrument(AcquisitionError),
    Disconnected,
}

impl fmt::Display for OutputWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Instrument(error) => error.fmt(formatter),
            Self::Disconnected => {
                formatter.write_str("instrument write response channel is disconnected")
            }
        }
    }
}

impl Error for OutputWriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Instrument(error) => Some(error),
            Self::Disconnected => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OutputRequestError {
    Arbiter(OutputArbiterError),
    RequestTargetMismatch {
        expected: InstrumentParameterAddress,
        actual: InstrumentParameterAddress,
    },
    Transport(String),
    Disconnected,
}

impl fmt::Display for OutputRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arbiter(error) => error.fmt(formatter),

            Self::RequestTargetMismatch { expected, actual } => {
                write!(
                    formatter,
                    "Output request targets {actual:?}, but output ownership belongs to {expected:?}",
                )
            }

            Self::Transport(message) => formatter.write_str(message),

            Self::Disconnected => formatter.write_str("Output control service is disconnected"),
        }
    }
}

impl Error for OutputRequestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Arbiter(error) => Some(error),
            Self::RequestTargetMismatch { .. } | Self::Transport(_) | Self::Disconnected => None,
        }
    }
}

impl From<OutputArbiterError> for OutputRequestError {
    fn from(error: OutputArbiterError) -> Self {
        Self::Arbiter(error)
    }
}
