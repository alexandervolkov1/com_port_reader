use crate::{
    acquisition::AcquisitionError,
    connection::ConnectionId,
    process_recorder::{ProcessActionContext, ProcessRecorder},
    serial_connection::{SerialConnectionRegistry, SerialPortConfig},
    user_command::SerialCommand,
    worker::{ConnectionRouter, WorkerHandle},
};

pub(crate) struct SerialCommandHandler<'a> {
    connections: &'a ConnectionRouter,
    serial_connections: &'a SerialConnectionRegistry,
    process_recorder: &'a ProcessRecorder,
}

impl<'a> SerialCommandHandler<'a> {
    pub(crate) fn new(
        connections: &'a ConnectionRouter,
        serial_connections: &'a SerialConnectionRegistry,
        process_recorder: &'a ProcessRecorder,
    ) -> Self {
        Self {
            connections,
            serial_connections,
            process_recorder,
        }
    }

    pub(crate) fn execute(
        &self,
        command: SerialCommand,
        action_context: Option<ProcessActionContext>,
    ) {
        match command {
            SerialCommand::SendText {
                connection_id,
                command,
            } => {
                self.send_text(connection_id, command, action_context);
            }
        }
    }

    fn send_text(
        &self,
        connection_id: ConnectionId,
        command: String,
        action_context: Option<ProcessActionContext>,
    ) {
        let action_id = action_context.map(|context| context.action_id());

        let config = match self.serial_config(connection_id) {
            Ok(config) => config,

            Err(error) => {
                if let Some(action_id) = action_id {
                    self.process_recorder
                        .record_action_failed(action_id, error.to_string());
                }

                return;
            }
        };

        let worker_handle = match self.connection_worker(connection_id) {
            Ok(worker_handle) => worker_handle,

            Err(error) => {
                if let Some(action_id) = action_id {
                    self.process_recorder
                        .record_action_failed(action_id, error.to_string());
                }

                return;
            }
        };

        if let Err(error) = worker_handle.send_serial_text(action_id, config, command) {
            let error = format!("Failed to send serial command: {error}",);

            if let Some(action_id) = action_id {
                self.process_recorder.record_action_failed(action_id, error);
            }
        }
    }

    fn connection_worker(
        &self,
        connection_id: ConnectionId,
    ) -> Result<WorkerHandle, AcquisitionError> {
        self.connections.handle(connection_id).ok_or_else(|| {
            AcquisitionError::from(format!(
                "Connection worker \
                     {connection_id:?} is not \
                     registered",
            ))
        })
    }

    fn serial_config(
        &self,
        connection_id: ConnectionId,
    ) -> Result<SerialPortConfig, AcquisitionError> {
        let store = self
            .serial_connections
            .store(connection_id)
            .ok_or_else(|| {
                AcquisitionError::from(format!(
                    "Serial connection \
                     {connection_id} is not \
                     registered",
                ))
            })?;

        store.snapshot().ok_or_else(|| {
            AcquisitionError::from(format!(
                "Serial connection {connection_id} \
                 has no configured COM port",
            ))
        })
    }
}
