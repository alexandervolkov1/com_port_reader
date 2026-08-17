use std::{error::Error, fmt, path::PathBuf};

use crossbeam_channel::Sender;

use crate::{
    acquisition::{InstrumentReadResult, InstrumentWriteResult, VirtualInstrumentDescribeResult},
    connection::ConnectionId,
    data::{NewSeries, SeriesId},
    instrument::{InstrumentReadRequest, InstrumentWriteRequest},
    serial_connection::SerialPortConfig,
};

use super::command::{ConnectionCommand, WorkerCommand};

#[derive(Clone)]
pub struct WorkerHandle {
    connection_id: ConnectionId,
    sender: Sender<WorkerCommand>,
}

impl WorkerHandle {
    pub(crate) fn new(connection_id: ConnectionId, sender: Sender<WorkerCommand>) -> Self {
        Self {
            connection_id,
            sender,
        }
    }

    pub(super) const fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }

    pub fn start(&self) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Start)
    }

    pub fn stop(&self) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Stop)
    }

    pub fn add_series(&self, new_series: NewSeries) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::AddSeries(new_series))
    }

    pub(super) fn shutdown(&self) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Shutdown)
    }

    fn send(&self, command: WorkerCommand) -> Result<(), WorkerHandleError> {
        self.sender.send(command).map_err(|_| WorkerHandleError)
    }

    pub fn rename_series(
        &self,
        current_name: String,
        new_name: String,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::RenameSeries {
            current_name,
            new_name,
        })
    }

    pub fn set_visibility(&self, id: SeriesId, visible: bool) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::SetVisibility { id, visible })
    }

    pub fn clear_series(&self) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::ClearSeries)
    }

    pub fn remove_series_by_name(&self, name: String) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::RemoveSeriesByName(name))
    }

    pub fn start_csv_recording(&self, path: PathBuf) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::StartCsvRecording(path))
    }

    pub fn stop_recording(&self) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::StopRecording)
    }

    pub fn send_serial_text(
        &self,
        config: SerialPortConfig,
        command: String,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Connection(
            ConnectionCommand::SendSerialText { config, command },
        ))
    }

    pub fn read_instrument(
        &self,
        port_name: String,
        request: InstrumentReadRequest,
        response_sender: Sender<InstrumentReadResult>,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Connection(
            ConnectionCommand::ReadInstrument {
                port_name,
                request,
                response_sender,
            },
        ))
    }

    pub fn write_instrument(
        &self,
        port_name: String,
        request: InstrumentWriteRequest,
        response_sender: Sender<InstrumentWriteResult>,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Connection(
            ConnectionCommand::WriteInstrument {
                port_name,
                request,
                response_sender,
            },
        ))
    }

    pub fn describe_virtual_instruments(
        &self,
        response_sender: Sender<VirtualInstrumentDescribeResult>,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Connection(
            ConnectionCommand::DescribeVirtualInstruments { response_sender },
        ))
    }

    pub fn refresh_series_schedule(&self) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::RefreshSeriesSchedule)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerHandleError;

impl fmt::Display for WorkerHandleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("worker command channel is disconnected")
    }
}

impl Error for WorkerHandleError {}

#[cfg(test)]
mod tests {
    use crossbeam_channel::unbounded;

    use super::WorkerHandle;
    use crate::connection::ConnectionId;

    #[test]
    fn stores_connection_id() {
        let (sender, _receiver) = unbounded();

        let connection_id = ConnectionId::new(7);

        let handle = WorkerHandle::new(connection_id, sender);

        assert_eq!(handle.connection_id(), connection_id,);
    }
}
