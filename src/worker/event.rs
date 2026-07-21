use crate::{
    acquisition::AcquisitionError,
    data::{AddSeriesError, SeriesId},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerEvent {
    SeriesAdded(SeriesId),
    SeriesAddFailed(AddSeriesError),
    AcquisitionFailed(AcquisitionError),
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

            Self::AcquisitionFailed(error) => {
                write!(formatter, "Acquisition stopped: {error}")
            }
        }
    }
}
