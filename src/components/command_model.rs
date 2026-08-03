use crossbeam_channel::Receiver;

use crate::{
    acquisition::AcquisitionError,
    app_log::LogHandle,
    components::{
        controls_model::ControlsModel, device_emulator_model::DeviceEmulatorModel,
        serial_settings_model::SerialSettingsModel,
    },
    data::{NewSeries, SeriesId},
    user_command::UserCommand,
    worker::{WorkerEvent, WorkerHandle, WorkerHandleError},
};

pub struct CommandModel {
    worker_handle: WorkerHandle,
    event_receiver: Receiver<WorkerEvent>,
    log: LogHandle,
}

impl CommandModel {
    pub fn new(
        worker_handle: WorkerHandle,
        event_receiver: Receiver<WorkerEvent>,
        log: LogHandle,
    ) -> Self {
        Self {
            worker_handle,
            event_receiver,
            log,
        }
    }

    pub fn poll_events(&mut self, controls: &mut ControlsModel) {
        while let Ok(event) = self.event_receiver.try_recv() {
            controls.handle_worker_event(&event);

            let message = event.to_string();

            if worker_event_is_error(&event) {
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
                if let Err(error) = self.worker_handle.remove_series_by_name(name) {
                    self.set_worker_error(error);
                }
            }

            UserCommand::Rename {
                current_name,
                new_name,
            } => {
                if let Err(error) = self.worker_handle.rename_series(current_name, new_name) {
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

            UserCommand::SendSerial { command } => {
                let Some(config) = serial_settings.serial_config() else {
                    self.log.error(
                        "Cannot send COM command: \
                         select a COM port in \
                         Settings.",
                    );

                    return;
                };

                if let Err(error) = self.worker_handle.send_serial_text(config, command) {
                    self.set_worker_error(error);
                }
            }

            UserCommand::ReadInstrument {
                request,
                response_sender,
            } => {
                let Some(config) = serial_settings.serial_config() else {
                    let error = AcquisitionError::from(
                        "Cannot read instrument: select a COM \
                         port in Settings",
                    );

                    self.log.error(error.to_string());

                    let _ = response_sender.send(Err(error));

                    return;
                };

                let send_result = self.worker_handle.read_instrument(
                    config.port_name().to_owned(),
                    request,
                    response_sender.clone(),
                );

                if let Err(send_error) = send_result {
                    let error = AcquisitionError::from(format!(
                        "Failed to request instrument read: \
                         {send_error}",
                    ));

                    self.log.error(error.to_string());

                    let _ = response_sender.send(Err(error));
                }
            }

            UserCommand::WriteInstrument {
                request,
                response_sender,
            } => {
                let Some(config) = serial_settings.serial_config() else {
                    let error = AcquisitionError::from(
                        "Cannot write instrument: select a COM \
                         port in Settings",
                    );

                    self.log.error(error.to_string());

                    let _ = response_sender.send(Err(error));

                    return;
                };

                let send_result = self.worker_handle.write_instrument(
                    config.port_name().to_owned(),
                    request,
                    response_sender.clone(),
                );

                if let Err(send_error) = send_result {
                    let error = AcquisitionError::from(format!(
                        "Failed to request instrument write: \
                         {send_error}",
                    ));

                    self.log.error(error.to_string());

                    let _ = response_sender.send(Err(error));
                }
            }
        }
    }

    pub fn set_visibility(&self, id: SeriesId, visible: bool) {
        if let Err(error) = self.worker_handle.set_visibility(id, visible) {
            self.set_worker_error(error);
        }
    }

    pub fn remove_series(&self, id: SeriesId) {
        if let Err(error) = self.worker_handle.remove_series(id) {
            self.set_worker_error(error);
        }
    }

    pub fn add_series(&self, new_series: NewSeries) {
        if let Err(error) = self.worker_handle.add_series(new_series) {
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
    )
}
