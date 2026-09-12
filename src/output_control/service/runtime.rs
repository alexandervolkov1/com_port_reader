use std::collections::HashMap;

use crossbeam_channel::{Receiver, Sender, bounded};

use super::{command::OutputCommand, error::OutputRequestError};
use crate::{
    acquisition::{InstrumentWriteCompletion, InstrumentWriteCompletionId, InstrumentWriteResult},
    connection::ConnectionId,
    instrument::{ConnectedParameterAddress, InstrumentWriteRequest},
    output_control::{
        AutomaticOutputIntent, AutomaticTransitionId, OutputArbiter, OutputMode, OutputSource,
    },
    process_control::ControllerInstanceId,
    process_recorder::ProcessActionId,
    serial_connection::SerialConnectionRegistry,
    worker::ConnectionRouter,
};

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

pub(super) fn run(
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

    let port_name = write_context
        .serial_connections
        .store(connection_id)
        .and_then(|store| store.snapshot())
        .map(|config| config.port_name().to_owned())
        .unwrap_or_default();

    require_serial_port_for_metakon(connection_id, request, &port_name)?;

    let completion_id = write_context.allocate_completion_id();

    worker
        .write_instrument_quiet_tracked(
            action_id,
            port_name,
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

    let port_name = serial_connections
        .store(connection_id)
        .and_then(|store| store.snapshot())
        .map(|config| config.port_name().to_owned())
        .unwrap_or_default();

    require_serial_port_for_metakon(connection_id, request, &port_name)?;

    worker
        .write_instrument_quiet(action_id, port_name, request, response_sender)
        .map_err(|error| {
            OutputRequestError::Transport(format!(
                "cannot enqueue instrument write for connection {connection_id}: {error}",
            ))
        })
}

fn require_serial_port_for_metakon(
    connection_id: ConnectionId,
    request: InstrumentWriteRequest,
    port_name: &str,
) -> Result<(), OutputRequestError> {
    if port_name.is_empty() && matches!(request, InstrumentWriteRequest::Metakon5x3 { .. }) {
        return Err(OutputRequestError::Transport(format!(
            "connection {connection_id} does not have a selected COM port",
        )));
    }
    Ok(())
}
