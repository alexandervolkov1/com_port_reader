use crate::{
    data::{Sample, SeriesMetadata, SeriesSample},
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeriesAcquisitionFailure {
    pub series_id: crate::data::SeriesId,
    pub series_name: String,
    pub error: AcquisitionError,
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
    ) -> Result<Option<Sample>, AcquisitionError> {
        Ok(None)
    }

    fn sample(
        &mut self,
        series: &[SeriesMetadata],
        output: &mut Vec<SeriesSample>,
        failures: &mut Vec<SeriesAcquisitionFailure>,
    ) {
        for series in series {
            match self.sample_series(series) {
                Ok(Some(sample)) => {
                    output.push(SeriesSample::new(series.id, sample));
                }

                Ok(None) => {
                    failures.push(SeriesAcquisitionFailure {
                        series_id: series.id,
                        series_name: series.name.clone(),

                        error: format!(
                            "No acquisition source \
                                 supports series '{}' ({})",
                            series.name, series.source,
                        )
                        .into(),
                    });
                }

                Err(error) => {
                    failures.push(SeriesAcquisitionFailure {
                        series_id: series.id,
                        series_name: series.name.clone(),
                        error,
                    });
                }
            }
        }
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
