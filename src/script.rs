use crate::{dsl::parse_command, user_command::UserCommand};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptError {
    Empty,

    InvalidCommand {
        line_number: usize,
        line: String,
        message: String,
    },
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("Script contains no commands"),

            Self::InvalidCommand {
                line_number,
                line,
                message,
            } => {
                write!(
                    formatter,
                    "Line {line_number}: {message}\n\
                     > {line}",
                )
            }
        }
    }
}

impl std::error::Error for ScriptError {}

pub fn parse_script(contents: &str) -> Result<Vec<UserCommand>, ScriptError> {
    let mut commands = Vec::new();

    for (line_index, source_line) in contents.lines().enumerate() {
        let line = source_line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let command = parse_command(line).map_err(|message| ScriptError::InvalidCommand {
            line_number: line_index + 1,
            line: line.to_owned(),
            message,
        })?;

        commands.push(command);
    }

    if commands.is_empty() {
        return Err(ScriptError::Empty);
    }

    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::{ScriptError, parse_script};

    use crate::{
        data::{SeriesSource, Signal},
        user_command::UserCommand,
    };

    #[test]
    fn parses_script_commands() {
        let script = "\
# Test script

sin --amp 10 --per 20 --name sine_test
serial get
delete old_series
rename sine_test new_sine
";

        let mut commands = parse_script(script).unwrap().into_iter();

        let Some(UserCommand::Add(generated)) = commands.next() else {
            panic!("expected generated series");
        };

        let (signal, name) = generated.into_parts();

        assert_eq!(
            signal,
            Signal::SineWave {
                amplitude: 10.0,
                period: 20.0,
                phase: 0.0,
            },
        );

        assert_eq!(name.as_deref(), Some("sine_test"),);

        let Some(UserCommand::Add(serial)) = commands.next() else {
            panic!("expected serial series");
        };

        let (source, name) = serial.into_source_parts();

        assert_eq!(name, None);

        assert_eq!(
            source,
            SeriesSource::SerialCommand {
                command: "walk".to_owned(),
                step: 1.0,
            }
        );

        let Some(UserCommand::Delete { name }) = commands.next() else {
            panic!("expected delete command");
        };

        assert_eq!(name, "old_series");

        let Some(UserCommand::Rename {
            current_name,
            new_name,
        }) = commands.next()
        else {
            panic!("expected rename command");
        };

        assert_eq!(current_name, "sine_test");
        assert_eq!(new_name, "new_sine");

        assert!(commands.next().is_none());
    }

    #[test]
    fn reports_line_number_and_source() {
        let script = "\
# Valid command
const 10

triangle --per 0
";

        let result = parse_script(script);

        assert_eq!(
            result.unwrap_err(),
            ScriptError::InvalidCommand {
                line_number: 4,
                line: "triangle --per 0".to_owned(),
                message: "Period must be greater than 0".to_owned(),
            },
        );
    }

    #[test]
    fn reports_unknown_command() {
        let result = parse_script(
            "sin --amp 10\n\
             something_unknown",
        );

        assert_eq!(
            result.unwrap_err(),
            ScriptError::InvalidCommand {
                line_number: 2,
                line: "something_unknown".to_owned(),
                message: "Unknown signal type: \
                     something_unknown"
                    .to_owned(),
            },
        );
    }

    #[test]
    fn rejects_empty_script() {
        let result = parse_script(
            "\n\
             # Only a comment\n\
             \n",
        );

        assert_eq!(result.unwrap_err(), ScriptError::Empty,);
    }

    #[test]
    fn formats_error_for_user() {
        let error = ScriptError::InvalidCommand {
            line_number: 7,
            line: "sin --per 0".to_owned(),
            message: "Period must be greater than 0".to_owned(),
        };

        assert_eq!(
            error.to_string(),
            "Line 7: Period must be greater than 0\n\
             > sin --per 0",
        );
    }
}
