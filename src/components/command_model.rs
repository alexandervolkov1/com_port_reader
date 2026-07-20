use crate::{data::Signal, dsl::parse_signal, worker::WorkerCommand};
use crossbeam_channel::{Receiver, Sender};

pub struct CommandModel {
    command_sender: Sender<WorkerCommand>,
    response_receiver: Receiver<String>,
    command_buffer: String,
    last_response: String,
}

impl CommandModel {
    pub fn new(command_sender: Sender<WorkerCommand>, response_receiver: Receiver<String>) -> Self {
        Self {
            command_sender,
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
            Ok(signal) => {
                if let Err(e) = self.command_sender.send(WorkerCommand::AddSignal(signal)) {
                    self.last_response = format!("Failed to send command: {}", e);
                }
            }

            Err(error) => {
                self.last_response = error;
            }
        }

        self.command_buffer.clear();
    }

    fn parse_command(&self) -> Result<Signal, String> {
        parse_signal(&self.command_buffer)
    }
}
