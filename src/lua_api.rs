use std::time::Duration;

use crossbeam_channel::{RecvTimeoutError, Sender, bounded};
use mlua::{FromLua, Lua, Table, UserData, UserDataMethods, Value};

use crate::{
    data::{
        DEFAULT_METAKON_CHANNEL, DEFAULT_METAKON_DEVICE, DEFAULT_METAKON_SCALE, NewSeries,
        SamplingInterval,
    },
    instrument::{
        InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest, ParameterRange,
        ParameterValueType,
        metakon_5x3::{Metakon5x3, Metakon5x3Register, Metakon5x3Write},
        virtual_instrument::{
            VirtualInstrumentDescriptor, VirtualInstrumentId, VirtualParameterDescriptor,
        },
    },
    user_command::UserCommand,
};

const INSTRUMENT_READ_TIMEOUT: Duration = Duration::from_secs(10);
const INSTRUMENT_WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const VIRTUAL_INSTRUMENT_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct SeriesAddOptions {
    name: Option<String>,
    sampling_interval: Option<SamplingInterval>,
}

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

    register_virtual_instrument_controller(lua, &app, command_sender.clone())?;

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

fn parse_series_add_options(lua: &Lua, value: Value) -> mlua::Result<SeriesAddOptions> {
    match value {
        Value::Nil => Ok(SeriesAddOptions::default()),

        Value::String(_) => Ok(SeriesAddOptions {
            name: Some(String::from_lua(value, lua)?),
            sampling_interval: None,
        }),

        Value::Table(table) => {
            validate_series_add_options(&table)?;

            let name = table.get::<Option<String>>("name")?;

            let interval_seconds = table.get::<Option<f64>>("interval")?;

            let sampling_interval = interval_seconds
                .map(SamplingInterval::from_secs_f64)
                .transpose()
                .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

            Ok(SeriesAddOptions {
                name,
                sampling_interval,
            })
        }

        value => Err(mlua::Error::RuntimeError(format!(
            "Series options must be a name, \
                 a table or nil, received {value:?}",
        ))),
    }
}

fn validate_series_add_options(table: &Table) -> mlua::Result<()> {
    for pair in table.pairs::<String, Value>() {
        let (key, _) = pair?;

        if !matches!(key.as_str(), "name" | "interval") {
            return Err(mlua::Error::RuntimeError(format!(
                "Unknown series option '{key}'",
            )));
        }
    }

    Ok(())
}

fn apply_sampling_interval(new_series: NewSeries, interval: Option<SamplingInterval>) -> NewSeries {
    match interval {
        Some(interval) => new_series.with_sampling_interval(interval),

        None => new_series,
    }
}

fn register_add_serial(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |lua, (command, options): (String, Value)| {
        let options = parse_series_add_options(lua, options)?;

        let new_series = match options.name {
            Some(name) => NewSeries::named_serial_command(command, name),

            None => NewSeries::unnamed_serial_command(command),
        };

        let new_series = apply_sampling_interval(new_series, options.sampling_interval);

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

fn register_virtual_instrument_controller(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |lua, options: Option<Table>| {
        let options = match options {
            Some(options) => options,

            None => lua.create_table()?,
        };

        validate_virtual_instrument_options(&options)?;

        let id = options.get::<Option<u16>>("id")?.unwrap_or(1);

        if id == 0 {
            return Err(mlua::Error::RuntimeError(
                "app.virtual_instrument id must be \
                     greater than zero"
                    .to_owned(),
            ));
        }

        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &command_sender,
            UserCommand::DescribeVirtualInstruments { response_sender },
        )?;

        let descriptors = match response_receiver.recv_timeout(VIRTUAL_INSTRUMENT_DISCOVERY_TIMEOUT)
        {
            Ok(Ok(descriptors)) => descriptors,

            Ok(Err(error)) => {
                return Err(mlua::Error::RuntimeError(format!(
                    "Virtual instrument discovery \
                             failed: {error}",
                )));
            }

            Err(RecvTimeoutError::Timeout) => {
                return Err(mlua::Error::RuntimeError(
                    "Timed out waiting for virtual \
                         instrument discovery"
                        .to_owned(),
                ));
            }

            Err(RecvTimeoutError::Disconnected) => {
                return Err(mlua::Error::RuntimeError(
                    "Virtual instrument discovery \
                         response channel is disconnected"
                        .to_owned(),
                ));
            }
        };

        let requested_id = VirtualInstrumentId::new(id);

        let descriptor = descriptors
            .into_iter()
            .find(|descriptor| descriptor.id() == requested_id)
            .ok_or_else(|| {
                mlua::Error::RuntimeError(format!(
                    "Virtual instrument with id {id} \
                         was not found",
                ))
            })?;

        lua.create_userdata(LuaVirtualInstrument {
            id,
            descriptor,
            command_sender: command_sender.clone(),
        })
    })?;

    app.set("virtual_instrument", function)
}

fn validate_virtual_instrument_options(options: &Table) -> mlua::Result<()> {
    for pair in options.pairs::<String, Value>() {
        let (key, _) = pair?;

        if key != "id" {
            return Err(mlua::Error::RuntimeError(format!(
                "unknown app.virtual_instrument \
                 option '{key}'",
            )));
        }
    }

    Ok(())
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
struct LuaVirtualInstrument {
    id: u16,
    descriptor: VirtualInstrumentDescriptor,
    command_sender: Sender<UserCommand>,
}

impl LuaVirtualInstrument {
    fn parameters(&self, lua: &Lua) -> mlua::Result<Table> {
        let descriptors = self.descriptor.parameters();

        let parameters = lua.create_table_with_capacity(descriptors.len(), 0)?;

        for (index, descriptor) in descriptors.iter().enumerate() {
            let entry = lua.create_table_with_capacity(0, 8)?;

            entry.set("key", descriptor.key())?;

            entry.set("name", descriptor.name())?;

            entry.set("access", descriptor.access().as_str())?;

            entry.set("value_type", descriptor.value_type().as_str())?;

            entry.set("series", descriptor.series())?;

            if let Some(unit) = descriptor.unit() {
                entry.set("unit", unit)?;
            }

            match descriptor.range() {
                Some(ParameterRange::Integer { minimum, maximum }) => {
                    entry.set("minimum", minimum)?;
                    entry.set("maximum", maximum)?;
                }

                Some(ParameterRange::Number { minimum, maximum }) => {
                    entry.set("minimum", minimum)?;
                    entry.set("maximum", maximum)?;
                }

                None => {}
            }

            parameters.raw_set((index + 1) as i64, entry)?;
        }

        Ok(parameters)
    }

    fn add_parameter_series(
        &self,
        parameter_key: &str,
        options: SeriesAddOptions,
    ) -> mlua::Result<()> {
        let parameter = self.parameter(parameter_key)?;

        if !parameter.access().readable() {
            return Err(mlua::Error::RuntimeError(format!(
                "Virtual instrument parameter \
                 '{parameter_key}' is write-only",
            )));
        }

        if !parameter.series() {
            return Err(mlua::Error::RuntimeError(format!(
                "Virtual instrument parameter \
                 '{parameter_key}' cannot be added \
                 as a series",
            )));
        }

        let request =
            InstrumentReadRequest::virtual_instrument(self.descriptor.id(), parameter.id());

        let new_series = match options.name {
            Some(name) => NewSeries::named_instrument(request, name),

            None => NewSeries::unnamed_instrument(request),
        };

        let new_series = apply_sampling_interval(new_series, options.sampling_interval);

        send_application_command(&self.command_sender, UserCommand::Add(new_series))
    }

    fn parameter(&self, key: &str) -> mlua::Result<&VirtualParameterDescriptor> {
        self.descriptor
            .parameters()
            .iter()
            .find(|parameter| parameter.key() == key)
            .ok_or_else(|| {
                mlua::Error::RuntimeError(format!(
                    "Virtual instrument '{}' has no \
                     parameter '{key}'",
                    self.descriptor.name(),
                ))
            })
    }

    fn read_parameter(&self, parameter_key: &str) -> mlua::Result<InstrumentValue> {
        let parameter = self.parameter(parameter_key)?;

        if !parameter.access().readable() {
            return Err(mlua::Error::RuntimeError(format!(
                "Virtual instrument parameter \
                 '{parameter_key}' is write-only",
            )));
        }

        let request =
            InstrumentReadRequest::virtual_instrument(self.descriptor.id(), parameter.id());

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
                "Virtual instrument read failed: \
                     {error}",
            ))),

            Err(RecvTimeoutError::Timeout) => Err(mlua::Error::RuntimeError(
                "Timed out waiting for virtual \
                     instrument read"
                    .to_owned(),
            )),

            Err(RecvTimeoutError::Disconnected) => Err(mlua::Error::RuntimeError(
                "Virtual instrument read response \
                     channel is disconnected"
                    .to_owned(),
            )),
        }
    }

    fn write_parameter(
        &self,
        parameter_key: &str,
        value: InstrumentValue,
    ) -> mlua::Result<InstrumentValue> {
        let parameter = self.parameter(parameter_key)?;

        if !parameter.access().writable() {
            return Err(mlua::Error::RuntimeError(format!(
                "Virtual instrument parameter \
                 '{parameter_key}' is read-only",
            )));
        }

        let request =
            InstrumentWriteRequest::virtual_instrument(self.descriptor.id(), parameter.id(), value);

        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            UserCommand::WriteInstrument {
                request,
                response_sender,
            },
        )?;

        match response_receiver.recv_timeout(INSTRUMENT_WRITE_TIMEOUT) {
            Ok(Ok(actual_value)) => Ok(actual_value),

            Ok(Err(error)) => Err(mlua::Error::RuntimeError(format!(
                "Virtual instrument write failed: \
                     {error}",
            ))),

            Err(RecvTimeoutError::Timeout) => Err(mlua::Error::RuntimeError(
                "Timed out waiting for virtual \
                     instrument write"
                    .to_owned(),
            )),

            Err(RecvTimeoutError::Disconnected) => Err(mlua::Error::RuntimeError(
                "Virtual instrument write response \
                     channel is disconnected"
                    .to_owned(),
            )),
        }
    }
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
        options: SeriesAddOptions,
    ) -> mlua::Result<()> {
        let request = InstrumentReadRequest::metakon_5x3(self.instrument, parameter, scale);

        let new_series = match options.name {
            Some(name) => NewSeries::named_instrument(request, name),

            None => NewSeries::unnamed_instrument(request),
        };

        let new_series = apply_sampling_interval(new_series, options.sampling_interval);

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
            | Metakon5x3Register::ProportionalBand
            | Metakon5x3Register::UpperSetpoint
            | Metakon5x3Register::UpperHysteresis
            | Metakon5x3Register::LowerSetpoint
            | Metakon5x3Register::LowerHysteresis => self.scale,

            Metakon5x3Register::ChannelType
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

fn virtual_instrument_value_from_lua(
    lua: &Lua,
    parameter: &VirtualParameterDescriptor,
    value: Value,
) -> mlua::Result<InstrumentValue> {
    let parameter_key = parameter.key();

    match parameter.value_type() {
        ParameterValueType::Boolean => {
            let value = bool::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Virtual instrument parameter \
                         '{parameter_key}' expects a \
                         Boolean value",
                ))
            })?;

            Ok(InstrumentValue::Boolean(value))
        }

        ParameterValueType::Integer => {
            let value = i64::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Virtual instrument parameter \
                         '{parameter_key}' expects an \
                         integer value",
                ))
            })?;

            Ok(InstrumentValue::Integer(value))
        }

        ParameterValueType::Number => {
            let value = f64::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Virtual instrument parameter \
                         '{parameter_key}' expects a \
                         numeric value",
                ))
            })?;

            if !value.is_finite() {
                return Err(mlua::Error::RuntimeError(format!(
                    "Virtual instrument parameter \
                         '{parameter_key}' must be finite",
                )));
            }

            Ok(InstrumentValue::Number(value))
        }
    }
}

impl UserData for LuaVirtualInstrument {
    fn add_methods<M>(methods: &mut M)
    where
        M: UserDataMethods<Self>,
    {
        methods.add_method("id", |_, instrument, ()| Ok(instrument.id));

        methods.add_method("name", |_, instrument, ()| {
            Ok(instrument.descriptor.name().to_owned())
        });

        methods.add_method("parameters", |lua, instrument, ()| {
            instrument.parameters(lua)
        });

        methods.add_method(
            "add",
            |lua, instrument, (parameter_key, options): (String, Value)| {
                let options = parse_series_add_options(lua, options)?;

                instrument.add_parameter_series(&parameter_key, options)
            },
        );

        methods.add_method("read", |_, instrument, parameter_key: String| {
            let value = instrument.read_parameter(&parameter_key)?;

            Ok(instrument_value_to_lua(value))
        });

        methods.add_method(
            "write",
            |lua, instrument, (parameter_key, value): (String, Value)| {
                let parameter = instrument.parameter(&parameter_key)?;

                let value = virtual_instrument_value_from_lua(lua, parameter, value)?;

                let actual_value = instrument.write_parameter(&parameter_key, value)?;

                Ok(instrument_value_to_lua(actual_value))
            },
        );
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
            |lua, controller, (parameter_key, options): (String, Value)| {
                let parameter = metakon_parameter_from_key(&parameter_key)?;

                let scale = controller.parameter_scale(parameter);

                let options = parse_series_add_options(lua, options)?;

                controller.add_parameter_series(parameter, scale, options)
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
