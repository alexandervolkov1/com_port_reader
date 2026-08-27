use std::{
    io,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, bounded};

use crate::{
    acquisition::InstrumentWriteResult,
    app_log::LogHandle,
    connection::ConnectionId,
    data::SeriesId,
    instrument::InstrumentWriteRequest,
    process_control::ControlEvent,
    process_recorder::{ProcessControlOutput, ProcessRecorder},
    serial_connection::SerialConnectionRegistry,
    worker::ConnectionRouter,
};

pub(crate) struct ProcessControlDispatcher {
    shutdown_sender: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl ProcessControlDispatcher {
    pub(crate) fn spawn(
        event_receiver: Receiver<ControlEvent<SeriesId>>,
        connection_router: ConnectionRouter,
        serial_connections: SerialConnectionRegistry,
        process_recorder: ProcessRecorder,
        log: LogHandle,
    ) -> io::Result<Self> {
        let (shutdown_sender, shutdown_receiver) = bounded(1);

        let thread = thread::Builder::new()
            .name("process-control-dispatcher".to_owned())
            .spawn(move || {
                run(
                    event_receiver,
                    shutdown_receiver,
                    connection_router,
                    serial_connections,
                    process_recorder,
                    log,
                );
            })?;

        Ok(Self {
            shutdown_sender,
            thread: Some(thread),
        })
    }
}

impl Drop for ProcessControlDispatcher {
    fn drop(&mut self) {
        let _ = self.shutdown_sender.send(());

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    event_receiver: Receiver<ControlEvent<SeriesId>>,
    shutdown_receiver: Receiver<()>,
    connection_router: ConnectionRouter,
    serial_connections: SerialConnectionRegistry,
    process_recorder: ProcessRecorder,
    log: LogHandle,
) {
    loop {
        crossbeam_channel::select! {
            recv(shutdown_receiver) -> _ => {
                break;
            }

            recv(event_receiver) -> event => {
                let Ok(event) = event else {
                    break;
                };

                match event {
                    ControlEvent::Output(output) => {
                        let loop_name =
                            output.loop_name;

                        let input_series_id =
                            output.input;

                        let timestamp =
                            output.timestamp;

                        let measurement =
                            output.measurement;

                        let controller_output =
                            output.output;

                        let connection_id =
                            output.connection_id;

                        let request =
                            output.request;

                        let actual_output =
                            match dispatch_write(
                                connection_id,
                                request,
                                &connection_router,
                                &serial_connections,
                            ) {
                                Ok(response_receiver) => {
                                    match response_receiver.recv() {
                                        Ok(Ok(actual_value)) => {
                                            Some(
                                                actual_value.as_f64(),
                                            )
                                        }

                                        Ok(Err(error)) => {
                                            log.error(
                                                format!(
                                                    "Control loop \
                                                     '{loop_name}' \
                                                     output failed: \
                                                     {error}",
                                                ),
                                            );

                                            None
                                        }

                                        Err(_) => {
                                            log.error(
                                                format!(
                                                    "Control loop \
                                                     '{loop_name}' \
                                                     output failed: \
                                                     instrument write \
                                                     response channel \
                                                     is disconnected",
                                                ),
                                            );

                                            None
                                        }
                                    }
                                }

                                Err(error) => {
                                    log.error(
                                        format!(
                                            "Control loop \
                                             '{loop_name}' \
                                             output failed: \
                                             {error}",
                                        ),
                                    );

                                    None
                                }
                            };

                        process_recorder
                            .record_control_output(
                                ProcessControlOutput {
                                    timestamp,
                                    loop_name,
                                    controller_kind:
                                        controller_output
                                            .kind()
                                            .as_str()
                                            .to_owned(),

                                    input_series_id,
                                    connection_id,
                                    setpoint:
                                        controller_output.setpoint(),
                                    measurement,
                                    requested_output:
                                        controller_output.value(),
                                    actual_output,
                                    unconstrained_output:
                                        controller_output
                                            .unconstrained_value(),
                                    proportional:
                                        controller_output
                                            .proportional(),
                                    integral:
                                        controller_output
                                            .integral(),
                                    derivative:
                                        controller_output
                                            .derivative(),
                                    saturated:
                                        controller_output
                                            .saturated(),
                                },
                            );
                    }

                    ControlEvent::Error(error) => {
                        log.error(format!(
                            "Control loop execution failed: \
                             {error}",
                        ));
                    }
                }
            }
        }
    }
}

fn dispatch_write(
    connection_id: ConnectionId,
    request: InstrumentWriteRequest,
    connection_router: &ConnectionRouter,
    serial_connections: &SerialConnectionRegistry,
) -> Result<Receiver<InstrumentWriteResult>, String> {
    let worker = connection_router.handle(connection_id).ok_or_else(|| {
        format!(
            "connection {connection_id} \
                     does not have a \
                     registered worker",
        )
    })?;

    let serial_config_store = serial_connections.store(connection_id).ok_or_else(|| {
        format!(
            "connection {connection_id} \
                     does not have a serial \
                     configuration store",
        )
    })?;

    let serial_config = serial_config_store.snapshot().ok_or_else(|| {
        format!(
            "connection {connection_id} \
                     does not have a selected \
                     COM port",
        )
    })?;

    let (response_sender, response_receiver) = bounded(1);

    worker
        .write_instrument_quiet(
            serial_config.port_name().to_owned(),
            request,
            response_sender,
        )
        .map_err(|error| {
            format!(
                "cannot enqueue instrument \
                 write for connection \
                 {connection_id}: {error}",
            )
        })?;

    Ok(response_receiver)
}

#[cfg(test)]
mod tests {
    use crossbeam_channel::unbounded;

    use serialport::{DataBits, FlowControl, Parity, StopBits};

    use super::dispatch_write;

    use crate::{
        connection::ConnectionId,
        instrument::{
            InstrumentValue, InstrumentWriteRequest,
            virtual_instrument::{VirtualInstrumentId, VirtualParameterId},
        },
        serial_connection::{SerialConnectionRegistry, SerialPortConfig},
        worker::{ConnectionCommand, ConnectionRouter, WorkerCommand, WorkerHandle},
    };

    fn write_request() -> InstrumentWriteRequest {
        InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(7),
            VirtualParameterId::new(3),
            InstrumentValue::Number(42.5),
        )
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
    fn routes_pid_output_to_selected_connection_worker() {
        let connection_id = ConnectionId::new(2);

        let serial_connections = SerialConnectionRegistry::new();

        serial_connections
            .register(connection_id)
            .unwrap()
            .set(Some(serial_config("COM9")));

        let connection_router = ConnectionRouter::default();

        let (command_sender, command_receiver) = unbounded();

        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let request = write_request();

        let _response_receiver = dispatch_write(
            connection_id,
            request,
            &connection_router,
            &serial_connections,
        )
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

        assert_eq!(port_name, "COM9");

        assert_eq!(received_request, request,);

        assert!(!emit_event);
    }

    #[test]
    fn rejects_connection_without_registered_worker() {
        let error = dispatch_write(
            ConnectionId::PRIMARY,
            write_request(),
            &ConnectionRouter::default(),
            &SerialConnectionRegistry::new(),
        )
        .unwrap_err();

        assert_eq!(
            error,
            "connection 1 does not have a \
             registered worker",
        );
    }

    #[test]
    fn rejects_connection_without_selected_port() {
        let connection_id = ConnectionId::PRIMARY;

        let serial_connections = SerialConnectionRegistry::new();

        let connection_router = ConnectionRouter::default();

        let (command_sender, _command_receiver) = unbounded();

        connection_router.insert(WorkerHandle::new(connection_id, command_sender));

        let error = dispatch_write(
            connection_id,
            write_request(),
            &connection_router,
            &serial_connections,
        )
        .unwrap_err();

        assert_eq!(
            error,
            "connection 1 does not have a \
             selected COM port",
        );
    }
}
