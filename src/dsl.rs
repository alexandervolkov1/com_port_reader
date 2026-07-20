use crate::data::Signal;

pub fn parse_signal(input: &str) -> Result<Signal, String> {
    let mut tokens = input.split_whitespace();

    let kind = tokens.next().ok_or_else(|| "Empty input".to_owned())?;

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
                return Err("Duty cycle must be between 0 and 1".to_owned());
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

        _ => Err(format!("Unknown signal type: {kind}")),
    }
}

fn parse_parameter(parameter: Option<&str>, default: f64) -> Result<f64, String> {
    match parameter {
        Some(value) => value
            .parse::<f64>()
            .map_err(|error| format!("Failed to parse number '{value}': {error}")),

        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_signal;
    use crate::data::Signal;

    #[test]
    fn parses_sine_wave_with_parameters() {
        let signal = parse_signal("sin 10 20 0.5").unwrap();

        match signal {
            Signal::SineWave {
                amplitude,
                period,
                phase,
            } => {
                assert_eq!(amplitude, 10.0);
                assert_eq!(period, 20.0);
                assert_eq!(phase, 0.5);
            }

            _ => panic!("expected sine wave"),
        }
    }

    #[test]
    fn uses_default_parameters() {
        let signal = parse_signal("square").unwrap();

        match signal {
            Signal::SquareWave {
                amplitude,
                period,
                duty_cycle,
            } => {
                assert_eq!(amplitude, 100.0);
                assert_eq!(period, 100.0);
                assert_eq!(duty_cycle, 0.5);
            }

            _ => panic!("expected square wave"),
        }
    }

    #[test]
    fn rejects_invalid_number() {
        let result = parse_signal("sin invalid");

        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_duty_cycle() {
        let result = parse_signal("square 10 20 1.5");

        assert_eq!(result.unwrap_err(), "Duty cycle must be between 0 and 1");
    }

    #[test]
    fn rejects_unknown_signal_type() {
        let result = parse_signal("unknown");

        assert_eq!(result.unwrap_err(), "Unknown signal type: unknown");
    }
}
