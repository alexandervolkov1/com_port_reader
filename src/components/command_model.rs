use crate::data::Signal;
use crossbeam_channel::{Receiver, Sender};

pub struct CommandModel {
    command_sender: Sender<Signal>,
    response_receiver: Receiver<String>,
    command_buffer: String,
    last_response: String,
}

impl CommandModel {
    pub fn new(command_sender: Sender<Signal>, response_receiver: Receiver<String>) -> Self {
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

    pub fn update(&mut self) {
        if let Ok(response) = self.response_receiver.try_recv() {
            self.last_response = response;
        }
    }

    pub fn submit(&mut self) {
        match Signal::from_string(&self.command_buffer) {
            Ok(signal) => {
                let _ = self.command_sender.send(signal);
            }

            Err(e) => {
                self.last_response = e;
            }
        }

        self.command_buffer.clear();
    }
}
