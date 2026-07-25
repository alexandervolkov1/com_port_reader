use crossbeam_channel::Receiver;

use crate::{
    app_log::LogHandle,
    components::controls_model::ControlsModel,
    data::{NewSeries, SeriesId},
    dsl::parse_command,
    user_command::UserCommand,
    worker::{WorkerEvent, WorkerHandle, WorkerHandleError},
};

pub struct CommandModel {
    worker_handle: WorkerHandle,
    event_receiver: Receiver<WorkerEvent>,
    command_buffer: String,
    log: LogHandle,
}

impl CommandModel {
    pub fn new(
        worker_handle: WorkerHandle,
        event_receiver: Receiver<WorkerEvent>,
        log: LogHandle,
    ) -> Self {
        Self {
            worker_handle,
            event_receiver,
            command_buffer: String::new(),
            log,
        }
    }

    pub fn command_buffer_mut(&mut self) -> &mut String {
        &mut self.command_buffer
    }

    pub fn poll_events(&mut self) {
        while let Ok(event) = self.event_receiver.try_recv() {
            let message = event.to_string();

            if worker_event_is_error(&event) {
                self.log.error(message);
            } else {
                self.log.info(message);
            }
        }
    }

    pub fn submit(&mut self, controls: &mut ControlsModel) {
        match parse_command(&self.command_buffer) {
            Ok(command) => {
                self.execute(command, controls);
            }

            Err(error) => {
                self.log.error(error);
            }
        }

        self.command_buffer.clear();
    }

    pub fn execute(&self, command: UserCommand, controls: &mut ControlsModel) {
        match command {
            UserCommand::Add(new_series) => {
                self.add_series(new_series);
            }

            UserCommand::Delete { name } => {
                if let Err(error) = self.worker_handle.remove_series_by_name(name) {
                    self.set_worker_error(error);
                }
            }

            UserCommand::Rename {
                current_name,
                new_name,
            } => {
                if let Err(error) = self.worker_handle.rename_series(current_name, new_name) {
                    self.set_worker_error(error);
                }
            }

            UserCommand::Start => {
                controls.start();
            }

            UserCommand::Stop => {
                controls.stop();
            }

            UserCommand::Clear => {
                controls.clear();
            }

            UserCommand::StartRecording => {
                controls.start_recording();
            }

            UserCommand::StopRecording => {
                controls.stop_recording();
            }
        }
    }

    pub fn set_visibility(&self, id: SeriesId, visible: bool) {
        if let Err(error) = self.worker_handle.set_visibility(id, visible) {
            self.set_worker_error(error);
        }
    }

    pub fn remove_series(&self, id: SeriesId) {
        if let Err(error) = self.worker_handle.remove_series(id) {
            self.set_worker_error(error);
        }
    }

    fn set_worker_error(&self, error: WorkerHandleError) {
        self.log.error(format!("Failed to send command: {error}",));
    }

    pub fn add_series(&self, new_series: NewSeries) {
        if let Err(error) = self.worker_handle.add_series(new_series) {
            self.set_worker_error(error);
        }
    }
}

fn worker_event_is_error(event: &WorkerEvent) -> bool {
    matches!(
        event,
        WorkerEvent::SeriesAddFailed(_)
            | WorkerEvent::AcquisitionStartFailed(_)
            | WorkerEvent::AcquisitionFailed(_)
            | WorkerEvent::AcquisitionStopFailed(_)
            | WorkerEvent::SeriesNotFound(_)
            | WorkerEvent::SeriesRenameFailed(_)
            | WorkerEvent::SampleSinkFailed(_)
            | WorkerEvent::SerialPortTestFailed { .. }
            | WorkerEvent::SerialCommandFailed { .. }
    )
}
