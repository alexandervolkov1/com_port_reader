use std::str::SplitWhitespace;

use crate::data::{NewSeries, Signal};

pub fn parse_series(input: &str) -> Result<NewSeries, String> {
    let mut tokens = input.split_whitespace();

    let kind = tokens.next().ok_or_else(|| "Empty input".to_owned())?;

    let mut name = None;

    let signal = match kind.to_ascii_lowercase().as_str() {
        "sin" | "sine" => {
            let amplitude = parse_parameter(next_argument(&mut tokens, &mut name)?, 100.0)?;

            let period = parse_parameter(next_argument(&mut tokens, &mut name)?, 100.0)?;

            let phase = parse_parameter(next_argument(&mut tokens, &mut name)?, 0.0)?;

            Signal::SineWave {
                amplitude,
                period,
                phase,
            }
        }

        "square" | "sq" => {
            let amplitude = parse_parameter(next_argument(&mut tokens, &mut name)?, 100.0)?;

            let period = parse_parameter(next_argument(&mut tokens, &mut name)?, 100.0)?;

            let duty_cycle = parse_parameter(next_argument(&mut tokens, &mut name)?, 0.5)?;

            Signal::SquareWave {
                amplitude,
                period,
                duty_cycle,
            }
        }

        "triangle" | "tri" => {
            let amplitude = parse_parameter(next_argument(&mut tokens, &mut name)?, 100.0)?;

            let period = parse_parameter(next_argument(&mut tokens, &mut name)?, 100.0)?;

            Signal::TriangleWave { amplitude, period }
        }

        "saw" | "sawtooth" => {
            let amplitude = parse_parameter(next_argument(&mut tokens, &mut name)?, 100.0)?;

            let period = parse_parameter(next_argument(&mut tokens, &mut name)?, 100.0)?;

            Signal::SawtoothWave { amplitude, period }
        }

        "const" | "constant" => {
            let value = parse_parameter(next_argument(&mut tokens, &mut name)?, 50.0)?;

            Signal::Constant { value }
        }

        _ => {
            return Err(format!("Unknown signal type: {kind}"));
        }
    };

    if let Some(argument) = next_argument(&mut tokens, &mut name)? {
        return Err(format!("Unexpected argument: {argument}"));
    }

    signal.validate().map_err(|error| error.to_string())?;

    Ok(match name {
        Some(name) => NewSeries::named(signal, name),
        None => NewSeries::unnamed(signal),
    })
}

fn next_argument<'a>(
    tokens: &mut SplitWhitespace<'a>,
    name: &mut Option<String>,
) -> Result<Option<&'a str>, String> {
    loop {
        match tokens.next() {
            Some("--name") => {
                if name.is_some() {
                    return Err("Option '--name' specified more than once".to_owned());
                }

                let Some(value) = tokens.next() else {
                    return Err("Missing value for option '--name'".to_owned());
                };

                if value.starts_with("--") {
                    return Err("Missing value for option '--name'".to_owned());
                }

                *name = Some(value.to_owned());
            }

            Some(option) if option.starts_with("--") => {
                return Err(format!("Unknown option: {option}"));
            }

            Some(value) => {
                return Ok(Some(value));
            }

            None => {
                return Ok(None);
            }
        }
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
    use super::parse_series;
    use crate::data::Signal;

    #[test]
    fn parses_positional_parameters() {
        let new_series = parse_series("sin 10 20 0.5").unwrap();

        let (signal, name) = new_series.into_parts();

        assert_eq!(name, None);

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
    fn parses_custom_name() {
        let new_series = parse_series("sin 10 20 0.5 --name sinus1").unwrap();

        let (_, name) = new_series.into_parts();

        assert_eq!(name.as_deref(), Some("sinus1"));
    }

    #[test]
    fn parses_name_before_parameters() {
        let new_series = parse_series("square --name pulse 10 20 0.25").unwrap();

        let (signal, name) = new_series.into_parts();

        assert_eq!(name.as_deref(), Some("pulse"));

        match signal {
            Signal::SquareWave {
                amplitude,
                period,
                duty_cycle,
            } => {
                assert_eq!(amplitude, 10.0);
                assert_eq!(period, 20.0);
                assert_eq!(duty_cycle, 0.25);
            }

            _ => panic!("expected square wave"),
        }
    }

    #[test]
    fn uses_defaults_with_custom_name() {
        let new_series = parse_series("const --name baseline").unwrap();

        let (signal, name) = new_series.into_parts();

        assert_eq!(name.as_deref(), Some("baseline"));

        match signal {
            Signal::Constant { value } => {
                assert_eq!(value, 50.0);
            }

            _ => panic!("expected constant"),
        }
    }

    #[test]
    fn rejects_duplicate_name_option() {
        let result = parse_series("const --name first --name second");

        assert_eq!(
            result.unwrap_err(),
            "Option '--name' specified more than once"
        );
    }

    #[test]
    fn rejects_missing_name() {
        let result = parse_series("const --name");

        assert_eq!(result.unwrap_err(), "Missing value for option '--name'");
    }

    #[test]
    fn rejects_unknown_option() {
        let result = parse_series("sin --unknown 10");

        assert_eq!(result.unwrap_err(), "Unknown option: --unknown");
    }

    #[test]
    fn rejects_extra_positional_argument() {
        let result = parse_series("const 10 20");

        assert_eq!(result.unwrap_err(), "Unexpected argument: 20");
    }

    #[test]
    fn rejects_invalid_period() {
        let result = parse_series("triangle 10 0");

        assert_eq!(result.unwrap_err(), "Period must be greater than 0");
    }
}
