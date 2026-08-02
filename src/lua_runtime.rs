use crossbeam_channel::Sender;
use mlua::{FromLua, Function, Lua, MultiValue};

use crate::{lua_execution::run_with_limit, user_command::UserCommand};

pub struct LuaRuntime {
    lua: Lua,
}

impl LuaRuntime {
    pub fn new() -> Self {
        Self { lua: Lua::new() }
    }

    pub(crate) fn install_application_api(
        &self,
        command_sender: Sender<UserCommand>,
    ) -> mlua::Result<()> {
        crate::lua_api::install(&self.lua, command_sender)
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

    use super::LuaRuntime;
    use crate::{
        data::SeriesSource,
        instrument::{
            InstrumentReadRequest, InstrumentValue,
            metakon_5x3::{Metakon5x3, Metakon5x3Register},
        },
        protocol::metakon::{WriteRegisterRequest, WriteRegisterValue},
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
                    "sine"
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

        let (source, name) = new_series.into_source_parts();

        assert_eq!(name.as_deref(), Some("sine"),);

        assert_eq!(
            source,
            SeriesSource::SerialCommand {
                command: "read sine".to_owned(),
            },
        );

        let UserCommand::Add(new_series) = command_receiver.try_recv().unwrap() else {
            panic!("expected Add command");
        };

        let (source, name) = new_series.into_source_parts();

        assert_eq!(name, None);

        assert_eq!(
            source,
            SeriesSource::SerialCommand {
                command: "read pressure".to_owned(),
            },
        );

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::SendSerial { command }
                if command
                    == "set amplitude 25",
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
    fn exposes_metakon_controller_series() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                controller = app.metakon({
                    device = 15,
                    channel = 0,
                    scale = 1.0
                })

                controller:add_measurement(
                    "temperature"
                )

                controller:add_setpoint(
                    "setpoint"
                )

                controller:add_output_power(
                    "power"
                )

                controller:add_pwm_positive(
                    "pwm_positive"
                )

                controller:add_pwm_negative(
                    "pwm_negative"
                )
                "#,
            )
            .unwrap();

        let expected = [
            ("temperature", Metakon5x3Register::Measurement),
            ("setpoint", Metakon5x3Register::Setpoint),
            ("power", Metakon5x3Register::OutputPower),
            ("pwm_positive", Metakon5x3Register::PwmPositive),
            ("pwm_negative", Metakon5x3Register::PwmNegative),
        ];

        for (expected_name, expected_parameter) in expected {
            let UserCommand::Add(new_series) = command_receiver.try_recv().unwrap() else {
                panic!("expected Add command");
            };

            let (source, name) = new_series.into_source_parts();

            assert_eq!(name.as_deref(), Some(expected_name),);

            assert_eq!(source, metakon_source(15, 0, expected_parameter, 1.0,),);
        }

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn exposes_metakon_controller_pid_commands() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                local controller = app.metakon({
                    device = 15,
                    channel = 0,
                })

                controller:setpoint(150)
                controller:proportional_band(250)
                controller:integral_time(120)
                controller:derivative_time(10)
                "#,
            )
            .unwrap();

        let expected = [
            WriteRegisterRequest::new(15, 0, 0x02, WriteRegisterValue::Int(150)),
            WriteRegisterRequest::new(15, 0, 0x03, WriteRegisterValue::Uint(250)),
            WriteRegisterRequest::new(15, 0, 0x04, WriteRegisterValue::Uint(120)),
            WriteRegisterRequest::new(15, 0, 0x05, WriteRegisterValue::Ubyte(10)),
        ];

        for expected_request in expected {
            assert!(matches!(
                command_receiver
                    .try_recv()
                    .unwrap(),
                UserCommand::WriteMetakon {
                    request
                }
                    if request
                        == expected_request,
            ));
        }

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn rejects_unknown_metakon_controller_option() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        let error = runtime
            .execute(
                r#"
                app.metakon({
                    devcie = 15
                })
                "#,
            )
            .unwrap_err()
            .to_string();

        assert!(error.contains(
            "unknown app.metakon option \
                 'devcie'",
        ),);

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn exposes_metakon_controller_parameter_series() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                local controller = app.metakon({
                    device = 15,
                    channel = 2,
                })

                controller:add_proportional_band(
                    "pid_p"
                )

                controller:add_integral_time(
                    "pid_i"
                )

                controller:add_derivative_time(
                    "pid_d"
                )
                "#,
            )
            .unwrap();

        let expected = [
            ("pid_p", Metakon5x3Register::ProportionalBand),
            ("pid_i", Metakon5x3Register::IntegralTime),
            ("pid_d", Metakon5x3Register::DerivativeTime),
        ];

        for (expected_name, expected_parameter) in expected {
            let UserCommand::Add(new_series) = command_receiver.try_recv().unwrap() else {
                panic!("expected Add command");
            };

            let (source, name) = new_series.into_source_parts();

            assert_eq!(name.as_deref(), Some(expected_name),);

            assert_eq!(source, metakon_source(15, 2, expected_parameter, 1.0,),);
        }

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn exposes_metakon_controller_alarm_series() {
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

                controller:add_upper_setpoint(
                    "high"
                )

                controller:add_upper_hysteresis(
                    "high_hyst"
                )

                controller:add_upper_output(
                    "high_active"
                )

                controller:add_lower_setpoint(
                    "low"
                )

                controller:add_lower_hysteresis(
                    "low_hyst"
                )

                controller:add_lower_output(
                    "low_active"
                )
                "#,
            )
            .unwrap();

        let expected = [
            ("high", Metakon5x3Register::UpperSetpoint, 0.1),
            ("high_hyst", Metakon5x3Register::UpperHysteresis, 0.1),
            ("high_active", Metakon5x3Register::UpperOutput, 1.0),
            ("low", Metakon5x3Register::LowerSetpoint, 0.1),
            ("low_hyst", Metakon5x3Register::LowerHysteresis, 0.1),
            ("low_active", Metakon5x3Register::LowerOutput, 1.0),
        ];

        for (expected_name, expected_parameter, expected_scale) in expected {
            let UserCommand::Add(new_series) = command_receiver.try_recv().unwrap() else {
                panic!("expected Add command");
            };

            let (source, name) = new_series.into_source_parts();

            assert_eq!(name.as_deref(), Some(expected_name),);

            assert_eq!(
                source,
                metakon_source(15, 2, expected_parameter, expected_scale,),
            );
        }

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn exposes_metakon_controller_output_commands() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                local controller = app.metakon({
                    device = 15,
                    channel = 2,
                })

                controller:output_power(-40)

                controller:upper_setpoint(200)
                controller:upper_hysteresis(5)
                controller:upper_output(true)

                controller:lower_setpoint(100)
                controller:lower_hysteresis(4)
                controller:lower_output(false)
                "#,
            )
            .unwrap();

        let expected = [
            WriteRegisterRequest::new(15, 2, 0x06, WriteRegisterValue::Byte(-40)),
            WriteRegisterRequest::new(15, 2, 0x09, WriteRegisterValue::Int(200)),
            WriteRegisterRequest::new(15, 2, 0x0A, WriteRegisterValue::Ubyte(5)),
            WriteRegisterRequest::new(15, 2, 0x0B, WriteRegisterValue::Bool(true)),
            WriteRegisterRequest::new(15, 2, 0x0C, WriteRegisterValue::Int(100)),
            WriteRegisterRequest::new(15, 2, 0x0D, WriteRegisterValue::Ubyte(4)),
            WriteRegisterRequest::new(15, 2, 0x0E, WriteRegisterValue::Bool(false)),
        ];

        for expected_request in expected {
            assert!(matches!(
                command_receiver
                    .try_recv()
                    .unwrap(),
                UserCommand::WriteMetakon {
                    request
                }
                    if request
                        == expected_request,
            ));
        }

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn rejects_metakon_output_power_out_of_range() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        let error = runtime
            .execute(
                r#"
                local controller = app.metakon()

                controller:output_power(101)
                "#,
            )
            .unwrap_err()
            .to_string();

        assert!(error.contains(
            "output power must be between \
                 -100 and 100",
        ),);

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
                        "measurement",
                        "temperature"
                    )
                "#,
            )
            .unwrap();

        let UserCommand::Add(new_series) = command_receiver.try_recv().unwrap() else {
            panic!("expected Add command");
        };

        let (source, name) = new_series.into_source_parts();

        assert_eq!(name.as_deref(), Some("temperature"));

        assert_eq!(
            source,
            metakon_source(15, 2, Metakon5x3Register::Measurement, 0.1,),
        );
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
                request,
                response_sender,
            } = command
            else {
                panic!("expected instrument read command");
            };

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
                request,
                response_sender,
            } = command
            else {
                panic!("expected instrument read command");
            };

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
}
