use crate::{
    acquisition::AcquisitionError,
    data::{AddSeriesError, RenameSeriesError, SeriesId},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerEvent {
    SeriesAdded(SeriesId),
    SeriesAddFailed(AddSeriesError),

    AcquisitionStartFailed(AcquisitionError),
    AcquisitionFailed(AcquisitionError),
    AcquisitionStopFailed(AcquisitionError),
    SeriesRemoved(SeriesId),
    SeriesNotFound(String),
    SeriesRenamed { id: SeriesId, name: String },
    SeriesRenameFailed(RenameSeriesError),
}

impl std::fmt::Display for WorkerEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SeriesAdded(id) => {
                write!(formatter, "Series {id} added.")
            }

            Self::SeriesAddFailed(error) => {
                write!(formatter, "Failed to add series: {error}")
            }

            Self::AcquisitionStartFailed(error) => {
                write!(formatter, "Failed to start acquisition: {error}")
            }

            Self::AcquisitionFailed(error) => {
                write!(formatter, "Acquisition stopped: {error}")
            }

            Self::AcquisitionStopFailed(error) => {
                write!(formatter, "Failed to stop acquisition: {error}")
            }

            Self::SeriesRemoved(id) => {
                write!(formatter, "Series {id} removed.")
            }

            Self::SeriesNotFound(name) => {
                write!(formatter, "Series '{name}' not found.")
            }

            Self::SeriesRenamed { id, name } => {
                write!(formatter, "Series {id} renamed to '{name}'.")
            }

            Self::SeriesRenameFailed(error) => {
                write!(formatter, "Failed to rename series: {error}")
            }
        }
    }
}
