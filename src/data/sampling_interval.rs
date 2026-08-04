use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SamplingInterval(Duration);

impl SamplingInterval {
    pub const fn new(duration: Duration) -> Result<Self, SamplingIntervalError> {
        if duration.is_zero() {
            return Err(SamplingIntervalError);
        }

        Ok(Self(duration))
    }

    pub fn from_secs_f64(seconds: f64) -> Result<Self, SamplingIntervalError> {
        let duration = Duration::try_from_secs_f64(seconds).map_err(|_| SamplingIntervalError)?;

        Self::new(duration)
    }

    pub const fn duration(self) -> Duration {
        self.0
    }

    pub fn as_secs_f64(self) -> f64 {
        self.0.as_secs_f64()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SamplingIntervalError;

impl std::fmt::Display for SamplingIntervalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "Sampling interval must be finite and \
             greater than zero",
        )
    }
}

impl std::error::Error for SamplingIntervalError {}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{SamplingInterval, SamplingIntervalError};

    #[test]
    fn stores_duration() {
        let interval = SamplingInterval::new(Duration::from_millis(250)).unwrap();

        assert_eq!(interval.duration(), Duration::from_millis(250),);

        assert_eq!(interval.as_secs_f64(), 0.25);
    }

    #[test]
    fn creates_interval_from_seconds() {
        let interval = SamplingInterval::from_secs_f64(1.5).unwrap();

        assert_eq!(interval.duration(), Duration::from_millis(1_500),);
    }

    #[test]
    fn rejects_zero_duration() {
        assert_eq!(
            SamplingInterval::new(Duration::ZERO),
            Err(SamplingIntervalError),
        );

        assert_eq!(
            SamplingInterval::from_secs_f64(0.0),
            Err(SamplingIntervalError),
        );
    }

    #[test]
    fn rejects_invalid_seconds() {
        for seconds in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                SamplingInterval::from_secs_f64(seconds,),
                Err(SamplingIntervalError),
            );
        }
    }
}
