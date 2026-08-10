use crossbeam_channel::Sender;
use mlua::{FromLua, Function, Lua, MultiValue};

use crate::{
    application_definition::ApplicationDefinition, lua_execution::run_with_limit,
    user_command::UserCommand,
};

pub struct LuaRuntime {
    lua: Lua,
    application_definition: ApplicationDefinition,
}

impl LuaRuntime {
    pub fn new() -> Self {
        Self::with_application_definition(ApplicationDefinition::default())
    }

    pub fn with_application_definition(application_definition: ApplicationDefinition) -> Self {
        Self {
            lua: Lua::new(),
            application_definition,
        }
    }

    pub(crate) fn install_application_api(
        &self,
        command_sender: Sender<UserCommand>,
    ) -> mlua::Result<()> {
        crate::lua_api::install(&self.lua, command_sender, &self.application_definition)
    }

    pub fn execute(&self, source: &str) -> mlua::Result<()> {
        run_with_limit(&self.lua, || self.lua.load(source).exec())
    }

    pub fn evaluate<T>(&self, source: &str) -> mlua::Result<T>
    where
        T: FromLua,
    {
        run_with_limit(&self.lua, || self.lua.load(source).eval())
    }

    pub fn evaluate_for_repl(&self, source: &str) -> mlua::Result<Vec<String>> {
        run_with_limit(&self.lua, || {
            let values: MultiValue = self.lua.load(source).eval()?;

            let tostring: Function = self.lua.globals().get("tostring")?;

            values
                .iter()
                .cloned()
                .map(|value| tostring.call::<String>(value))
                .collect()
        })
    }
}

impl Default for LuaRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crossbeam_channel::unbounded;
    use serialport::{DataBits, FlowControl, Parity, StopBits};

    use super::LuaRuntime;
    use crate::{
        application_definition::{ApplicationDefinition, SerialConnectionDefinition},
        connection::ConnectionId,
        data::{SamplingInterval, SeriesSource},
        instrument::{
            InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest,
            metakon_5x3::{Metakon5x3, Metakon5x3Register, Metakon5x3Write},
        },
        serial_connection::SerialPortConfig,
        user_command::UserCommand,
    };

    fn metakon_source(
        device: u8,
        channel: u8,
        parameter: Metakon5x3Register,
        scale: f64,
    ) -> SeriesSource {
        SeriesSource::Instrument(InstrumentReadRequest::metakon_5x3(
            Metakon5x3::new(device, channel),
            parameter,
            scale,
        ))
    }

    fn metakon_write_request(
        device: u8,
        channel: u8,
        parameter: Metakon5x3Write,
        scale: f64,
    ) -> InstrumentWriteRequest {
        InstrumentWriteRequest::metakon_5x3(Metakon5x3::new(device, channel), parameter, scale)
            .unwrap()
    }

    #[test]
    fn evaluates_lua_code() {
        let runtime = LuaRuntime::new();

        let result: i64 = runtime.evaluate("return 20 + 22").unwrap();

        assert_eq!(result, 42);
    }

    #[test]
    fn preserves_state_between_commands() {
        let runtime = LuaRuntime::new();

        runtime.execute("counter = 40").unwrap();

        runtime.execute("counter = counter + 2").unwrap();

        let counter: i64 = runtime.evaluate("return counter").unwrap();

        assert_eq!(counter, 42);
    }

    #[test]
    fn reports_invalid_lua_code() {
        let runtime = LuaRuntime::new();

        let result = runtime.execute("this is not valid lua");

        assert!(result.is_err());
    }

    #[test]
    fn evaluates_repl_expression() {
        let runtime = LuaRuntime::new();

        let output = runtime.evaluate_for_repl("20 + 22").unwrap();

        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn executes_repl_statement() {
        let runtime = LuaRuntime::new();

        let output = runtime.evaluate_for_repl("answer = 42").unwrap();

        assert!(output.is_empty());

        let output = runtime.evaluate_for_repl("answer").unwrap();

        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn returns_multiple_repl_values() {
        let runtime = LuaRuntime::new();

        let output = runtime
            .evaluate_for_repl("return 42, true, 'hello'")
            .unwrap();

        assert_eq!(output, vec!["42", "true", "hello"],);
    }

    #[test]
    fn exposes_application_commands() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                app.start()
                app.stop()
                app.clear()
                app.start_rec()
                app.stop_rec()
                app.start_emu()
                app.stop_emu()
                "#,
            )
            .unwrap();

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::Start,
        ));

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::Stop,
        ));

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::Clear,
        ));

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::StartRecording,
        ));

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::StopRecording,
        ));

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::StartEmulator,
        ));

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::StopEmulator,
        ));

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn reports_disconnected_application_channel() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        drop(command_receiver);

        let result = runtime.execute("app.start()");

        let error = result.unwrap_err().to_string();

        assert!(error.contains(
            "application command channel \
                 is disconnected",
        ),);
    }

    #[test]
    fn interrupts_endless_execution() {
        let runtime = LuaRuntime::new();

        let error = runtime.execute("while true do end").unwrap_err();

        assert!(error.to_string().contains("Lua execution exceeded"),);
    }

    #[test]
    fn exposes_serial_application_commands() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                    app.add_serial(
                        "read sine",
                        {
                            name = "sine",
                            interval = 2.5,
                        }
                    )

                    app.add_serial(
                        "read pressure"
                    )

                    app.send_serial(
                        "set amplitude 25"
                    )
                "#,
            )
            .unwrap();

        let UserCommand::Add(new_series) = command_receiver.try_recv().unwrap() else {
            panic!("expected Add command");
        };

        assert_eq!(new_series.connection_id(), ConnectionId::PRIMARY,);

        let (source, name, sampling_interval) = new_series.into_parts();

        assert_eq!(name.as_deref(), Some("sine"));

        assert_eq!(
            sampling_interval,
            Some(SamplingInterval::from_secs_f64(2.5).unwrap(),),
        );

        assert_eq!(
            source,
            SeriesSource::SerialCommand {
                command: "read sine".to_owned(),
            },
        );

        let UserCommand::Add(new_series) = command_receiver.try_recv().unwrap() else {
            panic!("expected Add command");
        };

        assert_eq!(new_series.connection_id(), ConnectionId::PRIMARY,);

        let (source, name, sampling_interval) = new_series.into_parts();

        assert_eq!(name, None);
        assert_eq!(sampling_interval, None);

        assert_eq!(
            source,
            SeriesSource::SerialCommand {
                command: "read pressure".to_owned(),
            },
        );

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::SendSerial {
                connection_id,
                command,
            }
                if connection_id == ConnectionId::PRIMARY
                    && command == "set amplitude 25",
        ));

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn exposes_series_management_commands() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                app.delete("temperature")

                app.rename(
                    "pressure",
                    "reactor_pressure"
                )
                "#,
            )
            .unwrap();

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::Delete { name }
                if name == "temperature",
        ));

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::Rename {
                current_name,
                new_name,
            }
                if current_name == "pressure"
                    && new_name
                        == "reactor_pressure",
        ));

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn exposes_application_log_command() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                app.log(
                    "Temperature reached setpoint"
                )
                "#,
            )
            .unwrap();

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::Log { message }
                if message
                    == "Temperature reached setpoint",
        ));

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn adds_metakon_series_by_parameter_key() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                    local controller = app.metakon({
                        device = 15,
                        channel = 2,
                        scale = 0.1,
                    })

                    controller:add(
                        "channel_type",
                        "channel_type"
                    )

                    controller:add(
                        "measurement",
                        "measurement"
                    )

                    controller:add(
                        "setpoint",
                        "setpoint"
                    )

                    controller:add(
                        "proportional_band",
                        "proportional_band"
                    )

                    controller:add(
                        "integral_time",
                        "integral_time"
                    )

                    controller:add(
                        "derivative_time",
                        "derivative_time"
                    )

                    controller:add(
                        "output_power",
                        "output_power"
                    )

                    controller:add(
                        "pwm_positive",
                        "pwm_positive"
                    )

                    controller:add(
                        "pwm_negative",
                        "pwm_negative"
                    )

                    controller:add(
                        "upper_setpoint",
                        "upper_setpoint"
                    )

                    controller:add(
                        "upper_hysteresis",
                        "upper_hysteresis"
                    )

                    controller:add(
                        "upper_output",
                        "upper_output"
                    )

                    controller:add(
                        "lower_setpoint",
                        "lower_setpoint"
                    )

                    controller:add(
                        "lower_hysteresis",
                        "lower_hysteresis"
                    )

                    controller:add(
                        "lower_output",
                        "lower_output"
                    )
                "#,
            )
            .unwrap();

        let expected = [
            ("channel_type", Metakon5x3Register::ChannelType, 1.0),
            ("measurement", Metakon5x3Register::Measurement, 0.1),
            ("setpoint", Metakon5x3Register::Setpoint, 0.1),
            (
                "proportional_band",
                Metakon5x3Register::ProportionalBand,
                0.1,
            ),
            ("integral_time", Metakon5x3Register::IntegralTime, 1.0),
            ("derivative_time", Metakon5x3Register::DerivativeTime, 1.0),
            ("output_power", Metakon5x3Register::OutputPower, 1.0),
            ("pwm_positive", Metakon5x3Register::PwmPositive, 1.0),
            ("pwm_negative", Metakon5x3Register::PwmNegative, 1.0),
            ("upper_setpoint", Metakon5x3Register::UpperSetpoint, 0.1),
            ("upper_hysteresis", Metakon5x3Register::UpperHysteresis, 0.1),
            ("upper_output", Metakon5x3Register::UpperOutput, 1.0),
            ("lower_setpoint", Metakon5x3Register::LowerSetpoint, 0.1),
            ("lower_hysteresis", Metakon5x3Register::LowerHysteresis, 0.1),
            ("lower_output", Metakon5x3Register::LowerOutput, 1.0),
        ];

        for (expected_name, expected_parameter, expected_scale) in expected {
            let UserCommand::Add(new_series) = command_receiver.try_recv().unwrap() else {
                panic!("expected Add command");
            };

            assert_eq!(new_series.connection_id(), ConnectionId::PRIMARY,);

            let (source, name, sampling_interval) = new_series.into_parts();

            assert_eq!(sampling_interval, None);

            assert_eq!(name.as_deref(), Some(expected_name),);

            assert_eq!(
                source,
                metakon_source(15, 2, expected_parameter, expected_scale,),
            );
        }

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn rejects_unknown_metakon_parameter_key() {
        let runtime = LuaRuntime::new();
        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        let error = runtime
            .execute(
                r#"
                    local controller = app.metakon()
                    controller:add("unknown")
                "#,
            )
            .unwrap_err()
            .to_string();

        assert!(error.contains("Unknown Metakon 5X3 parameter: 'unknown'",),);

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn reads_metakon_parameter_from_lua() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                    controller = app.metakon({
                        device = 15,
                        channel = 2,
                        scale = 0.1,
                    })
                "#,
            )
            .unwrap();

        let responder = std::thread::spawn(move || {
            let command = command_receiver.recv().unwrap();

            let UserCommand::ReadInstrument {
                connection_id,
                request,
                response_sender,
            } = command
            else {
                panic!("expected ReadInstrument command");
            };

            assert_eq!(connection_id, ConnectionId::PRIMARY,);

            assert_eq!(
                request,
                InstrumentReadRequest::metakon_5x3(
                    Metakon5x3::new(15, 2),
                    Metakon5x3Register::Measurement,
                    0.1,
                ),
            );

            response_sender
                .send(Ok(InstrumentValue::Number(123.5)))
                .unwrap();
        });

        let output = runtime
            .evaluate_for_repl(r#"controller:read("measurement")"#)
            .unwrap();

        responder.join().unwrap();

        assert_eq!(output, vec!["123.5"]);
    }

    #[test]
    fn returns_boolean_instrument_value_to_lua() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime.execute("controller = app.metakon()").unwrap();

        let responder = std::thread::spawn(move || {
            let command = command_receiver.recv().unwrap();

            let UserCommand::ReadInstrument {
                connection_id,
                request,
                response_sender,
            } = command
            else {
                panic!("expected ReadInstrument command");
            };

            assert_eq!(connection_id, ConnectionId::PRIMARY,);

            assert_eq!(
                request,
                InstrumentReadRequest::metakon_5x3(
                    Metakon5x3::default(),
                    Metakon5x3Register::PwmPositive,
                    1.0,
                ),
            );

            response_sender
                .send(Ok(InstrumentValue::Boolean(true)))
                .unwrap();
        });

        let output = runtime
            .evaluate_for_repl(r#"controller:read("pwm_positive")"#)
            .unwrap();

        responder.join().unwrap();

        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn writes_scaled_metakon_parameter_by_key() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                controller = app.metakon({
                    device = 15,
                    channel = 2,
                    scale = 0.1,
                })
                "#,
            )
            .unwrap();

        let responder = std::thread::spawn(move || {
            let command = command_receiver.recv().unwrap();

            let UserCommand::WriteInstrument {
                connection_id,
                request,
                response_sender,
            } = command
            else {
                panic!("expected WriteInstrument command");
            };

            assert_eq!(connection_id, ConnectionId::PRIMARY,);

            assert_eq!(
                request,
                metakon_write_request(15, 2, Metakon5x3Write::Setpoint(150,), 0.1,),
            );

            response_sender
                .send(Ok(InstrumentValue::Number(15.0)))
                .unwrap();
        });

        let output = runtime
            .evaluate_for_repl(
                r#"controller:write(
                    "setpoint",
                    15.0
                )"#,
            )
            .unwrap();

        responder.join().unwrap();

        assert_eq!(output, vec!["15.0"]);
    }

    #[test]
    fn rejects_writing_read_only_parameter() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime.execute("controller = app.metakon()").unwrap();

        let error = runtime
            .evaluate_for_repl(
                r#"controller:write(
                    "measurement",
                    100
                )"#,
            )
            .unwrap_err()
            .to_string();

        assert!(error.contains("parameter 'measurement' is read-only",),);

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn exposes_metakon_parameter_descriptors() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                local controller = app.metakon({
                    device = 15,
                    channel = 2,
                    scale = 0.1,
                })

                local parameters =
                    controller:parameters()

                assert(#parameters == 15)

                local by_key = {}

                for _, parameter in ipairs(parameters) do
                    by_key[parameter.key] = parameter
                end

                local measurement =
                    by_key.measurement

                assert(
                    measurement.name
                        == "measurement"
                )

                assert(
                    measurement.access
                        == "read_only"
                )

                assert(
                    measurement.value_type
                        == "number"
                )

                assert(
                    math.abs(
                        measurement.minimum
                            - (-99.9)
                    ) < 0.000001
                )

                assert(
                    math.abs(
                        measurement.maximum
                            - 999.9
                    ) < 0.000001
                )

                assert(
                    measurement.scale == 0.1
                )

                local setpoint =
                    by_key.setpoint

                assert(
                    setpoint.access
                        == "read_write"
                )

                assert(
                    setpoint.value_type
                        == "number"
                )

                assert(
                    math.abs(
                        setpoint.minimum
                            - (-99.9)
                    ) < 0.000001
                )

                assert(
                    math.abs(
                        setpoint.maximum
                            - 999.9
                    ) < 0.000001
                )

                assert(
                    setpoint.scale == 0.1
                )

                local proportional_band =
                    by_key.proportional_band

                assert(
                    proportional_band.access
                        == "read_write"
                )

                assert(
                    proportional_band.value_type
                        == "number"
                )

                assert(
                    math.abs(
                        proportional_band.minimum
                            - 0.1
                    ) < 0.000001
                )

                assert(
                    math.abs(
                        proportional_band.maximum
                            - 999.9
                    ) < 0.000001
                )

                assert(
                    proportional_band.scale == 0.1
                )

                local output_power =
                    by_key.output_power

                assert(
                    output_power.access
                        == "read_write"
                )

                assert(
                    output_power.value_type
                        == "integer"
                )

                assert(
                    output_power.minimum == -100
                )

                assert(
                    output_power.maximum == 100
                )

                assert(
                    output_power.scale == 1.0
                )

                local pwm_positive =
                    by_key.pwm_positive

                assert(
                    pwm_positive.access
                        == "read_only"
                )

                assert(
                    pwm_positive.value_type
                        == "boolean"
                )

                assert(
                    pwm_positive.minimum == 0
                )

                assert(
                    pwm_positive.maximum == 1
                )

                assert(
                    pwm_positive.scale == 1.0
                )
                "#,
            )
            .unwrap();

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn rejects_scaled_value_that_cannot_be_represented() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                controller = app.metakon({
                    scale = 0.1,
                })
                "#,
            )
            .unwrap();

        let error = runtime
            .evaluate_for_repl(
                r#"controller:write(
                    "setpoint",
                    15.05
                )"#,
            )
            .unwrap_err()
            .to_string();

        assert!(error.contains("cannot be represented",));

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn rejects_invalid_series_sampling_interval() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        let error = runtime
            .execute(
                r#"
                    app.add_serial(
                        "read value",
                        {
                            interval = 0,
                        }
                    )
                "#,
            )
            .unwrap_err()
            .to_string();

        assert!(
            error.contains(
                "Sampling interval must be finite \
                 and greater than zero",
            ),
            "{error}",
        );

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn rejects_unknown_series_option() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        let error = runtime
            .execute(
                r#"
                    app.add_serial(
                        "read value",
                        {
                            interwal = 2,
                        }
                    )
                "#,
            )
            .unwrap_err()
            .to_string();

        assert!(
            error.contains("Unknown series option 'interwal'",),
            "{error}",
        );

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn selects_metakon_connection_by_name() {
        let mut definition = ApplicationDefinition::default();

        let serial_config = SerialPortConfig::new(
            "COM8".to_owned(),
            9_600,
            DataBits::Eight,
            Parity::None,
            StopBits::One,
            FlowControl::None,
            250,
        );

        definition
            .add_serial_connection(
                SerialConnectionDefinition::new(ConnectionId::new(2), "vacuum_bus", serial_config)
                    .unwrap(),
            )
            .unwrap();

        let runtime = LuaRuntime::with_application_definition(definition);

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                    local controller =
                        app.metakon({
                            connection =
                                "vacuum_bus",
                            device = 15,
                            channel = 0,
                        })

                    controller:add(
                        "measurement",
                        "temperature"
                    )
                "#,
            )
            .unwrap();

        let UserCommand::Add(new_series) = command_receiver.try_recv().unwrap() else {
            panic!("expected add-series command");
        };

        assert_eq!(new_series.connection_id(), ConnectionId::new(2),);
    }

    #[test]
    fn selects_virtual_instrument_connection_by_name() {
        let mut definition = ApplicationDefinition::default();

        let serial_config = SerialPortConfig::new(
            "COM8".to_owned(),
            9_600,
            DataBits::Eight,
            Parity::None,
            StopBits::One,
            FlowControl::None,
            250,
        );

        definition
            .add_serial_connection(
                SerialConnectionDefinition::new(ConnectionId::new(2), "virtual_bus", serial_config)
                    .unwrap(),
            )
            .unwrap();

        let runtime = LuaRuntime::with_application_definition(definition);

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        let responder = std::thread::spawn(move || {
            let command = command_receiver.recv().unwrap();

            let UserCommand::DescribeVirtualInstruments {
                connection_id,
                response_sender,
            } = command
            else {
                panic!(
                    "expected virtual instrument \
                         discovery command"
                );
            };

            assert_eq!(connection_id, ConnectionId::new(2),);

            response_sender.send(Ok(Vec::new())).unwrap();
        });

        let error = runtime
            .execute(
                r#"
                    app.virtual_instrument({
                        connection = "virtual_bus",
                        id = 1,
                    })
                "#,
            )
            .unwrap_err()
            .to_string();

        responder.join().unwrap();

        assert!(
            error.contains(
                "Virtual instrument with id 1 \
                 was not found",
            ),
            "{error}",
        );
    }

    #[test]
    fn selects_connection_for_serial_commands() {
        let mut definition = ApplicationDefinition::default();

        let serial_config = SerialPortConfig::new(
            "COM8".to_owned(),
            9_600,
            DataBits::Eight,
            Parity::None,
            StopBits::One,
            FlowControl::None,
            250,
        );

        definition
            .add_serial_connection(
                SerialConnectionDefinition::new(ConnectionId::new(2), "vacuum_bus", serial_config)
                    .unwrap(),
            )
            .unwrap();

        let runtime = LuaRuntime::with_application_definition(definition);

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                    app.add_serial(
                        "read pressure",
                        {
                            connection =
                                "vacuum_bus",
                            name = "pressure",
                            interval = 2.0,
                        }
                    )

                    app.send_serial(
                        "reset",
                        {
                            connection =
                                "vacuum_bus",
                        }
                    )
                "#,
            )
            .unwrap();

        let UserCommand::Add(new_series) = command_receiver.try_recv().unwrap() else {
            panic!("expected Add command");
        };

        assert_eq!(new_series.connection_id(), ConnectionId::new(2),);

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::SendSerial {
                connection_id,
                command,
            }
                if connection_id
                    == ConnectionId::new(2)
                    && command == "reset",
        ));
    }
}
