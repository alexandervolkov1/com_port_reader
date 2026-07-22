use std::{error::Error, fmt, path::PathBuf};

use crossbeam_channel::Sender;

use crate::data::{NewSeries, SeriesId};

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

    pub fn add_series(&self, new_series: NewSeries) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::AddSeries(new_series))
    }

    pub(super) fn shutdown(&self) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::Shutdown)
    }

    fn send(&self, command: WorkerCommand) -> Result<(), WorkerHandleError> {
        self.sender.send(command).map_err(|_| WorkerHandleError)
    }

    pub fn remove_series(&self, id: SeriesId) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::RemoveSeries(id))
    }

    pub fn rename_series(
        &self,
        current_name: String,
        new_name: String,
    ) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::RenameSeries {
            current_name,
            new_name,
        })
    }

    pub fn set_visibility(&self, id: SeriesId, visible: bool) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::SetVisibility { id, visible })
    }

    pub fn clear_series(&self) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::ClearSeries)
    }

    pub fn remove_series_by_name(&self, name: String) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::RemoveSeriesByName(name))
    }

    pub fn start_csv_recording(&self, path: PathBuf) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::StartCsvRecording(path))
    }

    pub fn stop_recording(&self) -> Result<(), WorkerHandleError> {
        self.send(WorkerCommand::StopRecording)
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
