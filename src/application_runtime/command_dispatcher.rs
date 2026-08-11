use crossbeam_channel::Receiver;

use crate::{
    acquisition::AcquisitionError,
    app_log::LogHandle,
    application_definition::ApplicationDefinition,
    connection::ConnectionId,
    data::{NewSeries, SeriesId},
    serial_connection::{SerialConnectionRegistry, SerialPortConfig},
    user_command::UserCommand,
    worker::{
        ConnectionRouter, ConnectionWorkerEvent, WorkerEvent, WorkerHandle, WorkerHandleError,
    },
};

use super::{
    acquisition_controller::AcquisitionController, device_emulator_service::DeviceEmulatorService,
};

pub(crate) struct CommandDispatcher {
    connections: ConnectionRouter,
    serial_connections: SerialConnectionRegistry,
    application_definition: ApplicationDefinition,
    event_receiver: Receiver<ConnectionWorkerEvent>,
    log: LogHandle,
}

impl CommandDispatcher {
    pub fn new(
        connections: ConnectionRouter,
        serial_connections: SerialConnectionRegistry,
        application_definition: ApplicationDefinition,
        event_receiver: Receiver<ConnectionWorkerEvent>,
        log: LogHandle,
    ) -> Self {
        Self {
            connections,
            serial_connections,
            application_definition,
            event_receiver,
            log,
        }
    }

    fn connection_worker(
        &self,
        connection_id: ConnectionId,
    ) -> Result<WorkerHandle, AcquisitionError> {
        self.connections.handle(connection_id).ok_or_else(|| {
            AcquisitionError::from(format!(
                "Connection worker {connection_id:?} is not registered",
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

    fn emulator_connection_id(&self) -> ConnectionId {
        self.application_definition
            .emulator()
            .map_or(ConnectionId::PRIMARY, |emulator| emulator.connection_id())
    }

    fn emulator_serial_config(&self) -> Result<SerialPortConfig, AcquisitionError> {
        self.serial_config(self.emulator_connection_id())
    }

    fn primary_worker(&self) -> WorkerHandle {
        self.connections
            .handle(ConnectionId::PRIMARY)
            .expect("primary connection worker must be registered")
    }

    fn format_worker_event(&self, connection_event: &ConnectionWorkerEvent) -> String {
        let connection_id = connection_event.connection_id();

        let event = connection_event.event();

        match self
            .application_definition
            .connection_name_by_id(connection_id)
        {
            Some(connection_name) => {
                format!(
                    "Connection '{connection_name}': \
                     {event}",
                )
            }

            None => connection_event.to_string(),
        }
    }

    pub fn poll_events(&mut self, controls: &mut AcquisitionController) {
        while let Ok(connection_event) = self.event_receiver.try_recv() {
            let event = connection_event.event();

            controls.handle_worker_event(event);

            let message = self.format_worker_event(&connection_event);

            if worker_event_is_error(event) {
                self.log.error(message);
            } else {
                self.log.info(message);
            }
        }
    }

    pub fn execute(
        &self,
        command: UserCommand,
        controls: &mut AcquisitionController,
        device_emulator: &mut DeviceEmulatorService,
    ) {
        match command {
            UserCommand::Add(new_series) => {
                self.add_series(new_series);
            }

            UserCommand::Delete { name } => {
                if let Err(error) = self.primary_worker().remove_series_by_name(name) {
                    self.set_worker_error(error);
                }
            }

            UserCommand::Rename {
                current_name,
                new_name,
            } => {
                if let Err(error) = self.primary_worker().rename_series(current_name, new_name) {
                    self.set_worker_error(error);
                }
            }

            UserCommand::Start => {
                controls.start();
            }

            UserCommand::Stop => {
                controls.stop();
            }

            UserCommand::Clear => {
                controls.clear();
            }

            UserCommand::StartRecording => {
                controls.start_recording();
            }

            UserCommand::StopRecording => {
                controls.stop_recording();
            }

            UserCommand::StartEmulator => {
                let serial_config = match self.emulator_serial_config() {
                    Ok(serial_config) => serial_config,

                    Err(error) => {
                        self.log.error(format!(
                            "Cannot start emulator: \
                                     {error}",
                        ));

                        return;
                    }
                };

                device_emulator.start(&serial_config);
            }

            UserCommand::StopEmulator => {
                device_emulator.stop();
            }

            UserCommand::Log { message } => {
                self.log.info(message);
            }

            UserCommand::SendSerial {
                connection_id,
                command,
            } => {
                let config = match self.serial_config(connection_id) {
                    Ok(config) => config,

                    Err(error) => {
                        self.log.error(error.to_string());

                        return;
                    }
                };

                let worker_handle = match self.connection_worker(connection_id) {
                    Ok(worker_handle) => worker_handle,

                    Err(error) => {
                        self.log.error(error.to_string());

                        return;
                    }
                };

                if let Err(error) = worker_handle.send_serial_text(config, command) {
                    self.set_worker_error(error);
                }
            }

            UserCommand::ReadInstrument {
                connection_id,
                request,
                response_sender,
            } => {
                let config = match self.serial_config(connection_id) {
                    Ok(config) => config,

                    Err(error) => {
                        self.log.error(error.to_string());

                        let _ = response_sender.send(Err(error));

                        return;
                    }
                };

                let worker_handle = match self.connection_worker(connection_id) {
                    Ok(worker_handle) => worker_handle,

                    Err(error) => {
                        self.log.error(error.to_string());

                        let _ = response_sender.send(Err(error));

                        return;
                    }
                };

                let send_result = worker_handle.read_instrument(
                    config.port_name().to_owned(),
                    request,
                    response_sender.clone(),
                );

                if let Err(send_error) = send_result {
                    let error = AcquisitionError::from(format!(
                        "Failed to request instrument \
                             read: {send_error}",
                    ));

                    self.log.error(error.to_string());

                    let _ = response_sender.send(Err(error));
                }
            }

            UserCommand::WriteInstrument {
                connection_id,
                request,
                response_sender,
            } => {
                let config = match self.serial_config(connection_id) {
                    Ok(config) => config,

                    Err(error) => {
                        self.log.error(error.to_string());

                        let _ = response_sender.send(Err(error));

                        return;
                    }
                };

                let worker_handle = match self.connection_worker(connection_id) {
                    Ok(worker_handle) => worker_handle,

                    Err(error) => {
                        self.log.error(error.to_string());

                        let _ = response_sender.send(Err(error));

                        return;
                    }
                };

                let send_result = worker_handle.write_instrument(
                    config.port_name().to_owned(),
                    request,
                    response_sender.clone(),
                );

                if let Err(send_error) = send_result {
                    let error = AcquisitionError::from(format!(
                        "Failed to request instrument \
                             write: {send_error}",
                    ));

                    self.log.error(error.to_string());

                    let _ = response_sender.send(Err(error));
                }
            }

            UserCommand::DescribeVirtualInstruments {
                connection_id,
                response_sender,
            } => {
                if let Err(error) = self.serial_config(connection_id) {
                    self.log.error(error.to_string());

                    let _ = response_sender.send(Err(error));

                    return;
                }

                let worker_handle = match self.connection_worker(connection_id) {
                    Ok(worker_handle) => worker_handle,

                    Err(error) => {
                        self.log.error(error.to_string());

                        let _ = response_sender.send(Err(error));

                        return;
                    }
                };

                let send_result =
                    worker_handle.describe_virtual_instruments(response_sender.clone());

                if let Err(send_error) = send_result {
                    let error = AcquisitionError::from(format!(
                        "Failed to request virtual \
                             instrument discovery: \
                             {send_error}",
                    ));

                    self.log.error(error.to_string());

                    let _ = response_sender.send(Err(error));
                }
            }
        }
    }

    pub fn set_visibility(&self, id: SeriesId, visible: bool) {
        if let Err(error) = self.primary_worker().set_visibility(id, visible) {
            self.set_worker_error(error);
        }
    }

    pub fn remove_series(&self, id: SeriesId) {
        if let Err(error) = self.primary_worker().remove_series(id) {
            self.set_worker_error(error);
        }
    }

    pub fn add_series(&self, new_series: NewSeries) {
        if let Err(error) = self.primary_worker().add_series(new_series) {
            self.set_worker_error(error);
        }
    }

    fn set_worker_error(&self, error: WorkerHandleError) {
        self.log.error(format!("Failed to send command: {error}",));
    }
}

fn worker_event_is_error(event: &WorkerEvent) -> bool {
    matches!(
        event,
        WorkerEvent::SeriesAddFailed(_)
            | WorkerEvent::AcquisitionStartFailed(_)
            | WorkerEvent::AcquisitionFailed(_)
            | WorkerEvent::AcquisitionStopFailed(_)
            | WorkerEvent::SeriesNotFound(_)
            | WorkerEvent::SeriesRenameFailed(_)
            | WorkerEvent::SampleSinkFailed(_)
            | WorkerEvent::SerialTextCommandFailed { .. }
            | WorkerEvent::InstrumentReadFailed { .. }
            | WorkerEvent::InstrumentWriteFailed { .. }
            | WorkerEvent::SeriesPollingFailed { .. }
    )
}
