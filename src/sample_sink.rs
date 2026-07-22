mod csv;

use crate::data::SeriesSample;
pub use csv::CsvSampleSink;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampleSinkError {
    message: String,
}

impl std::fmt::Display for SampleSinkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SampleSinkError {}

impl From<String> for SampleSinkError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for SampleSinkError {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_owned(),
        }
    }
}

pub trait SampleSink: Send {
    fn write_batch(&mut self, samples: &[SeriesSample]) -> Result<(), SampleSinkError>;

    fn flush(&mut self) -> Result<(), SampleSinkError> {
        Ok(())
    }
}

#[derive(Default)]
pub struct NullSampleSink;

impl NullSampleSink {
    pub fn new() -> Self {
        Self
    }
}

impl SampleSink for NullSampleSink {
    fn write_batch(&mut self, _samples: &[SeriesSample]) -> Result<(), SampleSinkError> {
        Ok(())
    }
}

impl From<std::io::Error> for SampleSinkError {
    fn from(error: std::io::Error) -> Self {
        Self::from(error.to_string())
    }
}
