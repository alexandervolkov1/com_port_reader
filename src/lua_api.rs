use crossbeam_channel::Sender;
use mlua::{Lua, Table};

use crate::{
    data::{
        DEFAULT_METAKON_CHANNEL, DEFAULT_METAKON_DEVICE, DEFAULT_METAKON_REGISTER,
        DEFAULT_METAKON_SCALE, MetakonValueType, NewSeries,
    },
    user_command::UserCommand,
};

pub fn install(lua: &Lua, command_sender: Sender<UserCommand>) -> mlua::Result<()> {
    let app = lua.create_table()?;

    register_command(lua, &app, "start", command_sender.clone(), start_command)?;

    register_command(lua, &app, "stop", command_sender.clone(), stop_command)?;

    register_command(lua, &app, "clear", command_sender.clone(), clear_command)?;

    register_command(
        lua,
        &app,
        "start_rec",
        command_sender.clone(),
        start_recording_command,
    )?;

    register_command(
        lua,
        &app,
        "stop_rec",
        command_sender.clone(),
        stop_recording_command,
    )?;

    register_command(
        lua,
        &app,
        "start_emu",
        command_sender.clone(),
        start_emulator_command,
    )?;

    register_command(
        lua,
        &app,
        "stop_emu",
        command_sender.clone(),
        stop_emulator_command,
    )?;

    register_add_serial(lua, &app, command_sender.clone())?;

    register_add_metakon(lua, &app, command_sender.clone())?;

    register_delete_series(lua, &app, command_sender.clone())?;

    register_rename_series(lua, &app, command_sender.clone())?;

    register_send_serial(lua, &app, command_sender)?;

    lua.globals().set("app", app)
}

fn register_command(
    lua: &Lua,
    app: &Table,
    name: &str,
    command_sender: Sender<UserCommand>,
    command_factory: fn() -> UserCommand,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, ()| {
        send_application_command(&command_sender, command_factory())
    })?;

    app.set(name, function)
}

fn register_add_serial(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, (command, name): (String, Option<String>)| {
        let new_series = match name {
            Some(name) => NewSeries::named_serial_command(command, name),

            None => NewSeries::unnamed_serial_command(command),
        };

        send_application_command(&command_sender, UserCommand::Add(new_series))
    })?;

    app.set("add_serial", function)
}

fn register_add_metakon(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |lua, options: Option<Table>| {
        let options = match options {
            Some(options) => options,
            None => lua.create_table()?,
        };

        validate_metakon_options(&options)?;

        let device = options
            .get::<Option<u8>>("device")?
            .unwrap_or(DEFAULT_METAKON_DEVICE);

        let channel = options
            .get::<Option<u8>>("channel")?
            .unwrap_or(DEFAULT_METAKON_CHANNEL);

        let register = options
            .get::<Option<u8>>("register")?
            .unwrap_or(DEFAULT_METAKON_REGISTER);

        let value_type = options
            .get::<Option<String>>("value_type")?
            .map(|value| parse_metakon_value_type(&value))
            .transpose()?
            .unwrap_or(MetakonValueType::Int);

        let scale = options
            .get::<Option<f64>>("scale")?
            .unwrap_or(DEFAULT_METAKON_SCALE);

        let name = options.get::<Option<String>>("name")?;

        let new_series = match name {
            Some(name) => {
                NewSeries::named_typed_metakon(device, channel, register, value_type, scale, name)
            }

            None => NewSeries::unnamed_typed_metakon(device, channel, register, value_type, scale),
        };

        send_application_command(&command_sender, UserCommand::Add(new_series))
    })?;

    app.set("add_metakon", function)
}

fn validate_metakon_options(options: &Table) -> mlua::Result<()> {
    for pair in options.pairs::<String, mlua::Value>() {
        let (key, _) = pair?;

        if !matches!(
            key.as_str(),
            "device" | "channel" | "register" | "value_type" | "scale" | "name"
        ) {
            return Err(mlua::Error::RuntimeError(format!(
                "unknown app.add_metakon \
                         option '{key}'",
            )));
        }
    }

    Ok(())
}

fn parse_metakon_value_type(value: &str) -> mlua::Result<MetakonValueType> {
    match value.to_ascii_lowercase().as_str() {
        "ubyte" => Ok(MetakonValueType::Ubyte),
        "byte" => Ok(MetakonValueType::Byte),
        "uint" => Ok(MetakonValueType::Uint),
        "int" => Ok(MetakonValueType::Int),

        _ => Err(mlua::Error::RuntimeError(format!(
            "unknown Metakon value type '{value}'; \
             expected ubyte, byte, uint or int",
        ))),
    }
}

fn register_delete_series(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, name: String| {
        send_application_command(&command_sender, UserCommand::Delete { name })
    })?;

    app.set("delete", function)
}

fn register_rename_series(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, (current_name, new_name): (String, String)| {
        send_application_command(
            &command_sender,
            UserCommand::Rename {
                current_name,
                new_name,
            },
        )
    })?;

    app.set("rename", function)
}

fn register_send_serial(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, command: String| {
        send_application_command(&command_sender, UserCommand::SendSerial { command })
    })?;

    app.set("send_serial", function)
}

fn send_application_command(
    command_sender: &Sender<UserCommand>,
    command: UserCommand,
) -> mlua::Result<()> {
    command_sender.send(command).map_err(|_| {
        mlua::Error::RuntimeError(
            "application command channel \
             is disconnected"
                .to_owned(),
        )
    })
}

fn start_command() -> UserCommand {
    UserCommand::Start
}

fn stop_command() -> UserCommand {
    UserCommand::Stop
}

fn clear_command() -> UserCommand {
    UserCommand::Clear
}

fn start_recording_command() -> UserCommand {
    UserCommand::StartRecording
}

fn stop_recording_command() -> UserCommand {
    UserCommand::StopRecording
}

fn start_emulator_command() -> UserCommand {
    UserCommand::StartEmulator
}

fn stop_emulator_command() -> UserCommand {
    UserCommand::StopEmulator
}
