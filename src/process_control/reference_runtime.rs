use std::{error::Error, fmt};

use crate::instrument::{InstrumentValue, ParameterDescriptor};

use super::{
    ReferenceParameterError, ReferenceSource, ReferenceSourceError, ReferenceSourceParameter,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReferenceRuntime {
    source: ReferenceSource,
    elapsed_seconds: f64,
    previous_timestamp: Option<f64>,
}

impl ReferenceRuntime {
    pub const fn new(source: ReferenceSource) -> Self {
        Self {
            source,
            elapsed_seconds: 0.0,
            previous_timestamp: None,
        }
    }

    pub const fn source(&self) -> ReferenceSource {
        self.source
    }

    pub fn parameters(&self) -> Vec<ParameterDescriptor> {
        self.source.parameters()
    }

    pub fn read(&self, key: &str) -> Result<InstrumentValue, ReferenceParameterError> {
        self.source.read(key)
    }

    pub fn set_source(&mut self, source: ReferenceSource) {
        self.source = source;
        self.reset();
    }

    pub fn configure<I, K>(&mut self, updates: I) -> Result<(), ReferenceParameterError>
    where
        I: IntoIterator<Item = (K, InstrumentValue)>,
        K: AsRef<str>,
    {
        let updates = updates
            .into_iter()
            .map(|(key, value)| (key.as_ref().to_owned(), value))
            .collect::<Vec<_>>();

        if updates.is_empty() {
            return Ok(());
        }

        let mut candidate = self.source;

        candidate.configure(updates.iter().map(|(key, value)| (key.as_str(), *value)))?;

        match (self.source, candidate) {
            (ReferenceSource::Fixed(_), ReferenceSource::Fixed(_)) => {
                self.source = candidate;

                Ok(())
            }

            (ReferenceSource::Ramp(_), ReferenceSource::Ramp(candidate_ramp)) => {
                let changes_start = updates.iter().any(|(key, _)| {
                    ReferenceSourceParameter::from_key(key) == Some(ReferenceSourceParameter::Start)
                });

                let source = if changes_start {
                    candidate
                } else {
                    let current = self
                        .current_value()
                        .map_err(ReferenceParameterError::Source)?;

                    ReferenceSource::ramp(current, candidate_ramp.target(), candidate_ramp.rate())
                        .map_err(ReferenceParameterError::Source)?
                };

                self.source = source;
                self.elapsed_seconds = 0.0;
                self.previous_timestamp = None;

                Ok(())
            }

            _ => {
                unreachable!(
                    "reference configuration \
                     cannot change reference kind"
                )
            }
        }
    }

    pub fn write(
        &mut self,
        key: &str,
        value: InstrumentValue,
    ) -> Result<InstrumentValue, ReferenceParameterError> {
        self.configure([(key, value)])?;

        self.read(key)
    }

    pub const fn elapsed_seconds(&self) -> f64 {
        self.elapsed_seconds
    }

    pub const fn previous_timestamp(&self) -> Option<f64> {
        self.previous_timestamp
    }

    pub fn current_value(&self) -> Result<f64, ReferenceSourceError> {
        self.source.value_at_elapsed(self.elapsed_seconds)
    }

    pub fn update(&mut self, timestamp: f64) -> Result<f64, ReferenceRuntimeError> {
        if !timestamp.is_finite() {
            return Err(ReferenceRuntimeError::NonFiniteTimestamp);
        }

        let elapsed_seconds = match self.previous_timestamp {
            Some(previous_timestamp) => {
                if timestamp <= previous_timestamp {
                    return Err(ReferenceRuntimeError::NonIncreasingTimestamp {
                        previous: previous_timestamp,
                        current: timestamp,
                    });
                }

                let elapsed_change = timestamp - previous_timestamp;

                if !elapsed_change.is_finite() {
                    return Err(ReferenceRuntimeError::NonFiniteElapsedTime);
                }

                let elapsed_seconds = self.elapsed_seconds + elapsed_change;

                if !elapsed_seconds.is_finite() {
                    return Err(ReferenceRuntimeError::NonFiniteElapsedTime);
                }

                elapsed_seconds
            }

            None => self.elapsed_seconds,
        };

        let value = self
            .source
            .value_at_elapsed(elapsed_seconds)
            .map_err(ReferenceRuntimeError::Source)?;

        self.elapsed_seconds = elapsed_seconds;

        self.previous_timestamp = Some(timestamp);

        Ok(value)
    }

    pub fn resynchronize(&mut self) {
        self.previous_timestamp = None;
    }

    pub fn reset(&mut self) {
        self.elapsed_seconds = 0.0;
        self.previous_timestamp = None;
    }
}

impl From<ReferenceSource> for ReferenceRuntime {
    fn from(source: ReferenceSource) -> Self {
        Self::new(source)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReferenceRuntimeError {
    NonFiniteTimestamp,

    NonIncreasingTimestamp { previous: f64, current: f64 },

    NonFiniteElapsedTime,

    Source(ReferenceSourceError),
}

impl fmt::Display for ReferenceRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteTimestamp => formatter.write_str(
                "Reference timestamp \
                     must be finite",
            ),

            Self::NonIncreasingTimestamp { previous, current } => {
                write!(
                    formatter,
                    "Reference timestamp \
                     must increase: previous \
                     {previous}, current {current}",
                )
            }

            Self::NonFiniteElapsedTime => formatter.write_str(
                "Reference elapsed time \
                     became non-finite",
            ),

            Self::Source(error) => error.fmt(formatter),
        }
    }
}

impl Error for ReferenceRuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),

            Self::NonFiniteTimestamp
            | Self::NonIncreasingTimestamp { .. }
            | Self::NonFiniteElapsedTime => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ReferenceRuntime, ReferenceRuntimeError};

    use crate::{
        instrument::InstrumentValue,
        process_control::{ReferenceParameterError, ReferenceSource, ReferenceSourceError},
    };

    fn ramp_runtime() -> ReferenceRuntime {
        ReferenceRuntime::new(ReferenceSource::ramp(20.0, 100.0, 10.0).unwrap())
    }

    #[test]
    fn fixed_reference_returns_constant_value() {
        let mut runtime = ReferenceRuntime::new(ReferenceSource::fixed(150.0).unwrap());

        assert_eq!(runtime.update(10.0), Ok(150.0),);

        assert_eq!(runtime.update(20.0), Ok(150.0),);

        assert_eq!(runtime.elapsed_seconds(), 10.0,);
    }

    #[test]
    fn ramp_uses_sample_timestamps() {
        let mut runtime = ramp_runtime();

        assert_eq!(runtime.update(10.0), Ok(20.0),);

        assert_eq!(runtime.update(12.0), Ok(40.0),);

        assert_eq!(runtime.update(15.0), Ok(70.0),);

        assert_eq!(runtime.update(18.0), Ok(100.0),);

        assert_eq!(runtime.update(30.0), Ok(100.0),);
    }

    #[test]
    fn first_timestamp_starts_ramp_without_advancing_it() {
        let mut runtime = ramp_runtime();

        assert_eq!(runtime.update(1_000.0), Ok(20.0),);

        assert_eq!(runtime.elapsed_seconds(), 0.0,);

        assert_eq!(runtime.previous_timestamp(), Some(1_000.0),);
    }

    #[test]
    fn resynchronize_preserves_ramp_progress() {
        let mut runtime = ramp_runtime();

        assert_eq!(runtime.update(10.0), Ok(20.0),);

        assert_eq!(runtime.update(12.0), Ok(40.0),);

        assert_eq!(runtime.elapsed_seconds(), 2.0,);

        runtime.resynchronize();

        assert_eq!(runtime.previous_timestamp(), None,);

        assert_eq!(runtime.elapsed_seconds(), 2.0,);

        assert_eq!(runtime.update(1_000.0), Ok(40.0),);

        assert_eq!(runtime.elapsed_seconds(), 2.0,);

        assert_eq!(runtime.update(1_002.0), Ok(60.0),);
    }

    #[test]
    fn reset_restarts_ramp() {
        let mut runtime = ramp_runtime();

        assert_eq!(runtime.update(10.0), Ok(20.0),);

        assert_eq!(runtime.update(12.0), Ok(40.0),);

        runtime.reset();

        assert_eq!(runtime.elapsed_seconds(), 0.0,);

        assert_eq!(runtime.previous_timestamp(), None,);

        assert_eq!(runtime.update(1_000.0), Ok(20.0),);

        assert_eq!(runtime.update(1_001.0), Ok(30.0),);
    }

    #[test]
    fn rejects_non_finite_timestamp_without_changing_state() {
        let mut runtime = ramp_runtime();

        assert_eq!(runtime.update(10.0), Ok(20.0),);

        assert_eq!(
            runtime.update(f64::NAN),
            Err(ReferenceRuntimeError::NonFiniteTimestamp,),
        );

        assert_eq!(runtime.previous_timestamp(), Some(10.0),);

        assert_eq!(runtime.elapsed_seconds(), 0.0,);

        assert_eq!(runtime.update(12.0), Ok(40.0),);
    }

    #[test]
    fn rejects_non_increasing_timestamp_without_changing_state() {
        let mut runtime = ramp_runtime();

        assert_eq!(runtime.update(10.0), Ok(20.0),);

        assert_eq!(
            runtime.update(10.0),
            Err(ReferenceRuntimeError::NonIncreasingTimestamp {
                previous: 10.0,
                current: 10.0,
            },),
        );

        assert_eq!(
            runtime.update(9.0),
            Err(ReferenceRuntimeError::NonIncreasingTimestamp {
                previous: 10.0,
                current: 9.0,
            },),
        );

        assert_eq!(runtime.previous_timestamp(), Some(10.0),);

        assert_eq!(runtime.elapsed_seconds(), 0.0,);

        assert_eq!(runtime.update(12.0), Ok(40.0),);
    }

    #[test]
    fn rejects_non_finite_elapsed_time_without_changing_state() {
        let mut runtime = ramp_runtime();

        assert_eq!(runtime.update(-f64::MAX,), Ok(20.0),);

        assert_eq!(
            runtime.update(f64::MAX,),
            Err(ReferenceRuntimeError::NonFiniteElapsedTime,),
        );

        assert_eq!(runtime.previous_timestamp(), Some(-f64::MAX),);

        assert_eq!(runtime.elapsed_seconds(), 0.0,);
    }

    #[test]
    fn wraps_reference_source_error() {
        let error = ReferenceRuntimeError::Source(ReferenceSourceError::NegativeElapsedTime);

        assert_eq!(
            error.to_string(),
            "Reference elapsed time \
             must not be negative",
        );
    }

    #[test]
    fn exposes_reference_parameters() {
        let runtime = ReferenceRuntime::new(ReferenceSource::ramp(20.0, 100.0, 10.0).unwrap());

        let keys = runtime
            .parameters()
            .into_iter()
            .map(|parameter| parameter.key)
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["start", "target", "rate",],);

        assert_eq!(runtime.read("target"), Ok(InstrumentValue::Number(100.0,),),);
    }

    #[test]
    fn changes_fixed_reference_live() {
        let mut runtime = ReferenceRuntime::new(ReferenceSource::fixed(150.0).unwrap());

        assert_eq!(runtime.update(10.0), Ok(150.0),);

        assert_eq!(
            runtime.write("value", InstrumentValue::Number(175.0,),),
            Ok(InstrumentValue::Number(175.0,),),
        );

        assert_eq!(runtime.current_value(), Ok(175.0),);

        assert_eq!(runtime.update(20.0), Ok(175.0),);
    }

    #[test]
    fn changing_ramp_target_continues_from_current_value() {
        let mut runtime = ramp_runtime();

        assert_eq!(runtime.update(10.0), Ok(20.0),);

        assert_eq!(runtime.update(12.0), Ok(40.0),);

        assert_eq!(
            runtime.write("target", InstrumentValue::Number(150.0,),),
            Ok(InstrumentValue::Number(150.0,),),
        );

        assert_eq!(runtime.current_value(), Ok(40.0),);

        assert_eq!(runtime.elapsed_seconds(), 0.0,);

        assert_eq!(runtime.previous_timestamp(), None,);

        assert_eq!(runtime.read("start"), Ok(InstrumentValue::Number(40.0,),),);

        assert_eq!(runtime.read("target"), Ok(InstrumentValue::Number(150.0,),),);

        assert_eq!(runtime.update(1_000.0), Ok(40.0),);

        assert_eq!(runtime.update(1_001.0), Ok(50.0),);
    }

    #[test]
    fn changing_ramp_rate_continues_from_current_value() {
        let mut runtime = ramp_runtime();

        assert_eq!(runtime.update(10.0), Ok(20.0),);

        assert_eq!(runtime.update(12.0), Ok(40.0),);

        assert_eq!(
            runtime.write("rate", InstrumentValue::Number(5.0,),),
            Ok(InstrumentValue::Number(5.0,),),
        );

        assert_eq!(runtime.read("start"), Ok(InstrumentValue::Number(40.0,),),);

        assert_eq!(runtime.update(100.0), Ok(40.0),);

        assert_eq!(runtime.update(102.0), Ok(50.0),);
    }

    #[test]
    fn changing_ramp_start_restarts_from_new_start() {
        let mut runtime = ramp_runtime();

        assert_eq!(runtime.update(10.0), Ok(20.0),);

        assert_eq!(runtime.update(12.0), Ok(40.0),);

        assert_eq!(
            runtime.write("start", InstrumentValue::Number(60.0,),),
            Ok(InstrumentValue::Number(60.0,),),
        );

        assert_eq!(runtime.current_value(), Ok(60.0),);

        assert_eq!(runtime.elapsed_seconds(), 0.0,);

        assert_eq!(runtime.previous_timestamp(), None,);

        assert_eq!(runtime.update(1_000.0), Ok(60.0),);

        assert_eq!(runtime.update(1_001.0), Ok(70.0),);
    }

    #[test]
    fn invalid_ramp_configuration_preserves_runtime() {
        let mut runtime = ramp_runtime();

        assert_eq!(runtime.update(10.0), Ok(20.0),);

        assert_eq!(runtime.update(12.0), Ok(40.0),);

        let result = runtime.configure([
            ("target", InstrumentValue::Number(200.0)),
            ("rate", InstrumentValue::Number(0.0)),
        ]);

        assert_eq!(
            result,
            Err(ReferenceParameterError::Source(
                ReferenceSourceError::NonPositiveRampRate,
            ),),
        );

        assert_eq!(runtime.current_value(), Ok(40.0),);

        assert_eq!(runtime.elapsed_seconds(), 2.0,);

        assert_eq!(runtime.previous_timestamp(), Some(12.0),);

        assert_eq!(runtime.read("target"), Ok(InstrumentValue::Number(100.0,),),);

        assert_eq!(runtime.read("rate"), Ok(InstrumentValue::Number(10.0,),),);
    }

    #[test]
    fn replacing_reference_source_resets_runtime() {
        let mut runtime = ramp_runtime();

        runtime.update(10.0).unwrap();
        runtime.update(12.0).unwrap();

        runtime.set_source(ReferenceSource::fixed(175.0).unwrap());

        assert_eq!(runtime.current_value(), Ok(175.0),);

        assert_eq!(runtime.elapsed_seconds(), 0.0,);

        assert_eq!(runtime.previous_timestamp(), None,);

        assert_eq!(runtime.update(1_000.0), Ok(175.0),);
    }
}
