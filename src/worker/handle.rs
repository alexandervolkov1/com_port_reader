use std::{error::Error, fmt};

use crossbeam_channel::Sender;

use crate::data::Signal;

use super::command::WorkerCommand;

#[derive(Clone)]
pub struct WorkerHandle {
    sender: Sender<WorkerCommand>,
}

impl WorkerHandle {
    pub(crate) fn new(sender: Sender<WorkerCommand>) -> Self {
        Self { sender }
    }

    pub fn start(&self) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Start)
    }

    pub fn stop(&self) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Stop)
    }

    pub fn add_signal(&self, signal: Signal) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::AddSignal(signal))
    }

    pub(super) fn shutdown(&self) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Shutdown)
    }

    fn send(&self, command: WorkerCommand) -> Result<(), WorkerHandleError> {
        self.sender.send(command).map_err(|_| WorkerHandleError)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerHandleError;

impl fmt::Display for WorkerHandleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("worker command channel is disconnected")
    }
}

impl Error for WorkerHandleError {}
