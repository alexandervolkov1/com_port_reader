use std::{error::Error, fmt};

use crossbeam_channel::Sender;

use crate::{
    acquisition::{InstrumentReadResult, InstrumentWriteResult, VirtualInstrumentDescribeResult},
    connection::ConnectionId,
    data::{NewFilteredSeries, NewSeries, SeriesColor, SeriesId},
    instrument::{InstrumentReadRequest, InstrumentWriteRequest},
    process_control::{ControlOutputTarget, NewPidLoop},
    serial_connection::SerialPortConfig,
    signal_processing::SignalFilterDefinition,
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

    pub fn add_filter(&self, filter: NewFilteredSeries) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::AddFilter(filter))
    }

    pub fn add_pid_loop(
        &self,
        pid_loop: NewPidLoop<ControlOutputTarget>,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::AddPidLoop(pid_loop))
    }

    pub fn set_filter(
        &self,
        name: String,
        definition: SignalFilterDefinition,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::SetFilter { name, definition })
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

    pub fn set_series_color(
        &self,
        name: String,
        color: Option<SeriesColor>,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::SetSeriesColor { name, color })
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

    use crate::{
        connection::ConnectionId,
        instrument::metakon_5x3::{Metakon5x3, Metakon5x3Register},
        process_control::{ControlOutputTarget, NewPidLoop, PidGains, PidOutputLimits},
        worker::WorkerCommand,
    };

    #[test]
    fn stores_connection_id() {
        let (sender, _receiver) = unbounded();

        let connection_id = ConnectionId::new(7);

        let handle = WorkerHandle::new(connection_id, sender);

        assert_eq!(handle.connection_id(), connection_id,);
    }

    #[test]
    fn requests_series_schedule_refresh() {
        let (sender, receiver) = unbounded();

        let handle = WorkerHandle::new(ConnectionId::PRIMARY, sender);

        handle.refresh_series_schedule().unwrap();

        assert!(matches!(
            receiver.try_recv().unwrap(),
            WorkerCommand::RefreshSeriesSchedule,
        ));
    }

    #[test]
    fn requests_pid_loop_registration() {
        let (sender, receiver) = unbounded();

        let handle = WorkerHandle::new(ConnectionId::PRIMARY, sender);

        let target = ControlOutputTarget::metakon_5x3(
            ConnectionId::PRIMARY,
            Metakon5x3::new(3, 0),
            Metakon5x3Register::OutputPower,
            1.0,
        )
        .unwrap();

        let pid_loop = NewPidLoop::new(
            "heater",
            "temperature",
            target,
            200.0,
            PidGains::new(2.0, 0.1, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap();

        handle.add_pid_loop(pid_loop).unwrap();

        let WorkerCommand::AddPidLoop(pid_loop) = receiver.try_recv().unwrap() else {
            panic!("expected AddPidLoop command",);
        };

        assert_eq!(pid_loop.name(), "heater");

        assert_eq!(pid_loop.input_name(), "temperature",);
    }
}
