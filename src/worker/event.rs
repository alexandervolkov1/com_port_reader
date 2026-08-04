use std::path::PathBuf;

use crate::instrument::{InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest};
use crate::serial_connection::SerialConnectionError;
use crate::{
    acquisition::AcquisitionError,
    data::{AddSeriesError, RenameSeriesError, SeriesId},
    sample_sink::SampleSinkError,
};

#[derive(Clone, Debug, PartialEq)]
pub enum WorkerEvent {
    AcquisitionStarted,
    AcquisitionStopped,
    RecordingStarted(PathBuf),
    RecordingStopped,
    SeriesCleared,
    SeriesAdded(SeriesId),
    SeriesAddFailed(AddSeriesError),
    AcquisitionStartFailed(AcquisitionError),
    AcquisitionFailed(AcquisitionError),
    AcquisitionStopFailed(AcquisitionError),
    SeriesRemoved(SeriesId),
    SeriesNotFound(String),
    SeriesRenamed {
        id: SeriesId,
        name: String,
    },
    SeriesRenameFailed(RenameSeriesError),
    SampleSinkFailed(SampleSinkError),
    SerialPortTestSucceeded(String),
    SerialPortTestFailed {
        port_name: String,
        error: SerialConnectionError,
    },

    SerialTextCommandSucceeded {
        port_name: String,
        command: String,
        response: String,
    },

    SerialTextCommandFailed {
        port_name: String,
        command: String,
        error: SerialConnectionError,
    },

    InstrumentReadSucceeded {
        port_name: String,
        request: InstrumentReadRequest,
        value: InstrumentValue,
    },

    InstrumentReadFailed {
        port_name: String,
        request: InstrumentReadRequest,
        error: AcquisitionError,
    },

    InstrumentWriteSucceeded {
        port_name: String,
        request: InstrumentWriteRequest,
        actual_value: InstrumentValue,
    },

    InstrumentWriteFailed {
        port_name: String,
        request: InstrumentWriteRequest,
        error: AcquisitionError,
    },

    SeriesPollingFailed {
        id: SeriesId,
        name: String,
        error: AcquisitionError,
        consecutive_failures: u64,
    },

    SeriesPollingRecovered {
        id: SeriesId,
        name: String,
        failed_attempts: u64,
    },
}

impl std::fmt::Display for WorkerEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AcquisitionStarted => formatter.write_str("Acquisition started."),

            Self::AcquisitionStopped => formatter.write_str("Acquisition stopped."),

            Self::RecordingStarted(path) => {
                write!(formatter, "CSV recording started: '{}'.", path.display(),)
            }

            Self::RecordingStopped => formatter.write_str("CSV recording stopped."),

            Self::SeriesCleared => formatter.write_str("All series cleared."),
            Self::SeriesAdded(id) => {
                write!(formatter, "Series {id} added.")
            }

            Self::SeriesAddFailed(error) => {
                write!(formatter, "Failed to add series: {error}")
            }

            Self::AcquisitionStartFailed(error) => {
                write!(formatter, "Failed to start acquisition: {error}")
            }

            Self::AcquisitionFailed(error) => {
                write!(formatter, "Acquisition stopped: {error}")
            }

            Self::AcquisitionStopFailed(error) => {
                write!(formatter, "Failed to stop acquisition: {error}")
            }

            Self::SeriesRemoved(id) => {
                write!(formatter, "Series {id} removed.")
            }

            Self::SeriesNotFound(name) => {
                write!(formatter, "Series '{name}' not found.")
            }

            Self::SeriesRenamed { id, name } => {
                write!(formatter, "Series {id} renamed to '{name}'.")
            }

            Self::SeriesRenameFailed(error) => {
                write!(formatter, "Failed to rename series: {error}")
            }

            Self::SampleSinkFailed(error) => {
                write!(formatter, "Sample output failed: {error}")
            }

            Self::SerialPortTestSucceeded(port_name) => {
                write!(formatter, "COM port '{port_name}' opened successfully.",)
            }

            Self::SerialPortTestFailed { port_name, error } => {
                write!(
                    formatter,
                    "Failed to open COM port '{port_name}': \
                     {error}",
                )
            }

            Self::SerialTextCommandSucceeded {
                port_name,
                command,
                response,
            } => {
                write!(
                    formatter,
                    "COM port '{port_name}': command \
                     '{command}' returned: {response}",
                )
            }

            Self::SerialTextCommandFailed {
                port_name,
                command,
                error,
            } => {
                write!(
                    formatter,
                    "COM port '{port_name}': command \
                     '{command}' failed: {error}",
                )
            }

            Self::InstrumentReadSucceeded {
                port_name,
                request,
                value,
            } => {
                write!(
                    formatter,
                    "COM port '{port_name}': {request} returned \
                     {value}.",
                )
            }

            Self::InstrumentReadFailed {
                port_name,
                request,
                error,
            } => {
                write!(
                    formatter,
                    "COM port '{port_name}': failed to read \
                     {request}: {error}",
                )
            }

            Self::InstrumentWriteSucceeded {
                port_name,
                request,
                actual_value,
            } => {
                write!(
                    formatter,
                    "COM port '{port_name}': wrote {request}; \
                     actual value: {actual_value}.",
                )
            }

            Self::InstrumentWriteFailed {
                port_name,
                request,
                error,
            } => {
                write!(
                    formatter,
                    "COM port '{port_name}': failed to write \
                     {request}: {error}",
                )
            }

            Self::SeriesPollingFailed {
                id,
                name,
                error,
                consecutive_failures,
            } => {
                if *consecutive_failures == 1 {
                    write!(
                        formatter,
                        "Series '{name}' ({id}) polling \
                         failed: {error}. Acquisition \
                         continues.",
                    )
                } else {
                    write!(
                        formatter,
                        "Series '{name}' ({id}) polling \
                         still fails after \
                         {consecutive_failures} consecutive \
                         attempts: {error}",
                    )
                }
            }

            Self::SeriesPollingRecovered {
                id,
                name,
                failed_attempts,
            } => {
                write!(
                    formatter,
                    "Series '{name}' ({id}) polling \
                     recovered after {failed_attempts} \
                     failed attempts.",
                )
            }
        }
    }
}
