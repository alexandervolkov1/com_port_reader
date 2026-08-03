use crate::{
    data::{SeriesMetadata, SeriesSample},
    instrument::{
        InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest,
        virtual_instrument::VirtualInstrumentDescriptor,
    },
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

pub type InstrumentReadResult = Result<InstrumentValue, AcquisitionError>;
pub type InstrumentWriteResult = Result<InstrumentValue, AcquisitionError>;
pub type VirtualInstrumentDescribeResult =
    Result<Vec<VirtualInstrumentDescriptor>, AcquisitionError>;

pub trait AcquisitionSource: Send {
    fn start(&mut self) -> Result<(), AcquisitionError> {
        Ok(())
    }

    fn sample_series(
        &mut self,
        _series: &SeriesMetadata,
        _timestamp: f64,
    ) -> Result<Option<SeriesSample>, AcquisitionError> {
        Ok(None)
    }

    fn sample(
        &mut self,
        series: &[SeriesMetadata],
        timestamp: f64,
        output: &mut Vec<SeriesSample>,
    ) -> Result<(), AcquisitionError> {
        let original_length = output.len();

        for series in series {
            match self.sample_series(series, timestamp) {
                Ok(Some(sample)) => {
                    output.push(sample);
                }

                Ok(None) => {
                    output.truncate(original_length);

                    return Err(format!(
                        "No acquisition source supports \
                         series '{}' ({})",
                        series.name, series.source,
                    )
                    .into());
                }

                Err(error) => {
                    output.truncate(original_length);

                    return Err(error);
                }
            }
        }

        Ok(())
    }

    fn describe_virtual_instruments(
        &mut self,
    ) -> Result<Option<Vec<VirtualInstrumentDescriptor>>, AcquisitionError> {
        Ok(None)
    }

    fn read_instrument(
        &mut self,
        _request: InstrumentReadRequest,
    ) -> Result<Option<InstrumentValue>, AcquisitionError> {
        Ok(None)
    }

    fn request_text(&mut self, _command: &str) -> Result<Option<String>, AcquisitionError> {
        Ok(None)
    }

    fn write_instrument(
        &mut self,
        _request: InstrumentWriteRequest,
    ) -> Result<Option<InstrumentValue>, AcquisitionError> {
        Ok(None)
    }

    fn stop(&mut self) -> Result<(), AcquisitionError> {
        Ok(())
    }
}
