use std::{
    error::Error,
    fmt, io,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};

use crate::{
    acquisition::InstrumentWriteResult,
    connection::ConnectionId,
    instrument::{ConnectedParameterAddress, InstrumentParameterAddress, InstrumentWriteRequest},
    serial_connection::SerialConnectionRegistry,
    worker::ConnectionRouter,
};

use super::{
    AutomaticOutputIntent, ManualOutputIntent, OutputArbiter, OutputArbiterError, OutputMode,
    OutputSource,
};

enum OutputCommand {
    RegisterController {
        target: ConnectedParameterAddress,
        controller: String,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    RollbackControllerRegistration {
        target: ConnectedParameterAddress,
        controller: String,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    Mode {
        target: ConnectedParameterAddress,
        response_sender: Sender<Result<OutputMode, OutputArbiterError>>,
    },

    ApplyAutomatic {
        intent: AutomaticOutputIntent,
        response_sender: Sender<Result<Receiver<InstrumentWriteResult>, OutputRequestError>>,
    },

    ApplyManual {
        intent: ManualOutputIntent,
        response_sender: Sender<Result<Receiver<InstrumentWriteResult>, OutputRequestError>>,
    },

    RequestAutomatic {
        controller: String,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    WriteInstrument {
        connection_id: ConnectionId,
        request: InstrumentWriteRequest,
        instrument_response_sender: Sender<InstrumentWriteResult>,
        response_sender: Sender<Result<(), OutputRequestError>>,
    },

    Shutdown,
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
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::RegisterController {
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

    pub(crate) fn apply_automatic(
        &self,
        intent: AutomaticOutputIntent,
    ) -> Result<Receiver<InstrumentWriteResult>, OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::ApplyAutomatic {
                intent,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
    }

    pub(crate) fn apply_manual(
        &self,
        intent: ManualOutputIntent,
    ) -> Result<Receiver<InstrumentWriteResult>, OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::ApplyManual {
                intent,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
    }

    pub(crate) fn write_instrument(
        &self,
        connection_id: ConnectionId,
        request: InstrumentWriteRequest,
        instrument_response_sender: Sender<InstrumentWriteResult>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::WriteInstrument {
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

        let handle = OutputHandle { command_sender };

        let thread = thread::Builder::new()
            .name("output-control".to_owned())
            .spawn(move || {
                run(command_receiver, connection_router, serial_connections);
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
    connection_router: ConnectionRouter,
    serial_connections: SerialConnectionRegistry,
) {
    let mut arbiter = OutputArbiter::new();

    while let Ok(command) = command_receiver.recv() {
        match command {
            OutputCommand::RegisterController {
                target,
                controller,
                response_sender,
            } => {
                let result = arbiter.register_controller(target, controller);

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

            OutputCommand::Mode {
                target,
                response_sender,
            } => {
                let result = arbiter.mode(target);

                let _ = response_sender.send(result);
            }

            OutputCommand::ApplyAutomatic {
                intent,
                response_sender,
            } => {
                let result = apply_automatic(
                    &mut arbiter,
                    intent,
                    &connection_router,
                    &serial_connections,
                );

                let _ = response_sender.send(result);
            }

            OutputCommand::ApplyManual {
                intent,
                response_sender,
            } => {
                let result = apply_manual(
                    &mut arbiter,
                    intent,
                    &connection_router,
                    &serial_connections,
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

            OutputCommand::WriteInstrument {
                connection_id,
                request,
                instrument_response_sender,
                response_sender,
            } => {
                let result = write_instrument(
                    &mut arbiter,
                    connection_id,
                    request,
                    instrument_response_sender,
                    &connection_router,
                    &serial_connections,
                );

                let _ = response_sender.send(result);
            }

            OutputCommand::Shutdown => {
                break;
            }
        }
    }
}

fn apply_automatic(
    arbiter: &mut OutputArbiter,
    intent: AutomaticOutputIntent,
    connection_router: &ConnectionRouter,
    serial_connections: &SerialConnectionRegistry,
) -> Result<Receiver<InstrumentWriteResult>, OutputRequestError> {
    let (target, controller, request) = intent.into_parts();

    arbiter.authorize(target, &OutputSource::controller(controller.clone()))?;

    validate_request_target(target, request)?;

    let response_receiver = dispatch_write(target, request, connection_router, serial_connections)?;

    arbiter.complete_automatic_transition(target, &controller)?;

    Ok(response_receiver)
}

fn apply_manual(
    arbiter: &mut OutputArbiter,
    intent: ManualOutputIntent,
    connection_router: &ConnectionRouter,
    serial_connections: &SerialConnectionRegistry,
) -> Result<Receiver<InstrumentWriteResult>, OutputRequestError> {
    let (target, request) = intent.into_parts();

    arbiter.mode(target)?;

    validate_request_target(target, request)?;

    let response_receiver = dispatch_write(target, request, connection_router, serial_connections)?;

    arbiter.set_mode(target, OutputMode::Manual)?;

    Ok(response_receiver)
}

fn write_instrument(
    arbiter: &mut OutputArbiter,
    connection_id: ConnectionId,
    request: InstrumentWriteRequest,
    instrument_response_sender: Sender<InstrumentWriteResult>,
    connection_router: &ConnectionRouter,
    serial_connections: &SerialConnectionRegistry,
) -> Result<(), OutputRequestError> {
    let target = ConnectedParameterAddress::new(connection_id, request.parameter_address());

    let controlled = arbiter.contains(target);

    dispatch_write_to_sender(
        target,
        request,
        instrument_response_sender,
        connection_router,
        serial_connections,
    )?;

    if controlled {
        arbiter.set_mode(target, OutputMode::Manual)?;
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

fn dispatch_write(
    target: ConnectedParameterAddress,
    request: InstrumentWriteRequest,
    connection_router: &ConnectionRouter,
    serial_connections: &SerialConnectionRegistry,
) -> Result<Receiver<InstrumentWriteResult>, OutputRequestError> {
    let (response_sender, response_receiver) = bounded(1);

    dispatch_write_to_sender(
        target,
        request,
        response_sender,
        connection_router,
        serial_connections,
    )?;

    Ok(response_receiver)
}

fn dispatch_write_to_sender(
    target: ConnectedParameterAddress,
    request: InstrumentWriteRequest,
    response_sender: Sender<InstrumentWriteResult>,
    connection_router: &ConnectionRouter,
    serial_connections: &SerialConnectionRegistry,
) -> Result<(), OutputRequestError> {
    let connection_id = target.connection_id();

    let worker = connection_router.handle(connection_id).ok_or_else(|| {
        OutputRequestError::Transport(format!(
            "connection \
                         {connection_id} does not \
                         have a registered worker",
        ))
    })?;

    let serial_config_store = serial_connections.store(connection_id).ok_or_else(|| {
        OutputRequestError::Transport(format!(
            "connection \
                         {connection_id} does not \
                         have a serial \
                         configuration store",
        ))
    })?;

    let serial_config = serial_config_store.snapshot().ok_or_else(|| {
        OutputRequestError::Transport(format!(
            "connection \
                         {connection_id} does not \
                         have a selected COM port",
        ))
    })?;

    worker
        .write_instrument_quiet(
            serial_config.port_name().to_owned(),
            request,
            response_sender,
        )
        .map_err(|error| {
            OutputRequestError::Transport(format!(
                "cannot enqueue instrument \
                     write for connection \
                     {connection_id}: {error}",
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
                    "Output request targets \
                     {actual:?}, but output ownership \
                     belongs to {expected:?}",
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
        connection::ConnectionId,
        instrument::{
            ConnectedParameterAddress, InstrumentParameterAddress, InstrumentValue,
            InstrumentWriteRequest,
            virtual_instrument::{VirtualInstrumentId, VirtualParameterId},
        },
        serial_connection::{SerialConnectionRegistry, SerialPortConfig},
        worker::{ConnectionCommand, ConnectionRouter, WorkerCommand, WorkerHandle},
    };

    use super::{
        AutomaticOutputIntent, ManualOutputIntent, OutputArbiterError, OutputMode,
        OutputRequestError, OutputService,
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

        handle.register_controller(target, "heater").unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Automatic),);
    }

    #[test]
    fn rejects_duplicate_registration() {
        let service = service();

        let handle = service.handle();

        let target = target();

        handle.register_controller(target, "heater").unwrap();

        assert_eq!(
            handle.register_controller(target, "other",),
            Err(OutputRequestError::Arbiter(
                OutputArbiterError::AlreadyRegistered {
                    controller: "heater".to_owned(),
                },
            ),),
        );
    }

    #[test]
    fn rolls_back_controller_registration() {
        let service = service();

        let handle = service.handle();

        let target = target();

        handle.register_controller(target, "heater").unwrap();

        handle
            .rollback_controller_registration(target, "heater")
            .unwrap();

        handle.register_controller(target, "other").unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Automatic),);
    }

    #[test]
    fn reports_disconnected_service() {
        let handle = {
            let service = service();

            service.handle()
        };

        assert_eq!(
            handle.mode(target()),
            Err(OutputRequestError::Disconnected,),
        );
    }

    #[test]
    fn rejects_request_for_another_parameter() {
        let service = service();

        let handle = service.handle();

        let target = target();

        handle.register_controller(target, "heater").unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(99),
            InstrumentValue::Number(42.0),
        );

        assert!(matches!(
            handle.apply_automatic(AutomaticOutputIntent::new(target, "heater", request,),),
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

        handle.register_controller(target, "heater").unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(42.5),
        );

        let _write_result = handle
            .apply_automatic(AutomaticOutputIntent::new(target, "heater", request))
            .unwrap();

        let command = command_receiver.try_recv().unwrap();

        let WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
            port_name,
            request: received_request,
            emit_event,
            ..
        }) = command
        else {
            panic!("expected instrument write command",);
        };

        assert_eq!(port_name, "COM9",);

        assert_eq!(received_request, request,);

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

        handle.register_controller(target, "heater").unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(35.0),
        );

        let _response_receiver = handle
            .apply_manual(ManualOutputIntent::new(target, request))
            .unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Manual),);

        let command = command_receiver.try_recv().unwrap();

        let WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
            request: received_request,
            ..
        }) = command
        else {
            panic!("expected instrument write command",);
        };

        assert_eq!(received_request, request,);
    }

    #[test]
    fn keeps_automatic_mode_when_manual_write_cannot_be_enqueued() {
        let service = service();

        let handle = service.handle();

        let target = target();

        handle.register_controller(target, "heater").unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(35.0),
        );

        assert!(matches!(
            handle.apply_manual(ManualOutputIntent::new(target, request,),),
            Err(OutputRequestError::Transport(_)),
        ));

        assert_eq!(handle.mode(target), Ok(OutputMode::Automatic),);
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

        handle.register_controller(target, "heater").unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(35.0),
        );

        let _ = handle
            .apply_manual(ManualOutputIntent::new(target, request))
            .unwrap();

        assert!(matches!(
            handle.apply_automatic(AutomaticOutputIntent::new(target, "heater", request,),),
            Err(OutputRequestError::Arbiter(
                OutputArbiterError::SourceNotAllowed {
                    mode: OutputMode::Manual,
                    ..
                }
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
            .write_instrument(connection_id, request, instrument_response_sender)
            .unwrap();

        assert_eq!(
            handle.mode(target),
            Err(OutputRequestError::Arbiter(
                OutputArbiterError::NotRegistered,
            ),),
        );

        let command = command_receiver.try_recv().unwrap();

        let WorkerCommand::Connection(ConnectionCommand::WriteInstrument {
            request: received_request,
            ..
        }) = command
        else {
            panic!("expected instrument write command",);
        };

        assert_eq!(received_request, request,);
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

        let (command_sender, _command_receiver) = unbounded();

        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let service = OutputService::spawn(connection_router, serial_connections).unwrap();

        let handle = service.handle();

        handle.register_controller(target, "heater").unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(25.0),
        );

        let (instrument_response_sender, _instrument_response_receiver) = bounded(1);

        handle
            .write_instrument(connection_id, request, instrument_response_sender)
            .unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Manual),);
    }

    #[test]
    fn completes_automatic_takeover_on_first_successful_output() {
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

        handle.register_controller(target, "heater").unwrap();

        let request = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(4),
            InstrumentValue::Number(35.0),
        );

        let _ = handle
            .apply_manual(ManualOutputIntent::new(target, request))
            .unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Manual),);

        handle.request_automatic("heater").unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::AutomaticPending,),);

        let _ = handle
            .apply_automatic(AutomaticOutputIntent::new(target, "heater", request))
            .unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Automatic),);
    }
}
