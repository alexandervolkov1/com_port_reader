use crate::data::Signal;
use crossbeam_channel::{Receiver, Sender};

pub struct CommandModel {
    pub command_sender: Sender<Signal>,
    pub response_receiver: Receiver<String>,
    pub command_buffer: String,
    pub last_response: String,
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

    pub fn update(&mut self) {
        if let Ok(response) = self.response_receiver.try_recv() {
            self.last_response = response;
        }
    }
}
