use crate::{
    data::{SeriesMetadata, SeriesSample},
    protocol::metakon::{RegisterValue, WriteRegisterRequest},
};

mod combined_source;
mod serial_command_source;

pub use combined_source::CombinedSource;
pub use serial_command_source::SerialCommandSource;

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
        output: &mut Vec<SeriesSample>,
    ) -> Result<(), AcquisitionError>;

    fn request_text(&mut self, _command: &str) -> Result<Option<String>, AcquisitionError> {
        Ok(None)
    }

    fn write_metakon_register(
        &mut self,
        _request: WriteRegisterRequest,
    ) -> Result<Option<RegisterValue>, AcquisitionError> {
        Ok(None)
    }

    fn stop(&mut self) -> Result<(), AcquisitionError> {
        Ok(())
    }
}
