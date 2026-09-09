use crossbeam_channel::Receiver;
use std::collections::{BTreeSet, HashMap};

use crate::{
    acquisition::AcquisitionError,
    app_log::LogHandle,
    application_definition::ApplicationDefinition,
    connection::ConnectionId,
    data::{
        NewControllerDiagnosticSeries, NewFilteredSeries, NewSeries, SeriesId, SeriesSource,
        SeriesStore,
    },
    instrument::ConnectedParameterAddress,
    output_control::{OutputHandle, OutputRequestError},
    process_control::{ControlLoopDefinition, ControlOutputTarget, NewController},
    process_recorder::{
        ProcessActionContext, ProcessActionId, ProcessActionResult, ProcessRecorder,
    },
    serial_connection::{SerialConnectionRegistry, SerialPortConfig},
    signal_processing::{ProcessingHandle, SignalFilterDefinition},
    user_command::{
        PauseControllerError, ResumeControllerError, SetControllerInputError, UserCommand,
    },
    worker::{
        ConnectionRouter, ConnectionWorkerEvent, WorkerEvent, WorkerHandle, WorkerHandleError,
    },
};

use super::{
    acquisition_controller::AcquisitionController, device_emulator_service::DeviceEmulatorService,
};

pub(crate) struct CommandDispatcherConnections {
    router: ConnectionRouter,
    serial: SerialConnectionRegistry,
    event_receiver: Receiver<ConnectionWorkerEvent>,
}

impl CommandDispatcherConnections {
    pub(crate) fn new(
        router: ConnectionRouter,
        serial: SerialConnectionRegistry,
        event_receiver: Receiver<ConnectionWorkerEvent>,
    ) -> Self {
        Self {
            router,
            serial,
            event_receiver,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AcquisitionActionKind {
    Start,
    Stop,
}

struct PendingAcquisitionAction {
    kind: AcquisitionActionKind,
    expected_responses: usize,
    responded_connections: BTreeSet<ConnectionId>,
    rollback_connections: BTreeSet<ConnectionId>,
}

pub(crate) struct CommandDispatcher {
    connections: ConnectionRouter,
    serial_connections: SerialConnectionRegistry,
    application_definition: ApplicationDefinition,
    series: SeriesStore,
    processing: ProcessingHandle<SeriesId>,
    output_control: OutputHandle,
    process_recorder: ProcessRecorder,
    pending_acquisition_actions: HashMap<ProcessActionId, PendingAcquisitionAction>,
    event_receiver: Receiver<ConnectionWorkerEvent>,
    log: LogHandle,
}

fn pause_controller_safely(
    output_control: &OutputHandle,
    processing: &ProcessingHandle<SeriesId>,
    name: &str,
) -> Result<(), PauseControllerError> {
    let safe_output_result = output_control.apply_safe(name);

    let pause_result = processing.pause_controller(name);

    match safe_output_result {
        Err(output) => match pause_result {
            Ok(()) => Err(PauseControllerError::Output(output)),

            Err(pause) => Err(PauseControllerError::OutputAndControllerPause { output, pause }),
        },

        Ok(response) => {
            let safe_write_result = response.recv();

            match (safe_write_result, pause_result) {
                (Ok(_), Ok(())) => Ok(()),

                (Err(write), Ok(())) => Err(PauseControllerError::SafeOutputWrite(write)),

                (Ok(_), Err(pause)) => Err(PauseControllerError::ControllerAfterSafeOutput(pause)),

                (Err(write), Err(pause)) => {
                    Err(PauseControllerError::SafeOutputWriteAndControllerPause { write, pause })
                }
            }
        }
    }
}

fn resume_controller_safely(
    output_control: &OutputHandle,
    processing: &ProcessingHandle<SeriesId>,
    name: &str,
) -> Result<(), ResumeControllerError> {
    output_control.request_automatic(name)?;

    if let Err(controller_error) = processing.resume_controller(name) {
        if let Err(rollback_error) = output_control.rollback_automatic_request(name) {
            return Err(ResumeControllerError::Rollback {
                controller: controller_error,
                rollback: rollback_error,
            });
        }

        return Err(ResumeControllerError::Controller(controller_error));
    }

    Ok(())
}

impl CommandDispatcher {
    pub fn new(
        connections: CommandDispatcherConnections,
        application_definition: ApplicationDefinition,
        series: SeriesStore,
        processing: ProcessingHandle<SeriesId>,
        output_control: OutputHandle,
        process_recorder: ProcessRecorder,
        log: LogHandle,
    ) -> Self {
        Self {
            connections: connections.router,
            serial_connections: connections.serial,
            application_definition,
            series,
            processing,
            output_control,
            process_recorder,
            pending_acquisition_actions: HashMap::new(),
            event_receiver: connections.event_receiver,
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

    fn output_write_error(error: OutputRequestError) -> AcquisitionError {
        AcquisitionError::from(format!(
            "Failed to request instrument \
                 write: {error}",
        ))
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

    fn rollback_acquisition_start(&self, connection_ids: &BTreeSet<ConnectionId>) {
        for &connection_id in connection_ids {
            let Some(worker) = self.connections.handle(connection_id) else {
                self.log.error(format!(
                    "Failed to roll back acquisition \
                     start for connection \
                     {connection_id}: worker is not \
                     registered.",
                ));

                continue;
            };

            if let Err(error) = worker.stop(None) {
                self.log.error(format!(
                    "Failed to roll back acquisition \
                     start for connection \
                     {connection_id}: {error}",
                ));
            }
        }
    }

    fn handle_acquisition_action_event(&mut self, connection_event: &ConnectionWorkerEvent) {
        let Some(action_id) = connection_event.action_id() else {
            return;
        };

        let connection_id = connection_event.connection_id();

        let event = connection_event.event();

        let failure = match event {
            WorkerEvent::AcquisitionStartFailed(_) | WorkerEvent::AcquisitionStopFailed(_) => {
                Some(self.format_worker_event(connection_event))
            }

            _ => None,
        };

        let completed = {
            let Some(pending) = self.pending_acquisition_actions.get_mut(&action_id) else {
                return;
            };

            let relevant = matches!(
                (pending.kind, event),
                (
                    AcquisitionActionKind::Start,
                    WorkerEvent::AcquisitionStarted | WorkerEvent::AcquisitionStartFailed(_)
                ) | (
                    AcquisitionActionKind::Stop,
                    WorkerEvent::AcquisitionStopped | WorkerEvent::AcquisitionStopFailed(_)
                )
            );

            if !relevant {
                return;
            }

            if !pending.responded_connections.insert(connection_id) {
                return;
            }

            pending.responded_connections.len() == pending.expected_responses
        };

        if let Some(error) = failure {
            let rollback_connections = self
                .pending_acquisition_actions
                .get(&action_id)
                .filter(|pending| pending.kind == AcquisitionActionKind::Start)
                .map(|pending| pending.rollback_connections.clone())
                .unwrap_or_default();

            self.pending_acquisition_actions.remove(&action_id);

            if !rollback_connections.is_empty() {
                self.rollback_acquisition_start(&rollback_connections);
            }

            self.process_recorder.record_action_failed(action_id, error);

            return;
        }

        if completed {
            self.pending_acquisition_actions.remove(&action_id);

            self.process_recorder
                .record_action_applied(action_id, None, None);
        }
    }

    fn handle_single_worker_action_event(&self, connection_event: &ConnectionWorkerEvent) {
        let Some(action_id) = connection_event.action_id() else {
            return;
        };

        match connection_event.event() {
            WorkerEvent::SerialTextCommandSucceeded { response, .. } => {
                self.process_recorder.record_action_applied_with_result(
                    action_id,
                    ProcessActionResult::SerialResponse(response.clone()),
                );
            }

            WorkerEvent::InstrumentReadSucceeded { value, .. } => {
                self.process_recorder.record_action_applied_with_result(
                    action_id,
                    ProcessActionResult::InstrumentValue(*value),
                );
            }

            WorkerEvent::InstrumentWriteSucceeded { actual_value, .. } => {
                self.process_recorder.record_action_applied_with_result(
                    action_id,
                    ProcessActionResult::InstrumentValue(*actual_value),
                );
            }

            WorkerEvent::VirtualInstrumentDescribeSucceeded { count } => {
                self.process_recorder.record_action_applied_with_result(
                    action_id,
                    ProcessActionResult::VirtualInstrumentCount(*count),
                );
            }

            WorkerEvent::SerialTextCommandFailed { .. }
            | WorkerEvent::InstrumentReadFailed { .. }
            | WorkerEvent::InstrumentWriteFailed { .. }
            | WorkerEvent::VirtualInstrumentDescribeFailed { .. } => {
                self.process_recorder
                    .record_action_failed(action_id, self.format_worker_event(connection_event));
            }

            _ => {}
        }
    }

    pub fn poll_events(&mut self) {
        while let Ok(connection_event) = self.event_receiver.try_recv() {
            self.handle_acquisition_action_event(&connection_event);

            self.handle_single_worker_action_event(&connection_event);

            let event = connection_event.event();

            if !worker_event_should_be_logged(event, connection_event.action_id().is_some()) {
                continue;
            }

            let message = self.format_worker_event(&connection_event);

            if worker_event_is_error(event) {
                self.log.error(message);
            } else {
                self.log.info(message);
            }
        }
    }

    fn resume_controller(&self, name: &str) -> Result<(), ResumeControllerError> {
        resume_controller_safely(&self.output_control, &self.processing, name)
    }

    fn pause_controller(&self, name: &str) -> Result<(), PauseControllerError> {
        pause_controller_safely(&self.output_control, &self.processing, name)
    }

    pub fn execute(
        &mut self,
        command: UserCommand,
        action_context: Option<ProcessActionContext>,
        controls: &mut AcquisitionController,
        device_emulator: &mut DeviceEmulatorService,
    ) {
        match command {
            UserCommand::Add(new_series) => match self.add_series(new_series) {
                Ok(id) => {
                    let series_name = self
                        .series
                        .metadata()
                        .into_iter()
                        .find(|series| series.id == id)
                        .map(|series| series.name);

                    if let Some(action_context) = action_context {
                        self.process_recorder.record_action_applied(
                            action_context.action_id(),
                            Some(id),
                            series_name,
                        );
                    }
                }

                Err(error) => {
                    if let Some(action_context) = action_context {
                        self.process_recorder
                            .record_action_failed(action_context.action_id(), error);
                    }
                }
            },

            UserCommand::AddFilter(filter) => match self.add_filter(filter) {
                Ok(id) => {
                    let series_name = self
                        .series
                        .metadata()
                        .into_iter()
                        .find(|series| series.id == id)
                        .map(|series| series.name);

                    if let Some(action_context) = action_context {
                        self.process_recorder.record_action_applied(
                            action_context.action_id(),
                            Some(id),
                            series_name,
                        );
                    }
                }

                Err(error) => {
                    if let Some(action_context) = action_context {
                        self.process_recorder
                            .record_action_failed(action_context.action_id(), error);
                    }
                }
            },

            UserCommand::AddControllerDiagnostic(diagnostic) => {
                self.add_controller_diagnostic(diagnostic);
            }

            UserCommand::AddController(new_controller) => {
                match self.add_controller(new_controller) {
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

            UserCommand::ControllerParameters {
                name,
                response_sender,
            } => {
                let result = self.processing.controller_parameters(&name);

                let _ = response_sender.send(result);
            }

            UserCommand::ControllerDiagnostics {
                name,
                response_sender,
            } => {
                let result = self.processing.controller_diagnostics(&name);

                let _ = response_sender.send(result);
            }

            UserCommand::ReadControllerParameter {
                name,
                key,
                response_sender,
            } => {
                let result = self.processing.read_controller_parameter(&name, &key);

                let _ = response_sender.send(result);
            }

            UserCommand::WriteControllerParameter {
                name,
                key,
                value,
                response_sender,
            } => {
                let result = self
                    .processing
                    .write_controller_parameter(&name, &key, value);

                match &result {
                    Ok(_) => {
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
                            self.process_recorder.record_action_failed(
                                action_context.action_id(),
                                error.to_string(),
                            );
                        }
                    }
                }

                let _ = response_sender.send(result);
            }

            UserCommand::ConfigureController {
                name,
                updates,
                response_sender,
            } => {
                let result = self.processing.configure_controller(&name, updates);

                match &result {
                    Ok(_) => {
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
                            self.process_recorder.record_action_failed(
                                action_context.action_id(),
                                error.to_string(),
                            );
                        }
                    }
                }

                let _ = response_sender.send(result);
            }

            UserCommand::ControllerReferenceKind {
                name,
                response_sender,
            } => {
                let result = self.processing.reference_kind(&name);

                let _ = response_sender.send(result);
            }

            UserCommand::ControllerReferenceParameters {
                name,
                response_sender,
            } => {
                let result = self.processing.reference_parameters(&name);

                let _ = response_sender.send(result);
            }

            UserCommand::ReadControllerReferenceParameter {
                name,
                key,
                response_sender,
            } => {
                let result = self.processing.read_reference_parameter(&name, &key);

                let _ = response_sender.send(result);
            }

            UserCommand::WriteControllerReferenceParameter {
                name,
                key,
                value,
                response_sender,
            } => {
                let result = self
                    .processing
                    .write_reference_parameter(&name, &key, value);

                match &result {
                    Ok(_) => {
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
                            self.process_recorder.record_action_failed(
                                action_context.action_id(),
                                error.to_string(),
                            );
                        }
                    }
                }

                let _ = response_sender.send(result);
            }

            UserCommand::ConfigureControllerReference {
                name,
                updates,
                response_sender,
            } => {
                let result = self.processing.configure_reference(&name, updates);

                match &result {
                    Ok(_) => {
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
                            self.process_recorder.record_action_failed(
                                action_context.action_id(),
                                error.to_string(),
                            );
                        }
                    }
                }

                let _ = response_sender.send(result);
            }

            UserCommand::SetControllerReference {
                name,
                source,
                response_sender,
            } => {
                let result = self.processing.set_reference(&name, source);

                match &result {
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
                            self.process_recorder.record_action_failed(
                                action_context.action_id(),
                                error.to_string(),
                            );
                        }
                    }
                }

                let _ = response_sender.send(result);
            }

            UserCommand::ResetController {
                name,
                response_sender,
            } => {
                let result = self.processing.reset_controller(&name);

                match &result {
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
                            self.process_recorder.record_action_failed(
                                action_context.action_id(),
                                error.to_string(),
                            );
                        }
                    }
                }

                let _ = response_sender.send(result);
            }

            UserCommand::SetControllerInput {
                name,
                input_name,
                response_sender,
            } => {
                let result = match self.series.id_by_name(&input_name) {
                    Some(input_id) => self
                        .processing
                        .set_controller_input(&name, input_id)
                        .map_err(Into::into),

                    None => Err(SetControllerInputError::SeriesNotFound(input_name)),
                };

                match &result {
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
                            self.process_recorder.record_action_failed(
                                action_context.action_id(),
                                error.to_string(),
                            );
                        }
                    }
                }

                let _ = response_sender.send(result);
            }

            UserCommand::ControllerState {
                name,
                response_sender,
            } => {
                let result = self.processing.controller_state(&name);

                let _ = response_sender.send(result);
            }

            UserCommand::PauseController {
                name,
                response_sender,
            } => {
                let result = self.pause_controller(&name);

                match &result {
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
                            self.process_recorder.record_action_failed(
                                action_context.action_id(),
                                error.to_string(),
                            );
                        }
                    }
                }

                let _ = response_sender.send(result);
            }

            UserCommand::ResumeController {
                name,
                response_sender,
            } => {
                let result = self.resume_controller(&name);

                match &result {
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
                            self.process_recorder.record_action_failed(
                                action_context.action_id(),
                                error.to_string(),
                            );
                        }
                    }
                }

                let _ = response_sender.send(result);
            }

            UserCommand::ResetControllerIntegral {
                name,
                response_sender,
            } => {
                let result = self.processing.reset_controller_integral(&name);

                match &result {
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
                            self.process_recorder.record_action_failed(
                                action_context.action_id(),
                                error.to_string(),
                            );
                        }
                    }
                }

                let _ = response_sender.send(result);
            }

            UserCommand::SetFilter { name, definition } => {
                match self.set_filter(&name, definition) {
                    Ok(id) => {
                        if let Some(action_context) = action_context {
                            self.process_recorder.record_action_applied(
                                action_context.action_id(),
                                Some(id),
                                Some(name),
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

            UserCommand::Delete { name } => match self.delete_series(&name) {
                Ok(id) => {
                    if let Some(action_context) = action_context {
                        self.process_recorder.record_action_applied(
                            action_context.action_id(),
                            Some(id),
                            Some(name),
                        );
                    }
                }

                Err(error) => {
                    if let Some(action_context) = action_context {
                        self.process_recorder
                            .record_action_failed(action_context.action_id(), error);
                    }
                }
            },

            UserCommand::Rename {
                current_name,
                new_name,
            } => match self.series.rename_series(&current_name, &new_name) {
                Ok(id) => {
                    if let Some(action_context) = action_context {
                        self.process_recorder.record_action_applied(
                            action_context.action_id(),
                            Some(id),
                            Some(new_name),
                        );
                    }
                }

                Err(error) => {
                    if let Some(action_context) = action_context {
                        self.process_recorder
                            .record_action_failed(action_context.action_id(), error.to_string());
                    }
                }
            },

            UserCommand::SetSeriesColor { name, color } => {
                match self.series.set_color_by_name(&name, color) {
                    Some(id) => {
                        if let Some(action_context) = action_context {
                            self.process_recorder.record_action_applied(
                                action_context.action_id(),
                                Some(id),
                                Some(name),
                            );
                        }
                    }

                    None => {
                        let error = format!("Series '{name}' not found.");

                        if let Some(action_context) = action_context {
                            self.process_recorder
                                .record_action_failed(action_context.action_id(), error);
                        }
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
                let action_id = action_context.map(|context| context.action_id());

                let rollback_connections = controls.stopped_connection_ids();

                if let Some(action_id) = action_id {
                    self.pending_acquisition_actions.insert(
                        action_id,
                        PendingAcquisitionAction {
                            kind: AcquisitionActionKind::Start,
                            expected_responses: controls.worker_count(),
                            responded_connections: BTreeSet::new(),
                            rollback_connections: rollback_connections.clone(),
                        },
                    );
                }

                if let Err(error) = controls.start(action_id) {
                    let error = format!("Failed to start acquisition: {error}",);

                    self.rollback_acquisition_start(&rollback_connections);

                    if let Some(action_id) = action_id {
                        self.pending_acquisition_actions.remove(&action_id);

                        self.process_recorder.record_action_failed(action_id, error);
                    }
                }
            }

            UserCommand::Stop => {
                let action_id = action_context.map(|context| context.action_id());

                if let Some(action_id) = action_id {
                    self.pending_acquisition_actions.insert(
                        action_id,
                        PendingAcquisitionAction {
                            kind: AcquisitionActionKind::Stop,
                            expected_responses: controls.worker_count(),
                            responded_connections: BTreeSet::new(),
                            rollback_connections: BTreeSet::new(),
                        },
                    );
                }

                if let Err(error) = controls.stop(action_id) {
                    let error = format!("Failed to stop acquisition: {error}",);

                    if let Some(action_id) = action_id {
                        self.pending_acquisition_actions.remove(&action_id);

                        self.process_recorder.record_action_failed(action_id, error);
                    }
                }
            }

            UserCommand::Clear => match self.clear_series() {
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
            },

            UserCommand::StartEmulator => {
                let result = (|| {
                    let serial_config = self.emulator_serial_config().map_err(|error| {
                        format!(
                            "Cannot start emulator: \
                                 {error}",
                        )
                    })?;

                    device_emulator.start(&serial_config).map_err(|error| {
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

            UserCommand::StopEmulator => {
                device_emulator.stop();

                if let Some(action_context) = action_context {
                    self.process_recorder.record_action_applied(
                        action_context.action_id(),
                        None,
                        None,
                    );
                }
            }

            UserCommand::Log { message } => {
                self.log.info(message);
            }

            UserCommand::SendSerial {
                connection_id,
                command,
            } => {
                let action_id = action_context.map(|context| context.action_id());

                let config = match self.serial_config(connection_id) {
                    Ok(config) => config,

                    Err(error) => {
                        let error = error.to_string();

                        if let Some(action_id) = action_id {
                            self.process_recorder.record_action_failed(action_id, error);
                        }

                        return;
                    }
                };

                let worker_handle = match self.connection_worker(connection_id) {
                    Ok(worker_handle) => worker_handle,

                    Err(error) => {
                        let error = error.to_string();

                        if let Some(action_id) = action_id {
                            self.process_recorder.record_action_failed(action_id, error);
                        }

                        return;
                    }
                };

                if let Err(error) = worker_handle.send_serial_text(action_id, config, command) {
                    let error = format!("Failed to send serial command: {error}");

                    if let Some(action_id) = action_id {
                        self.process_recorder.record_action_failed(action_id, error);
                    }
                }
            }

            UserCommand::ReadInstrument {
                connection_id,
                request,
                response_sender,
            } => {
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

            UserCommand::WriteInstrument {
                connection_id,
                request,
                response_sender,
            } => {
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

            UserCommand::DescribeVirtualInstruments {
                connection_id,
                response_sender,
            } => {
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
        }
    }

    fn install_control_loop(
        &self,
        name: &str,
        target: ConnectedParameterAddress,
        definition: ControlLoopDefinition<SeriesId, ControlOutputTarget>,
    ) -> Result<(), String> {
        let instance_id = definition.instance_id();

        let safe_request = definition
            .output_target()
            .safe_write_request()
            .map_err(|error| {
                format!(
                    "failed to create safe \
                         output request: {error}",
                )
            })?;

        self.output_control
            .register_controller(target, name, instance_id, safe_request)
            .map_err(|error| {
                format!(
                    "failed to register output: \
                     {error}",
                )
            })?;

        if let Err(error) = self.processing.add_control_loop(definition) {
            let rollback_result = self
                .output_control
                .rollback_controller_registration(target, name);

            return match rollback_result {
                Ok(()) => Err(error.to_string()),

                Err(rollback_error) => Err(format!(
                    "{error}; output ownership \
                         rollback also failed: \
                         {rollback_error}",
                )),
            };
        }

        Ok(())
    }

    fn add_controller(
        &self,
        new_controller: NewController<ControlOutputTarget>,
    ) -> Result<(), String> {
        let (name, input_name, output_target, controller) = new_controller.into_parts();

        let kind = controller.kind();

        let target = output_target.connected_parameter_address();

        let Some(input_id) = self.series.id_by_name(&input_name) else {
            return Err(format!(
                "Failed to add {kind} controller \
                 '{name}': input series \
                 '{input_name}' was not found",
            ));
        };

        let definition =
            ControlLoopDefinition::new(name.clone(), input_id, output_target, controller).map_err(
                |error| {
                    format!(
                        "Failed to add {kind} controller \
                 '{name}': {error}",
                    )
                },
            )?;

        self.install_control_loop(&name, target, definition)
            .map_err(|error| {
                format!(
                    "Failed to add {kind} controller \
                 '{name}': {error}",
                )
            })?;

        Ok(())
    }

    pub fn set_visibility(&self, id: SeriesId, visible: bool) -> bool {
        self.series.set_visibility(id, visible)
    }

    pub fn add_series(&self, new_series: NewSeries) -> Result<SeriesId, String> {
        let id = self.series.add_series(new_series).map_err(|error| {
            format!(
                "Failed to add series: \
                     {error}",
            )
        })?;

        Ok(id)
    }

    fn add_filter(&self, filter: NewFilteredSeries) -> Result<SeriesId, String> {
        let (input_name, output_name, definition, color) = filter.into_parts();

        let Some(input_id) = self.series.id_by_name(&input_name) else {
            return Err(format!(
                "Signal processing failed: \
                 cannot add filtered series \
                 '{output_name}': input series \
                 '{input_name}' was not found",
            ));
        };

        let mut new_series = NewSeries::named_filtered(input_id, definition, output_name.clone());

        if let Some(color) = color {
            new_series = new_series.with_color(color);
        }

        let output_id = self.series.add_series(new_series).map_err(|error| {
            format!(
                "Failed to add series: \
                         {error}",
            )
        })?;

        if let Err(error) = self.processing.add_filter(input_id, output_id, definition) {
            self.series.remove_series(output_id);

            return Err(format!(
                "Signal processing failed: \
                 cannot add filtered series \
                 '{output_name}' from \
                 '{input_name}': {error}",
            ));
        }

        Ok(output_id)
    }

    fn add_controller_diagnostic(&self, diagnostic_series: NewControllerDiagnosticSeries) {
        let (controller, diagnostic, name, connection_id, color) = diagnostic_series.into_parts();

        let mut new_series =
            NewSeries::named_controller_diagnostic(controller.clone(), diagnostic, name.clone())
                .with_connection(connection_id);

        if let Some(color) = color {
            new_series = new_series.with_color(color);
        }

        let output_id = match self.series.add_series(new_series) {
            Ok(output_id) => output_id,

            Err(error) => {
                self.log.error(format!(
                    "Failed to add controller \
                             diagnostic series \
                             '{name}': {error}",
                ));

                return;
            }
        };

        if let Err(error) =
            self.processing
                .add_controller_diagnostic(controller.clone(), diagnostic, output_id)
        {
            self.series.remove_series(output_id);

            self.log.error(format!(
                "Signal processing failed: \
                     cannot add diagnostic \
                     series '{name}' for \
                     controller '{controller}': \
                     {error}",
            ));

            return;
        }

        self.log.info(format!(
            "Controller diagnostic \
                 series '{name}' \
                 ({output_id}) added for \
                 controller '{controller}' \
                 diagnostic '{diagnostic}'.",
        ));
    }

    fn set_filter(
        &self,
        name: &str,
        definition: SignalFilterDefinition,
    ) -> Result<SeriesId, String> {
        let Some(output_id) = self.series.id_by_name(name) else {
            return Err(format!("Series '{name}' not found.",));
        };

        let old_definition = self
            .series
            .metadata()
            .into_iter()
            .find(|series| series.id == output_id)
            .and_then(|series| match series.source {
                SeriesSource::Filtered { definition, .. } => Some(definition),

                _ => None,
            })
            .ok_or_else(|| {
                format!(
                    "Signal processing failed: \
                     cannot change filter for \
                     series '{name}': series is \
                     not a filtered series",
                )
            })?;

        self.processing
            .replace_filter(output_id, definition)
            .map_err(|error| {
                format!(
                    "Signal processing failed: \
                     cannot change filter for \
                     series '{name}': {error}",
                )
            })?;

        if !self.series.set_filter_definition(output_id, definition) {
            let rollback_result = self.processing.replace_filter(output_id, old_definition);

            return match rollback_result {
                Ok(()) => Err(format!(
                    "Signal processing failed: \
                     cannot update stored filter \
                     definition for series \
                     '{name}'",
                )),

                Err(rollback_error) => Err(format!(
                    "Signal processing failed: \
                     cannot update stored filter \
                     definition for series \
                     '{name}'; processing rollback \
                     also failed: {rollback_error}",
                )),
            };
        }

        Ok(output_id)
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

    fn delete_series(&self, name: &str) -> Result<SeriesId, String> {
        let Some(id) = self.series.id_by_name(name) else {
            return Err(format!("Series '{name}' not found."));
        };

        let affected_controllers = self
            .processing
            .controllers_affected_by_removal(id)
            .map_err(|error| {
                format!(
                    "Processing failed: \
                     cannot preview controllers \
                     affected by removal of \
                     series '{name}': {error}",
                )
            })?;

        for controller in &affected_controllers {
            self.pause_controller(controller).map_err(|error| {
                format!(
                    "Cannot remove series \
                         '{name}': failed to safely \
                         pause controller \
                         '{controller}': {error}",
                )
            })?;
        }

        let dependent_ids = self.processing.remove_from(id).map_err(|error| {
            format!(
                "Processing failed: \
                     cannot remove \
                     processing branch \
                     for series '{name}': \
                     {error}",
            )
        })?;

        for controller in &affected_controllers {
            if let Err(error) = self.output_control.release_controller(controller) {
                self.log.error(format!(
                    "Controller \
                     '{controller}' was removed \
                     from processing, but its \
                     output ownership could not \
                     be released: {error}",
                ));
            }
        }

        for dependent_id in dependent_ids
            .iter()
            .copied()
            .filter(|dependent_id| *dependent_id != id)
        {
            self.series.remove_series(dependent_id);
        }

        self.series.remove_series(id);

        Ok(id)
    }

    fn clear_series(&self) -> Result<(), String> {
        let controllers = self.processing.controller_names().map_err(|error| {
            format!(
                "Processing failed: \
                     cannot inspect controllers \
                     before clearing: {error}",
            )
        })?;

        for controller in &controllers {
            self.pause_controller(controller).map_err(|error| {
                format!(
                    "Cannot clear processing: \
                         failed to safely pause \
                         controller \
                         '{controller}': {error}",
                )
            })?;
        }

        self.processing.clear().map_err(|error| {
            format!(
                "Processing failed: \
                 cannot clear processing \
                 state: {error}",
            )
        })?;

        for controller in &controllers {
            if let Err(error) = self.output_control.release_controller(controller) {
                self.log.error(format!(
                    "Controller \
                     '{controller}' was \
                     removed from processing, \
                     but its output ownership \
                     could not be released: \
                     {error}",
                ));
            }
        }

        self.series.clear();

        Ok(())
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
            | WorkerEvent::VirtualInstrumentDescribeFailed { .. }
            | WorkerEvent::SeriesPollingSuspended { .. }
    )
}

fn worker_event_completes_action(event: &WorkerEvent) -> bool {
    matches!(
        event,
        WorkerEvent::AcquisitionStarted
            | WorkerEvent::AcquisitionStopped
            | WorkerEvent::AcquisitionStartFailed(_)
            | WorkerEvent::AcquisitionStopFailed(_)
            | WorkerEvent::SerialTextCommandSucceeded { .. }
            | WorkerEvent::SerialTextCommandFailed { .. }
            | WorkerEvent::InstrumentReadSucceeded { .. }
            | WorkerEvent::InstrumentReadFailed { .. }
            | WorkerEvent::InstrumentWriteSucceeded { .. }
            | WorkerEvent::InstrumentWriteFailed { .. }
            | WorkerEvent::VirtualInstrumentDescribeSucceeded { .. }
            | WorkerEvent::VirtualInstrumentDescribeFailed { .. }
    )
}

fn worker_event_should_be_logged(event: &WorkerEvent, has_action_id: bool) -> bool {
    !has_action_id || !worker_event_completes_action(event)
}

#[cfg(test)]
mod tests {
    use crossbeam_channel::{bounded, unbounded};
    use serialport::{DataBits, FlowControl, Parity, StopBits};

    use crate::{
        connection::ConnectionId,
        data::SeriesId,
        instrument::{
            ConnectedParameterAddress, InstrumentParameterAddress, InstrumentValue,
            InstrumentWriteRequest, ParameterAccess, ParameterRange, ParameterValueType,
            virtual_instrument::{
                VirtualInstrumentId, VirtualParameterDescriptor, VirtualParameterId,
            },
        },
        output_control::{OutputMode, OutputService},
        process_control::{
            ControlLoopDefinition, ControlLoopState, ControlOutputTarget, ControllerInstanceId,
            OnOffController,
        },
        serial_connection::{SerialConnectionRegistry, SerialPortConfig},
        signal_processing::ProcessingService,
        user_command::ResumeControllerError,
        worker::{ConnectionRouter, WorkerEvent, WorkerHandle},
    };

    use super::{pause_controller_safely, resume_controller_safely, worker_event_should_be_logged};

    fn test_serial_config(port_name: &str) -> SerialPortConfig {
        SerialPortConfig::new(
            port_name.to_owned(),
            9_600,
            DataBits::Eight,
            Parity::None,
            StopBits::One,
            FlowControl::None,
            250,
        )
    }

    #[test]
    fn suppresses_action_worker_event_duplicate_log() {
        let event = WorkerEvent::SerialTextCommandSucceeded {
            port_name: "COM3".to_owned(),
            command: "get".to_owned(),
            response: "42".to_owned(),
        };

        assert!(!worker_event_should_be_logged(&event, true,),);
    }

    #[test]
    fn keeps_autonomous_worker_event_log() {
        let event = WorkerEvent::ProcessingFailed("test failure".to_owned());

        assert!(worker_event_should_be_logged(&event, false,),);
    }

    #[test]
    fn pauses_controller_when_safe_output_request_fails() {
        let processing = ProcessingService::<SeriesId>::spawn().unwrap();

        let processing_handle = processing.handle();

        let descriptor = VirtualParameterDescriptor::new(
            VirtualParameterId::new(1),
            "heater_power",
            "Heater power",
            ParameterAccess::ReadWrite,
            ParameterValueType::Number,
        )
        .with_range(ParameterRange::Number {
            minimum: 0.0,
            maximum: 100.0,
        });

        let output = ControlOutputTarget::virtual_instrument(
            ConnectionId::PRIMARY,
            VirtualInstrumentId::new(1),
            &descriptor,
        )
        .unwrap();

        let output_address = output.connected_parameter_address();

        let controller = OnOffController::new(100.0, 2.0, 0.0, 100.0).unwrap().into();

        let definition =
            ControlLoopDefinition::new("heater", SeriesId::new(1), output, controller).unwrap();

        let instance_id = definition.instance_id();

        processing_handle.add_control_loop(definition).unwrap();

        let output_service =
            OutputService::spawn(ConnectionRouter::default(), SerialConnectionRegistry::new())
                .unwrap();

        let output_handle = output_service.handle();

        output_handle
            .register_controller(output_address, "heater", instance_id, None)
            .unwrap();

        let result = pause_controller_safely(&output_handle, &processing_handle, "heater");

        assert!(result.is_err());

        assert_eq!(
            processing_handle.controller_state("heater"),
            Ok(ControlLoopState::Paused),
        );
    }

    #[test]
    fn rolls_back_automatic_request_when_controller_resume_fails() {
        let processing = ProcessingService::<SeriesId>::spawn().unwrap();

        let processing_handle = processing.handle();

        let connection_id = ConnectionId::new(2);

        let instrument_id = VirtualInstrumentId::new(7);

        let parameter_id = VirtualParameterId::new(4);

        let target = ConnectedParameterAddress::new(
            connection_id,
            InstrumentParameterAddress::virtual_instrument(instrument_id, parameter_id),
        );

        let serial_connections = SerialConnectionRegistry::new();

        serial_connections
            .register(connection_id)
            .unwrap()
            .set(Some(test_serial_config("COM9")));

        let connection_router = ConnectionRouter::default();

        let (command_sender, command_receiver) = unbounded();

        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let output_service = OutputService::spawn(connection_router, serial_connections).unwrap();

        let output_handle = output_service.handle();

        output_handle
            .register_controller(target, "heater", ControllerInstanceId::for_test(1), None)
            .unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            instrument_id,
            parameter_id,
            InstrumentValue::Number(35.0),
        );

        let (response_sender, _response_receiver) = bounded(1);

        output_handle
            .write_instrument(None, connection_id, request, response_sender)
            .unwrap();

        assert_eq!(output_handle.mode(target), Ok(OutputMode::Manual),);

        output_handle.request_automatic("heater").unwrap();

        assert_eq!(output_handle.mode(target), Ok(OutputMode::AutomaticPending),);

        output_handle.rollback_automatic_request("heater").unwrap();

        assert_eq!(output_handle.mode(target), Ok(OutputMode::Manual),);

        let _ = command_receiver.try_recv().unwrap();

        let result = resume_controller_safely(&output_handle, &processing_handle, "heater");

        assert!(matches!(result, Err(ResumeControllerError::Controller(_)),));

        assert_eq!(output_handle.mode(target), Ok(OutputMode::Manual),);
    }
}
