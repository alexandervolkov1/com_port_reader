use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, bounded};
use mlua::{FromLua, Lua, Table, UserData, UserDataMethods, Value};

use crate::{
    application_definition::ApplicationDefinition,
    connection::ConnectionId,
    data::{
        DEFAULT_METAKON_CHANNEL, DEFAULT_METAKON_DEVICE, DEFAULT_METAKON_SCALE, NewFilteredSeries,
        NewSeries, SamplingInterval, SeriesColor,
    },
    instrument::{
        InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest, ParameterDescriptor,
        ParameterRange, ParameterValueType,
        metakon_5x3::{Metakon5x3, Metakon5x3Register, Metakon5x3Write},
        virtual_instrument::{
            VirtualInstrumentDescriptor, VirtualInstrumentId, VirtualParameterDescriptor,
        },
    },
    lua_application_script::LuaApplicationEvent,
    process_control::{ControlOutputTarget, NewPidLoop, PidGains, PidOutputLimits},
    signal_processing::{ControllerRequestError, SignalFilterDefinition},
    user_command::UserCommand,
};

const INSTRUMENT_READ_TIMEOUT: Duration = Duration::from_secs(10);

const INSTRUMENT_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

const CONTROLLER_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

const VIRTUAL_INSTRUMENT_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct LuaSeriesOptions {
    name: Option<String>,
    sampling_interval: Option<SamplingInterval>,
    color: Option<SeriesColor>,
}

pub fn install(
    lua: &Lua,
    command_sender: Sender<UserCommand>,
    application_event_sender: Sender<LuaApplicationEvent>,
    application_definition: &ApplicationDefinition,
) -> mlua::Result<()> {
    let app = lua.create_table()?;

    register_command(lua, &app, "start", command_sender.clone(), start_command)?;

    register_command(lua, &app, "stop", command_sender.clone(), stop_command)?;

    register_command(lua, &app, "clear", command_sender.clone(), clear_command)?;

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

    register_add_serial(
        lua,
        &app,
        command_sender.clone(),
        application_definition.clone(),
    )?;

    register_add_filter(lua, &app, command_sender.clone())?;

    register_set_filter(lua, &app, command_sender.clone())?;

    register_metakon_controller(
        lua,
        &app,
        command_sender.clone(),
        application_definition.clone(),
    )?;

    register_virtual_instrument_controller(
        lua,
        &app,
        command_sender.clone(),
        application_definition.clone(),
    )?;

    register_delete_series(lua, &app, command_sender.clone())?;

    register_rename_series(lua, &app, command_sender.clone())?;

    register_set_series_color(lua, &app, command_sender.clone())?;

    register_retry_series(lua, &app, command_sender.clone())?;

    register_command(
        lua,
        &app,
        "retry_all",
        command_sender.clone(),
        retry_all_command,
    )?;

    register_send_serial(lua, &app, command_sender, application_definition.clone())?;

    crate::lua_application_script::install(lua, &app, application_event_sender)?;

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

fn parse_series_options(value: Option<Value>) -> mlua::Result<LuaSeriesOptions> {
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

        let known_option = matches!(key.as_str(), "name" | "interval" | "color")
            || (allow_connection && key == "connection");

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

    let color = options
        .get::<Option<String>>("color")?
        .map(|value| value.parse::<SeriesColor>())
        .transpose()
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

    Ok(LuaSeriesOptions {
        name,
        sampling_interval,
        color,
    })
}

fn apply_series_options(
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

    new_series
}

fn register_add_serial(
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

        send_application_command(&command_sender, UserCommand::Add(new_series))
    })?;

    app.set("add_serial", function)
}

fn register_add_filter(
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

        send_application_command(&command_sender, UserCommand::AddFilter(filter))
    })?;

    app.set("filter", function)
}

fn register_set_filter(
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

        send_application_command(&command_sender, UserCommand::SetFilter { name, definition })
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

fn connection_id_from_options(
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

fn add_pid_loop(
    command_sender: &Sender<UserCommand>,
    output_target: ControlOutputTarget,
    options: &Table,
) -> mlua::Result<LuaControllerHandle> {
    validate_pid_options(options)?;

    let name = options
        .get::<Option<String>>("name")?
        .ok_or_else(|| mlua::Error::RuntimeError("PID option 'name' is required".to_owned()))?;

    let input_name = options
        .get::<Option<String>>("input")?
        .ok_or_else(|| mlua::Error::RuntimeError("PID option 'input' is required".to_owned()))?;

    let setpoint = options
        .get::<Option<f64>>("setpoint")?
        .ok_or_else(|| mlua::Error::RuntimeError("PID option 'setpoint' is required".to_owned()))?;

    let proportional = options
        .get::<Option<f64>>("kp")?
        .ok_or_else(|| mlua::Error::RuntimeError("PID option 'kp' is required".to_owned()))?;

    let integral = options.get::<Option<f64>>("ki")?.unwrap_or(0.0);

    let derivative = options.get::<Option<f64>>("kd")?.unwrap_or(0.0);

    let output_minimum = options.get::<Option<f64>>("output_min")?.ok_or_else(|| {
        mlua::Error::RuntimeError(
            "PID option 'output_min' \
                 is required"
                .to_owned(),
        )
    })?;

    let output_maximum = options.get::<Option<f64>>("output_max")?.ok_or_else(|| {
        mlua::Error::RuntimeError(
            "PID option 'output_max' \
                 is required"
                .to_owned(),
        )
    })?;

    let gains = PidGains::new(proportional, integral, derivative)
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

    let output_limits = PidOutputLimits::new(output_minimum, output_maximum)
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

    let pid_loop = NewPidLoop::new(
        name.clone(),
        input_name,
        output_target,
        setpoint,
        gains,
        output_limits,
    )
    .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

    send_application_command(command_sender, UserCommand::AddPidLoop(pid_loop))?;

    Ok(LuaControllerHandle {
        name,
        command_sender: command_sender.clone(),
    })
}

fn validate_pid_options(options: &Table) -> mlua::Result<()> {
    for pair in options.pairs::<String, Value>() {
        let (key, _) = pair?;

        if !matches!(
            key.as_str(),
            "name" | "input" | "setpoint" | "kp" | "ki" | "kd" | "output_min" | "output_max"
        ) {
            return Err(mlua::Error::RuntimeError(format!(
                "Unknown PID option '{key}'",
            )));
        }
    }

    Ok(())
}

fn register_metakon_controller(
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

fn register_virtual_instrument_controller(
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
            UserCommand::DescribeVirtualInstruments {
                connection_id,
                response_sender,
            },
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
struct LuaControllerHandle {
    name: String,
    command_sender: Sender<UserCommand>,
}

fn receive_controller_response<T>(
    receiver: Receiver<Result<T, ControllerRequestError>>,
    operation: &str,
) -> mlua::Result<T> {
    match receiver.recv_timeout(CONTROLLER_REQUEST_TIMEOUT) {
        Ok(Ok(value)) => Ok(value),

        Ok(Err(error)) => Err(mlua::Error::RuntimeError(format!(
            "Controller {operation} failed: {error}",
        ))),

        Err(RecvTimeoutError::Timeout) => Err(mlua::Error::RuntimeError(format!(
            "Timed out waiting for controller {operation}",
        ))),

        Err(RecvTimeoutError::Disconnected) => Err(mlua::Error::RuntimeError(format!(
            "Controller {operation} response channel is disconnected",
        ))),
    }
}

impl LuaControllerHandle {
    fn controller_parameters(&self) -> mlua::Result<Vec<ParameterDescriptor>> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            UserCommand::ControllerParameters {
                name: self.name.clone(),
                response_sender,
            },
        )?;

        receive_controller_response(response_receiver, "parameter discovery")
    }

    fn parameters(&self, lua: &Lua) -> mlua::Result<Table> {
        let descriptors = self.controller_parameters()?;

        let parameters = lua.create_table_with_capacity(descriptors.len(), 0)?;

        for (index, descriptor) in descriptors.into_iter().enumerate() {
            let entry = lua.create_table_with_capacity(0, 6)?;

            entry.set("key", descriptor.key)?;
            entry.set("name", descriptor.name)?;
            entry.set("access", descriptor.access.as_str())?;
            entry.set("value_type", descriptor.value_type.as_str())?;

            match descriptor.range {
                ParameterRange::Integer { minimum, maximum } => {
                    entry.set("minimum", minimum)?;
                    entry.set("maximum", maximum)?;
                }

                ParameterRange::Number { minimum, maximum } => {
                    entry.set("minimum", minimum)?;
                    entry.set("maximum", maximum)?;
                }
            }

            parameters.raw_set((index + 1) as i64, entry)?;
        }

        Ok(parameters)
    }

    fn parameter_descriptor(&self, key: &str) -> mlua::Result<ParameterDescriptor> {
        self.controller_parameters()?
            .into_iter()
            .find(|parameter| parameter.key == key)
            .ok_or_else(|| {
                mlua::Error::RuntimeError(format!(
                    "Controller '{}' has no parameter '{key}'",
                    self.name,
                ))
            })
    }

    fn read_parameter(&self, key: &str) -> mlua::Result<InstrumentValue> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            UserCommand::ReadControllerParameter {
                name: self.name.clone(),
                key: key.to_owned(),
                response_sender,
            },
        )?;

        receive_controller_response(response_receiver, "parameter read")
    }

    fn write_parameter(&self, key: &str, value: InstrumentValue) -> mlua::Result<InstrumentValue> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            UserCommand::WriteControllerParameter {
                name: self.name.clone(),
                key: key.to_owned(),
                value,
                response_sender,
            },
        )?;

        receive_controller_response(response_receiver, "parameter write")
    }

    fn configure(&self, lua: &Lua, updates: Table) -> mlua::Result<()> {
        let descriptors = self.controller_parameters()?;

        let mut resolved_updates = Vec::new();

        for pair in updates.pairs::<String, Value>() {
            let (key, value) = pair?;

            let descriptor = descriptors
                .iter()
                .find(|descriptor| descriptor.key == key)
                .copied()
                .ok_or_else(|| {
                    mlua::Error::RuntimeError(format!(
                        "Controller '{}' \
                                     has no parameter \
                                     '{key}'",
                        self.name,
                    ))
                })?;

            if !descriptor.access.writable() {
                return Err(mlua::Error::RuntimeError(format!(
                    "Controller \
                                 parameter '{}' \
                                 is read-only",
                    key,
                )));
            }

            let value = controller_value_from_lua(lua, descriptor, value)?;

            resolved_updates.push((key, value));
        }

        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            UserCommand::ConfigureController {
                name: self.name.clone(),
                updates: resolved_updates,
                response_sender,
            },
        )?;

        receive_controller_response(response_receiver, "configuration")
    }

    fn reset(&self) -> mlua::Result<()> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            UserCommand::ResetController {
                name: self.name.clone(),
                response_sender,
            },
        )?;

        receive_controller_response(response_receiver, "reset")
    }
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
                connection_id: self.connection_id,
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
                connection_id: self.connection_id,
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
    connection_id: ConnectionId,
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
            UserCommand::WriteInstrument {
                connection_id: self.connection_id,
                request,
                response_sender,
            },
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
            UserCommand::ReadInstrument {
                connection_id: self.connection_id,
                request,
                response_sender,
            },
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
            "Metakon 5X3 parameter '{}' expects \
                 a numeric value",
            parameter.descriptor().key,
        ))
    })?;

    if !engineering_value.is_finite() {
        return Err(mlua::Error::RuntimeError(format!(
            "Metakon 5X3 parameter '{}' must \
                 be finite",
            parameter.descriptor().key,
        )));
    }

    let raw_value = engineering_value / scale;
    let rounded_value = raw_value.round();

    let tolerance = raw_value.abs().max(1.0) * 1.0e-9;

    if (raw_value - rounded_value).abs() > tolerance {
        return Err(mlua::Error::RuntimeError(format!(
            "Value {engineering_value} cannot be \
                 represented by Metakon 5X3 parameter \
                 '{}' with scale {scale}",
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
            "Metakon 5X3 parameter '{}' expects \
             a Boolean value",
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

fn controller_value_from_lua(
    lua: &Lua,
    parameter: ParameterDescriptor,
    value: Value,
) -> mlua::Result<InstrumentValue> {
    match parameter.value_type {
        ParameterValueType::Boolean => {
            let value = bool::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Controller parameter '{}' expects a Boolean value",
                    parameter.key,
                ))
            })?;

            Ok(InstrumentValue::Boolean(value))
        }

        ParameterValueType::Integer => {
            let value = i64::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Controller parameter '{}' expects an integer value",
                    parameter.key,
                ))
            })?;

            Ok(InstrumentValue::Integer(value))
        }

        ParameterValueType::Number => {
            let value = f64::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Controller parameter '{}' expects a numeric value",
                    parameter.key,
                ))
            })?;

            if !value.is_finite() {
                return Err(mlua::Error::RuntimeError(format!(
                    "Controller parameter '{}' must be finite",
                    parameter.key,
                )));
            }

            Ok(InstrumentValue::Number(value))
        }
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
                         '{parameter_key}' must be \
                         finite",
                )));
            }

            Ok(InstrumentValue::Number(value))
        }
    }
}

impl UserData for LuaControllerHandle {
    fn add_methods<M>(methods: &mut M)
    where
        M: UserDataMethods<Self>,
    {
        methods.add_method("name", |_, controller, ()| Ok(controller.name.clone()));

        methods.add_method("parameters", |lua, controller, ()| {
            controller.parameters(lua)
        });

        methods.add_method("read", |_, controller, key: String| {
            let value = controller.read_parameter(&key)?;

            Ok(instrument_value_to_lua(value))
        });

        methods.add_method("write", |lua, controller, (key, value): (String, Value)| {
            let parameter = controller.parameter_descriptor(&key)?;

            if !parameter.access.writable() {
                return Err(mlua::Error::RuntimeError(format!(
                    "Controller parameter '{}' is read-only",
                    key,
                )));
            }

            let value = controller_value_from_lua(lua, parameter, value)?;

            let actual = controller.write_parameter(&key, value)?;

            Ok(instrument_value_to_lua(actual))
        });

        methods.add_method("configure", |lua, controller, updates: Table| {
            controller.configure(lua, updates)
        });

        methods.add_method("reset", |_, controller, ()| controller.reset());
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

fn register_set_series_color(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, (name, color): (String, Option<String>)| {
        let color = color
            .map(|value| value.parse::<SeriesColor>())
            .transpose()
            .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

        send_application_command(&command_sender, UserCommand::SetSeriesColor { name, color })
    })?;

    app.set("set_color", function)
}

fn register_retry_series(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, name: String| {
        send_application_command(&command_sender, UserCommand::Retry { name })
    })?;

    app.set("retry", function)
}

fn register_log(lua: &Lua, app: &Table, command_sender: Sender<UserCommand>) -> mlua::Result<()> {
    let function = lua.create_function(move |_, message: String| {
        send_application_command(&command_sender, UserCommand::Log { message })
    })?;

    app.set("log", function)
}

fn validate_send_serial_options(options: &Table) -> mlua::Result<()> {
    for pair in options.pairs::<String, Value>() {
        let (key, _) = pair?;

        if key != "connection" {
            return Err(mlua::Error::RuntimeError(format!(
                "unknown app.send_serial \
                         option '{key}'",
            )));
        }
    }

    Ok(())
}

fn register_send_serial(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
    application_definition: ApplicationDefinition,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, (command, options): (String, Option<Table>)| {
        let connection_id = match options {
            Some(options) => {
                validate_send_serial_options(&options)?;

                connection_id_from_options(&options, &application_definition)?
            }

            None => ConnectionId::PRIMARY,
        };

        send_application_command(
            &command_sender,
            UserCommand::SendSerial {
                connection_id,
                command,
            },
        )
    })?;

    app.set("send_serial", function)
}

fn send_application_command(
    command_sender: &Sender<UserCommand>,
    command: UserCommand,
) -> mlua::Result<()> {
    command_sender.send(command).map_err(|_| {
        mlua::Error::RuntimeError("application command channel is disconnected".to_owned())
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

fn start_emulator_command() -> UserCommand {
    UserCommand::StartEmulator
}

fn stop_emulator_command() -> UserCommand {
    UserCommand::StopEmulator
}

fn retry_all_command() -> UserCommand {
    UserCommand::RetryAll
}
