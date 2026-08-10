use crossbeam_channel::Receiver;

use crate::{
    acquisition::AcquisitionError,
    app_log::LogHandle,
    components::{
        controls_model::ControlsModel, device_emulator_model::DeviceEmulatorModel,
        serial_settings_model::SerialSettingsModel,
    },
    connection::ConnectionId,
    data::{NewSeries, SeriesId},
    serial_connection::{SerialConnectionRegistry, SerialPortConfig},
    user_command::UserCommand,
    worker::{
        ConnectionRouter, ConnectionWorkerEvent, WorkerEvent, WorkerHandle, WorkerHandleError,
    },
};

pub struct CommandModel {
    connections: ConnectionRouter,
    serial_connections: SerialConnectionRegistry,
    event_receiver: Receiver<ConnectionWorkerEvent>,
    log: LogHandle,
}

impl CommandModel {
    pub fn new(
        connections: ConnectionRouter,
        serial_connections: SerialConnectionRegistry,
        event_receiver: Receiver<ConnectionWorkerEvent>,
        log: LogHandle,
    ) -> Self {
        Self {
            connections,
            serial_connections,
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

    fn primary_worker(&self) -> WorkerHandle {
        self.connections
            .handle(ConnectionId::PRIMARY)
            .expect("primary connection worker must be registered")
    }

    pub fn poll_events(&mut self, controls: &mut ControlsModel) {
        while let Ok(connection_event) = self.event_receiver.try_recv() {
            let event = connection_event.event();

            controls.handle_worker_event(event);

            let message = connection_event.to_string();

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
        controls: &mut ControlsModel,
        serial_settings: &SerialSettingsModel,
        device_emulator: &mut DeviceEmulatorModel,
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
                device_emulator.start(serial_settings.settings(), serial_settings.selected_port());
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
            | WorkerEvent::SerialPortTestFailed { .. }
            | WorkerEvent::SerialTextCommandFailed { .. }
            | WorkerEvent::InstrumentReadFailed { .. }
            | WorkerEvent::InstrumentWriteFailed { .. }
            | WorkerEvent::SeriesPollingFailed { .. }
    )
}
