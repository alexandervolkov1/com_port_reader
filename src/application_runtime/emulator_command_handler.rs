use super::device_emulator_service::DeviceEmulatorService;
use crate::{
    acquisition::AcquisitionError,
    application_definition::ApplicationDefinition,
    connection::ConnectionId,
    process_recorder::{ProcessActionContext, ProcessRecorder},
    serial_connection::{SerialConnectionRegistry, SerialPortConfig},
    user_command::EmulatorCommand,
};

pub(crate) struct EmulatorCommandHandler<'a> {
    application_definition: &'a ApplicationDefinition,
    serial_connections: &'a SerialConnectionRegistry,
    process_recorder: &'a ProcessRecorder,
    device_emulator: &'a mut DeviceEmulatorService,
}

impl<'a> EmulatorCommandHandler<'a> {
    pub(crate) fn new(
        application_definition: &'a ApplicationDefinition,
        serial_connections: &'a SerialConnectionRegistry,
        process_recorder: &'a ProcessRecorder,
        device_emulator: &'a mut DeviceEmulatorService,
    ) -> Self {
        Self {
            application_definition,
            serial_connections,
            process_recorder,
            device_emulator,
        }
    }

    pub(crate) fn execute(
        &mut self,
        command: EmulatorCommand,
        action_context: Option<ProcessActionContext>,
    ) {
        match command {
            EmulatorCommand::Start => {
                self.start(action_context);
            }

            EmulatorCommand::Stop => {
                self.stop(action_context);
            }
        }
    }

    fn start(&mut self, action_context: Option<ProcessActionContext>) {
        let result = (|| {
            let serial_config = self.emulator_serial_config().map_err(|error| {
                format!(
                    "Cannot start emulator: \
                             {error}",
                )
            })?;

            self.device_emulator.start(&serial_config).map_err(|error| {
                format!(
                    "Cannot start emulator: \
                         {error}",
                )
            })
        })();

        match result {
            Ok(()) => {
                if let Some(action_context) = action_context {
                    self.process_recorder.record_action_applied(
                        action_context.action_id(),
                        None,
                        None,
                    );
                }
            }

            Err(error) => {
                if let Some(action_context) = action_context {
                    self.process_recorder
                        .record_action_failed(action_context.action_id(), error);
                }
            }
        }
    }

    fn stop(&mut self, action_context: Option<ProcessActionContext>) {
        self.device_emulator.stop();

        if let Some(action_context) = action_context {
            self.process_recorder
                .record_action_applied(action_context.action_id(), None, None);
        }
    }

    fn emulator_connection_id(&self) -> ConnectionId {
        self.application_definition
            .emulator()
            .map_or(ConnectionId::PRIMARY, |emulator| emulator.connection_id())
    }

    fn emulator_serial_config(&self) -> Result<SerialPortConfig, AcquisitionError> {
        let connection_id = self.emulator_connection_id();

        let store = self
            .serial_connections
            .store(connection_id)
            .ok_or_else(|| {
                AcquisitionError::from(format!(
                    "Serial connection {connection_id} \
                     is not registered",
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
