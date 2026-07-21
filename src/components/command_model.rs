use crate::{data::NewSeries, dsl::parse_signal, worker::WorkerHandle};
use crossbeam_channel::Receiver;

pub struct CommandModel {
    worker_handle: WorkerHandle,
    response_receiver: Receiver<String>,
    command_buffer: String,
    last_response: String,
}

impl CommandModel {
    pub fn new(worker_handle: WorkerHandle, response_receiver: Receiver<String>) -> Self {
        Self {
            worker_handle,
            response_receiver,
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

    pub fn poll_response(&mut self) {
        if let Ok(response) = self.response_receiver.try_recv() {
            self.last_response = response;
        }
    }

    pub fn submit(&mut self) {
        match self.parse_command() {
            Ok(new_series) => {
                if let Err(error) = self.worker_handle.add_series(new_series) {
                    self.last_response = format!("Failed to send command: {error}");
                }
            }

            Err(error) => {
                self.last_response = error;
            }
        }

        self.command_buffer.clear();
    }

    fn parse_command(&self) -> Result<NewSeries, String> {
        parse_signal(&self.command_buffer).map(NewSeries::unnamed)
    }
}
