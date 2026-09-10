use crossbeam_channel::Sender;
use mlua::{Lua, Table, Value};

use super::send_application_command;
use crate::{
    data::{NewFilteredSeries, SeriesColor},
    signal_processing::SignalFilterDefinition,
    user_command::{SeriesCommand, UserCommand},
};

pub(super) fn register_add_filter(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, (input_name, options): (String, Table)| {
        let name = options.get::<Option<String>>("name")?.ok_or_else(|| {
            mlua::Error::RuntimeError(
                "Filtered series option 'name' \
                         is required"
                    .to_owned(),
            )
        })?;

        let kind = options.get::<Option<String>>("kind")?.ok_or_else(|| {
            mlua::Error::RuntimeError(
                "Filtered series option 'kind' \
                         is required"
                    .to_owned(),
            )
        })?;

        validate_filter_option_keys(&options, &kind, true)?;

        let definition = parse_filter_definition(&options, &kind)?;

        let color = options
            .get::<Option<String>>("color")?
            .map(|value| value.parse::<SeriesColor>())
            .transpose()
            .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

        let mut filter = NewFilteredSeries::new(input_name, name, definition);

        if let Some(color) = color {
            filter = filter.with_color(color);
        }

        send_application_command(&command_sender, SeriesCommand::AddFilter(filter).into())
    })?;

    app.set("filter", function)
}

pub(super) fn register_set_filter(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, (name, options): (String, Table)| {
        let kind = options.get::<Option<String>>("kind")?.ok_or_else(|| {
            mlua::Error::RuntimeError(
                "Signal filter option 'kind' \
                         is required"
                    .to_owned(),
            )
        })?;

        validate_filter_option_keys(&options, &kind, false)?;

        let definition = parse_filter_definition(&options, &kind)?;

        send_application_command(
            &command_sender,
            SeriesCommand::SetFilter { name, definition }.into(),
        )
    })?;

    app.set("set_filter", function)
}

fn validate_filter_option_keys(
    options: &Table,
    kind: &str,
    allow_series_options: bool,
) -> mlua::Result<()> {
    for pair in options.pairs::<String, Value>() {
        let (key, _) = pair?;

        let parameter_option = match kind {
            "exponential" => {
                matches!(key.as_str(), "kind" | "time_constant")
            }

            "moving_average" | "median" => {
                matches!(key.as_str(), "kind" | "window")
            }

            _ => {
                return Err(mlua::Error::RuntimeError(format!(
                    "Unknown signal filter kind \
                         '{kind}'",
                )));
            }
        };

        let series_option = matches!(key.as_str(), "name" | "color");

        if !(parameter_option || allow_series_options && series_option) {
            return Err(mlua::Error::RuntimeError(format!(
                "Unknown option '{key}' for \
                     signal filter kind '{kind}'",
            )));
        }
    }

    Ok(())
}

fn parse_filter_definition(options: &Table, kind: &str) -> mlua::Result<SignalFilterDefinition> {
    let result = match kind {
        "exponential" => {
            let time_constant = options
                .get::<Option<f64>>("time_constant")?
                .ok_or_else(|| {
                    mlua::Error::RuntimeError(
                        "Exponential filter option \
                         'time_constant' is required"
                            .to_owned(),
                    )
                })?;

            SignalFilterDefinition::exponential(time_constant)
        }

        "moving_average" => {
            let window = options.get::<Option<usize>>("window")?.ok_or_else(|| {
                mlua::Error::RuntimeError(
                    "Moving-average filter option \
                         'window' is required"
                        .to_owned(),
                )
            })?;

            SignalFilterDefinition::moving_average(window)
        }

        "median" => {
            let window = options.get::<Option<usize>>("window")?.ok_or_else(|| {
                mlua::Error::RuntimeError(
                    "Median filter option 'window' \
                         is required"
                        .to_owned(),
                )
            })?;

            SignalFilterDefinition::median(window)
        }

        _ => {
            return Err(mlua::Error::RuntimeError(format!(
                "Unknown signal filter kind '{kind}'",
            )));
        }
    };

    result.map_err(|error| mlua::Error::RuntimeError(error.to_string()))
}
