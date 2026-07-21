use crate::{
    data::NewSeries,
    dsl::parse_series,
    worker::{WorkerEvent, WorkerHandle, WorkerHandleError},
};
use crossbeam_channel::Receiver;

pub struct CommandModel {
    worker_handle: WorkerHandle,
    event_receiver: Receiver<WorkerEvent>,
    command_buffer: String,
    last_response: String,
}

impl CommandModel {
    pub fn new(worker_handle: WorkerHandle, event_receiver: Receiver<WorkerEvent>) -> Self {
        Self {
            worker_handle,
            event_receiver,
            command_buffer: String::new(),
            last_response: String::new(),
        }
    }

    pub fn command_buffer_mut(&mut self) -> &mut String {
        &mut self.command_buffer
    }

    pub fn last_response(&self) -> &str {
        &self.last_response
    }

    pub fn poll_events(&mut self) {
        while let Ok(event) = self.event_receiver.try_recv() {
            self.last_response = event.to_string();
        }
    }

    pub fn submit(&mut self) {
        match self.parse_command() {
            Ok(new_series) => {
                if let Err(error) = self.worker_handle.add_series(new_series) {
                    self.set_worker_error(error);
                }
            }

            Err(error) => {
                self.last_response = error;
            }
        }

        self.command_buffer.clear();
    }

    fn set_worker_error(&mut self, error: WorkerHandleError) {
        self.last_response = format!("Failed to send command: {error}");
    }

    fn parse_command(&self) -> Result<NewSeries, String> {
        parse_series(&self.command_buffer)
    }
}
