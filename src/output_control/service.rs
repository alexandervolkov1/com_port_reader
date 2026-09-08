use std::{
    collections::HashMap,
    error::Error,
    fmt, io,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};

use crate::{
    acquisition::{
        AcquisitionError, InstrumentWriteCompletion, InstrumentWriteCompletionId,
        InstrumentWriteResult,
    },
    connection::ConnectionId,
    instrument::{
        ConnectedParameterAddress, InstrumentParameterAddress, InstrumentValue,
        InstrumentWriteRequest,
    },
    process_control::ControllerInstanceId,
    process_recorder::ProcessActionId,
    serial_connection::SerialConnectionRegistry,
    worker::ConnectionRouter,
};

use super::{
    AutomaticOutputIntent, AutomaticTransitionId, OutputArbiter, OutputArbiterError, OutputMode,
    OutputSource,
};

enum OutputCommand {
    RegisterController {
        target: ConnectedParameterAddress,
        controller: String,
        instance_id: ControllerInstanceId,
        safe_request: Option<InstrumentWriteRequest>,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    RollbackControllerRegistration {
        target: ConnectedParameterAddress,
        controller: String,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    ReleaseController {
        controller: String,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    #[cfg(test)]
    Mode {
        target: ConnectedParameterAddress,
        response_sender: Sender<Result<OutputMode, OutputArbiterError>>,
    },

    #[cfg(test)]
    LastApplied {
        target: ConnectedParameterAddress,
        response_sender: Sender<Result<Option<InstrumentValue>, OutputArbiterError>>,
    },

    #[cfg(test)]
    LastWriteFailure {
        target: ConnectedParameterAddress,
        response_sender: Sender<Result<Option<AcquisitionError>, OutputArbiterError>>,
    },

    ApplyAutomatic {
        intent: AutomaticOutputIntent,
        response_sender: Sender<Result<Receiver<InstrumentWriteResult>, OutputRequestError>>,
    },

    RequestAutomatic {
        controller: String,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    RollbackAutomaticRequest {
        controller: String,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    WriteInstrument {
        action_id: Option<ProcessActionId>,
        connection_id: ConnectionId,
        request: InstrumentWriteRequest,
        instrument_response_sender: Sender<InstrumentWriteResult>,
        response_sender: Sender<Result<(), OutputRequestError>>,
    },

    ApplySafe {
        controller: String,
        response_sender: Sender<Result<Receiver<InstrumentWriteResult>, OutputRequestError>>,
    },

    Shutdown,
}

#[derive(Clone, Copy, Debug)]
struct PendingWrite {
    target: ConnectedParameterAddress,
    instance_id: ControllerInstanceId,
    automatic_transition_id: Option<AutomaticTransitionId>,
}

struct OutputWriteContext<'a> {
    completion_sender: &'a Sender<InstrumentWriteCompletion>,
    pending_writes: HashMap<InstrumentWriteCompletionId, PendingWrite>,
    next_completion_id: u64,
    connection_router: &'a ConnectionRouter,
    serial_connections: &'a SerialConnectionRegistry,
}

impl<'a> OutputWriteContext<'a> {
    fn new(
        completion_sender: &'a Sender<InstrumentWriteCompletion>,
        connection_router: &'a ConnectionRouter,
        serial_connections: &'a SerialConnectionRegistry,
    ) -> Self {
        Self {
            completion_sender,
            pending_writes: HashMap::new(),
            next_completion_id: 1,
            connection_router,
            serial_connections,
        }
    }

    fn allocate_completion_id(&mut self) -> InstrumentWriteCompletionId {
        let id = InstrumentWriteCompletionId(self.next_completion_id);

        self.next_completion_id = self
            .next_completion_id
            .checked_add(1)
            .expect("instrument write completion id overflow");

        id
    }
}

pub(crate) struct OutputWriteResponse {
    receiver: Receiver<InstrumentWriteResult>,
}

impl OutputWriteResponse {
    fn new(receiver: Receiver<InstrumentWriteResult>) -> Self {
        Self { receiver }
    }

    pub(crate) fn recv(&self) -> Result<InstrumentValue, OutputWriteError> {
        self.receiver
            .recv()
            .map_err(|_| OutputWriteError::Disconnected)?
            .map_err(OutputWriteError::Instrument)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OutputWriteError {
    Instrument(AcquisitionError),
    Disconnected,
}

impl fmt::Display for OutputWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Instrument(error) => error.fmt(formatter),
            Self::Disconnected => {
                formatter.write_str("instrument write response channel is disconnected")
            }
        }
    }
}

impl Error for OutputWriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Instrument(error) => Some(error),
            Self::Disconnected => None,
        }
    }
}

#[derive(Clone)]
pub(crate) struct OutputHandle {
    command_sender: Sender<OutputCommand>,
}

impl OutputHandle {
    pub(crate) fn register_controller(
        &self,
        target: ConnectedParameterAddress,
        controller: impl Into<String>,
        instance_id: ControllerInstanceId,
        safe_request: Option<InstrumentWriteRequest>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::RegisterController {
                target,
                controller: controller.into(),
                instance_id,
                safe_request,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub(crate) fn rollback_controller_registration(
        &self,
        target: ConnectedParameterAddress,
        controller: impl Into<String>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::RollbackControllerRegistration {
                target,
                controller: controller.into(),
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub(crate) fn release_controller(
        &self,
        controller: impl Into<String>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::ReleaseController {
                controller: controller.into(),
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    #[cfg(test)]
    pub(crate) fn mode(
        &self,
        target: ConnectedParameterAddress,
    ) -> Result<OutputMode, OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::Mode {
                target,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    #[cfg(test)]
    pub(crate) fn last_applied(
        &self,
        target: ConnectedParameterAddress,
    ) -> Result<Option<InstrumentValue>, OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::LastApplied {
                target,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    #[cfg(test)]
    pub(crate) fn last_write_failure(
        &self,
        target: ConnectedParameterAddress,
    ) -> Result<Option<AcquisitionError>, OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::LastWriteFailure {
                target,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub(crate) fn apply_automatic(
        &self,
        intent: AutomaticOutputIntent,
    ) -> Result<OutputWriteResponse, OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::ApplyAutomatic {
                intent,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        let result = response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?;

        result.map(OutputWriteResponse::new)
    }

    pub(crate) fn write_instrument(
        &self,
        action_id: Option<ProcessActionId>,
        connection_id: ConnectionId,
        request: InstrumentWriteRequest,
        instrument_response_sender: Sender<InstrumentWriteResult>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::WriteInstrument {
                action_id,
                connection_id,
                request,
                instrument_response_sender,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
    }

    pub(crate) fn request_automatic(
        &self,
        controller: impl Into<String>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::RequestAutomatic {
                controller: controller.into(),
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub(crate) fn rollback_automatic_request(
        &self,
        controller: impl Into<String>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::RollbackAutomaticRequest {
                controller: controller.into(),
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub(crate) fn apply_safe(
        &self,
        controller: impl Into<String>,
    ) -> Result<OutputWriteResponse, OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::ApplySafe {
                controller: controller.into(),
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        let result = response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?;

        result.map(OutputWriteResponse::new)
    }
}

pub(crate) struct OutputService {
    handle: OutputHandle,
    thread: Option<JoinHandle<()>>,
}

impl OutputService {
    pub(crate) fn spawn(
        connection_router: ConnectionRouter,
        serial_connections: SerialConnectionRegistry,
    ) -> io::Result<Self> {
        let (command_sender, command_receiver) = unbounded();
        let (completion_sender, completion_receiver) = unbounded();

        let handle = OutputHandle { command_sender };

        let thread = thread::Builder::new()
            .name("output-control".to_owned())
            .spawn(move || {
                run(
                    command_receiver,
                    completion_sender,
                    completion_receiver,
                    connection_router,
                    serial_connections,
                );
            })?;

        Ok(Self {
            handle,
            thread: Some(thread),
        })
    }

    pub(crate) fn handle(&self) -> OutputHandle {
        self.handle.clone()
    }
}

impl Drop for OutputService {
    fn drop(&mut self) {
        let _ = self.handle.command_sender.send(OutputCommand::Shutdown);

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    command_receiver: Receiver<OutputCommand>,
    completion_sender: Sender<InstrumentWriteCompletion>,
    completion_receiver: Receiver<InstrumentWriteCompletion>,
    connection_router: ConnectionRouter,
    serial_connections: SerialConnectionRegistry,
) {
    let mut arbiter = OutputArbiter::new();

    let mut write_context =
        OutputWriteContext::new(&completion_sender, &connection_router, &serial_connections);

    'run: loop {
        crossbeam_channel::select! {
            recv(command_receiver) -> command => {
                let Ok(command) = command else {
                    break 'run;
                };

                match command {
                    OutputCommand::RegisterController {
                        target,
                        controller,
                        instance_id,
                        safe_request,
                        response_sender,
                    } => {
                        let result = arbiter.register_controller(
                            target,
                            controller,
                            instance_id,
                            safe_request,
                        );

                        let _ = response_sender.send(result);
                    }

                    OutputCommand::RollbackControllerRegistration {
                        target,
                        controller,
                        response_sender,
                    } => {
                        let result = arbiter.unregister_controller(target, &controller);

                        let _ = response_sender.send(result);
                    }

                    OutputCommand::ReleaseController {
                        controller,
                        response_sender,
                    } => {
                        let result = arbiter.release_controller(&controller);

                        let _ = response_sender.send(result);
                    }

                    #[cfg(test)]
                    OutputCommand::Mode {
                        target,
                        response_sender,
                    } => {
                        let result = arbiter.mode(target);

                        let _ = response_sender.send(result);
                    }

                    #[cfg(test)]
                    OutputCommand::LastApplied {
                        target,
                        response_sender,
                    } => {
                        let result = arbiter.last_applied(target);

                        let _ = response_sender.send(result);
                    }

                    #[cfg(test)]
                    OutputCommand::LastWriteFailure {
                        target,
                        response_sender,
                    } => {
                        let result = arbiter.last_write_failure(target);

                        let _ = response_sender.send(result);
                    }

                    OutputCommand::ApplyAutomatic {
                        intent,
                        response_sender,
                    } => {
                        let result = apply_automatic(
                            &mut arbiter,
                            intent,
                            &mut write_context,
                        );

                        let _ = response_sender.send(result);
                    }


                    OutputCommand::RequestAutomatic {
                        controller,
                        response_sender,
                    } => {
                        let result = arbiter.request_automatic(&controller);

                        let _ = response_sender.send(result);
                    }

                    OutputCommand::RollbackAutomaticRequest {
                        controller,
                        response_sender,
                    } => {
                        let result =
                            arbiter.rollback_automatic_request(
                                &controller,
                            );

                        let _ = response_sender.send(result);
                    }

                    OutputCommand::WriteInstrument {
                        action_id,
                        connection_id,
                        request,
                        instrument_response_sender,
                        response_sender,
                    } => {
                        let result = write_instrument(
                            &mut arbiter,
                            action_id,
                            connection_id,
                            request,
                            instrument_response_sender,
                            &mut write_context,
                        );

                        let _ = response_sender.send(result);
                    }

                    OutputCommand::ApplySafe {
                        controller,
                        response_sender,
                    } => {
                        let result = apply_safe(
                            &mut arbiter,
                            &controller,
                            &mut write_context,
                        );

                        let _ = response_sender.send(result);
                    }

                    OutputCommand::Shutdown => {
                        break 'run;
                    }
                }
            }

            recv(completion_receiver) -> completion => {
                let Ok(completion) = completion else {
                    break 'run;
                };

                handle_write_completion(
                    &mut arbiter,
                    &mut write_context.pending_writes,
                    completion,
                );
            }
        }
    }
}

fn handle_write_completion(
    arbiter: &mut OutputArbiter,
    pending_writes: &mut HashMap<InstrumentWriteCompletionId, PendingWrite>,
    completion: InstrumentWriteCompletion,
) {
    let Some(pending) = pending_writes.remove(&completion.id) else {
        return;
    };

    match completion.result {
        Ok(actual_value) => {
            let acknowledged = arbiter.acknowledge_write(
                pending.target,
                pending.instance_id,
                completion.id,
                actual_value,
            );

            if !acknowledged {
                return;
            }

            if let Some(transition_id) = pending.automatic_transition_id {
                let _ = arbiter.complete_automatic_transition(
                    pending.target,
                    pending.instance_id,
                    transition_id,
                );
            }
        }

        Err(error) => {
            let _ = arbiter.acknowledge_write_failure(
                pending.target,
                pending.instance_id,
                completion.id,
                error,
            );
        }
    }
}

fn apply_automatic(
    arbiter: &mut OutputArbiter,
    intent: AutomaticOutputIntent,
    write_context: &mut OutputWriteContext<'_>,
) -> Result<Receiver<InstrumentWriteResult>, OutputRequestError> {
    let (target, controller, instance_id, request) = intent.into_parts();

    arbiter.authorize(target, &OutputSource::controller(controller, instance_id))?;

    let automatic_transition_id = arbiter.automatic_transition_id(target)?;

    validate_request_target(target, request)?;

    dispatch_tracked_write(
        target,
        request,
        instance_id,
        automatic_transition_id,
        write_context,
    )
}

fn apply_safe(
    arbiter: &mut OutputArbiter,
    controller: &str,
    write_context: &mut OutputWriteContext<'_>,
) -> Result<Receiver<InstrumentWriteResult>, OutputRequestError> {
    let (target, request) = arbiter.safe_output(controller)?;

    let instance_id = arbiter.controller_instance_id(target)?;

    arbiter.authorize(target, &OutputSource::Safety)?;

    validate_request_target(target, request)?;

    let response_receiver =
        dispatch_tracked_write(target, request, instance_id, None, write_context)?;

    arbiter.set_mode(target, OutputMode::Manual)?;

    Ok(response_receiver)
}

fn write_instrument(
    arbiter: &mut OutputArbiter,
    action_id: Option<ProcessActionId>,
    connection_id: ConnectionId,
    request: InstrumentWriteRequest,
    instrument_response_sender: Sender<InstrumentWriteResult>,
    write_context: &mut OutputWriteContext<'_>,
) -> Result<(), OutputRequestError> {
    let target = ConnectedParameterAddress::new(connection_id, request.parameter_address());

    if arbiter.contains(target) {
        let instance_id = arbiter.controller_instance_id(target)?;

        dispatch_tracked_write_to_sender(
            target,
            request,
            instance_id,
            None,
            action_id,
            instrument_response_sender,
            write_context,
        )?;

        arbiter.set_mode(target, OutputMode::Manual)?;
    } else {
        dispatch_write_to_sender(
            target,
            request,
            action_id,
            instrument_response_sender,
            write_context.connection_router,
            write_context.serial_connections,
        )?;
    }

    Ok(())
}

fn validate_request_target(
    target: ConnectedParameterAddress,
    request: InstrumentWriteRequest,
) -> Result<(), OutputRequestError> {
    let expected = target.parameter();
    let actual = request.parameter_address();

    if actual != expected {
        return Err(OutputRequestError::RequestTargetMismatch { expected, actual });
    }

    Ok(())
}

fn dispatch_tracked_write(
    target: ConnectedParameterAddress,
    request: InstrumentWriteRequest,
    instance_id: ControllerInstanceId,
    automatic_transition_id: Option<AutomaticTransitionId>,
    write_context: &mut OutputWriteContext<'_>,
) -> Result<Receiver<InstrumentWriteResult>, OutputRequestError> {
    let (response_sender, response_receiver) = bounded(1);

    dispatch_tracked_write_to_sender(
        target,
        request,
        instance_id,
        automatic_transition_id,
        None,
        response_sender,
        write_context,
    )?;

    Ok(response_receiver)
}

fn dispatch_tracked_write_to_sender(
    target: ConnectedParameterAddress,
    request: InstrumentWriteRequest,
    instance_id: ControllerInstanceId,
    automatic_transition_id: Option<AutomaticTransitionId>,
    action_id: Option<ProcessActionId>,
    response_sender: Sender<InstrumentWriteResult>,
    write_context: &mut OutputWriteContext<'_>,
) -> Result<(), OutputRequestError> {
    let connection_id = target.connection_id();

    let worker = write_context
        .connection_router
        .handle(connection_id)
        .ok_or_else(|| {
            OutputRequestError::Transport(format!(
                "connection {connection_id} \
                     does not have a registered worker",
            ))
        })?;

    let serial_config_store = write_context
        .serial_connections
        .store(connection_id)
        .ok_or_else(|| {
            OutputRequestError::Transport(format!(
                "connection {connection_id} \
                         does not have a serial \
                         configuration store",
            ))
        })?;

    let serial_config = serial_config_store.snapshot().ok_or_else(|| {
        OutputRequestError::Transport(format!(
            "connection {connection_id} \
                         does not have a selected \
                         COM port",
        ))
    })?;

    let completion_id = write_context.allocate_completion_id();

    worker
        .write_instrument_quiet_tracked(
            action_id,
            serial_config.port_name().to_owned(),
            request,
            completion_id,
            write_context.completion_sender.clone(),
            response_sender,
        )
        .map_err(|error| {
            OutputRequestError::Transport(format!(
                "cannot enqueue instrument \
                     write for connection \
                     {connection_id}: {error}",
            ))
        })?;

    write_context.pending_writes.insert(
        completion_id,
        PendingWrite {
            target,
            instance_id,
            automatic_transition_id,
        },
    );

    Ok(())
}

fn dispatch_write_to_sender(
    target: ConnectedParameterAddress,
    request: InstrumentWriteRequest,
    action_id: Option<ProcessActionId>,
    response_sender: Sender<InstrumentWriteResult>,
    connection_router: &ConnectionRouter,
    serial_connections: &SerialConnectionRegistry,
) -> Result<(), OutputRequestError> {
    let connection_id = target.connection_id();

    let worker = connection_router.handle(connection_id).ok_or_else(|| {
        OutputRequestError::Transport(format!(
            "connection {connection_id} does not have a registered worker",
        ))
    })?;

    let serial_config_store = serial_connections.store(connection_id).ok_or_else(|| {
        OutputRequestError::Transport(format!(
            "connection {connection_id} does not have a serial configuration store",
        ))
    })?;

    let serial_config = serial_config_store.snapshot().ok_or_else(|| {
        OutputRequestError::Transport(format!(
            "connection {connection_id} does not have a selected COM port",
        ))
    })?;

    worker
        .write_instrument_quiet(
            action_id,
            serial_config.port_name().to_owned(),
            request,
            response_sender,
        )
        .map_err(|error| {
            OutputRequestError::Transport(format!(
                "cannot enqueue instrument write for connection {connection_id}: {error}",
            ))
        })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OutputRequestError {
    Arbiter(OutputArbiterError),
    RequestTargetMismatch {
        expected: InstrumentParameterAddress,
        actual: InstrumentParameterAddress,
    },
    Transport(String),
    Disconnected,
}

impl fmt::Display for OutputRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arbiter(error) => error.fmt(formatter),

            Self::RequestTargetMismatch { expected, actual } => {
                write!(
                    formatter,
                    "Output request targets {actual:?}, but output ownership belongs to {expected:?}",
                )
            }

            Self::Transport(message) => formatter.write_str(message),

            Self::Disconnected => formatter.write_str("Output control service is disconnected"),
        }
    }
}

impl Error for OutputRequestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Arbiter(error) => Some(error),
            Self::RequestTargetMismatch { .. } | Self::Transport(_) | Self::Disconnected => None,
        }
    }
}

impl From<OutputArbiterError> for OutputRequestError {
    fn from(error: OutputArbiterError) -> Self {
        Self::Arbiter(error)
    }
}

#[cfg(test)]
mod tests {
    use crossbeam_channel::{bounded, unbounded};

    use serialport::{DataBits, FlowControl, Parity, StopBits};

    use crate::{
        acquisition::{AcquisitionError, InstrumentWriteCompletion},
        connection::ConnectionId,
        instrument::{
            ConnectedParameterAddress, InstrumentParameterAddress, InstrumentValue,
            InstrumentWriteRequest,
            virtual_instrument::{VirtualInstrumentId, VirtualParameterId},
        },
        process_control::ControllerInstanceId,
        serial_connection::{SerialConnectionRegistry, SerialPortConfig},
        worker::{ConnectionCommand, ConnectionRouter, WorkerCommand, WorkerHandle},
    };

    use super::{
        AutomaticOutputIntent, OutputArbiterError, OutputMode, OutputRequestError, OutputService,
    };

    fn target() -> ConnectedParameterAddress {
        ConnectedParameterAddress::new(
            ConnectionId::new(2),
            InstrumentParameterAddress::virtual_instrument(
                VirtualInstrumentId::new(7),
                VirtualParameterId::new(4),
            ),
        )
    }

    fn instance_id() -> ControllerInstanceId {
        ControllerInstanceId::for_test(1)
    }

    fn other_instance_id() -> ControllerInstanceId {
        ControllerInstanceId::for_test(2)
    }

    fn service() -> OutputService {
        OutputService::spawn(ConnectionRouter::default(), SerialConnectionRegistry::new()).unwrap()
    }

    fn serial_config(port_name: &str) -> SerialPortConfig {
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
    fn owns_registered_output_state() {
        let service = service();
        let handle = service.handle();
        let target = target();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Automatic));
    }

    #[test]
    fn rejects_duplicate_registration() {
        let service = service();
        let handle = service.handle();
        let target = target();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        assert_eq!(
            handle.register_controller(target, "other", other_instance_id(), None),
            Err(OutputRequestError::Arbiter(
                OutputArbiterError::AlreadyRegistered {
                    controller: "heater".to_owned(),
                },
            )),
        );
    }

    #[test]
    fn rolls_back_controller_registration() {
        let service = service();
        let handle = service.handle();
        let target = target();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        handle
            .rollback_controller_registration(target, "heater")
            .unwrap();

        handle
            .register_controller(target, "other", other_instance_id(), None)
            .unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Automatic));
    }

    #[test]
    fn reports_disconnected_service() {
        let handle = {
            let service = service();
            service.handle()
        };

        assert_eq!(handle.mode(target()), Err(OutputRequestError::Disconnected),);
    }

    #[test]
    fn rejects_request_for_another_parameter() {
        let service = service();
        let handle = service.handle();
        let target = target();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(99),
            InstrumentValue::Number(42.0),
        );

        assert!(matches!(
            handle.apply_automatic(AutomaticOutputIntent::new(
                target,
                "heater",
                instance_id(),
                request,
            )),
            Err(OutputRequestError::RequestTargetMismatch { .. }),
        ));
    }

    #[test]
    fn routes_automatic_output_to_connection_worker() {
        let target = target();
        let connection_id = target.connection_id();

        let serial_connections = SerialConnectionRegistry::new();
        serial_connections
            .register(connection_id)
            .unwrap()
            .set(Some(serial_config("COM9")));

        let connection_router = ConnectionRouter::default();
        let (command_sender, command_receiver) = unbounded();
        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let service = OutputService::spawn(connection_router, serial_connections).unwrap();
        let handle = service.handle();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(42.5),
        );

        let _write_result = handle
            .apply_automatic(AutomaticOutputIntent::new(
                target,
                "heater",
                instance_id(),
                request,
            ))
            .unwrap();

        let command = command_receiver.try_recv().unwrap();

        let WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
            port_name,
            request: received_request,
            emit_event,
            completion: Some(_),
            ..
        }) = command
        else {
            panic!("expected tracked instrument write command");
        };

        assert_eq!(port_name, "COM9");
        assert_eq!(received_request, request);
        assert!(!emit_event);
    }

    #[test]
    fn enters_manual_after_enqueuing_write() {
        let target = target();
        let connection_id = target.connection_id();

        let serial_connections = SerialConnectionRegistry::new();
        serial_connections
            .register(connection_id)
            .unwrap()
            .set(Some(serial_config("COM9")));

        let connection_router = ConnectionRouter::default();
        let (command_sender, command_receiver) = unbounded();
        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let service = OutputService::spawn(connection_router, serial_connections).unwrap();
        let handle = service.handle();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(35.0),
        );

        let (response_sender, _response_receiver) = bounded(1);

        handle
            .write_instrument(None, connection_id, request, response_sender)
            .unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Manual));

        let command = command_receiver.try_recv().unwrap();

        let WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
            request: received_request,
            completion: Some(_),
            ..
        }) = command
        else {
            panic!("expected tracked instrument write command");
        };

        assert_eq!(received_request, request);
    }

    #[test]
    fn keeps_automatic_mode_when_manual_write_cannot_be_enqueued() {
        let service = service();
        let handle = service.handle();
        let target = target();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(35.0),
        );

        let (response_sender, _response_receiver) = bounded(1);

        assert!(matches!(
            handle.write_instrument(None, target.connection_id(), request, response_sender,),
            Err(OutputRequestError::Transport(_)),
        ));

        assert_eq!(handle.mode(target), Ok(OutputMode::Automatic));
    }

    #[test]
    fn rejects_automatic_output_in_manual_mode() {
        let target = target();
        let connection_id = target.connection_id();

        let serial_connections = SerialConnectionRegistry::new();
        serial_connections
            .register(connection_id)
            .unwrap()
            .set(Some(serial_config("COM9")));

        let connection_router = ConnectionRouter::default();
        let (command_sender, _command_receiver) = unbounded();
        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let service = OutputService::spawn(connection_router, serial_connections).unwrap();
        let handle = service.handle();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(35.0),
        );

        let (response_sender, _response_receiver) = bounded(1);

        handle
            .write_instrument(None, connection_id, request, response_sender)
            .unwrap();

        assert!(matches!(
            handle.apply_automatic(AutomaticOutputIntent::new(
                target,
                "heater",
                instance_id(),
                request,
            )),
            Err(OutputRequestError::Arbiter(
                OutputArbiterError::SourceNotAllowed {
                    mode: OutputMode::Manual,
                    ..
                },
            )),
        ));
    }

    #[test]
    fn routes_uncontrolled_instrument_write_without_changing_ownership() {
        let target = target();
        let connection_id = target.connection_id();

        let serial_connections = SerialConnectionRegistry::new();
        serial_connections
            .register(connection_id)
            .unwrap()
            .set(Some(serial_config("COM9")));

        let connection_router = ConnectionRouter::default();
        let (command_sender, command_receiver) = unbounded();
        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let service = OutputService::spawn(connection_router, serial_connections).unwrap();
        let handle = service.handle();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(25.0),
        );

        let (instrument_response_sender, _instrument_response_receiver) = bounded(1);

        handle
            .write_instrument(None, connection_id, request, instrument_response_sender)
            .unwrap();

        assert_eq!(
            handle.mode(target),
            Err(OutputRequestError::Arbiter(
                OutputArbiterError::NotRegistered
            )),
        );

        let command = command_receiver.try_recv().unwrap();

        let WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
            request: received_request,
            completion: None,
            ..
        }) = command
        else {
            panic!("expected untracked instrument write command");
        };

        assert_eq!(received_request, request);
    }

    #[test]
    fn explicit_write_to_controlled_output_enters_manual_mode() {
        let target = target();
        let connection_id = target.connection_id();

        let serial_connections = SerialConnectionRegistry::new();
        serial_connections
            .register(connection_id)
            .unwrap()
            .set(Some(serial_config("COM9")));

        let connection_router = ConnectionRouter::default();
        let (command_sender, command_receiver) = unbounded();
        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let service = OutputService::spawn(connection_router, serial_connections).unwrap();
        let handle = service.handle();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(25.0),
        );

        let (instrument_response_sender, _instrument_response_receiver) = bounded(1);

        handle
            .write_instrument(None, connection_id, request, instrument_response_sender)
            .unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Manual));

        let command = command_receiver.try_recv().unwrap();

        assert!(matches!(
            command,
            WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
                completion: Some(_),
                ..
            }),
        ));
    }

    #[test]
    fn completes_automatic_takeover_only_after_hardware_acknowledgement() {
        let target = target();
        let connection_id = target.connection_id();

        let serial_connections = SerialConnectionRegistry::new();
        serial_connections
            .register(connection_id)
            .unwrap()
            .set(Some(serial_config("COM9")));

        let connection_router = ConnectionRouter::default();
        let (command_sender, command_receiver) = unbounded();
        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let service = OutputService::spawn(connection_router, serial_connections).unwrap();
        let handle = service.handle();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(35.0),
        );

        let (response_sender, _response_receiver) = bounded(1);

        handle
            .write_instrument(None, connection_id, request, response_sender)
            .unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Manual));

        let _manual_command = command_receiver.try_recv().unwrap();

        handle.request_automatic("heater").unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::AutomaticPending));

        let _ = handle
            .apply_automatic(AutomaticOutputIntent::new(
                target,
                "heater",
                instance_id(),
                request,
            ))
            .unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::AutomaticPending));

        let command = command_receiver.try_recv().unwrap();

        let WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
            completion: Some((completion_id, completion_sender)),
            ..
        }) = command
        else {
            panic!("expected tracked automatic write");
        };

        completion_sender
            .send(InstrumentWriteCompletion {
                id: completion_id,
                result: Ok(InstrumentValue::Number(35.0)),
            })
            .unwrap();

        let mut mode = OutputMode::AutomaticPending;

        for _ in 0..100 {
            mode = handle.mode(target).unwrap();

            if mode == OutputMode::Automatic {
                break;
            }

            std::thread::yield_now();
        }

        assert_eq!(mode, OutputMode::Automatic);
    }

    #[test]
    fn applies_configured_safe_output_and_enters_manual_mode() {
        let target = target();
        let connection_id = target.connection_id();

        let serial_connections = SerialConnectionRegistry::new();
        serial_connections
            .register(connection_id)
            .unwrap()
            .set(Some(serial_config("COM9")));

        let connection_router = ConnectionRouter::default();
        let (command_sender, command_receiver) = unbounded();
        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let service = OutputService::spawn(connection_router, serial_connections).unwrap();
        let handle = service.handle();

        let safe_request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(0.0),
        );

        handle
            .register_controller(target, "heater", instance_id(), Some(safe_request))
            .unwrap();

        let _response = handle.apply_safe("heater").unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Manual));

        let command = command_receiver.try_recv().unwrap();

        let WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
            request,
            completion: Some(_),
            ..
        }) = command
        else {
            panic!("expected tracked instrument write command");
        };

        assert_eq!(request, safe_request);
    }

    #[test]
    fn rejects_safe_output_without_configuration() {
        let service = service();
        let handle = service.handle();
        let target = target();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        assert!(matches!(
            handle.apply_safe("heater"),
            Err(OutputRequestError::Arbiter(
                OutputArbiterError::SafeOutputNotConfigured(controller),
            )) if controller == "heater"
        ));

        assert_eq!(handle.mode(target), Ok(OutputMode::Automatic));
    }

    #[test]
    fn stale_controller_instance_is_not_dispatched() {
        let target = target();
        let connection_id = target.connection_id();

        let serial_connections = SerialConnectionRegistry::new();
        serial_connections
            .register(connection_id)
            .unwrap()
            .set(Some(serial_config("COM9")));

        let connection_router = ConnectionRouter::default();
        let (command_sender, command_receiver) = unbounded();
        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let service = OutputService::spawn(connection_router, serial_connections).unwrap();
        let handle = service.handle();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        let stale_instance = other_instance_id();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(42.5),
        );

        let result = handle.apply_automatic(AutomaticOutputIntent::new(
            target,
            "heater",
            stale_instance,
            request,
        ));

        assert!(matches!(
            result,
            Err(OutputRequestError::Arbiter(
                OutputArbiterError::ControllerInstanceMismatch {
                    controller,
                    expected,
                    actual,
                },
            )) if controller == "heater"
                && expected == instance_id()
                && actual == stale_instance
        ));

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn rejects_output_from_replaced_controller_instance() {
        let target = target();
        let connection_id = target.connection_id();

        let serial_connections = SerialConnectionRegistry::new();
        serial_connections
            .register(connection_id)
            .unwrap()
            .set(Some(serial_config("COM9")));

        let connection_router = ConnectionRouter::default();
        let (command_sender, command_receiver) = unbounded();
        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let service = OutputService::spawn(connection_router, serial_connections).unwrap();
        let handle = service.handle();

        let old_instance = instance_id();
        let new_instance = other_instance_id();

        let safe_request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(0.0),
        );

        let automatic_request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(42.5),
        );

        handle
            .register_controller(target, "heater", old_instance, Some(safe_request))
            .unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Automatic));

        let _safe_response = handle.apply_safe("heater").unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Manual));

        let safe_command = command_receiver.try_recv().unwrap();

        let WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
            request: received_safe_request,
            ..
        }) = safe_command
        else {
            panic!("expected safe instrument write");
        };

        assert_eq!(received_safe_request, safe_request);

        handle.release_controller("heater").unwrap();

        assert_eq!(
            handle.mode(target),
            Err(OutputRequestError::Arbiter(
                OutputArbiterError::NotRegistered
            )),
        );

        handle
            .register_controller(target, "heater", new_instance, Some(safe_request))
            .unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Automatic));

        let stale_result = handle.apply_automatic(AutomaticOutputIntent::new(
            target,
            "heater",
            old_instance,
            automatic_request,
        ));

        assert!(matches!(
            stale_result,
            Err(OutputRequestError::Arbiter(
                OutputArbiterError::ControllerInstanceMismatch {
                    controller,
                    expected,
                    actual,
                },
            )) if controller == "heater"
                && expected == new_instance
                && actual == old_instance
        ));

        assert!(command_receiver.try_recv().is_err());

        let _current_response = handle
            .apply_automatic(AutomaticOutputIntent::new(
                target,
                "heater",
                new_instance,
                automatic_request,
            ))
            .unwrap();

        let current_command = command_receiver.try_recv().unwrap();

        let WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
            request: received_request,
            ..
        }) = current_command
        else {
            panic!("expected current controller instrument write");
        };

        assert_eq!(received_request, automatic_request);
    }

    #[test]
    fn records_hardware_confirmed_automatic_output() {
        let target = target();
        let connection_id = target.connection_id();

        let serial_connections = SerialConnectionRegistry::new();
        serial_connections
            .register(connection_id)
            .unwrap()
            .set(Some(serial_config("COM9")));

        let connection_router = ConnectionRouter::default();
        let (command_sender, command_receiver) = unbounded();
        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let service = OutputService::spawn(connection_router, serial_connections).unwrap();
        let handle = service.handle();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(42.5),
        );

        let _response = handle
            .apply_automatic(AutomaticOutputIntent::new(
                target,
                "heater",
                instance_id(),
                request,
            ))
            .unwrap();

        let command = command_receiver.try_recv().unwrap();

        let WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
            completion: Some((completion_id, completion_sender)),
            ..
        }) = command
        else {
            panic!("expected tracked instrument write");
        };

        completion_sender
            .send(InstrumentWriteCompletion {
                id: completion_id,
                result: Ok(InstrumentValue::Number(41.75)),
            })
            .unwrap();

        let mut confirmed = None;

        for _ in 0..100 {
            confirmed = handle.last_applied(target).unwrap();

            if confirmed.is_some() {
                break;
            }

            std::thread::yield_now();
        }

        assert_eq!(confirmed, Some(InstrumentValue::Number(41.75)));
    }
    #[test]
    fn records_hardware_write_failure() {
        let target = target();
        let connection_id = target.connection_id();

        let serial_connections = SerialConnectionRegistry::new();
        serial_connections
            .register(connection_id)
            .unwrap()
            .set(Some(serial_config("COM9")));

        let connection_router = ConnectionRouter::default();
        let (command_sender, command_receiver) = unbounded();
        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let service = OutputService::spawn(connection_router, serial_connections).unwrap();
        let handle = service.handle();

        handle
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(42.5),
        );

        let _response = handle
            .apply_automatic(AutomaticOutputIntent::new(
                target,
                "heater",
                instance_id(),
                request,
            ))
            .unwrap();

        let command = command_receiver.try_recv().unwrap();

        let WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
            completion: Some((completion_id, completion_sender)),
            ..
        }) = command
        else {
            panic!("expected tracked instrument write");
        };

        let error = AcquisitionError::from("write failed");

        completion_sender
            .send(InstrumentWriteCompletion {
                id: completion_id,
                result: Err(error.clone()),
            })
            .unwrap();

        let mut failure = None;

        for _ in 0..100 {
            failure = handle.last_write_failure(target).unwrap();

            if failure.is_some() {
                break;
            }

            std::thread::yield_now();
        }

        assert_eq!(failure, Some(error));
        assert_eq!(handle.last_applied(target), Ok(None));
    }
}
