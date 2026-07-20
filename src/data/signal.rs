use std::{f64::consts::TAU, fmt::Display};

#[derive(Clone)]
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
    pub fn from_string(input: &str) -> Result<Self, String> {
        let mut tokens = input.split_whitespace();

        let kind = tokens.next().ok_or("Empty input")?;

        match kind.to_lowercase().as_str() {
            "sin" | "sine" => {
                let amplitude = parse_parameter(tokens.next(), 100.0)?;
                let period = parse_parameter(tokens.next(), 100.0)?;
                let phase = parse_parameter(tokens.next(), 0.0)?;

                Ok(Signal::SineWave {
                    amplitude,
                    period,
                    phase,
                })
            }
            "square" | "sq" => {
                let amplitude = parse_parameter(tokens.next(), 100.0)?;
                let period = parse_parameter(tokens.next(), 100.0)?;
                let duty_cycle = parse_parameter(tokens.next(), 0.5)?;

                if !(0.0..=1.0).contains(&duty_cycle) {
                    return Err("Duty cycle must be between 0 and 1".to_string());
                }

                Ok(Signal::SquareWave {
                    amplitude,
                    period,
                    duty_cycle,
                })
            }
            "triangle" | "tri" => {
                let amplitude = parse_parameter(tokens.next(), 100.0)?;
                let period = parse_parameter(tokens.next(), 100.0)?;

                Ok(Signal::TriangleWave { amplitude, period })
            }
            "saw" | "sawtooth" => {
                let amplitude = parse_parameter(tokens.next(), 100.0)?;
                let period = parse_parameter(tokens.next(), 100.0)?;

                Ok(Signal::SawtoothWave { amplitude, period })
            }
            "const" | "constant" => {
                let value = parse_parameter(tokens.next(), 50.0)?;

                Ok(Signal::Constant { value })
            }
            _ => Err(format!("Unknown signal type: {}", kind)),
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

fn parse_parameter(param: Option<&str>, default: f64) -> Result<f64, String> {
    match param {
        Some(s) => s
            .parse::<f64>()
            .map_err(|e| format!("Failed to parse number '{}': {}", s, e)),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::Signal;

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
}
