mod command;
mod error;
mod handle;
mod runtime;

use std::{
    io,
    thread::{self, JoinHandle},
};

use crossbeam_channel::unbounded;
pub(crate) use error::{OutputRequestError, OutputWriteError};
pub(crate) use handle::OutputHandle;

use self::{command::OutputCommand, runtime::run};
#[cfg(test)]
use crate::output_control::{AutomaticOutputIntent, OutputArbiterError, OutputMode};
use crate::{serial_connection::SerialConnectionRegistry, worker::ConnectionRouter};

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
#[cfg(test)]
mod tests {
    use crossbeam_channel::{bounded, unbounded};
    use serialport::{DataBits, FlowControl, Parity, StopBits};

    use super::{
        AutomaticOutputIntent, OutputArbiterError, OutputMode, OutputRequestError, OutputService,
    };
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
