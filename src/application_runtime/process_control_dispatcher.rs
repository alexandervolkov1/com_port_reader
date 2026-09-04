use std::{
    io,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, bounded};

use crate::{
    app_log::LogHandle,
    data::SeriesId,
    output_control::{AutomaticOutputIntent, OutputHandle},
    process_control::ControlEvent,
    process_recorder::{ProcessControlOutput, ProcessRecorder},
};

pub(crate) struct ProcessControlDispatcher {
    shutdown_sender: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl ProcessControlDispatcher {
    pub(crate) fn spawn(
        event_receiver: Receiver<ControlEvent<SeriesId>>,
        output_control: OutputHandle,
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
                    output_control,
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
    output_control: OutputHandle,
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

                        let target =
                            output.target;

                        let connection_id =
                            target.connection_id();

                        let request =
                            output.request;

                        let intent =
                            AutomaticOutputIntent::new(
                                target,
                                loop_name.clone(),
                                request,
                            );

                        let actual_output =
                            match output_control
                                .apply_automatic(intent)
                            {
                                Ok(response_receiver) => {
                                    match response_receiver.recv() {
                                        Ok(Ok(actual_value)) => {
                                            Some(
                                                actual_value.as_f64(),
                                            )
                                        }

                                        Ok(Err(error)) => {
                                            log.error(format!(
                                                "Control loop \
                                                 '{loop_name}' \
                                                 output failed: \
                                                 {error}",
                                            ));

                                            None
                                        }

                                        Err(_) => {
                                            log.error(format!(
                                                "Control loop \
                                                 '{loop_name}' \
                                                 output failed: \
                                                 instrument write \
                                                 response channel \
                                                 is disconnected",
                                            ));

                                            None
                                        }
                                    }
                                }

                                Err(error) => {
                                    log.error(format!(
                                        "Control loop \
                                         '{loop_name}' \
                                         output rejected: \
                                         {error}",
                                    ));

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
