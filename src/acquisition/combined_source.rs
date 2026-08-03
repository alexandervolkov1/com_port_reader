use crate::{
    data::{SeriesMetadata, SeriesSample},
    instrument::{InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest},
};

use super::{AcquisitionError, AcquisitionSource};

pub struct CombinedSource {
    sources: Vec<Box<dyn AcquisitionSource>>,
}

impl CombinedSource {
    pub fn new(sources: Vec<Box<dyn AcquisitionSource>>) -> Self {
        Self { sources }
    }
}

impl AcquisitionSource for CombinedSource {
    fn start(&mut self) -> Result<(), AcquisitionError> {
        for index in 0..self.sources.len() {
            let result = self.sources[index].start();

            if let Err(mut error) = result {
                for source in self.sources[..index].iter_mut().rev() {
                    if let Err(stop_error) = source.stop() {
                        error = format!(
                            "{error}; additionally \
                             failed to stop a \
                             previously started \
                             source: {stop_error}",
                        )
                        .into();
                    }
                }

                return Err(error);
            }
        }

        Ok(())
    }

    fn sample_series(
        &mut self,
        series: &SeriesMetadata,
        timestamp: f64,
    ) -> Result<Option<SeriesSample>, AcquisitionError> {
        for source in &mut self.sources {
            if let Some(sample) = source.sample_series(series, timestamp)? {
                return Ok(Some(sample));
            }
        }

        Ok(None)
    }

    fn read_instrument(
        &mut self,
        request: InstrumentReadRequest,
    ) -> Result<Option<InstrumentValue>, AcquisitionError> {
        for source in &mut self.sources {
            if let Some(value) = source.read_instrument(request)? {
                return Ok(Some(value));
            }
        }

        Ok(None)
    }

    fn request_text(&mut self, command: &str) -> Result<Option<String>, AcquisitionError> {
        for source in &mut self.sources {
            if let Some(response) = source.request_text(command)? {
                return Ok(Some(response));
            }
        }

        Ok(None)
    }

    fn write_instrument(
        &mut self,
        request: InstrumentWriteRequest,
    ) -> Result<Option<InstrumentValue>, AcquisitionError> {
        for source in &mut self.sources {
            if let Some(value) = source.write_instrument(request)? {
                return Ok(Some(value));
            }
        }

        Ok(None)
    }

    fn stop(&mut self) -> Result<(), AcquisitionError> {
        let mut combined_error = None;

        for source in self.sources.iter_mut().rev() {
            if let Err(error) = source.stop() {
                combined_error = Some(match combined_error {
                    None => error,

                    Some(previous_error) => format!(
                        "{previous_error}; \
                                 additionally failed \
                                 to stop another \
                                 source: {error}",
                    )
                    .into(),
                });
            }
        }

        match combined_error {
            Some(error) => Err(error),

            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AcquisitionSource, CombinedSource};

    use crate::data::{Sample, SeriesId, SeriesMetadata, SeriesSample, SeriesSource};

    struct FixedSource {
        series_id: SeriesId,
        value: f64,
    }

    impl FixedSource {
        fn new(series_id: SeriesId, value: f64) -> Self {
            Self { series_id, value }
        }
    }

    impl AcquisitionSource for FixedSource {
        fn sample_series(
            &mut self,
            series: &SeriesMetadata,
            timestamp: f64,
        ) -> Result<Option<SeriesSample>, crate::acquisition::AcquisitionError> {
            if series.id != self.series_id {
                return Ok(None);
            }

            Ok(Some(SeriesSample::new(
                self.series_id,
                Sample::new(timestamp, self.value),
            )))
        }
    }

    struct TextSource;

    impl AcquisitionSource for TextSource {
        fn request_text(
            &mut self,
            command: &str,
        ) -> Result<Option<String>, crate::acquisition::AcquisitionError> {
            Ok(Some(format!("response to '{command}'",)))
        }
    }

    #[test]
    fn routes_each_series_to_supporting_source() {
        let first_id = SeriesId::new(1);
        let second_id = SeriesId::new(2);

        let mut source = CombinedSource::new(vec![
            Box::new(FixedSource::new(first_id, 10.0)),
            Box::new(FixedSource::new(second_id, 20.0)),
        ]);

        let series = [metadata(first_id, "first"), metadata(second_id, "second")];

        let mut output = Vec::new();

        source.sample(&series, 1_000.0, &mut output).unwrap();

        assert_eq!(
            output,
            vec![
                SeriesSample::new(first_id, Sample::new(1_000.0, 10.0,),),
                SeriesSample::new(second_id, Sample::new(1_000.0, 20.0,),),
            ],
        );
    }

    #[test]
    fn rejects_unsupported_series() {
        let mut source = CombinedSource::new(vec![Box::new(TextSource)]);

        let series = [metadata(SeriesId::new(1), "temperature")];

        let mut output = Vec::new();

        let error = source.sample(&series, 1_000.0, &mut output).unwrap_err();

        assert_eq!(
            error.to_string(),
            "No acquisition source supports \
             series 'temperature' \
             (COM command: test)",
        );

        assert!(output.is_empty());
    }

    #[test]
    fn routes_text_request_to_supporting_source() {
        let mut source = CombinedSource::new(vec![
            Box::new(FixedSource::new(SeriesId::new(1), 10.0)),
            Box::new(TextSource),
        ]);

        let response = source.request_text("status").unwrap();

        assert_eq!(response.as_deref(), Some("response to 'status'"),);
    }

    fn metadata(id: SeriesId, name: &str) -> SeriesMetadata {
        SeriesMetadata {
            id,
            name: name.to_owned(),

            source: SeriesSource::SerialCommand {
                command: "test".to_owned(),
            },

            visible: true,
        }
    }
}
