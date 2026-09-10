use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, bounded};
use mlua::{Lua, Table, UserData, UserDataMethods, Value};

use super::{
    conversion::{controller_value_from_lua, instrument_value_to_lua, reference_value_from_lua},
    send_application_command,
    series::parse_series_options,
};
use crate::{
    connection::ConnectionId,
    data::NewControllerDiagnosticSeries,
    instrument::{InstrumentValue, ParameterDescriptor, ParameterRange},
    process_control::{
        ControlLoopState, ControlOutputTarget, ControllerDiagnostic, FurnaceController,
        FurnaceGains, FurnaceModel, FurnaceOutputLimits, NewController, OnOffController,
        PidController, PidGains, PidOutputLimits, ReferenceKind, ReferenceSource,
    },
    user_command::{ControllerCommand, UserCommand},
};

const CONTROLLER_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn add_pid_loop(
    command_sender: &Sender<UserCommand>,
    output_target: ControlOutputTarget,
    options: &Table,
) -> mlua::Result<LuaControllerHandle> {
    validate_pid_options(options)?;

    let output_target = configure_safe_output(output_target, options)?;

    let connection_id = output_target.connection_id();

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

    let controller = PidController::with_output_limits(setpoint, gains, output_limits)
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

    let new_controller = NewController::new(name.clone(), input_name, output_target, controller)
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

    send_application_command(
        command_sender,
        ControllerCommand::Add(new_controller).into(),
    )?;

    Ok(LuaControllerHandle {
        name,
        connection_id,
        command_sender: command_sender.clone(),
    })
}

pub(super) fn add_furnace_loop(
    command_sender: &Sender<UserCommand>,
    output_target: ControlOutputTarget,
    options: &Table,
) -> mlua::Result<LuaControllerHandle> {
    for pair in options.pairs::<String, Value>() {
        let (key, _) = pair?;

        if !matches!(
            key.as_str(),
            "name"
                | "input"
                | "setpoint"
                | "kp"
                | "ki"
                | "output_min"
                | "output_max"
                | "ambient_temperature"
                | "max_power"
                | "heater_lag"
                | "linear_loss"
                | "radiation_loss_1000c"
                | "safe_output"
        ) {
            return Err(mlua::Error::RuntimeError(format!(
                "Unknown furnace option '{key}'"
            )));
        }
    }

    let required_number = |key: &str| -> mlua::Result<f64> {
        options
            .get::<Option<f64>>(key)?
            .ok_or_else(|| mlua::Error::RuntimeError(format!("Furnace option '{key}' is required")))
    };

    let name = options
        .get::<Option<String>>("name")?
        .ok_or_else(|| mlua::Error::RuntimeError("Furnace option 'name' is required".to_owned()))?;
    let input = options.get::<Option<String>>("input")?.ok_or_else(|| {
        mlua::Error::RuntimeError("Furnace option 'input' is required".to_owned())
    })?;

    let output_target = configure_safe_output(output_target, options)?;
    let connection_id = output_target.connection_id();
    let gains = FurnaceGains::new(
        required_number("kp")?,
        options.get::<Option<f64>>("ki")?.unwrap_or(0.0),
    )
    .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;
    let model = FurnaceModel::new(
        required_number("ambient_temperature")?,
        required_number("max_power")?,
        required_number("heater_lag")?,
        required_number("linear_loss")?,
        required_number("radiation_loss_1000c")?,
    )
    .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;
    let limits = FurnaceOutputLimits::new(
        required_number("output_min")?,
        required_number("output_max")?,
    )
    .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;
    let controller = FurnaceController::new(required_number("setpoint")?, gains, model, limits)
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;
    let new_controller = NewController::new(name.clone(), input, output_target, controller)
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

    send_application_command(
        command_sender,
        ControllerCommand::Add(new_controller).into(),
    )?;

    Ok(LuaControllerHandle {
        name,
        connection_id,
        command_sender: command_sender.clone(),
    })
}

fn configure_safe_output(
    output_target: ControlOutputTarget,
    options: &Table,
) -> mlua::Result<ControlOutputTarget> {
    let Some(safe_output) = options.get::<Option<f64>>("safe_output")? else {
        return Ok(output_target);
    };

    output_target.with_safe_value(safe_output).map_err(|error| {
        mlua::Error::RuntimeError(format!(
            "Invalid safe output: \
                     {error}",
        ))
    })
}

pub(super) fn add_on_off_loop(
    command_sender: &Sender<UserCommand>,
    output_target: ControlOutputTarget,
    options: &Table,
) -> mlua::Result<LuaControllerHandle> {
    validate_on_off_options(options)?;

    let output_target = configure_safe_output(output_target, options)?;

    let connection_id = output_target.connection_id();

    let name = options
        .get::<Option<String>>("name")?
        .ok_or_else(|| mlua::Error::RuntimeError("On/off option 'name' is required".to_owned()))?;

    let input_name = options
        .get::<Option<String>>("input")?
        .ok_or_else(|| mlua::Error::RuntimeError("On/off option 'input' is required".to_owned()))?;

    let setpoint = options.get::<Option<f64>>("setpoint")?.ok_or_else(|| {
        mlua::Error::RuntimeError("On/off option 'setpoint' is required".to_owned())
    })?;

    let hysteresis = options.get::<Option<f64>>("hysteresis")?.ok_or_else(|| {
        mlua::Error::RuntimeError("On/off option 'hysteresis' is required".to_owned())
    })?;

    let output_off = options.get::<Option<f64>>("output_off")?.ok_or_else(|| {
        mlua::Error::RuntimeError("On/off option 'output_off' is required".to_owned())
    })?;

    let output_on = options.get::<Option<f64>>("output_on")?.ok_or_else(|| {
        mlua::Error::RuntimeError("On/off option 'output_on' is required".to_owned())
    })?;

    let controller = OnOffController::new(setpoint, hysteresis, output_off, output_on)
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

    let new_controller = NewController::new(name.clone(), input_name, output_target, controller)
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

    send_application_command(
        command_sender,
        ControllerCommand::Add(new_controller).into(),
    )?;

    Ok(LuaControllerHandle {
        name,
        connection_id,
        command_sender: command_sender.clone(),
    })
}

fn validate_pid_options(options: &Table) -> mlua::Result<()> {
    for pair in options.pairs::<String, Value>() {
        let (key, _) = pair?;

        if !matches!(
            key.as_str(),
            "name"
                | "input"
                | "setpoint"
                | "kp"
                | "ki"
                | "kd"
                | "output_min"
                | "output_max"
                | "safe_output"
        ) {
            return Err(mlua::Error::RuntimeError(format!(
                "Unknown PID option '{key}'",
            )));
        }
    }

    Ok(())
}

pub(super) fn validate_on_off_options(options: &Table) -> mlua::Result<()> {
    for pair in options.pairs::<String, Value>() {
        let (key, _) = pair?;

        if !matches!(
            key.as_str(),
            "name"
                | "input"
                | "setpoint"
                | "hysteresis"
                | "output_off"
                | "output_on"
                | "safe_output"
        ) {
            return Err(mlua::Error::RuntimeError(format!(
                "Unknown on/off option \
                         '{key}'",
            )));
        }
    }

    Ok(())
}

#[derive(Clone)]
pub(super) struct LuaControllerHandle {
    pub(super) name: String,
    pub(super) connection_id: ConnectionId,
    pub(super) command_sender: Sender<UserCommand>,
}

fn receive_controller_response<T, E>(
    receiver: Receiver<Result<T, E>>,
    operation: &str,
) -> mlua::Result<T>
where
    E: std::fmt::Display,
{
    match receiver.recv_timeout(CONTROLLER_REQUEST_TIMEOUT) {
        Ok(Ok(value)) => Ok(value),

        Ok(Err(error)) => Err(mlua::Error::RuntimeError(format!(
            "Controller {operation} failed: \
                     {error}",
        ))),

        Err(RecvTimeoutError::Timeout) => Err(mlua::Error::RuntimeError(format!(
            "Timed out waiting for \
                     controller {operation}",
        ))),

        Err(RecvTimeoutError::Disconnected) => Err(mlua::Error::RuntimeError(format!(
            "Controller {operation} response \
                     channel is disconnected",
        ))),
    }
}

fn parameter_descriptors_to_lua(
    lua: &Lua,
    descriptors: Vec<ParameterDescriptor>,
) -> mlua::Result<Table> {
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

impl LuaControllerHandle {
    fn controller_parameters(&self) -> mlua::Result<Vec<ParameterDescriptor>> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::Parameters {
                name: self.name.clone(),
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "parameter discovery")
    }

    fn parameters(&self, lua: &Lua) -> mlua::Result<Table> {
        let descriptors = self.controller_parameters()?;

        parameter_descriptors_to_lua(lua, descriptors)
    }

    fn reference_kind(&self) -> mlua::Result<Option<String>> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::ReferenceKind {
                name: self.name.clone(),
                response_sender,
            }
            .into(),
        )?;

        let kind: Option<ReferenceKind> =
            receive_controller_response(response_receiver, "reference kind discovery")?;

        Ok(kind.map(|kind| kind.as_str().to_owned()))
    }

    fn controller_reference_parameters(&self) -> mlua::Result<Vec<ParameterDescriptor>> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::ReferenceParameters {
                name: self.name.clone(),
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "reference parameter discovery")
    }

    fn reference_parameters(&self, lua: &Lua) -> mlua::Result<Table> {
        let descriptors = self.controller_reference_parameters()?;

        parameter_descriptors_to_lua(lua, descriptors)
    }

    fn reference_parameter_descriptor(&self, key: &str) -> mlua::Result<ParameterDescriptor> {
        self.controller_reference_parameters()?
            .into_iter()
            .find(|parameter| parameter.key == key)
            .ok_or_else(|| {
                mlua::Error::RuntimeError(format!(
                    "Controller '{}' \
                         reference has no \
                         parameter '{key}'",
                    self.name,
                ))
            })
    }

    fn read_reference_parameter(&self, key: &str) -> mlua::Result<InstrumentValue> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::ReadReferenceParameter {
                name: self.name.clone(),
                key: key.to_owned(),
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "reference parameter read")
    }

    fn write_reference_parameter(
        &self,
        key: &str,
        value: InstrumentValue,
    ) -> mlua::Result<InstrumentValue> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::WriteReferenceParameter {
                name: self.name.clone(),
                key: key.to_owned(),
                value,
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "reference parameter write")
    }

    fn configure_reference(&self, lua: &Lua, updates: Table) -> mlua::Result<()> {
        let descriptors = self.controller_reference_parameters()?;

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
                             reference has no \
                             parameter '{key}'",
                        self.name,
                    ))
                })?;

            if !descriptor.access.writable() {
                return Err(mlua::Error::RuntimeError(format!(
                    "Reference parameter \
                             '{}' is read-only",
                    key,
                )));
            }

            let value = reference_value_from_lua(lua, descriptor, value)?;

            resolved_updates.push((key, value));
        }

        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::ConfigureReference {
                name: self.name.clone(),
                updates: resolved_updates,
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "reference configuration")
    }

    fn set_reference(&self, source: ReferenceSource) -> mlua::Result<()> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::SetReference {
                name: self.name.clone(),
                source,
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "reference replacement")
    }

    fn set_fixed_reference(&self, value: f64) -> mlua::Result<()> {
        let source = ReferenceSource::fixed(value).map_err(|error| {
            mlua::Error::RuntimeError(format!(
                "Invalid fixed \
                                     reference: \
                                     {error}",
            ))
        })?;

        self.set_reference(source)
    }

    fn set_ramp_reference(&self, options: &Table) -> mlua::Result<()> {
        let source = ramp_reference_from_options(options)?;

        self.set_reference(source)
    }

    fn add_diagnostic_series(
        &self,
        diagnostic_key: &str,
        options: Option<Value>,
    ) -> mlua::Result<()> {
        let diagnostic = ControllerDiagnostic::from_key(diagnostic_key).ok_or_else(|| {
            mlua::Error::RuntimeError(format!(
                "Unknown controller \
                         diagnostic \
                         '{diagnostic_key}'",
            ))
        })?;

        let options = parse_series_options(options)?;

        if options.sampling_interval.is_some() {
            return Err(mlua::Error::RuntimeError(
                "Controller diagnostic \
                     series cannot have \
                     'interval'"
                    .to_owned(),
            ));
        }

        let supported = self.controller_diagnostics()?;

        if !supported.contains(&diagnostic) {
            return Err(mlua::Error::RuntimeError(format!(
                "Controller '{}' does not \
                         support diagnostic '{}'",
                self.name, diagnostic,
            )));
        }

        let name = options
            .name
            .unwrap_or_else(|| format!("{}_{}", self.name, diagnostic.key(),));

        let mut series = NewControllerDiagnosticSeries::new(self.name.clone(), diagnostic, name)
            .with_connection(self.connection_id);

        if let Some(color) = options.color {
            series = series.with_color(color);
        }

        send_application_command(
            &self.command_sender,
            ControllerCommand::AddDiagnostic(series).into(),
        )
    }

    fn controller_diagnostics(&self) -> mlua::Result<Vec<ControllerDiagnostic>> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::Diagnostics {
                name: self.name.clone(),
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "diagnostic discovery")
    }

    fn diagnostics(&self, lua: &Lua) -> mlua::Result<Table> {
        let diagnostics = self.controller_diagnostics()?;

        let table = lua.create_table_with_capacity(diagnostics.len(), 0)?;

        for (index, diagnostic) in diagnostics.into_iter().enumerate() {
            table.raw_set((index + 1) as i64, diagnostic.key())?;
        }

        Ok(table)
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
            ControllerCommand::ReadParameter {
                name: self.name.clone(),
                key: key.to_owned(),
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "parameter read")
    }

    fn write_parameter(&self, key: &str, value: InstrumentValue) -> mlua::Result<InstrumentValue> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::WriteParameter {
                name: self.name.clone(),
                key: key.to_owned(),
                value,
                response_sender,
            }
            .into(),
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
            ControllerCommand::Configure {
                name: self.name.clone(),
                updates: resolved_updates,
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "configuration")
    }

    fn set_input(&self, input_name: &str) -> mlua::Result<()> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::SetInput {
                name: self.name.clone(),
                input_name: input_name.to_owned(),
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "input change")
    }

    fn state(&self) -> mlua::Result<ControlLoopState> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::State {
                name: self.name.clone(),
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "state read")
    }

    fn pause(&self) -> mlua::Result<()> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::Pause {
                name: self.name.clone(),
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "pause")
    }

    fn remove(&self) -> mlua::Result<()> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::Remove {
                name: self.name.clone(),
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "removal")
    }

    fn resume(&self) -> mlua::Result<()> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::Resume {
                name: self.name.clone(),
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "resume")
    }

    fn reset_integral(&self) -> mlua::Result<()> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::ResetIntegral {
                name: self.name.clone(),
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "integral reset")
    }

    fn reset(&self) -> mlua::Result<()> {
        let (response_sender, response_receiver) = bounded(1);

        send_application_command(
            &self.command_sender,
            ControllerCommand::Reset {
                name: self.name.clone(),
                response_sender,
            }
            .into(),
        )?;

        receive_controller_response(response_receiver, "reset")
    }
}

fn ramp_reference_from_options(options: &Table) -> mlua::Result<ReferenceSource> {
    for pair in options.pairs::<String, Value>() {
        let (key, _) = pair?;

        if !matches!(key.as_str(), "start" | "target" | "rate") {
            return Err(mlua::Error::RuntimeError(format!(
                "Unknown ramp reference \
                         option '{key}'",
            )));
        }
    }

    let start = options.get::<Option<f64>>("start")?.ok_or_else(|| {
        mlua::Error::RuntimeError(
            "Ramp reference option \
                 'start' is required"
                .to_owned(),
        )
    })?;

    let target = options.get::<Option<f64>>("target")?.ok_or_else(|| {
        mlua::Error::RuntimeError(
            "Ramp reference option \
                 'target' is required"
                .to_owned(),
        )
    })?;

    let rate = options.get::<Option<f64>>("rate")?.ok_or_else(|| {
        mlua::Error::RuntimeError(
            "Ramp reference option \
                 'rate' is required"
                .to_owned(),
        )
    })?;

    ReferenceSource::ramp(start, target, rate).map_err(|error| {
        mlua::Error::RuntimeError(format!(
            "Invalid ramp reference: \
                     {error}",
        ))
    })
}

impl UserData for LuaControllerHandle {
    fn add_methods<M>(methods: &mut M)
    where
        M: UserDataMethods<Self>,
    {
        methods.add_method("name", |_, controller, ()| Ok(controller.name.clone()));

        methods.add_method(
            "add",
            |_, controller, (diagnostic_key, options): (String, Option<Value>)| {
                controller.add_diagnostic_series(&diagnostic_key, options)
            },
        );

        methods.add_method("parameters", |lua, controller, ()| {
            controller.parameters(lua)
        });

        methods.add_method("reference_kind", |_, controller, ()| {
            controller.reference_kind()
        });

        methods.add_method("reference_parameters", |lua, controller, ()| {
            controller.reference_parameters(lua)
        });

        methods.add_method("diagnostics", |lua, controller, ()| {
            controller.diagnostics(lua)
        });

        methods.add_method("read", |_, controller, key: String| {
            let value = controller.read_parameter(&key)?;

            Ok(instrument_value_to_lua(value))
        });

        methods.add_method("read_reference", |_, controller, key: String| {
            let value = controller.read_reference_parameter(&key)?;

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

        methods.add_method(
            "write_reference",
            |lua, controller, (key, value): (String, Value)| {
                let parameter = controller.reference_parameter_descriptor(&key)?;

                if !parameter.access.writable() {
                    return Err(mlua::Error::RuntimeError(format!(
                        "Reference parameter \
                                 '{}' is read-only",
                        key,
                    )));
                }

                let value = reference_value_from_lua(lua, parameter, value)?;

                let actual = controller.write_reference_parameter(&key, value)?;

                Ok(instrument_value_to_lua(actual))
            },
        );

        methods.add_method("configure", |lua, controller, updates: Table| {
            controller.configure(lua, updates)
        });

        methods.add_method("configure_reference", |lua, controller, updates: Table| {
            controller.configure_reference(lua, updates)
        });

        methods.add_method("set_fixed_reference", |_, controller, value: f64| {
            controller.set_fixed_reference(value)
        });

        methods.add_method("set_ramp_reference", |_, controller, options: Table| {
            controller.set_ramp_reference(&options)
        });

        methods.add_method("set_input", |_, controller, input_name: String| {
            controller.set_input(&input_name)
        });

        methods.add_method("state", |_, controller, ()| {
            Ok(controller.state()?.as_str().to_owned())
        });

        methods.add_method("pause", |_, controller, ()| controller.pause());

        methods.add_method("remove", |_, controller, ()| controller.remove());

        methods.add_method("resume", |_, controller, ()| controller.resume());

        methods.add_method("reset_integral", |_, controller, ()| {
            controller.reset_integral()
        });

        methods.add_method("reset", |_, controller, ()| controller.reset());
    }
}
