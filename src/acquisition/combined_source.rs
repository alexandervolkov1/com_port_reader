use crate::data::{SeriesMetadata, SeriesSample};

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
                            "{error}; additionally failed \
                             to stop a previously started \
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

    fn sample(
        &mut self,
        series: &[SeriesMetadata],
        timestamp: f64,
        elapsed_seconds: f64,
        output: &mut Vec<SeriesSample>,
    ) -> Result<(), AcquisitionError> {
        for source in &mut self.sources {
            source.sample(series, timestamp, elapsed_seconds, output)?;
        }

        Ok(())
    }

    fn stop(&mut self) -> Result<(), AcquisitionError> {
        let mut combined_error = None;

        for source in self.sources.iter_mut().rev() {
            if let Err(error) = source.stop() {
                combined_error = Some(match combined_error {
                    None => error,

                    Some(previous_error) => format!(
                        "{previous_error}; \
                                 additionally failed to \
                                 stop another source: \
                                 {error}",
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

    use crate::data::{Sample, SeriesId, SeriesMetadata, SeriesSample};

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
        fn sample(
            &mut self,
            _series: &[SeriesMetadata],
            timestamp: f64,
            _elapsed_seconds: f64,
            output: &mut Vec<SeriesSample>,
        ) -> Result<(), crate::acquisition::AcquisitionError> {
            output.push(SeriesSample::new(
                self.series_id,
                Sample::new(timestamp, self.value),
            ));

            Ok(())
        }
    }

    #[test]
    fn collects_samples_from_all_sources() {
        let first_id = SeriesId::new(1);
        let second_id = SeriesId::new(2);

        let mut source = CombinedSource::new(vec![
            Box::new(FixedSource::new(first_id, 10.0)),
            Box::new(FixedSource::new(second_id, 20.0)),
        ]);

        let mut output = Vec::new();

        source.sample(&[], 1_000.0, 5.0, &mut output).unwrap();

        assert_eq!(
            output,
            vec![
                SeriesSample::new(first_id, Sample::new(1_000.0, 10.0),),
                SeriesSample::new(second_id, Sample::new(1_000.0, 20.0),),
            ],
        );
    }
}
