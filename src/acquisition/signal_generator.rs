use crate::data::{Sample, SignalSeries};

#[derive(Default)]
pub struct SignalGenerator;

impl SignalGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn sample(&self, series: &mut [SignalSeries], timestamp: f64, elapsed_seconds: f64) {
        for signal_series in series {
            let value = signal_series.signal.value_at(elapsed_seconds);

            signal_series.samples.push(Sample::new(timestamp, value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SignalGenerator;

    use crate::data::{SeriesId, Signal, SignalSeries};

    #[test]
    fn generates_sample_for_each_series() {
        let generator = SignalGenerator::new();

        let mut series = vec![
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

        generator.sample(&mut series, 1_000.0, 5.0);

        assert_eq!(series[0].samples.len(), 1);
        assert_eq!(series[0].samples[0].timestamp, 1_000.0);
        assert_eq!(series[0].samples[0].value, 10.0);

        assert_eq!(series[1].samples.len(), 1);
        assert_eq!(series[1].samples[0].timestamp, 1_000.0);
        assert_eq!(series[1].samples[0].value, 20.0);
    }
}
