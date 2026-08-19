use std::{collections::VecDeque, error::Error, fmt};

pub const MAX_FILTER_WINDOW_SIZE: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalFilterKind {
    Exponential,
    MovingAverage,
    Median,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SignalFilterDefinition {
    parameters: SignalFilterParameters,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SignalFilterParameters {
    Exponential { time_constant_seconds: f64 },

    MovingAverage { window_size: usize },

    Median { window_size: usize },
}

impl SignalFilterDefinition {
    pub fn exponential(time_constant_seconds: f64) -> Result<Self, SignalFilterDefinitionError> {
        if !time_constant_seconds.is_finite() || time_constant_seconds <= 0.0 {
            return Err(SignalFilterDefinitionError::InvalidTimeConstant);
        }

        Ok(Self {
            parameters: SignalFilterParameters::Exponential {
                time_constant_seconds,
            },
        })
    }

    pub fn moving_average(window_size: usize) -> Result<Self, SignalFilterDefinitionError> {
        validate_window_size(window_size)?;

        Ok(Self {
            parameters: SignalFilterParameters::MovingAverage { window_size },
        })
    }

    pub fn median(window_size: usize) -> Result<Self, SignalFilterDefinitionError> {
        validate_window_size(window_size)?;

        if window_size.is_multiple_of(2) {
            return Err(SignalFilterDefinitionError::MedianWindowMustBeOdd);
        }

        Ok(Self {
            parameters: SignalFilterParameters::Median { window_size },
        })
    }

    pub const fn kind(self) -> SignalFilterKind {
        match self.parameters {
            SignalFilterParameters::Exponential { .. } => SignalFilterKind::Exponential,

            SignalFilterParameters::MovingAverage { .. } => SignalFilterKind::MovingAverage,

            SignalFilterParameters::Median { .. } => SignalFilterKind::Median,
        }
    }

    pub const fn time_constant_seconds(self) -> Option<f64> {
        match self.parameters {
            SignalFilterParameters::Exponential {
                time_constant_seconds,
            } => Some(time_constant_seconds),

            SignalFilterParameters::MovingAverage { .. }
            | SignalFilterParameters::Median { .. } => None,
        }
    }

    pub const fn window_size(self) -> Option<usize> {
        match self.parameters {
            SignalFilterParameters::MovingAverage { window_size }
            | SignalFilterParameters::Median { window_size } => Some(window_size),

            SignalFilterParameters::Exponential { .. } => None,
        }
    }
}

fn validate_window_size(window_size: usize) -> Result<(), SignalFilterDefinitionError> {
    if window_size == 0 || window_size > MAX_FILTER_WINDOW_SIZE {
        return Err(SignalFilterDefinitionError::InvalidWindowSize);
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalFilterDefinitionError {
    InvalidTimeConstant,
    InvalidWindowSize,
    MedianWindowMustBeOdd,
}

impl fmt::Display for SignalFilterDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTimeConstant => formatter.write_str(
                "Exponential filter time constant must be \
                 finite and greater than zero",
            ),

            Self::InvalidWindowSize => write!(
                formatter,
                "Filter window size must be between 1 and \
                 {MAX_FILTER_WINDOW_SIZE} samples",
            ),

            Self::MedianWindowMustBeOdd => {
                formatter.write_str("Median filter window size must be odd")
            }
        }
    }
}

impl Error for SignalFilterDefinitionError {}

#[derive(Debug)]
pub struct SignalFilter {
    definition: SignalFilterDefinition,
    last_timestamp: Option<f64>,
    state: SignalFilterState,
}

#[derive(Debug)]
enum SignalFilterState {
    Exponential {
        filtered_value: Option<f64>,
        time_constant_seconds: f64,
    },

    MovingAverage {
        values: VecDeque<f64>,
        sum: f64,
        window_size: usize,
    },

    Median {
        values: VecDeque<f64>,
        scratch: Vec<f64>,
        window_size: usize,
    },
}

impl SignalFilter {
    pub fn new(definition: SignalFilterDefinition) -> Self {
        let state = match definition.parameters {
            SignalFilterParameters::Exponential {
                time_constant_seconds,
            } => SignalFilterState::Exponential {
                filtered_value: None,
                time_constant_seconds,
            },

            SignalFilterParameters::MovingAverage { window_size } => {
                SignalFilterState::MovingAverage {
                    values: VecDeque::with_capacity(window_size),
                    sum: 0.0,
                    window_size,
                }
            }

            SignalFilterParameters::Median { window_size } => SignalFilterState::Median {
                values: VecDeque::with_capacity(window_size),
                scratch: Vec::with_capacity(window_size),
                window_size,
            },
        };

        Self {
            definition,
            last_timestamp: None,
            state,
        }
    }

    pub const fn definition(&self) -> SignalFilterDefinition {
        self.definition
    }

    pub fn process(&mut self, timestamp: f64, value: f64) -> Result<f64, SignalFilterError> {
        if !timestamp.is_finite() {
            return Err(SignalFilterError::NonFiniteTimestamp);
        }

        if !value.is_finite() {
            return Err(SignalFilterError::NonFiniteValue);
        }

        let delta_seconds = match self.last_timestamp {
            Some(previous_timestamp) => {
                if timestamp <= previous_timestamp {
                    return Err(SignalFilterError::NonIncreasingTimestamp {
                        previous: previous_timestamp,
                        current: timestamp,
                    });
                }

                Some(timestamp - previous_timestamp)
            }

            None => None,
        };

        let filtered_value = match &mut self.state {
            SignalFilterState::Exponential {
                filtered_value,
                time_constant_seconds,
            } => {
                let result = match (*filtered_value, delta_seconds) {
                    (Some(previous_value), Some(delta_seconds)) => {
                        let exponent = -delta_seconds / *time_constant_seconds;

                        let alpha = -exponent.exp_m1();

                        previous_value + alpha * (value - previous_value)
                    }

                    _ => value,
                };

                *filtered_value = Some(result);

                result
            }

            SignalFilterState::MovingAverage {
                values,
                sum,
                window_size,
            } => {
                if values.len() == *window_size {
                    let removed = values.pop_front().expect("non-empty moving-average window");

                    *sum -= removed;
                }

                values.push_back(value);
                *sum += value;

                *sum / values.len() as f64
            }

            SignalFilterState::Median {
                values,
                scratch,
                window_size,
            } => {
                if values.len() == *window_size {
                    values.pop_front();
                }

                values.push_back(value);

                scratch.clear();
                scratch.extend(values.iter().copied());
                scratch.sort_by(f64::total_cmp);

                median_value(scratch)
            }
        };

        self.last_timestamp = Some(timestamp);

        Ok(filtered_value)
    }

    pub fn reset(&mut self) {
        self.last_timestamp = None;

        match &mut self.state {
            SignalFilterState::Exponential { filtered_value, .. } => {
                *filtered_value = None;
            }

            SignalFilterState::MovingAverage { values, sum, .. } => {
                values.clear();
                *sum = 0.0;
            }

            SignalFilterState::Median {
                values, scratch, ..
            } => {
                values.clear();
                scratch.clear();
            }
        }
    }
}

fn median_value(sorted_values: &[f64]) -> f64 {
    let middle = sorted_values.len() / 2;

    if sorted_values.len().is_multiple_of(2) {
        sorted_values[middle - 1] * 0.5 + sorted_values[middle] * 0.5
    } else {
        sorted_values[middle]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SignalFilterError {
    NonFiniteTimestamp,
    NonFiniteValue,

    NonIncreasingTimestamp { previous: f64, current: f64 },
}

impl fmt::Display for SignalFilterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteTimestamp => {
                formatter.write_str("Filter input timestamp must be finite")
            }

            Self::NonFiniteValue => formatter.write_str("Filter input value must be finite"),

            Self::NonIncreasingTimestamp { previous, current } => write!(
                formatter,
                "Filter input timestamps must increase: \
                 previous timestamp is {previous}, current timestamp is {current}",
            ),
        }
    }
}

impl Error for SignalFilterError {}

#[cfg(test)]
mod tests {
    use super::{
        MAX_FILTER_WINDOW_SIZE, SignalFilter, SignalFilterDefinition, SignalFilterDefinitionError,
        SignalFilterError, SignalFilterKind,
    };

    #[test]
    fn creates_filter_definitions() {
        let exponential = SignalFilterDefinition::exponential(5.0).unwrap();

        assert_eq!(exponential.kind(), SignalFilterKind::Exponential);
        assert_eq!(exponential.time_constant_seconds(), Some(5.0));
        assert_eq!(exponential.window_size(), None);

        let moving_average = SignalFilterDefinition::moving_average(10).unwrap();

        assert_eq!(moving_average.kind(), SignalFilterKind::MovingAverage,);

        assert_eq!(moving_average.time_constant_seconds(), None);
        assert_eq!(moving_average.window_size(), Some(10));

        let median = SignalFilterDefinition::median(5).unwrap();

        assert_eq!(median.kind(), SignalFilterKind::Median);
        assert_eq!(median.time_constant_seconds(), None);
        assert_eq!(median.window_size(), Some(5));
    }

    #[test]
    fn rejects_invalid_time_constants() {
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                SignalFilterDefinition::exponential(value),
                Err(SignalFilterDefinitionError::InvalidTimeConstant),
            );
        }
    }

    #[test]
    fn rejects_invalid_window_sizes() {
        for window_size in [0, MAX_FILTER_WINDOW_SIZE + 1] {
            assert_eq!(
                SignalFilterDefinition::moving_average(window_size),
                Err(SignalFilterDefinitionError::InvalidWindowSize),
            );

            assert_eq!(
                SignalFilterDefinition::median(window_size),
                Err(SignalFilterDefinitionError::InvalidWindowSize),
            );
        }
    }

    #[test]
    fn rejects_even_median_window() {
        assert_eq!(
            SignalFilterDefinition::median(4),
            Err(SignalFilterDefinitionError::MedianWindowMustBeOdd),
        );
    }

    #[test]
    fn applies_exponential_filter_using_elapsed_time() {
        let definition = SignalFilterDefinition::exponential(2.0).unwrap();

        let mut filter = SignalFilter::new(definition);

        assert_eq!(filter.process(10.0, 2.0), Ok(2.0));

        let filtered = filter.process(11.0, 10.0).unwrap();

        let alpha = 1.0 - (-0.5_f64).exp();

        let expected = 2.0 + alpha * (10.0 - 2.0);

        assert_close(filtered, expected);
    }

    #[test]
    fn applies_moving_average_filter() {
        let definition = SignalFilterDefinition::moving_average(3).unwrap();

        let mut filter = SignalFilter::new(definition);

        assert_close(filter.process(0.0, 1.0).unwrap(), 1.0);
        assert_close(filter.process(1.0, 2.0).unwrap(), 1.5);
        assert_close(filter.process(2.0, 6.0).unwrap(), 3.0);
        assert_close(filter.process(3.0, 10.0).unwrap(), 6.0);
    }

    #[test]
    fn applies_median_filter() {
        let definition = SignalFilterDefinition::median(3).unwrap();

        let mut filter = SignalFilter::new(definition);

        assert_close(filter.process(0.0, 10.0).unwrap(), 10.0);
        assert_close(filter.process(1.0, 1.0).unwrap(), 5.5);
        assert_close(filter.process(2.0, 4.0).unwrap(), 4.0);
        assert_close(filter.process(3.0, 100.0).unwrap(), 4.0);
    }

    #[test]
    fn rejects_invalid_samples_without_changing_state() {
        let definition = SignalFilterDefinition::moving_average(2).unwrap();

        let mut filter = SignalFilter::new(definition);

        assert_eq!(filter.process(1.0, 10.0), Ok(10.0));

        assert_eq!(
            filter.process(f64::NAN, 20.0),
            Err(SignalFilterError::NonFiniteTimestamp),
        );

        assert_eq!(
            filter.process(2.0, f64::NAN),
            Err(SignalFilterError::NonFiniteValue),
        );

        assert_eq!(
            filter.process(1.0, 20.0),
            Err(SignalFilterError::NonIncreasingTimestamp {
                previous: 1.0,
                current: 1.0,
            }),
        );

        assert_close(filter.process(2.0, 20.0).unwrap(), 15.0);
    }

    #[test]
    fn reset_removes_filter_history() {
        let definition = SignalFilterDefinition::moving_average(3).unwrap();

        let mut filter = SignalFilter::new(definition);

        filter.process(10.0, 10.0).unwrap();
        filter.process(11.0, 20.0).unwrap();

        filter.reset();

        assert_close(filter.process(0.0, 100.0).unwrap(), 100.0);
    }

    fn assert_close(actual: f64, expected: f64) {
        let tolerance = expected.abs().max(1.0) * 1.0e-12;

        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}",
        );
    }
}
