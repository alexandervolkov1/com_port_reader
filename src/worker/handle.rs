use std::{error::Error, fmt};

use crossbeam_channel::Sender;

use crate::{
    acquisition::{
        InstrumentReadResult, InstrumentWriteCompletion, InstrumentWriteCompletionId,
        InstrumentWriteResult, VirtualInstrumentDescribeResult,
    },
    connection::ConnectionId,
    instrument::{InstrumentReadRequest, InstrumentWriteRequest},
    process_recorder::ProcessActionId,
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

    pub fn start(&self, action_id: Option<ProcessActionId>) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Start { action_id })
    }

    pub fn stop(&self, action_id: Option<ProcessActionId>) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Stop { action_id })
    }

    pub(super) fn shutdown(&self) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Shutdown)
    }

    fn send(&self, command: WorkerCommand) -> Result<(), WorkerHandleError> {
        self.sender.send(command).map_err(|_| WorkerHandleError)
    }

    pub fn send_serial_text(
        &self,
        action_id: Option<ProcessActionId>,
        config: SerialPortConfig,
        command: String,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Connection(
            ConnectionCommand::SendSerialText {
                action_id,
                config,
                command,
            },
        ))
    }

    pub fn read_instrument(
        &self,
        action_id: Option<ProcessActionId>,
        port_name: String,
        request: InstrumentReadRequest,
        response_sender: Sender<InstrumentReadResult>,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Connection(
            ConnectionCommand::ReadInstrument {
                action_id,
                port_name,
                request,
                response_sender,
            },
        ))
    }

    pub(crate) fn write_instrument_quiet(
        &self,
        action_id: Option<ProcessActionId>,
        port_name: String,
        request: InstrumentWriteRequest,
        response_sender: Sender<InstrumentWriteResult>,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Connection(
            ConnectionCommand::WriteInstrument {
                action_id,
                port_name,
                request,
                emit_event: false,
                completion: None,
                response_sender,
            },
        ))
    }

    pub(crate) fn write_instrument_quiet_tracked(
        &self,
        action_id: Option<ProcessActionId>,
        port_name: String,
        request: InstrumentWriteRequest,
        completion_id: InstrumentWriteCompletionId,
        completion_sender: Sender<InstrumentWriteCompletion>,
        response_sender: Sender<InstrumentWriteResult>,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Connection(
            ConnectionCommand::WriteInstrument {
                action_id,
                port_name,
                request,
                emit_event: false,
                completion: Some((completion_id, completion_sender)),
                response_sender,
            },
        ))
    }

    pub fn describe_virtual_instruments(
        &self,
        action_id: Option<ProcessActionId>,
        response_sender: Sender<VirtualInstrumentDescribeResult>,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Connection(
            ConnectionCommand::DescribeVirtualInstruments {
                action_id,
                response_sender,
            },
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
    use crossbeam_channel::{bounded, unbounded};

    use super::WorkerHandle;

    use crate::{
        acquisition::InstrumentWriteCompletionId,
        connection::ConnectionId,
        instrument::{
            InstrumentValue, InstrumentWriteRequest,
            virtual_instrument::{VirtualInstrumentId, VirtualParameterId},
        },
        worker::{ConnectionCommand, WorkerCommand},
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
    fn attaches_completion_tracking_to_quiet_write() {
        let (sender, receiver) = unbounded();

        let handle = WorkerHandle::new(ConnectionId::new(7), sender);

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(3),
            VirtualParameterId::new(4),
            InstrumentValue::Number(42.0),
        );

        let (response_sender, _response_receiver) = bounded(1);

        let (completion_sender, _completion_receiver) = unbounded();

        let completion_id = InstrumentWriteCompletionId(17);

        handle
            .write_instrument_quiet_tracked(
                None,
                "COM9".to_owned(),
                request,
                completion_id,
                completion_sender,
                response_sender,
            )
            .unwrap();

        let command = receiver.try_recv().unwrap();

        let WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
            request: actual_request,
            emit_event,
            completion: Some((actual_completion_id, _)),
            ..
        }) = command
        else {
            panic!("expected tracked instrument write",);
        };

        assert_eq!(actual_request, request,);

        assert!(!emit_event);

        assert_eq!(actual_completion_id, completion_id,);
    }
}
