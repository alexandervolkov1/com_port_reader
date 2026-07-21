use std::str::SplitWhitespace;

use crate::{
    data::{NewSeries, Signal},
    user_command::UserCommand,
};

const AMPLITUDE_OPTIONS: &[&str] = &["--amp", "--amplitude"];

const PERIOD_OPTIONS: &[&str] = &["--per", "--period"];

const PHASE_OPTIONS: &[&str] = &["--phase"];

const DUTY_CYCLE_OPTIONS: &[&str] = &["--duty", "--duty-cycle"];

const VALUE_OPTIONS: &[&str] = &["--value", "--val"];

pub fn parse_command(input: &str) -> Result<UserCommand, String> {
    let input = input.trim();

    let Some(first_token) = input.split_whitespace().next() else {
        return Err("Empty input".to_owned());
    };

    if first_token.eq_ignore_ascii_case("add") {
        let series_definition = input[first_token.len()..].trim_start();

        if series_definition.is_empty() {
            return Err("Missing series definition for command 'add'".to_owned());
        }

        return parse_series(series_definition).map(UserCommand::AddSeries);
    }

    parse_series(input).map(UserCommand::AddSeries)
}

pub fn parse_series(input: &str) -> Result<NewSeries, String> {
    let mut tokens = input.split_whitespace();

    let kind = tokens.next().ok_or_else(|| "Empty input".to_owned())?;

    match kind.to_ascii_lowercase().as_str() {
        "sin" | "sine" => parse_sine(&mut tokens),
        "square" | "sq" => parse_square(&mut tokens),
        "triangle" | "tri" => parse_triangle(&mut tokens),
        "saw" | "sawtooth" => parse_sawtooth(&mut tokens),
        "const" | "constant" => parse_constant(&mut tokens),

        _ => Err(format!("Unknown signal type: {kind}")),
    }
}

fn parse_sine(tokens: &mut SplitWhitespace<'_>) -> Result<NewSeries, String> {
    let mut parameters = [
        NumericParameter::new("amplitude", AMPLITUDE_OPTIONS, 100.0),
        NumericParameter::new("period", PERIOD_OPTIONS, 100.0),
        NumericParameter::new("phase", PHASE_OPTIONS, 0.0),
    ];

    let name = parse_arguments(tokens, &mut parameters)?;

    finish(
        Signal::SineWave {
            amplitude: parameters[0].value(),
            period: parameters[1].value(),
            phase: parameters[2].value(),
        },
        name,
    )
}

fn parse_square(tokens: &mut SplitWhitespace<'_>) -> Result<NewSeries, String> {
    let mut parameters = [
        NumericParameter::new("amplitude", AMPLITUDE_OPTIONS, 100.0),
        NumericParameter::new("period", PERIOD_OPTIONS, 100.0),
        NumericParameter::new("duty cycle", DUTY_CYCLE_OPTIONS, 0.5),
    ];

    let name = parse_arguments(tokens, &mut parameters)?;

    finish(
        Signal::SquareWave {
            amplitude: parameters[0].value(),
            period: parameters[1].value(),
            duty_cycle: parameters[2].value(),
        },
        name,
    )
}

fn parse_triangle(tokens: &mut SplitWhitespace<'_>) -> Result<NewSeries, String> {
    let mut parameters = [
        NumericParameter::new("amplitude", AMPLITUDE_OPTIONS, 100.0),
        NumericParameter::new("period", PERIOD_OPTIONS, 100.0),
    ];

    let name = parse_arguments(tokens, &mut parameters)?;

    finish(
        Signal::TriangleWave {
            amplitude: parameters[0].value(),
            period: parameters[1].value(),
        },
        name,
    )
}

fn parse_sawtooth(tokens: &mut SplitWhitespace<'_>) -> Result<NewSeries, String> {
    let mut parameters = [
        NumericParameter::new("amplitude", AMPLITUDE_OPTIONS, 100.0),
        NumericParameter::new("period", PERIOD_OPTIONS, 100.0),
    ];

    let name = parse_arguments(tokens, &mut parameters)?;

    finish(
        Signal::SawtoothWave {
            amplitude: parameters[0].value(),
            period: parameters[1].value(),
        },
        name,
    )
}

fn parse_constant(tokens: &mut SplitWhitespace<'_>) -> Result<NewSeries, String> {
    let mut parameters = [NumericParameter::new("value", VALUE_OPTIONS, 50.0)];

    let name = parse_arguments(tokens, &mut parameters)?;

    finish(
        Signal::Constant {
            value: parameters[0].value(),
        },
        name,
    )
}

fn finish(signal: Signal, name: Option<String>) -> Result<NewSeries, String> {
    signal.validate().map_err(|error| error.to_string())?;

    Ok(match name {
        Some(name) => NewSeries::named(signal, name),
        None => NewSeries::unnamed(signal),
    })
}

struct NumericParameter {
    name: &'static str,
    options: &'static [&'static str],
    value: Option<f64>,
    default: f64,
}

impl NumericParameter {
    const fn new(name: &'static str, options: &'static [&'static str], default: f64) -> Self {
        Self {
            name,
            options,
            value: None,
            default,
        }
    }

    fn value(&self) -> f64 {
        self.value.unwrap_or(self.default)
    }

    fn set(&mut self, value: &str) -> Result<(), String> {
        if self.value.is_some() {
            return Err(format!(
                "Parameter '{}' specified more than once",
                self.name,
            ));
        }

        self.value = Some(value.parse::<f64>().map_err(|error| {
            format!("Failed to parse '{}' as {}: {}", value, self.name, error,)
        })?);

        Ok(())
    }
}

fn parse_arguments(
    tokens: &mut SplitWhitespace<'_>,
    parameters: &mut [NumericParameter],
) -> Result<Option<String>, String> {
    let mut name = None;

    while let Some(argument) = tokens.next() {
        match argument {
            "--name" => {
                parse_name(tokens, &mut name)?;
            }

            option if option.starts_with("--") => {
                let Some(parameter) = parameters
                    .iter_mut()
                    .find(|parameter| parameter.options.contains(&option))
                else {
                    return Err(format!("Unknown option: {option}"));
                };

                let value = next_option_value(tokens, option)?;

                parameter.set(value)?;
            }

            value => {
                let Some(parameter) = parameters
                    .iter_mut()
                    .find(|parameter| parameter.value.is_none())
                else {
                    return Err(format!("Unexpected argument: {value}"));
                };

                parameter.set(value)?;
            }
        }
    }

    Ok(name)
}

fn parse_name(tokens: &mut SplitWhitespace<'_>, name: &mut Option<String>) -> Result<(), String> {
    if name.is_some() {
        return Err("Option '--name' specified more than once".to_owned());
    }

    let value = next_option_value(tokens, "--name")?;

    *name = Some(value.to_owned());

    Ok(())
}

fn next_option_value<'a>(
    tokens: &mut SplitWhitespace<'a>,
    option: &str,
) -> Result<&'a str, String> {
    let Some(value) = tokens.next() else {
        return Err(format!("Missing value for option '{option}'"));
    };

    if value.starts_with("--") {
        return Err(format!("Missing value for option '{option}'"));
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{parse_command, parse_series};

    use crate::{data::Signal, user_command::UserCommand};

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
    fn parses_named_parameters() {
        let new_series = parse_series(
            "sin --amp 10 --per 20 \
             --phase 0.5 --name sinus1",
        )
        .unwrap();

        let (signal, name) = new_series.into_parts();

        assert_eq!(name.as_deref(), Some("sinus1"));

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
    fn accepts_options_in_any_order() {
        let new_series = parse_series(
            "square --duty 0.25 --name pulse \
             --per 20 --amp 10",
        )
        .unwrap();

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
    fn mixes_named_and_positional_parameters() {
        let new_series = parse_series("triangle --per 20 10").unwrap();

        let (signal, _) = new_series.into_parts();

        match signal {
            Signal::TriangleWave { amplitude, period } => {
                assert_eq!(amplitude, 10.0);
                assert_eq!(period, 20.0);
            }

            _ => panic!("expected triangle wave"),
        }
    }

    #[test]
    fn uses_default_parameters() {
        let new_series = parse_series("square --name pulse").unwrap();

        let (signal, name) = new_series.into_parts();

        assert_eq!(name.as_deref(), Some("pulse"));

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
    fn parses_option_aliases() {
        let new_series = parse_series("constant --val 25 --name baseline").unwrap();

        let (signal, name) = new_series.into_parts();

        assert_eq!(name.as_deref(), Some("baseline"));

        match signal {
            Signal::Constant { value } => {
                assert_eq!(value, 25.0);
            }

            _ => panic!("expected constant"),
        }
    }

    #[test]
    fn rejects_duplicate_parameter() {
        let result = parse_series("sin 10 --amp 20");

        assert_eq!(
            result.unwrap_err(),
            "Parameter 'amplitude' specified more than once"
        );
    }

    #[test]
    fn rejects_duplicate_name() {
        let result = parse_series("const --name first --name second");

        assert_eq!(
            result.unwrap_err(),
            "Option '--name' specified more than once"
        );
    }

    #[test]
    fn rejects_missing_option_value() {
        let result = parse_series("sin --amp");

        assert_eq!(result.unwrap_err(), "Missing value for option '--amp'");
    }

    #[test]
    fn rejects_unknown_option() {
        let result = parse_series("sin --frequency 10");

        assert_eq!(result.unwrap_err(), "Unknown option: --frequency");
    }

    #[test]
    fn rejects_extra_positional_argument() {
        let result = parse_series("const 10 20");

        assert_eq!(result.unwrap_err(), "Unexpected argument: 20");
    }

    #[test]
    fn rejects_invalid_period() {
        let result = parse_series("triangle --per 0");

        assert_eq!(result.unwrap_err(), "Period must be greater than 0");
    }

    #[test]
    fn rejects_invalid_duty_cycle() {
        let result = parse_series("square --duty 2");

        assert_eq!(result.unwrap_err(), "Duty cycle must be between 0 and 1");
    }

    #[test]
    fn parses_explicit_add_command() {
        let command = parse_command("add const --value 10 --name baseline").unwrap();

        let UserCommand::AddSeries(new_series) = command;

        let (signal, name) = new_series.into_parts();

        assert_eq!(name.as_deref(), Some("baseline"));
        assert_eq!(signal, Signal::Constant { value: 10.0 });
    }

    #[test]
    fn treats_legacy_signal_syntax_as_add_command() {
        let command = parse_command("const 10 --name baseline").unwrap();

        let UserCommand::AddSeries(new_series) = command;

        let (signal, name) = new_series.into_parts();

        assert_eq!(name.as_deref(), Some("baseline"));
        assert_eq!(signal, Signal::Constant { value: 10.0 });
    }

    #[test]
    fn rejects_add_without_series_definition() {
        let result = parse_command("add");

        assert_eq!(
            result.unwrap_err(),
            "Missing series definition for command 'add'",
        );
    }
}
