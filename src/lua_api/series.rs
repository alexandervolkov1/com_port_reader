use crossbeam_channel::Sender;
use mlua::{Lua, Table, Value};

use super::send_application_command;
use crate::{
    application_definition::ApplicationDefinition,
    connection::ConnectionId,
    data::{NewSeries, SamplingInterval, SeriesColor},
    presentation::PlotPaneKey,
    user_command::{SeriesCommand, UserCommand},
};

pub(super) struct LuaSeriesOptions {
    pub(super) name: Option<String>,
    pub(super) sampling_interval: Option<SamplingInterval>,
    pub(super) color: Option<SeriesColor>,
    pub(super) visible: bool,
    pub(super) pane: Option<PlotPaneKey>,
}

impl Default for LuaSeriesOptions {
    fn default() -> Self {
        Self {
            name: None,
            sampling_interval: None,
            color: None,
            visible: true,
            pane: None,
        }
    }
}

pub(super) fn parse_series_options(value: Option<Value>) -> mlua::Result<LuaSeriesOptions> {
    match value {
        None | Some(Value::Nil) => Ok(LuaSeriesOptions::default()),

        Some(Value::String(name)) => Ok(LuaSeriesOptions {
            name: Some(name.to_str()?.to_string()),
            sampling_interval: None,
            ..LuaSeriesOptions::default()
        }),

        Some(Value::Table(options)) => parse_series_options_table(&options),

        Some(_) => Err(mlua::Error::RuntimeError(
            "Series options must be a name string \
             or an options table"
                .to_owned(),
        )),
    }
}

fn parse_series_options_table(options: &Table) -> mlua::Result<LuaSeriesOptions> {
    validate_series_option_keys(options, false)?;

    parse_series_option_values(options)
}

fn parse_serial_series_options(
    value: Option<Value>,
    application_definition: &ApplicationDefinition,
) -> mlua::Result<(LuaSeriesOptions, ConnectionId)> {
    match value {
        None | Some(Value::Nil) => Ok((LuaSeriesOptions::default(), ConnectionId::PRIMARY)),

        Some(Value::String(name)) => Ok((
            LuaSeriesOptions {
                name: Some(name.to_str()?.to_string()),
                sampling_interval: None,
                ..LuaSeriesOptions::default()
            },
            ConnectionId::PRIMARY,
        )),

        Some(Value::Table(options)) => {
            validate_series_option_keys(&options, true)?;

            let series_options = parse_series_option_values(&options)?;

            let connection_id = connection_id_from_options(&options, application_definition)?;

            Ok((series_options, connection_id))
        }

        Some(_) => Err(mlua::Error::RuntimeError(
            "Series options must be a name \
                 string or an options table"
                .to_owned(),
        )),
    }
}

fn validate_series_option_keys(options: &Table, allow_connection: bool) -> mlua::Result<()> {
    for pair in options.pairs::<String, Value>() {
        let (key, _) = pair?;

        let known_option = matches!(
            key.as_str(),
            "name" | "interval" | "color" | "visible" | "pane"
        ) || (allow_connection && key == "connection");

        if !known_option {
            return Err(mlua::Error::RuntimeError(format!(
                "Unknown series option \
                         '{key}'",
            )));
        }
    }

    Ok(())
}

fn parse_series_option_values(options: &Table) -> mlua::Result<LuaSeriesOptions> {
    let name = options.get::<Option<String>>("name")?;

    let sampling_interval = options
        .get::<Option<f64>>("interval")?
        .map(SamplingInterval::from_secs_f64)
        .transpose()
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

    let (visible, color, pane) = parse_series_presentation(options)?;

    Ok(LuaSeriesOptions {
        name,
        sampling_interval,
        color,
        visible,
        pane,
    })
}

pub(super) fn parse_series_presentation(
    options: &Table,
) -> mlua::Result<(bool, Option<SeriesColor>, Option<PlotPaneKey>)> {
    let visible = options.get::<Option<bool>>("visible")?.unwrap_or(true);
    let color = options
        .get::<Option<String>>("color")?
        .map(|value| value.parse::<SeriesColor>())
        .transpose()
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;
    let pane = options
        .get::<Option<String>>("pane")?
        .map(PlotPaneKey::new)
        .transpose()
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

    Ok((visible, color, pane))
}

pub(super) fn apply_series_options(
    mut new_series: NewSeries,
    options: LuaSeriesOptions,
    connection_id: ConnectionId,
) -> NewSeries {
    new_series = new_series.with_connection(connection_id);

    if let Some(interval) = options.sampling_interval {
        new_series = new_series.with_sampling_interval(interval);
    }

    if let Some(color) = options.color {
        new_series = new_series.with_color(color);
    }

    new_series = new_series.with_visibility(options.visible);

    if let Some(pane) = options.pane {
        new_series = new_series.with_pane(pane);
    }

    new_series
}

pub(super) fn register_add_serial(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
    application_definition: ApplicationDefinition,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, (command, options): (String, Option<Value>)| {
        let (options, connection_id) =
            parse_serial_series_options(options, &application_definition)?;

        let new_series = match &options.name {
            Some(name) => NewSeries::named_serial_command(command, name),

            None => NewSeries::unnamed_serial_command(command),
        };

        let new_series = apply_series_options(new_series, options, connection_id);

        send_application_command(&command_sender, SeriesCommand::Add(new_series).into())
    })?;

    app.set("add_serial", function)
}

pub(super) fn connection_id_from_options(
    options: &Table,
    application_definition: &ApplicationDefinition,
) -> mlua::Result<ConnectionId> {
    let Some(connection_name) = options.get::<Option<String>>("connection")? else {
        return Ok(ConnectionId::PRIMARY);
    };

    application_definition
        .connection_id_by_name(&connection_name)
        .ok_or_else(|| {
            mlua::Error::RuntimeError(format!(
                "Unknown serial connection \
                 '{connection_name}'",
            ))
        })
}

pub(super) fn register_delete_series(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, name: String| {
        send_application_command(&command_sender, SeriesCommand::Delete { name }.into())
    })?;

    app.set("delete", function)
}

pub(super) fn register_rename_series(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, (current_name, new_name): (String, String)| {
        send_application_command(
            &command_sender,
            SeriesCommand::Rename {
                current_name,
                new_name,
            }
            .into(),
        )
    })?;

    app.set("rename", function)
}

pub(super) fn register_set_series_color(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, (name, color): (String, Option<String>)| {
        let color = color
            .map(|value| value.parse::<SeriesColor>())
            .transpose()
            .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

        send_application_command(
            &command_sender,
            SeriesCommand::SetColor { name, color }.into(),
        )
    })?;

    app.set("set_color", function)
}

pub(super) fn register_set_series_pane(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, (name, pane): (String, String)| {
        let pane =
            PlotPaneKey::new(pane).map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

        send_application_command(
            &command_sender,
            SeriesCommand::SetPane { name, pane }.into(),
        )
    })?;

    app.set("set_series_pane", function)
}

pub(super) fn register_retry_series(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, name: String| {
        send_application_command(&command_sender, SeriesCommand::Retry { name }.into())
    })?;

    app.set("retry", function)
}
