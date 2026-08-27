use crossbeam_channel::Receiver;
use std::collections::BTreeSet;

use crate::{
    acquisition::AcquisitionError,
    app_log::LogHandle,
    application_definition::ApplicationDefinition,
    connection::ConnectionId,
    data::{NewFilteredSeries, NewSeries, SeriesId, SeriesStore},
    instrument::InstrumentValue,
    process_control::{
        ControlLoopDefinition, ControlOutputTarget, Controller, NewPidLoop, PidController,
    },
    serial_connection::{SerialConnectionRegistry, SerialPortConfig},
    signal_processing::{ProcessingHandle, SignalFilterDefinition},
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
    series: SeriesStore,
    processing: ProcessingHandle<SeriesId>,
    event_receiver: Receiver<ConnectionWorkerEvent>,
    log: LogHandle,
}

impl CommandDispatcher {
    pub fn new(
        connections: ConnectionRouter,
        serial_connections: SerialConnectionRegistry,
        application_definition: ApplicationDefinition,
        series: SeriesStore,
        processing: ProcessingHandle<SeriesId>,
        event_receiver: Receiver<ConnectionWorkerEvent>,
        log: LogHandle,
    ) -> Self {
        Self {
            connections,
            serial_connections,
            application_definition,
            series,
            processing,
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

    pub fn poll_events(&mut self) {
        while let Ok(connection_event) = self.event_receiver.try_recv() {
            let event = connection_event.event();

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

            UserCommand::AddFilter(filter) => {
                self.add_filter(filter);
            }

            UserCommand::AddPidLoop(pid_loop) => {
                self.add_pid_loop(pid_loop);
            }

            UserCommand::SetPidSetpoint { name, setpoint } => {
                self.set_pid_setpoint(name, setpoint);
            }

            UserCommand::SetFilter { name, definition } => {
                self.set_filter(name, definition);
            }

            UserCommand::Delete { name } => {
                self.delete_series(name);
            }

            UserCommand::Rename {
                current_name,
                new_name,
            } => match self.series.rename_series(&current_name, &new_name) {
                Ok(id) => {
                    self.log.info(format!(
                        "Series {id} renamed to \
                                 '{new_name}'.",
                    ));
                }

                Err(error) => {
                    self.log.error(format!(
                        "Failed to rename series: \
                                 {error}",
                    ));
                }
            },

            UserCommand::SetSeriesColor { name, color } => {
                match self.series.set_color_by_name(&name, color) {
                    Some(id) => match color {
                        Some(color) => {
                            self.log.info(format!(
                                "Series '{name}' \
                                         ({id}) color \
                                         changed to \
                                         {color}.",
                            ));
                        }

                        None => {
                            self.log.info(format!(
                                "Series '{name}' \
                                         ({id}) color \
                                         reset to \
                                         automatic.",
                            ));
                        }
                    },

                    None => {
                        self.log.error(format!(
                            "Series '{name}' \
                                 not found.",
                        ));
                    }
                }
            }

            UserCommand::Retry { name } => {
                self.retry_series(name);
            }

            UserCommand::RetryAll => {
                self.retry_all_series();
            }

            UserCommand::Start => {
                controls.start();
            }

            UserCommand::Stop => {
                controls.stop();
            }

            UserCommand::Clear => {
                self.clear_series();
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

    fn add_pid_loop(&self, pid_loop: NewPidLoop<ControlOutputTarget>) {
        let (name, input_name, output_target, setpoint, gains, output_limits) =
            pid_loop.into_parts();

        let Some(input_id) = self.series.id_by_name(&input_name) else {
            self.log.error(format!(
                "Failed to add PID loop \
                     '{name}': input series \
                     '{input_name}' was not \
                     found",
            ));

            return;
        };

        let controller = match PidController::with_output_limits(setpoint, gains, output_limits) {
            Ok(controller) => Controller::Pid(controller),

            Err(error) => {
                self.log.error(format!(
                    "Failed to add PID loop \
                         '{name}': {error}",
                ));

                return;
            }
        };

        let definition =
            match ControlLoopDefinition::new(name.clone(), input_id, output_target, controller) {
                Ok(definition) => definition,

                Err(error) => {
                    self.log.error(format!(
                        "Failed to add PID loop \
                         '{name}': {error}",
                    ));

                    return;
                }
            };

        match self.processing.add_control_loop(definition) {
            Ok(()) => {
                self.log.info(format!(
                    "PID loop '{name}' \
                         added for input series \
                         '{input_name}' \
                         ({input_id}).",
                ));
            }

            Err(error) => {
                self.log.error(format!(
                    "Failed to add PID loop \
                         '{name}': {error}",
                ));
            }
        }
    }

    fn set_pid_setpoint(&self, name: String, setpoint: f64) {
        match self.processing.write_controller_parameter(
            &name,
            "setpoint",
            InstrumentValue::Number(setpoint),
        ) {
            Ok(_) => {
                self.log.info(format!(
                    "PID loop '{name}' \
                     setpoint changed to \
                     {setpoint}.",
                ));
            }

            Err(error) => {
                self.log.error(format!(
                    "Failed to change PID \
                     loop '{name}' setpoint: \
                     {error}",
                ));
            }
        }
    }

    pub fn set_visibility(&self, id: SeriesId, visible: bool) {
        self.series.set_visibility(id, visible);
    }

    pub fn add_series(&self, new_series: NewSeries) {
        match self.series.add_series(new_series) {
            Ok(id) => {
                self.log.info(format!("Series {id} added.",));
            }

            Err(error) => {
                self.log.error(format!(
                    "Failed to add series: \
                         {error}",
                ));
            }
        }
    }

    fn add_filter(&self, filter: NewFilteredSeries) {
        let (input_name, output_name, definition, color) = filter.into_parts();

        let Some(input_id) = self.series.id_by_name(&input_name) else {
            self.log.error(format!(
                "Signal processing failed: \
                     cannot add filtered series \
                     '{output_name}': input series \
                     '{input_name}' was not found",
            ));

            return;
        };

        let mut new_series = NewSeries::named_filtered(input_id, definition, output_name.clone());

        if let Some(color) = color {
            new_series = new_series.with_color(color);
        }

        let output_id = match self.series.add_series(new_series) {
            Ok(output_id) => output_id,

            Err(error) => {
                self.log.error(format!(
                    "Failed to add series: \
                             {error}",
                ));

                return;
            }
        };

        if let Err(error) = self.processing.add_filter(input_id, output_id, definition) {
            self.series.remove_series(output_id);

            self.log.error(format!(
                "Signal processing failed: \
                     cannot add filtered series \
                     '{output_name}' from \
                     '{input_name}': {error}",
            ));

            return;
        }

        self.log.info(format!("Series {output_id} added.",));
    }

    fn set_filter(&self, name: String, definition: SignalFilterDefinition) {
        let Some(output_id) = self.series.id_by_name(&name) else {
            self.log.error(format!("Series '{name}' not found.",));

            return;
        };

        if let Err(error) = self.processing.replace_filter(output_id, definition) {
            self.log.error(format!(
                "Signal processing failed: \
                     cannot change filter for \
                     series '{name}': {error}",
            ));

            return;
        }

        if !self.series.set_filter_definition(output_id, definition) {
            self.log.error(format!(
                "Signal processing failed: \
                     cannot change filter for \
                     series '{name}': series is \
                     not a filtered series",
            ));

            return;
        }

        self.log.info(format!(
            "Filter for series '{name}' \
                 ({output_id}) changed to \
                 {definition}.",
        ));
    }

    fn retry_series(&self, name: String) {
        let Some((id, connection_id, was_suspended)) = self.series.resume_polling_by_name(&name)
        else {
            self.log.error(format!("Series '{name}' not found."));

            return;
        };

        if !was_suspended {
            self.log.info(format!(
                "Series '{name}' ({id}) polling is already enabled.",
            ));

            return;
        }

        let worker = match self.connection_worker(connection_id) {
            Ok(worker) => worker,

            Err(error) => {
                self.log.error(error.to_string());
                return;
            }
        };

        if let Err(error) = worker.refresh_series_schedule() {
            self.set_worker_error(error);
            return;
        }

        self.log
            .info(format!("Series '{name}' ({id}) polling retry requested.",));
    }

    fn retry_all_series(&self) {
        let resumed = self.series.resume_all_polling();

        if resumed.is_empty() {
            self.log.info("There are no suspended series to retry.");

            return;
        }

        let connection_ids = resumed
            .iter()
            .map(|(_, _, connection_id)| *connection_id)
            .collect::<BTreeSet<_>>();

        for connection_id in connection_ids {
            let worker = match self.connection_worker(connection_id) {
                Ok(worker) => worker,

                Err(error) => {
                    self.log.error(error.to_string());
                    continue;
                }
            };

            if let Err(error) = worker.refresh_series_schedule() {
                self.set_worker_error(error);
            }
        }

        self.log.info(format!(
            "Polling retry requested for {} suspended series.",
            resumed.len(),
        ));
    }

    fn set_worker_error(&self, error: WorkerHandleError) {
        self.log.error(format!("Failed to send command: {error}",));
    }

    fn delete_series(&self, name: String) {
        let Some(id) = self.series.id_by_name(&name) else {
            self.log.error(format!("Series '{name}' not found.",));

            return;
        };

        let dependent_ids = match self.processing.remove_from(id) {
            Ok(dependent_ids) => dependent_ids,

            Err(error) => {
                self.log.error(format!(
                    "Processing failed: \
                         cannot remove \
                         processing branch \
                         for series '{name}': \
                         {error}",
                ));

                return;
            }
        };

        for dependent_id in dependent_ids
            .iter()
            .copied()
            .filter(|dependent_id| *dependent_id != id)
        {
            self.series.remove_series(dependent_id);
        }

        self.series.remove_series(id);

        let dependent_count = dependent_ids
            .iter()
            .filter(|&&dependent_id| dependent_id != id)
            .count();

        if dependent_count == 0 {
            self.log.info(format!("Series '{name}' ({id}) removed.",));
        } else {
            self.log.info(format!(
                "Series '{name}' ({id}) removed \
                     with {dependent_count} dependent \
                     series.",
            ));
        }
    }

    fn clear_series(&self) {
        match self.processing.clear() {
            Ok(()) => {
                self.series.clear();

                self.log.info("All series cleared.");
            }

            Err(error) => {
                self.log.error(format!(
                    "Processing failed: \
                         cannot clear \
                         processing state: \
                         {error}",
                ));
            }
        }
    }
}

fn worker_event_is_error(event: &WorkerEvent) -> bool {
    matches!(
        event,
        WorkerEvent::AcquisitionStartFailed(_)
            | WorkerEvent::AcquisitionFailed(_)
            | WorkerEvent::AcquisitionStopFailed(_)
            | WorkerEvent::ProcessingFailed(_)
            | WorkerEvent::SerialTextCommandFailed { .. }
            | WorkerEvent::InstrumentReadFailed { .. }
            | WorkerEvent::InstrumentWriteFailed { .. }
            | WorkerEvent::SeriesPollingSuspended { .. }
    )
}
