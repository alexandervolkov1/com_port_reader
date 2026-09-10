use std::time::Duration;

use crossbeam_channel::{RecvTimeoutError, Sender, bounded};
use mlua::{Lua, Table, UserData, UserDataMethods, Value};

use super::{
    controllers::{add_on_off_loop, add_pid_loop},
    conversion::{
        boolean_parameter_value, instrument_value_to_lua, scaled_integer_parameter_value,
    },
    send_application_command,
    series::{
        LuaSeriesOptions, apply_series_options, connection_id_from_options, parse_series_options,
    },
};
use crate::{
    application_definition::ApplicationDefinition,
    connection::ConnectionId,
    data::{DEFAULT_METAKON_CHANNEL, DEFAULT_METAKON_DEVICE, DEFAULT_METAKON_SCALE, NewSeries},
    instrument::{
        InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest, ParameterRange,
        metakon_5x3::{Metakon5x3, Metakon5x3Register, Metakon5x3Write},
    },
    process_control::ControlOutputTarget,
    user_command::{InstrumentCommand, SeriesCommand, UserCommand},
};

const INSTRUMENT_READ_TIMEOUT: Duration = Duration::from_secs(10);
const INSTRUMENT_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn register_metakon_controller(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
    application_definition: ApplicationDefinition,
) -> mlua::Result<()> {
    let function = lua.create_function(move |lua, options: Option<Table>| {
        let options = match options {
            Some(options) => options,

            None => lua.create_table()?,
        };

        validate_metakon_controller_options(&options)?;

        let connection_id = connection_id_from_options(&options, &application_definition)?;

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
            connection_id,
            instrument: Metakon5x3::new(device, channel),
            scale,
            command_sender: command_sender.clone(),
        })
    })?;

    app.set("metakon", function)
}

fn validate_metakon_controller_options(options: &Table) -> mlua::Result<()> {
    for pair in options.pairs::<String, Value>() {
        let (key, _) = pair?;

        if !matches!(key.as_str(), "connection" | "device" | "channel" | "scale") {
            return Err(mlua::Error::RuntimeError(format!(
                "unknown app.metakon option '{key}'",
            )));
        }
    }

    Ok(())
}

#[derive(Clone)]
struct LuaMetakon5x3 {
    connection_id: ConnectionId,
    instrument: Metakon5x3,
    scale: f64,
    command_sender: Sender<UserCommand>,
}

impl LuaMetakon5x3 {
    fn add_series(&self, new_series: NewSeries) -> mlua::Result<()> {
        send_application_command(&self.command_sender, SeriesCommand::Add(new_series).into())
    }

    fn add_parameter_series(
        &self,
        parameter: Metakon5x3Register,
        scale: f64,
        options: LuaSeriesOptions,
    ) -> mlua::Result<()> {
        let request = InstrumentReadRequest::metakon_5x3(self.instrument, parameter, scale);

        let new_series = match &options.name {
            Some(name) => NewSeries::named_instrument(request, name),

            None => NewSeries::unnamed_instrument(request),
        };

        let new_series = apply_series_options(new_series, options, self.connection_id);

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
            InstrumentCommand::Write {
                connection_id: self.connection_id,
                request,
                response_sender,
            }
            .into(),
        )?;

        match response_receiver.recv_timeout(INSTRUMENT_WRITE_TIMEOUT) {
            Ok(Ok(value)) => Ok(value),

            Ok(Err(error)) => Err(mlua::Error::RuntimeError(format!(
                "Instrument write failed: {error}",
            ))),

            Err(RecvTimeoutError::Timeout) => Err(mlua::Error::RuntimeError(
                "Timed out waiting for instrument write".to_owned(),
            )),

            Err(RecvTimeoutError::Disconnected) => Err(mlua::Error::RuntimeError(
                "Instrument write response channel \
                     is disconnected"
                    .to_owned(),
            )),
        }
    }

    fn parameter_scale(&self, parameter: Metakon5x3Register) -> f64 {
        parameter.engineering_scale(self.scale)
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
            InstrumentCommand::Read {
                connection_id: self.connection_id,
                request,
                response_sender,
            }
            .into(),
        )?;

        match response_receiver.recv_timeout(INSTRUMENT_READ_TIMEOUT) {
            Ok(Ok(value)) => Ok(value),

            Ok(Err(error)) => Err(mlua::Error::RuntimeError(format!(
                "Instrument read failed: {error}",
            ))),

            Err(RecvTimeoutError::Timeout) => Err(mlua::Error::RuntimeError(
                "Timed out waiting for instrument read".to_owned(),
            )),

            Err(RecvTimeoutError::Disconnected) => Err(mlua::Error::RuntimeError(
                "Instrument read response channel \
                     is disconnected"
                    .to_owned(),
            )),
        }
    }
}

fn metakon_parameter_from_key(key: &str) -> mlua::Result<Metakon5x3Register> {
    Metakon5x3Register::from_key(key).ok_or_else(|| {
        mlua::Error::RuntimeError(format!("Unknown Metakon 5X3 parameter: '{key}'",))
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
            "Metakon 5X3 parameter '{}' is read-only",
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
            unreachable!("read-only parameters were rejected above",)
        }
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
            |_, controller, (parameter_key, options): (String, Option<Value>)| {
                let parameter = metakon_parameter_from_key(&parameter_key)?;

                let scale = controller.parameter_scale(parameter);

                let options = parse_series_options(options)?;

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

        methods.add_method(
            "pid",
            |lua, controller, (parameter_key, options): (String, Table)| {
                let parameter = metakon_parameter_from_key(&parameter_key)?;

                let scale = controller.parameter_scale(parameter);

                let output_target = ControlOutputTarget::metakon_5x3(
                    controller.connection_id,
                    controller.instrument,
                    parameter,
                    scale,
                )
                .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

                let handle = add_pid_loop(&controller.command_sender, output_target, &options)?;

                lua.create_userdata(handle)
            },
        );

        methods.add_method(
            "on_off",
            |lua, controller, (parameter_key, options): (String, Table)| {
                let parameter = metakon_parameter_from_key(&parameter_key)?;

                let scale = controller.parameter_scale(parameter);

                let output_target = ControlOutputTarget::metakon_5x3(
                    controller.connection_id,
                    controller.instrument,
                    parameter,
                    scale,
                )
                .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

                let handle = add_on_off_loop(&controller.command_sender, output_target, &options)?;

                lua.create_userdata(handle)
            },
        );
    }
}
