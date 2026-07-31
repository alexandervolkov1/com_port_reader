use crossbeam_channel::Sender;
use mlua::{Lua, Table, UserData, UserDataMethods};

use crate::{
    data::{
        DEFAULT_METAKON_CHANNEL, DEFAULT_METAKON_DEVICE, DEFAULT_METAKON_REGISTER,
        DEFAULT_METAKON_SCALE, MetakonValueType, NewSeries,
    },
    instrument::metakon_5x3::{Metakon5x3, Metakon5x3Write},
    protocol::metakon::{WriteRegisterRequest, WriteRegisterValue},
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

    register_metakon_controller(lua, &app, command_sender.clone())?;

    register_add_metakon(lua, &app, command_sender.clone())?;

    register_delete_series(lua, &app, command_sender.clone())?;

    register_rename_series(lua, &app, command_sender.clone())?;

    register_metakon_write_command(
        lua,
        &app,
        command_sender.clone(),
        MetakonWriteCommand {
            name: "set_metakon_setpoint",
            register: 0x02,
            minimum: -999,
            maximum: 9_999,
            value_factory: metakon_int_value,
        },
    )?;

    register_metakon_write_command(
        lua,
        &app,
        command_sender.clone(),
        MetakonWriteCommand {
            name: "set_metakon_proportional_band",
            register: 0x03,
            minimum: 1,
            maximum: 9_999,
            value_factory: metakon_uint_value,
        },
    )?;

    register_metakon_write_command(
        lua,
        &app,
        command_sender.clone(),
        MetakonWriteCommand {
            name: "set_metakon_integral_time",
            register: 0x04,
            minimum: 1,
            maximum: 30_000,
            value_factory: metakon_uint_value,
        },
    )?;

    register_metakon_write_command(
        lua,
        &app,
        command_sender.clone(),
        MetakonWriteCommand {
            name: "set_metakon_derivative_time",
            register: 0x05,
            minimum: 0,
            maximum: 255,
            value_factory: metakon_ubyte_value,
        },
    )?;
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

fn register_metakon_controller(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |lua, options: Option<Table>| {
        let options = match options {
            Some(options) => options,
            None => lua.create_table()?,
        };

        validate_metakon_controller_options(&options)?;

        let device = options
            .get::<Option<u8>>("device")?
            .unwrap_or(DEFAULT_METAKON_DEVICE);

        let channel = options
            .get::<Option<u8>>("channel")?
            .unwrap_or(DEFAULT_METAKON_CHANNEL);

        let scale = options
            .get::<Option<f64>>("scale")?
            .unwrap_or(DEFAULT_METAKON_SCALE);

        if !scale.is_finite() || scale <= 0.0 {
            return Err(mlua::Error::RuntimeError(
                "app.metakon scale must be finite and \
                     greater than zero"
                    .to_owned(),
            ));
        }

        lua.create_userdata(LuaMetakon5x3 {
            instrument: Metakon5x3::new(device, channel),
            scale,
            command_sender: command_sender.clone(),
        })
    })?;

    app.set("metakon", function)
}

fn validate_metakon_controller_options(options: &Table) -> mlua::Result<()> {
    for pair in options.pairs::<String, mlua::Value>() {
        let (key, _) = pair?;

        if !matches!(key.as_str(), "device" | "channel" | "scale") {
            return Err(mlua::Error::RuntimeError(format!(
                "unknown app.metakon option '{key}'",
            )));
        }
    }

    Ok(())
}

#[derive(Clone)]
struct LuaMetakon5x3 {
    instrument: Metakon5x3,
    scale: f64,
    command_sender: Sender<UserCommand>,
}

impl LuaMetakon5x3 {
    fn add_series(&self, new_series: NewSeries) -> mlua::Result<()> {
        send_application_command(&self.command_sender, UserCommand::Add(new_series))
    }

    fn write(&self, parameter: Metakon5x3Write) -> mlua::Result<()> {
        let request = self
            .instrument
            .write_request(parameter)
            .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

        send_application_command(&self.command_sender, UserCommand::WriteMetakon { request })
    }
}

impl UserData for LuaMetakon5x3 {
    fn add_methods<M>(methods: &mut M)
    where
        M: UserDataMethods<Self>,
    {
        methods.add_method("add_measurement", |_, controller, name: Option<String>| {
            controller.add_series(
                controller
                    .instrument
                    .measurement_series(controller.scale, name),
            )
        });

        methods.add_method("add_setpoint", |_, controller, name: Option<String>| {
            controller.add_series(
                controller
                    .instrument
                    .setpoint_series(controller.scale, name),
            )
        });

        methods.add_method("add_output_power", |_, controller, name: Option<String>| {
            controller.add_series(controller.instrument.output_power_series(name))
        });

        methods.add_method("setpoint", |_, controller, value: i64| {
            let value = i16::try_from(value).map_err(|_| {
                mlua::Error::RuntimeError(
                    "Metakon 5X3 setpoint does not \
                             fit into Int"
                        .to_owned(),
                )
            })?;

            controller.write(Metakon5x3Write::Setpoint(value))
        });

        methods.add_method("proportional_band", |_, controller, value: i64| {
            let value = u16::try_from(value).map_err(|_| {
                mlua::Error::RuntimeError(
                    "Metakon 5X3 proportional band \
                             does not fit into Uint"
                        .to_owned(),
                )
            })?;

            controller.write(Metakon5x3Write::ProportionalBand(value))
        });

        methods.add_method("integral_time", |_, controller, value: i64| {
            let value = u16::try_from(value).map_err(|_| {
                mlua::Error::RuntimeError(
                    "Metakon 5X3 integral time does \
                             not fit into Uint"
                        .to_owned(),
                )
            })?;

            controller.write(Metakon5x3Write::IntegralTime(value))
        });

        methods.add_method("derivative_time", |_, controller, value: i64| {
            let value = u8::try_from(value).map_err(|_| {
                mlua::Error::RuntimeError(
                    "Metakon 5X3 derivative time \
                             must be between 0 and 255"
                        .to_owned(),
                )
            })?;

            controller.write(Metakon5x3Write::DerivativeTime(value))
        });
    }
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

#[derive(Clone, Copy)]
struct MetakonWriteCommand {
    name: &'static str,
    register: u8,
    minimum: i64,
    maximum: i64,
    value_factory: fn(i64) -> WriteRegisterValue,
}

fn register_metakon_write_command(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
    command: MetakonWriteCommand,
) -> mlua::Result<()> {
    let function = lua.create_function(move |lua, options: Option<Table>| {
        let options = match options {
            Some(options) => options,

            None => lua.create_table()?,
        };

        validate_metakon_write_options(&options, command.name)?;

        let device = options
            .get::<Option<u8>>("device")?
            .unwrap_or(DEFAULT_METAKON_DEVICE);

        let channel = options
            .get::<Option<u8>>("channel")?
            .unwrap_or(DEFAULT_METAKON_CHANNEL);

        let value = options.get::<Option<i64>>("value")?.ok_or_else(|| {
            mlua::Error::RuntimeError(format!(
                "app.{} requires option \
                         'value'",
                command.name,
            ))
        })?;

        if !(command.minimum..=command.maximum).contains(&value) {
            return Err(mlua::Error::RuntimeError(format!(
                "app.{} value must be between \
                         {} and {}, received {}",
                command.name, command.minimum, command.maximum, value,
            )));
        }

        let request = WriteRegisterRequest::new(
            device,
            channel,
            command.register,
            (command.value_factory)(value),
        );

        send_application_command(&command_sender, UserCommand::WriteMetakon { request })
    })?;

    app.set(command.name, function)
}

fn validate_metakon_write_options(options: &Table, command_name: &str) -> mlua::Result<()> {
    for pair in options.pairs::<String, mlua::Value>() {
        let (key, _) = pair?;

        if !matches!(key.as_str(), "device" | "channel" | "value") {
            return Err(mlua::Error::RuntimeError(format!(
                "unknown app.{command_name} \
                     option '{key}'",
            )));
        }
    }

    Ok(())
}

fn metakon_int_value(value: i64) -> WriteRegisterValue {
    WriteRegisterValue::Int(value as i16)
}

fn metakon_uint_value(value: i64) -> WriteRegisterValue {
    WriteRegisterValue::Uint(value as u16)
}

fn metakon_ubyte_value(value: i64) -> WriteRegisterValue {
    WriteRegisterValue::Ubyte(value as u8)
}
