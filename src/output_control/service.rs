use std::{
    error::Error,
    fmt, io,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};

use crate::{
    acquisition::InstrumentWriteResult,
    instrument::{ConnectedParameterAddress, InstrumentParameterAddress, InstrumentWriteRequest},
    serial_connection::SerialConnectionRegistry,
    worker::ConnectionRouter,
};

use super::{AutomaticOutputIntent, OutputArbiter, OutputArbiterError, OutputMode, OutputSource};

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
                let result =
                    apply_automatic(&arbiter, intent, &connection_router, &serial_connections);

                let _ = response_sender.send(result);
            }

            OutputCommand::Shutdown => {
                break;
            }
        }
    }
}

fn apply_automatic(
    arbiter: &OutputArbiter,
    intent: AutomaticOutputIntent,
    connection_router: &ConnectionRouter,
    serial_connections: &SerialConnectionRegistry,
) -> Result<Receiver<InstrumentWriteResult>, OutputRequestError> {
    let (target, controller, request) = intent.into_parts();

    arbiter.authorize(target, &OutputSource::controller(controller))?;

    let expected_parameter = target.parameter();

    let request_parameter = request.parameter_address();

    if request_parameter != expected_parameter {
        return Err(OutputRequestError::RequestTargetMismatch {
            expected: expected_parameter,
            actual: request_parameter,
        });
    }

    dispatch_write(target, request, connection_router, serial_connections)
}

fn dispatch_write(
    target: ConnectedParameterAddress,
    request: InstrumentWriteRequest,
    connection_router: &ConnectionRouter,
    serial_connections: &SerialConnectionRegistry,
) -> Result<Receiver<InstrumentWriteResult>, OutputRequestError> {
    let connection_id = target.connection_id();

    let worker = connection_router.handle(connection_id).ok_or_else(|| {
        OutputRequestError::Transport(format!(
            "connection {connection_id} \
                     does not have a registered \
                     worker",
        ))
    })?;

    let serial_config_store = serial_connections.store(connection_id).ok_or_else(|| {
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

    let (response_sender, response_receiver) = bounded(1);

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
        })?;

    Ok(response_receiver)
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
    use crossbeam_channel::unbounded;

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
}
