use std::fmt::Display;

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

impl Default for Signal {
    fn default() -> Self {
        Signal::Constant { value: 0.0 }
    }
}

impl Signal {
    pub fn from_string(input: &str) -> Result<Self, String> {
        let mut tokens = input.trim().split_whitespace();

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
