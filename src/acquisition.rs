mod combined_source;
mod signal_generator;

use crate::data::{SeriesMetadata, SeriesSample};

pub use combined_source::CombinedSource;
pub use signal_generator::SignalGenerator;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcquisitionError {
    message: String,
}

impl std::fmt::Display for AcquisitionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AcquisitionError {}

impl From<String> for AcquisitionError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for AcquisitionError {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_owned(),
        }
    }
}

pub trait AcquisitionSource: Send {
    fn start(&mut self) -> Result<(), AcquisitionError> {
        Ok(())
    }

    fn sample(
        &mut self,
        series: &[SeriesMetadata],
        timestamp: f64,
        elapsed_seconds: f64,
        output: &mut Vec<SeriesSample>,
    ) -> Result<(), AcquisitionError>;

    fn stop(&mut self) -> Result<(), AcquisitionError> {
        Ok(())
    }
}
