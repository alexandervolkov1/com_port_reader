use crate::instrument::{InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest};
use crate::serial_connection::SerialConnectionError;
use crate::{
    acquisition::AcquisitionError,
    connection::ConnectionId,
    data::{AddSeriesError, SeriesId},
    signal_processing::SignalFilterDefinition,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionWorkerEvent {
    connection_id: ConnectionId,
    event: WorkerEvent,
}

impl ConnectionWorkerEvent {
    pub(crate) const fn new(connection_id: ConnectionId, event: WorkerEvent) -> Self {
        Self {
            connection_id,
            event,
        }
    }

    pub const fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }

    pub const fn event(&self) -> &WorkerEvent {
        &self.event
    }
}

impl std::fmt::Display for ConnectionWorkerEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Connection {}: {}",
            self.connection_id, self.event,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorkerEvent {
    AcquisitionStarted,
    AcquisitionStopped,
    SeriesCleared,
    SeriesAdded(SeriesId),
    SeriesFilterChanged {
        id: SeriesId,
        name: String,
        definition: SignalFilterDefinition,
    },
    PidLoopAdded {
        name: String,
        input_id: SeriesId,
        input_name: String,
    },
    PidLoopSetpointChanged {
        name: String,
        setpoint: f64,
    },

    PidLoopSetpointChangeFailed {
        name: String,
        error: String,
    },
    PidLoopAddFailed {
        name: String,
        error: String,
    },
    SeriesAddFailed(AddSeriesError),
    AcquisitionStartFailed(AcquisitionError),
    AcquisitionFailed(AcquisitionError),
    AcquisitionStopFailed(AcquisitionError),
    SignalProcessingFailed(String),
    SeriesRemoved(SeriesId),
    SeriesNotFound(String),

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

    SeriesPollingSuspended {
        id: SeriesId,
        name: String,
        error: AcquisitionError,
    },

    SeriesPollingResumed {
        id: SeriesId,
        name: String,
    },
}

impl std::fmt::Display for WorkerEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AcquisitionStarted => formatter.write_str("Acquisition started."),

            Self::AcquisitionStopped => formatter.write_str("Acquisition stopped."),

            Self::SeriesCleared => formatter.write_str("All series cleared."),
            Self::SeriesAdded(id) => {
                write!(formatter, "Series {id} added.")
            }

            Self::SeriesFilterChanged {
                id,
                name,
                definition,
            } => {
                write!(
                    formatter,
                    "Filter for series '{name}' ({id}) changed to \
                     {definition}.",
                )
            }

            Self::PidLoopAdded {
                name,
                input_id,
                input_name,
            } => {
                write!(
                    formatter,
                    "PID loop '{name}' added for \
                     input series '{input_name}' \
                     ({input_id}).",
                )
            }

            Self::PidLoopAddFailed { name, error } => {
                write!(
                    formatter,
                    "Failed to add PID loop \
                     '{name}': {error}",
                )
            }

            Self::PidLoopSetpointChanged { name, setpoint } => {
                write!(
                    formatter,
                    "PID loop '{name}' setpoint changed to {setpoint}.",
                )
            }

            Self::PidLoopSetpointChangeFailed { name, error } => {
                write!(
                    formatter,
                    "Failed to change PID loop \
                     '{name}' setpoint: {error}",
                )
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

            Self::SignalProcessingFailed(error) => {
                write!(formatter, "Signal processing failed: {error}",)
            }

            Self::SeriesRemoved(id) => {
                write!(formatter, "Series {id} removed.")
            }

            Self::SeriesNotFound(name) => {
                write!(formatter, "Series '{name}' not found.")
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

            Self::SeriesPollingSuspended { id, name, error } => {
                write!(
                    formatter,
                    "Series '{name}' ({id}) polling was \
                     suspended after three consecutive polling \
                     failures: {error}. Use app.retry(\"{name}\"), \
                     refresh the instrument, or restart \
                     acquisition to retry.",
                )
            }

            Self::SeriesPollingResumed { id, name } => {
                write!(
                    formatter,
                    "Series '{name}' ({id}) polling resumed \
                     after successful manual instrument access.",
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConnectionWorkerEvent, WorkerEvent};

    use crate::connection::ConnectionId;

    #[test]
    fn attaches_connection_to_worker_event() {
        let event =
            ConnectionWorkerEvent::new(ConnectionId::new(2), WorkerEvent::AcquisitionStarted);

        assert_eq!(event.connection_id(), ConnectionId::new(2),);

        assert_eq!(event.event(), &WorkerEvent::AcquisitionStarted,);

        assert_eq!(event.to_string(), "Connection 2: Acquisition started.",);
    }
}
