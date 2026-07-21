use std::{f64::consts::TAU, fmt::Display};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalValidationError {
    NotFinite(&'static str),
    NotPositive(&'static str),
    DutyCycleOutOfRange,
}

impl std::fmt::Display for SignalValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFinite(parameter) => {
                write!(formatter, "{parameter} must be finite")
            }

            Self::NotPositive(parameter) => {
                write!(formatter, "{parameter} must be greater than 0")
            }

            Self::DutyCycleOutOfRange => formatter.write_str("Duty cycle must be between 0 and 1"),
        }
    }
}

impl std::error::Error for SignalValidationError {}

#[derive(Clone, Debug)]
pub enum Signal {
    SineWave {
        amplitude: f64,
        period: f64,
        phase: f64,
    },
    SquareWave {
        amplitude: f64,
        period: f64,
        duty_cycle: f64,
    },
    TriangleWave {
        amplitude: f64,
        period: f64,
    },
    SawtoothWave {
        amplitude: f64,
        period: f64,
    },
    Constant {
        value: f64,
    },
}

impl Signal {
    pub fn validate(&self) -> Result<(), SignalValidationError> {
        match self {
            Signal::SineWave {
                amplitude,
                period,
                phase,
            } => {
                require_finite("Amplitude", *amplitude)?;
                require_positive("Period", *period)?;
                require_finite("Phase", *phase)?;
            }

            Signal::SquareWave {
                amplitude,
                period,
                duty_cycle,
            } => {
                require_finite("Amplitude", *amplitude)?;
                require_positive("Period", *period)?;
                require_finite("Duty cycle", *duty_cycle)?;

                if !(0.0..=1.0).contains(duty_cycle) {
                    return Err(SignalValidationError::DutyCycleOutOfRange);
                }
            }

            Signal::TriangleWave { amplitude, period }
            | Signal::SawtoothWave { amplitude, period } => {
                require_finite("Amplitude", *amplitude)?;
                require_positive("Period", *period)?;
            }

            Signal::Constant { value } => {
                require_finite("Value", *value)?;
            }
        }

        Ok(())
    }

    pub fn value_at(&self, elapsed_seconds: f64) -> f64 {
        match self {
            Signal::SineWave {
                amplitude,
                period,
                phase,
            } => amplitude * (TAU / period * elapsed_seconds + phase).sin(),

            Signal::SquareWave {
                amplitude,
                period,
                duty_cycle,
            } => {
                let time_in_period = elapsed_seconds % period;

                if time_in_period < period * duty_cycle {
                    *amplitude
                } else {
                    -*amplitude
                }
            }

            Signal::TriangleWave { amplitude, period } => {
                let time_in_period = elapsed_seconds % period;
                let normalized_time = time_in_period / period;

                let normalized_value = if normalized_time < 0.5 {
                    4.0 * normalized_time - 1.0
                } else {
                    3.0 - 4.0 * normalized_time
                };

                amplitude * normalized_value
            }

            Signal::SawtoothWave { amplitude, period } => {
                let time_in_period = elapsed_seconds % period;
                let normalized_time = time_in_period / period;

                amplitude * (2.0 * normalized_time - 1.0)
            }

            Signal::Constant { value } => *value,
        }
    }
}

impl Display for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Signal::SineWave {
                amplitude,
                period,
                phase,
            } => {
                write!(
                    f,
                    "SineWave(amp={}, period={}, phase={})",
                    amplitude, period, phase
                )
            }
            Signal::SquareWave {
                amplitude,
                period,
                duty_cycle,
            } => {
                write!(
                    f,
                    "SquareWave(amp={}, period={}, duty={})",
                    amplitude, period, duty_cycle
                )
            }
            Signal::TriangleWave { amplitude, period } => {
                write!(f, "TriangleWave(amp={}, period={})", amplitude, period)
            }
            Signal::SawtoothWave { amplitude, period } => {
                write!(f, "SawtoothWave(amp={}, period={})", amplitude, period)
            }
            Signal::Constant { value } => {
                write!(f, "Constant(val={})", value)
            }
        }
    }
}

fn require_finite(parameter: &'static str, value: f64) -> Result<(), SignalValidationError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(SignalValidationError::NotFinite(parameter))
    }
}

fn require_positive(parameter: &'static str, value: f64) -> Result<(), SignalValidationError> {
    require_finite(parameter, value)?;

    if value > 0.0 {
        Ok(())
    } else {
        Err(SignalValidationError::NotPositive(parameter))
    }
}

#[cfg(test)]
mod tests {
    use super::{Signal, SignalValidationError};

    const EPSILON: f64 = 1e-10;

    fn assert_approx_eq(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < EPSILON,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn calculates_constant_signal() {
        let signal = Signal::Constant { value: 42.0 };

        assert_approx_eq(signal.value_at(100.0), 42.0);
    }

    #[test]
    fn calculates_sine_wave() {
        let signal = Signal::SineWave {
            amplitude: 2.0,
            period: 4.0,
            phase: 0.0,
        };

        assert_approx_eq(signal.value_at(0.0), 0.0);
        assert_approx_eq(signal.value_at(1.0), 2.0);
        assert_approx_eq(signal.value_at(2.0), 0.0);
    }

    #[test]
    fn calculates_square_wave() {
        let signal = Signal::SquareWave {
            amplitude: 3.0,
            period: 4.0,
            duty_cycle: 0.25,
        };

        assert_approx_eq(signal.value_at(0.5), 3.0);
        assert_approx_eq(signal.value_at(2.0), -3.0);
    }

    #[test]
    fn calculates_triangle_wave() {
        let signal = Signal::TriangleWave {
            amplitude: 2.0,
            period: 4.0,
        };

        assert_approx_eq(signal.value_at(0.0), -2.0);
        assert_approx_eq(signal.value_at(1.0), 0.0);
        assert_approx_eq(signal.value_at(2.0), 2.0);
        assert_approx_eq(signal.value_at(3.0), 0.0);
    }

    #[test]
    fn calculates_sawtooth_wave() {
        let signal = Signal::SawtoothWave {
            amplitude: 2.0,
            period: 4.0,
        };

        assert_approx_eq(signal.value_at(0.0), -2.0);
        assert_approx_eq(signal.value_at(2.0), 0.0);
        assert_approx_eq(signal.value_at(3.0), 1.0);
    }
    #[test]
    fn rejects_zero_period() {
        let signal = Signal::SineWave {
            amplitude: 1.0,
            period: 0.0,
            phase: 0.0,
        };

        assert_eq!(
            signal.validate(),
            Err(SignalValidationError::NotPositive("Period"))
        );
    }

    #[test]
    fn rejects_negative_period() {
        let signal = Signal::TriangleWave {
            amplitude: 1.0,
            period: -10.0,
        };

        assert_eq!(
            signal.validate(),
            Err(SignalValidationError::NotPositive("Period"))
        );
    }

    #[test]
    fn rejects_non_finite_amplitude() {
        let signal = Signal::SawtoothWave {
            amplitude: f64::NAN,
            period: 10.0,
        };

        assert_eq!(
            signal.validate(),
            Err(SignalValidationError::NotFinite("Amplitude"))
        );
    }

    #[test]
    fn rejects_non_finite_constant() {
        let signal = Signal::Constant {
            value: f64::INFINITY,
        };

        assert_eq!(
            signal.validate(),
            Err(SignalValidationError::NotFinite("Value"))
        );
    }

    #[test]
    fn accepts_negative_amplitude() {
        let signal = Signal::SineWave {
            amplitude: -10.0,
            period: 5.0,
            phase: 0.0,
        };

        assert_eq!(signal.validate(), Ok(()));
    }
}
