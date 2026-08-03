use std::time::Duration;

use crossbeam_channel::{RecvTimeoutError, Sender, bounded};
use mlua::{FromLua, Lua, Table, UserData, UserDataMethods, Value};

use crate::{
    data::{DEFAULT_METAKON_CHANNEL, DEFAULT_METAKON_DEVICE, DEFAULT_METAKON_SCALE, NewSeries},
    instrument::{
        InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest, ParameterRange,
        metakon_5x3::{Metakon5x3, Metakon5x3Register, Metakon5x3Write},
    },
    user_command::UserCommand,
};

const INSTRUMENT_READ_TIMEOUT: Duration = Duration::from_secs(10);

const INSTRUMENT_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

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

    register_log(lua, &app, command_sender.clone())?;

    register_add_serial(lua, &app, command_sender.clone())?;

    register_metakon_controller(lua, &app, command_sender.clone())?;

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
                "app.metakon scale must be \
                         finite and greater than zero"
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
                "unknown app.metakon \
                         option '{key}'",
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

    fn add_parameter_series(
        &self,
        parameter: Metakon5x3Register,
        scale: f64,
        name: Option<String>,
    ) -> mlua::Result<()> {
        let request = InstrumentReadRequest::metakon_5x3(self.instrument, parameter, scale);

        let new_series = match name {
            Some(name) => NewSeries::named_instrument(request, name),

            None => NewSeries::unnamed_instrument(request),
        };

        self.add_series(new_series)
    }

    fn write_request(
        &self,
        parameter: Metakon5x3Write,
        scale: f64,
    ) -> mlua::Result<InstrumentWriteRequest> {
        InstrumentWriteRequest::metakon_5x3(self.instrument, parameter, scale)
            .map_err(|error| mlua::Error::RuntimeError(error.to_string()))
    }

    fn write_and_wait(
        &self,
        parameter: Metakon5x3Write,
        scale: f64,
    ) -> mlua::Result<InstrumentValue> {
        let request = self.write_request(parameter, scale)?;

        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            UserCommand::WriteInstrument {
                request,
                response_sender,
            },
        )?;

        match response_receiver.recv_timeout(INSTRUMENT_WRITE_TIMEOUT) {
            Ok(Ok(value)) => Ok(value),

            Ok(Err(error)) => Err(mlua::Error::RuntimeError(format!(
                "Instrument write failed: \
                         {error}",
            ))),

            Err(RecvTimeoutError::Timeout) => Err(mlua::Error::RuntimeError(
                "Timed out waiting for \
                     instrument write"
                    .to_owned(),
            )),

            Err(RecvTimeoutError::Disconnected) => Err(mlua::Error::RuntimeError(
                "Instrument write response \
                     channel is disconnected"
                    .to_owned(),
            )),
        }
    }

    fn parameter_scale(&self, parameter: Metakon5x3Register) -> f64 {
        match parameter {
            Metakon5x3Register::Measurement
            | Metakon5x3Register::Setpoint
            | Metakon5x3Register::UpperSetpoint
            | Metakon5x3Register::UpperHysteresis
            | Metakon5x3Register::LowerSetpoint
            | Metakon5x3Register::LowerHysteresis => self.scale,

            Metakon5x3Register::ChannelType
            | Metakon5x3Register::ProportionalBand
            | Metakon5x3Register::IntegralTime
            | Metakon5x3Register::DerivativeTime
            | Metakon5x3Register::OutputPower
            | Metakon5x3Register::PwmPositive
            | Metakon5x3Register::PwmNegative
            | Metakon5x3Register::UpperOutput
            | Metakon5x3Register::LowerOutput => 1.0,
        }
    }

    fn parameters(&self, lua: &Lua) -> mlua::Result<Table> {
        let parameters = lua.create_table_with_capacity(Metakon5x3Register::ALL.len(), 0)?;

        for (index, parameter) in Metakon5x3Register::ALL.into_iter().enumerate() {
            let descriptor = parameter.descriptor();

            let scale = self.parameter_scale(parameter);

            let value_type = descriptor.value_type.scaled(scale);

            let range = descriptor.range.scaled(scale);

            let entry = lua.create_table_with_capacity(0, 7)?;

            entry.set("key", descriptor.key)?;

            entry.set("name", descriptor.name)?;

            entry.set("access", descriptor.access.as_str())?;

            entry.set("value_type", value_type.as_str())?;

            match range {
                ParameterRange::Integer { minimum, maximum } => {
                    entry.set("minimum", minimum)?;

                    entry.set("maximum", maximum)?;
                }

                ParameterRange::Number { minimum, maximum } => {
                    entry.set("minimum", minimum)?;

                    entry.set("maximum", maximum)?;
                }
            }

            entry.set("scale", scale)?;

            parameters.raw_set((index + 1) as i64, entry)?;
        }

        Ok(parameters)
    }

    fn read_parameter(&self, parameter: Metakon5x3Register) -> mlua::Result<InstrumentValue> {
        let scale = self.parameter_scale(parameter);

        let request = InstrumentReadRequest::metakon_5x3(self.instrument, parameter, scale);

        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            UserCommand::ReadInstrument {
                request,
                response_sender,
            },
        )?;

        match response_receiver.recv_timeout(INSTRUMENT_READ_TIMEOUT) {
            Ok(Ok(value)) => Ok(value),

            Ok(Err(error)) => Err(mlua::Error::RuntimeError(format!(
                "Instrument read failed: \
                         {error}",
            ))),

            Err(RecvTimeoutError::Timeout) => Err(mlua::Error::RuntimeError(
                "Timed out waiting for \
                     instrument read"
                    .to_owned(),
            )),

            Err(RecvTimeoutError::Disconnected) => Err(mlua::Error::RuntimeError(
                "Instrument read response \
                     channel is disconnected"
                    .to_owned(),
            )),
        }
    }
}

fn metakon_parameter_from_key(key: &str) -> mlua::Result<Metakon5x3Register> {
    Metakon5x3Register::from_key(key).ok_or_else(|| {
        mlua::Error::RuntimeError(format!(
            "Unknown Metakon 5X3 \
                     parameter: '{key}'",
        ))
    })
}

fn metakon_write_from_lua(
    lua: &Lua,
    parameter: Metakon5x3Register,
    value: Value,
    scale: f64,
) -> mlua::Result<Metakon5x3Write> {
    if !parameter.writable() {
        return Err(mlua::Error::RuntimeError(format!(
            "Metakon 5X3 parameter '{}' \
                 is read-only",
            parameter.descriptor().key,
        )));
    }

    match parameter {
        Metakon5x3Register::Setpoint => Ok(Metakon5x3Write::Setpoint(
            scaled_integer_parameter_value(lua, parameter, value, scale)?,
        )),

        Metakon5x3Register::ProportionalBand => Ok(Metakon5x3Write::ProportionalBand(
            scaled_integer_parameter_value(lua, parameter, value, scale)?,
        )),

        Metakon5x3Register::IntegralTime => Ok(Metakon5x3Write::IntegralTime(
            scaled_integer_parameter_value(lua, parameter, value, scale)?,
        )),

        Metakon5x3Register::DerivativeTime => Ok(Metakon5x3Write::DerivativeTime(
            scaled_integer_parameter_value(lua, parameter, value, scale)?,
        )),

        Metakon5x3Register::OutputPower => Ok(Metakon5x3Write::OutputPower(
            scaled_integer_parameter_value(lua, parameter, value, scale)?,
        )),

        Metakon5x3Register::UpperSetpoint => Ok(Metakon5x3Write::UpperSetpoint(
            scaled_integer_parameter_value(lua, parameter, value, scale)?,
        )),

        Metakon5x3Register::UpperHysteresis => Ok(Metakon5x3Write::UpperHysteresis(
            scaled_integer_parameter_value(lua, parameter, value, scale)?,
        )),

        Metakon5x3Register::UpperOutput => Ok(Metakon5x3Write::UpperOutput(
            boolean_parameter_value(lua, parameter, value)?,
        )),

        Metakon5x3Register::LowerSetpoint => Ok(Metakon5x3Write::LowerSetpoint(
            scaled_integer_parameter_value(lua, parameter, value, scale)?,
        )),

        Metakon5x3Register::LowerHysteresis => Ok(Metakon5x3Write::LowerHysteresis(
            scaled_integer_parameter_value(lua, parameter, value, scale)?,
        )),

        Metakon5x3Register::LowerOutput => Ok(Metakon5x3Write::LowerOutput(
            boolean_parameter_value(lua, parameter, value)?,
        )),

        Metakon5x3Register::ChannelType
        | Metakon5x3Register::Measurement
        | Metakon5x3Register::PwmPositive
        | Metakon5x3Register::PwmNegative => {
            unreachable!("read-only parameters were rejected above")
        }
    }
}

fn scaled_integer_parameter_value<T>(
    lua: &Lua,
    parameter: Metakon5x3Register,
    value: Value,
    scale: f64,
) -> mlua::Result<T>
where
    T: TryFrom<i64>,
{
    let engineering_value = f64::from_lua(value, lua).map_err(|_| {
        mlua::Error::RuntimeError(format!(
            "Metakon 5X3 parameter '{}' \
                 expects a numeric value",
            parameter.descriptor().key,
        ))
    })?;

    if !engineering_value.is_finite() {
        return Err(mlua::Error::RuntimeError(format!(
            "Metakon 5X3 parameter '{}' \
                 must be finite",
            parameter.descriptor().key,
        )));
    }

    let raw_value = engineering_value / scale;

    let rounded_value = raw_value.round();

    let tolerance = raw_value.abs().max(1.0) * 1.0e-9;

    if (raw_value - rounded_value).abs() > tolerance {
        return Err(mlua::Error::RuntimeError(format!(
            "Value {engineering_value} cannot \
                 be represented by Metakon 5X3 \
                 parameter '{}' with scale {scale}",
            parameter.descriptor().key,
        )));
    }

    let raw_value = rounded_value as i64;

    T::try_from(raw_value).map_err(|_| {
        mlua::Error::RuntimeError(format!(
            "Raw value {raw_value} does not fit \
             Metakon 5X3 parameter '{}'",
            parameter.descriptor().key,
        ))
    })
}

fn boolean_parameter_value(
    lua: &Lua,
    parameter: Metakon5x3Register,
    value: Value,
) -> mlua::Result<bool> {
    bool::from_lua(value, lua).map_err(|_| {
        mlua::Error::RuntimeError(format!(
            "Metakon 5X3 parameter \
                     '{}' expects a Boolean \
                     value",
            parameter.descriptor().key,
        ))
    })
}

fn instrument_value_to_lua(value: InstrumentValue) -> Value {
    match value {
        InstrumentValue::Boolean(value) => Value::Boolean(value),

        InstrumentValue::Integer(value) => Value::Integer(value),

        InstrumentValue::Number(value) => Value::Number(value),
    }
}

impl UserData for LuaMetakon5x3 {
    fn add_methods<M>(methods: &mut M)
    where
        M: UserDataMethods<Self>,
    {
        methods.add_method("parameters", |lua, controller, ()| {
            controller.parameters(lua)
        });

        methods.add_method(
            "add",
            |_, controller, (parameter_key, name): (String, Option<String>)| {
                let parameter = metakon_parameter_from_key(&parameter_key)?;

                let scale = controller.parameter_scale(parameter);

                controller.add_parameter_series(parameter, scale, name)
            },
        );

        methods.add_method("read", |_, controller, parameter_key: String| {
            let parameter = metakon_parameter_from_key(&parameter_key)?;

            let value = controller.read_parameter(parameter)?;

            Ok(instrument_value_to_lua(value))
        });

        methods.add_method(
            "write",
            |lua, controller, (parameter_key, value): (String, Value)| {
                let parameter = metakon_parameter_from_key(&parameter_key)?;

                let scale = controller.parameter_scale(parameter);

                let write = metakon_write_from_lua(lua, parameter, value, scale)?;

                let actual_value = controller.write_and_wait(write, scale)?;

                Ok(instrument_value_to_lua(actual_value))
            },
        );
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

fn register_log(lua: &Lua, app: &Table, command_sender: Sender<UserCommand>) -> mlua::Result<()> {
    let function = lua.create_function(move |_, message: String| {
        send_application_command(&command_sender, UserCommand::Log { message })
    })?;

    app.set("log", function)
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
