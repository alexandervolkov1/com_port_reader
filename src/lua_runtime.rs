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
        data::{MetakonValueType, SeriesSource},
        protocol::metakon::{WriteRegisterRequest, WriteRegisterValue},
        user_command::UserCommand,
    };

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

        assert!(error.contains("application command channel is disconnected",));
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

        assert_eq!(name.as_deref(), Some("sine"));

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
                if command == "set amplitude 25",
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

                controller:add_measurement("temperature")
                controller:add_setpoint("setpoint")
                controller:add_output_power("power")
                "#,
            )
            .unwrap();

        let expected = [
            ("temperature", 0x01, MetakonValueType::Int),
            ("setpoint", 0x02, MetakonValueType::Int),
            ("power", 0x06, MetakonValueType::Byte),
        ];

        for (expected_name, register, value_type) in expected {
            let UserCommand::Add(new_series) = command_receiver.try_recv().unwrap() else {
                panic!("expected Add command");
            };

            let (source, name) = new_series.into_source_parts();

            assert_eq!(name.as_deref(), Some(expected_name),);

            assert_eq!(
                source,
                SeriesSource::Metakon {
                    device: 15,
                    channel: 0,
                    register,
                    value_type,
                    scale: 1.0,
                },
            );
        }

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn exposes_metakon_controller_setpoint() {
        let runtime = LuaRuntime::new();

        let (command_sender, command_receiver) = unbounded();

        runtime.install_application_api(command_sender).unwrap();

        runtime
            .execute(
                r#"
                controller = app.metakon({
                    device = 15,
                    channel = 0
                })

                controller:setpoint(150)
                "#,
            )
            .unwrap();

        assert!(matches!(
            command_receiver.try_recv().unwrap(),
            UserCommand::WriteMetakon {
                request
            } if request
                == WriteRegisterRequest::new(
                    15,
                    0,
                    0x02,
                    WriteRegisterValue::Int(150),
                ),
        ));

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

        assert!(error.contains("unknown app.metakon option 'devcie'",),);

        assert!(command_receiver.try_recv().is_err());
    }
}
