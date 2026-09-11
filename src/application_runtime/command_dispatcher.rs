use std::collections::{BTreeSet, HashMap};

use crossbeam_channel::Receiver;

use super::{
    acquisition_command_handler::AcquisitionCommandHandler,
    acquisition_controller::AcquisitionController,
    controller_command_handler::ControllerCommandHandler,
    device_emulator_service::DeviceEmulatorService,
    emulator_command_handler::EmulatorCommandHandler,
    instrument_command_handler::InstrumentCommandHandler,
    serial_command_handler::SerialCommandHandler, series_command_handler::SeriesCommandHandler,
};
use crate::{
    app_log::LogHandle,
    application_definition::ApplicationDefinition,
    connection::ConnectionId,
    data::{SeriesId, SeriesStore},
    output_control::OutputHandle,
    process_recorder::{
        ProcessActionContext, ProcessActionId, ProcessActionResult, ProcessRecorder,
    },
    serial_connection::SerialConnectionRegistry,
    signal_processing::ProcessingHandle,
    user_command::UserCommand,
    worker::{ConnectionRouter, ConnectionWorkerEvent, WorkerEvent},
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
pub(super) enum AcquisitionActionKind {
    Start,
    Stop,
}

pub(super) struct PendingAcquisitionAction {
    pub(super) kind: AcquisitionActionKind,
    pub(super) expected_responses: usize,
    pub(super) responded_connections: BTreeSet<ConnectionId>,
    pub(super) rollback_connections: BTreeSet<ConnectionId>,
}

#[derive(Debug, PartialEq, Eq)]
enum AcquisitionActionEventOutcome {
    Ignored,
    Pending,

    Applied {
        action_id: ProcessActionId,
    },

    Failed {
        action_id: ProcessActionId,
        rollback_connections: BTreeSet<ConnectionId>,
    },
}

fn update_pending_acquisition_action(
    pending_actions: &mut HashMap<ProcessActionId, PendingAcquisitionAction>,
    connection_event: &ConnectionWorkerEvent,
) -> AcquisitionActionEventOutcome {
    let Some(action_id) = connection_event.action_id() else {
        return AcquisitionActionEventOutcome::Ignored;
    };

    let connection_id = connection_event.connection_id();

    let event = connection_event.event();

    let completed = {
        let Some(pending) = pending_actions.get_mut(&action_id) else {
            return AcquisitionActionEventOutcome::Ignored;
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
            return AcquisitionActionEventOutcome::Ignored;
        }

        if !pending.responded_connections.insert(connection_id) {
            return AcquisitionActionEventOutcome::Ignored;
        }

        pending.responded_connections.len() == pending.expected_responses
    };

    let failed = matches!(
        event,
        WorkerEvent::AcquisitionStartFailed(_) | WorkerEvent::AcquisitionStopFailed(_)
    );

    if failed {
        let rollback_connections = pending_actions
            .get(&action_id)
            .filter(|pending| pending.kind == AcquisitionActionKind::Start)
            .map(|pending| pending.rollback_connections.clone())
            .unwrap_or_default();

        pending_actions.remove(&action_id);

        return AcquisitionActionEventOutcome::Failed {
            action_id,
            rollback_connections,
        };
    }

    if completed {
        pending_actions.remove(&action_id);

        AcquisitionActionEventOutcome::Applied { action_id }
    } else {
        AcquisitionActionEventOutcome::Pending
    }
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

pub(super) fn rollback_acquisition_connections(
    connections: &ConnectionRouter,
    log: &LogHandle,
    connection_ids: &BTreeSet<ConnectionId>,
) {
    for &connection_id in connection_ids {
        let Some(worker) = connections.handle(connection_id) else {
            log.error(format!(
                "Failed to roll back acquisition \
                 start for connection \
                 {connection_id}: worker is not \
                 registered.",
            ));

            continue;
        };

        if let Err(error) = worker.stop(None) {
            log.error(format!(
                "Failed to roll back acquisition \
                 start for connection \
                 {connection_id}: {error}",
            ));
        }
    }
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
        rollback_acquisition_connections(&self.connections, &self.log, connection_ids);
    }

    fn handle_acquisition_action_event(&mut self, connection_event: &ConnectionWorkerEvent) {
        match update_pending_acquisition_action(
            &mut self.pending_acquisition_actions,
            connection_event,
        ) {
            AcquisitionActionEventOutcome::Ignored | AcquisitionActionEventOutcome::Pending => {}

            AcquisitionActionEventOutcome::Applied { action_id } => {
                self.process_recorder
                    .record_action_applied(action_id, None, None);
            }

            AcquisitionActionEventOutcome::Failed {
                action_id,
                rollback_connections,
            } => {
                let error = self.format_worker_event(connection_event);

                if !rollback_connections.is_empty() {
                    self.rollback_acquisition_start(&rollback_connections);
                }

                self.process_recorder.record_action_failed(action_id, error);
            }
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

    pub fn execute(
        &mut self,
        command: UserCommand,
        action_context: Option<ProcessActionContext>,
        controls: &mut AcquisitionController,
        device_emulator: &mut DeviceEmulatorService,
    ) {
        match command {
            UserCommand::Acquisition(command) => {
                AcquisitionCommandHandler::new(
                    controls,
                    &self.connections,
                    &self.log,
                    &self.process_recorder,
                    &mut self.pending_acquisition_actions,
                )
                .execute(command, action_context);
            }

            UserCommand::Emulator(command) => {
                EmulatorCommandHandler::new(
                    &self.application_definition,
                    &self.serial_connections,
                    &self.process_recorder,
                    device_emulator,
                )
                .execute(command, action_context);
            }

            UserCommand::Instrument(command) => {
                InstrumentCommandHandler::new(
                    &self.connections,
                    &self.serial_connections,
                    &self.output_control,
                    &self.process_recorder,
                )
                .execute(command, action_context);
            }

            UserCommand::Serial(command) => {
                SerialCommandHandler::new(
                    &self.connections,
                    &self.serial_connections,
                    &self.process_recorder,
                )
                .execute(command, action_context);
            }

            UserCommand::Series(command) => {
                SeriesCommandHandler::new(
                    &self.connections,
                    &self.series,
                    &self.processing,
                    &self.output_control,
                    &self.process_recorder,
                    &self.log,
                    self.application_definition.plot_layout(),
                )
                .execute(command, action_context);
            }

            UserCommand::Controller(command) => {
                ControllerCommandHandler::new(
                    &self.series,
                    &self.processing,
                    &self.output_control,
                    &self.process_recorder,
                    &self.log,
                    self.application_definition.plot_layout(),
                )
                .execute(command, action_context);
            }

            UserCommand::Scenario(_) => {
                unreachable!("scenario commands are handled by ApplicationRuntime")
            }

            UserCommand::Log { message } => {
                self.log.info(message);
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
    use std::collections::{BTreeSet, HashMap};

    use crossbeam_channel::{bounded, unbounded};
    use serialport::{DataBits, FlowControl, Parity, StopBits};

    use super::{
        AcquisitionActionEventOutcome, AcquisitionActionKind, PendingAcquisitionAction,
        rollback_acquisition_connections, update_pending_acquisition_action,
        worker_event_should_be_logged,
    };
    use crate::{
        acquisition::AcquisitionError,
        app_log::LogModel,
        application_runtime::controller_command_handler::{
            pause_controller_safely, resume_controller_safely,
        },
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
        process_recorder::{ProcessActionId, ProcessRecorder},
        serial_connection::{SerialConnectionRegistry, SerialPortConfig},
        signal_processing::ProcessingService,
        user_command::ResumeControllerError,
        worker::{
            ConnectionRouter, ConnectionWorkerEvent, WorkerCommand, WorkerEvent, WorkerHandle,
        },
    };

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

    #[test]
    fn rolls_back_only_connections_selected_for_start() {
        let connections = ConnectionRouter::default();

        let previously_running_id = ConnectionId::new(2);

        let previously_stopped_id = ConnectionId::new(3);

        let (running_sender, running_receiver) = unbounded();

        let (stopped_sender, stopped_receiver) = unbounded();

        connections.insert(WorkerHandle::new(previously_running_id, running_sender));

        connections.insert(WorkerHandle::new(previously_stopped_id, stopped_sender));

        let rollback_connections = BTreeSet::from([previously_stopped_id]);

        let recorder = ProcessRecorder::default();

        let (_log_model, log) = LogModel::new(std::env::temp_dir(), recorder);

        rollback_acquisition_connections(&connections, &log, &rollback_connections);

        assert!(matches!(
            stopped_receiver.try_recv(),
            Ok(WorkerCommand::Stop { action_id: None }),
        ));

        assert!(running_receiver.try_recv().is_err(),);
    }

    #[test]
    fn acquisition_start_failure_returns_saved_rollback_connections() {
        let action_id = ProcessActionId::new(1);

        let failed_connection = ConnectionId::new(3);

        let previously_running = ConnectionId::new(2);

        let previously_stopped = ConnectionId::new(3);

        let rollback_connections = BTreeSet::from([previously_stopped]);

        let mut pending_actions = HashMap::from([(
            action_id,
            PendingAcquisitionAction {
                kind: AcquisitionActionKind::Start,

                expected_responses: 2,

                responded_connections: BTreeSet::new(),

                rollback_connections: rollback_connections.clone(),
            },
        )]);

        let event = ConnectionWorkerEvent::new(
            failed_connection,
            Some(action_id),
            WorkerEvent::AcquisitionStartFailed(AcquisitionError::from("test start failure")),
        );

        let outcome = update_pending_acquisition_action(&mut pending_actions, &event);

        assert_eq!(
            outcome,
            AcquisitionActionEventOutcome::Failed {
                action_id,
                rollback_connections: rollback_connections.clone(),
            },
        );

        assert!(!pending_actions.contains_key(&action_id),);

        assert!(rollback_connections.contains(&previously_stopped),);

        assert!(!rollback_connections.contains(&previously_running),);
    }

    #[test]
    fn acquisition_start_applies_only_after_all_workers_respond() {
        let action_id = ProcessActionId::new(1);

        let first_connection = ConnectionId::new(2);

        let second_connection = ConnectionId::new(3);

        let mut pending_actions = HashMap::from([(
            action_id,
            PendingAcquisitionAction {
                kind: AcquisitionActionKind::Start,

                expected_responses: 2,

                responded_connections: BTreeSet::new(),

                rollback_connections: BTreeSet::from([second_connection]),
            },
        )]);

        let first_event = ConnectionWorkerEvent::new(
            first_connection,
            Some(action_id),
            WorkerEvent::AcquisitionStarted,
        );

        let first_outcome = update_pending_acquisition_action(&mut pending_actions, &first_event);

        assert_eq!(first_outcome, AcquisitionActionEventOutcome::Pending,);

        assert!(pending_actions.contains_key(&action_id),);

        let second_event = ConnectionWorkerEvent::new(
            second_connection,
            Some(action_id),
            WorkerEvent::AcquisitionStarted,
        );

        let second_outcome = update_pending_acquisition_action(&mut pending_actions, &second_event);

        assert_eq!(
            second_outcome,
            AcquisitionActionEventOutcome::Applied { action_id },
        );

        assert!(!pending_actions.contains_key(&action_id),);
    }
}
