use std::time::Duration;

use crossbeam_channel::{RecvTimeoutError, Sender, bounded};
use mlua::{Lua, Table, UserData, UserDataMethods, Value};

use super::{
    controllers::{add_on_off_loop, add_pid_loop},
    conversion::{instrument_value_to_lua, virtual_instrument_value_from_lua},
    send_application_command,
    series::{
        LuaSeriesOptions, apply_series_options, connection_id_from_options, parse_series_options,
    },
};
use crate::{
    application_definition::ApplicationDefinition,
    connection::ConnectionId,
    data::NewSeries,
    instrument::{
        InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest, ParameterRange,
        virtual_instrument::{
            VirtualInstrumentDescriptor, VirtualInstrumentId, VirtualParameterDescriptor,
        },
    },
    process_control::ControlOutputTarget,
    user_command::{InstrumentCommand, SeriesCommand, UserCommand},
};

const INSTRUMENT_READ_TIMEOUT: Duration = Duration::from_secs(10);
const INSTRUMENT_WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const VIRTUAL_INSTRUMENT_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn register_virtual_instrument_controller(
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

        validate_virtual_instrument_options(&options)?;

        let connection_id = connection_id_from_options(&options, &application_definition)?;

        let id = options.get::<Option<u16>>("id")?.unwrap_or(1);

        if id == 0 {
            return Err(mlua::Error::RuntimeError(
                "app.virtual_instrument id \
                         must be greater than zero"
                    .to_owned(),
            ));
        }

        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &command_sender,
            InstrumentCommand::DescribeVirtualInstruments {
                connection_id,
                response_sender,
            }
            .into(),
        )?;

        let descriptors = match response_receiver.recv_timeout(VIRTUAL_INSTRUMENT_DISCOVERY_TIMEOUT)
        {
            Ok(Ok(descriptors)) => descriptors,

            Ok(Err(error)) => {
                return Err(mlua::Error::RuntimeError(format!(
                    "Virtual instrument \
                                     discovery failed: \
                                     {error}",
                )));
            }

            Err(RecvTimeoutError::Timeout) => {
                return Err(mlua::Error::RuntimeError(
                    "Timed out waiting for \
                                 virtual instrument \
                                 discovery"
                        .to_owned(),
                ));
            }

            Err(RecvTimeoutError::Disconnected) => {
                return Err(mlua::Error::RuntimeError(
                    "Virtual instrument \
                                 discovery response \
                                 channel is disconnected"
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
                    "Virtual instrument with \
                             id {id} was not found",
                ))
            })?;

        lua.create_userdata(LuaVirtualInstrument {
            connection_id,
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

        if !matches!(key.as_str(), "connection" | "id") {
            return Err(mlua::Error::RuntimeError(format!(
                "unknown \
                         app.virtual_instrument \
                         option '{key}'",
            )));
        }
    }

    Ok(())
}

#[derive(Clone)]
struct LuaVirtualInstrument {
    connection_id: ConnectionId,
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
        options: LuaSeriesOptions,
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
                     '{parameter_key}' cannot be \
                     added as a series",
            )));
        }

        let request =
            InstrumentReadRequest::virtual_instrument(self.descriptor.id(), parameter.id());

        let new_series = match &options.name {
            Some(name) => NewSeries::named_instrument(request, name),

            None => NewSeries::unnamed_instrument(request),
        };

        let new_series = apply_series_options(new_series, options, self.connection_id);

        send_application_command(&self.command_sender, SeriesCommand::Add(new_series).into())
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
            InstrumentCommand::Write {
                connection_id: self.connection_id,
                request,
                response_sender,
            }
            .into(),
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
            |_, instrument, (parameter_key, options): (String, Option<Value>)| {
                let options = parse_series_options(options)?;

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

        methods.add_method(
            "pid",
            |lua, instrument, (parameter_key, options): (String, Table)| {
                let parameter = instrument.parameter(&parameter_key)?;

                let output_target = ControlOutputTarget::virtual_instrument(
                    instrument.connection_id,
                    instrument.descriptor.id(),
                    parameter,
                )
                .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

                let controller = add_pid_loop(&instrument.command_sender, output_target, &options)?;

                lua.create_userdata(controller)
            },
        );

        methods.add_method(
            "on_off",
            |lua, instrument, (parameter_key, options): (String, Table)| {
                let parameter = instrument.parameter(&parameter_key)?;

                let output_target = ControlOutputTarget::virtual_instrument(
                    instrument.connection_id,
                    instrument.descriptor.id(),
                    parameter,
                )
                .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

                let controller =
                    add_on_off_loop(&instrument.command_sender, output_target, &options)?;

                lua.create_userdata(controller)
            },
        );
    }
}
