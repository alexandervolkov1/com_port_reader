use crate::data::{Sample, SeriesMetadata, SeriesSample};

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
        series: &[SeriesMetadata],
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

    use crate::data::{Sample, SeriesId, SeriesMetadata, SeriesSample, Signal};

    #[test]
    fn generates_sample_for_each_series() {
        let mut generator = SignalGenerator::new();

        let series = vec![
            SeriesMetadata {
                id: SeriesId::new(1),
                name: "first".to_owned(),
                signal: Signal::Constant { value: 10.0 },
                visible: true,
            },
            SeriesMetadata {
                id: SeriesId::new(2),
                name: "second".to_owned(),
                signal: Signal::Constant { value: 20.0 },
                visible: true,
            },
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
    }
}
