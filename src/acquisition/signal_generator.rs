use crate::data::{Sample, SeriesSample, SignalSeries};

use super::{AcquisitionError, AcquisitionSource};

#[derive(Default)]
pub struct SignalGenerator;

impl SignalGenerator {
    pub fn new() -> Self {
        Self
    }
}

impl AcquisitionSource for SignalGenerator {
    fn sample(
        &mut self,
        series: &[SignalSeries],
        timestamp: f64,
        elapsed_seconds: f64,
        output: &mut Vec<SeriesSample>,
    ) -> Result<(), AcquisitionError> {
        output.reserve(series.len());

        for signal_series in series {
            let value = signal_series.signal.value_at(elapsed_seconds);

            output.push(SeriesSample::new(
                signal_series.id,
                Sample::new(timestamp, value),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{AcquisitionSource, SignalGenerator};

    use crate::data::{Sample, SeriesId, SeriesSample, Signal, SignalSeries};

    #[test]
    fn generates_sample_for_each_series() {
        let mut generator = SignalGenerator::new();

        let series = vec![
            SignalSeries::new(
                SeriesId::new(1),
                "first".to_owned(),
                Signal::Constant { value: 10.0 },
            ),
            SignalSeries::new(
                SeriesId::new(2),
                "second".to_owned(),
                Signal::Constant { value: 20.0 },
            ),
        ];

        let mut output = Vec::new();

        generator
            .sample(&series, 1_000.0, 5.0, &mut output)
            .unwrap();

        assert_eq!(
            output,
            vec![
                SeriesSample::new(SeriesId::new(1), Sample::new(1_000.0, 10.0),),
                SeriesSample::new(SeriesId::new(2), Sample::new(1_000.0, 20.0),),
            ],
        );

        assert!(series[0].samples.is_empty());
        assert!(series[1].samples.is_empty());
    }
}
