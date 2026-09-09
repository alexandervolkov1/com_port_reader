use crate::{
    acquisition::AcquisitionError,
    connection::ConnectionId,
    output_control::{OutputHandle, OutputRequestError},
    process_recorder::{ProcessActionContext, ProcessRecorder},
    serial_connection::{SerialConnectionRegistry, SerialPortConfig},
    user_command::InstrumentCommand,
    worker::{ConnectionRouter, WorkerHandle},
};

pub(crate) struct InstrumentCommandHandler<'a> {
    connections: &'a ConnectionRouter,
    serial_connections: &'a SerialConnectionRegistry,
    output_control: &'a OutputHandle,
    process_recorder: &'a ProcessRecorder,
}

impl<'a> InstrumentCommandHandler<'a> {
    pub(crate) fn new(
        connections: &'a ConnectionRouter,
        serial_connections: &'a SerialConnectionRegistry,
        output_control: &'a OutputHandle,
        process_recorder: &'a ProcessRecorder,
    ) -> Self {
        Self {
            connections,
            serial_connections,
            output_control,
            process_recorder,
        }
    }

    pub(crate) fn execute(
        &self,
        command: InstrumentCommand,
        action_context: Option<ProcessActionContext>,
    ) {
        match command {
            InstrumentCommand::Read {
                connection_id,
                request,
                response_sender,
            } => {
                self.read(connection_id, request, response_sender, action_context);
            }

            InstrumentCommand::Write {
                connection_id,
                request,
                response_sender,
            } => {
                self.write(connection_id, request, response_sender, action_context);
            }

            InstrumentCommand::DescribeVirtualInstruments {
                connection_id,
                response_sender,
            } => {
                self.describe_virtual_instruments(connection_id, response_sender, action_context);
            }
        }
    }

    fn read(
        &self,
        connection_id: ConnectionId,
        request: crate::instrument::InstrumentReadRequest,
        response_sender: crossbeam_channel::Sender<crate::acquisition::InstrumentReadResult>,
        action_context: Option<ProcessActionContext>,
    ) {
        let action_id = action_context.map(|context| context.action_id());

        let config = match self.serial_config(connection_id) {
            Ok(config) => config,

            Err(error) => {
                let error_message = error.to_string();

                if let Some(action_id) = action_id {
                    self.process_recorder
                        .record_action_failed(action_id, error_message);
                }

                let _ = response_sender.send(Err(error));

                return;
            }
        };

        let worker_handle = match self.connection_worker(connection_id) {
            Ok(worker_handle) => worker_handle,

            Err(error) => {
                let error_message = error.to_string();

                if let Some(action_id) = action_id {
                    self.process_recorder
                        .record_action_failed(action_id, error_message);
                }

                let _ = response_sender.send(Err(error));

                return;
            }
        };

        let send_result = worker_handle.read_instrument(
            action_id,
            config.port_name().to_owned(),
            request,
            response_sender.clone(),
        );

        if let Err(send_error) = send_result {
            let error = AcquisitionError::from(format!(
                "Failed to request instrument \
                 read: {send_error}",
            ));

            let error_message = error.to_string();

            if let Some(action_id) = action_id {
                self.process_recorder
                    .record_action_failed(action_id, error_message);
            }

            let _ = response_sender.send(Err(error));
        }
    }

    fn write(
        &self,
        connection_id: ConnectionId,
        request: crate::instrument::InstrumentWriteRequest,
        response_sender: crossbeam_channel::Sender<crate::acquisition::InstrumentWriteResult>,
        action_context: Option<ProcessActionContext>,
    ) {
        let action_id = action_context.map(|context| context.action_id());

        if let Err(error) = self.output_control.write_instrument(
            action_id,
            connection_id,
            request,
            response_sender.clone(),
        ) {
            let error = Self::output_write_error(error);

            let error_message = error.to_string();

            if let Some(action_id) = action_id {
                self.process_recorder
                    .record_action_failed(action_id, error_message);
            }

            let _ = response_sender.send(Err(error));
        }
    }

    fn describe_virtual_instruments(
        &self,
        connection_id: ConnectionId,
        response_sender: crossbeam_channel::Sender<
            crate::acquisition::VirtualInstrumentDescribeResult,
        >,
        action_context: Option<ProcessActionContext>,
    ) {
        let action_id = action_context.map(|context| context.action_id());

        if let Err(error) = self.serial_config(connection_id) {
            let error_message = error.to_string();

            if let Some(action_id) = action_id {
                self.process_recorder
                    .record_action_failed(action_id, error_message);
            }

            let _ = response_sender.send(Err(error));

            return;
        }

        let worker_handle = match self.connection_worker(connection_id) {
            Ok(worker_handle) => worker_handle,

            Err(error) => {
                let error_message = error.to_string();

                if let Some(action_id) = action_id {
                    self.process_recorder
                        .record_action_failed(action_id, error_message);
                }

                let _ = response_sender.send(Err(error));

                return;
            }
        };

        let send_result =
            worker_handle.describe_virtual_instruments(action_id, response_sender.clone());

        if let Err(send_error) = send_result {
            let error = AcquisitionError::from(format!(
                "Failed to request virtual \
                 instrument discovery: {send_error}",
            ));

            let error_message = error.to_string();

            if let Some(action_id) = action_id {
                self.process_recorder
                    .record_action_failed(action_id, error_message);
            }

            let _ = response_sender.send(Err(error));
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

    fn output_write_error(error: OutputRequestError) -> AcquisitionError {
        AcquisitionError::from(format!(
            "Failed to request instrument \
             write: {error}",
        ))
    }
}
