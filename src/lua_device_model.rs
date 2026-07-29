use std::time::Duration;

use mlua::{Function, Lua};

use crate::{
    device_model::{DeviceModel, DeviceModelError},
    lua_execution::run_with_limit,
};

pub struct LuaDeviceModel {
    lua: Lua,
}

impl LuaDeviceModel {
    pub fn from_source(source: &str) -> Result<Self, DeviceModelError> {
        let lua = Lua::new();

        run_with_limit(&lua, || lua.load(source).exec()).map_err(|error| {
            DeviceModelError::from(format!("Failed to load Lua device model: {error}",))
        })?;

        let handle: mlua::Result<Function> = lua.globals().get("handle");

        handle.map_err(|error| {
            DeviceModelError::from(format!(
                "Lua device model must define global \
                 function 'handle': {error}",
            ))
        })?;

        Ok(Self { lua })
    }
}

impl DeviceModel for LuaDeviceModel {
    fn handle_command(
        &mut self,
        command: &str,
        elapsed: Duration,
    ) -> Result<String, DeviceModelError> {
        run_with_limit(&self.lua, || {
            let handle: Function = self.lua.globals().get("handle")?;

            handle.call::<String>((command, elapsed.as_secs_f64()))
        })
        .map_err(|error| DeviceModelError::from(format!("Lua device handler failed: {error}",)))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::LuaDeviceModel;
    use crate::device_model::DeviceModel;

    #[test]
    fn passes_command_to_handler() {
        let mut model = LuaDeviceModel::from_source(
            r#"
                function handle(command, time)
                    if command == "read value" then
                        return "42"
                    end

                    return "error unknown command"
                end
                "#,
        )
        .unwrap();

        let response = model.handle_command("read value", Duration::ZERO).unwrap();

        assert_eq!(response, "42");
    }

    #[test]
    fn passes_elapsed_time_to_handler() {
        let mut model = LuaDeviceModel::from_source(
            r#"
                function handle(command, time)
                    if command ~= "read time" then
                        return "error"
                    end

                    if math.abs(time - 12.5)
                        < 0.000001
                    then
                        return "correct time"
                    end

                    return "incorrect time"
                end
                "#,
        )
        .unwrap();

        let response = model
            .handle_command("read time", Duration::from_secs_f64(12.5))
            .unwrap();

        assert_eq!(response, "correct time");
    }

    #[test]
    fn preserves_state_between_commands() {
        let mut model = LuaDeviceModel::from_source(
            r#"
                local counter = 0

                function handle(command, time)
                    if command ~= "next" then
                        return "error"
                    end

                    counter = counter + 1

                    return tostring(counter)
                end
                "#,
        )
        .unwrap();

        let first = model.handle_command("next", Duration::ZERO).unwrap();

        let second = model.handle_command("next", Duration::ZERO).unwrap();

        assert_eq!(first, "1");
        assert_eq!(second, "2");
    }

    #[test]
    fn reports_script_loading_error() {
        let result = LuaDeviceModel::from_source("this is not valid lua");

        let error = result.err().unwrap().to_string();

        assert!(error.contains("Failed to load Lua device model",));
    }

    #[test]
    fn requires_handle_function() {
        let result = LuaDeviceModel::from_source("value = 42");

        let error = result.err().unwrap().to_string();

        assert!(error.contains("must define global function 'handle'",));
    }

    #[test]
    fn reports_handler_error() {
        let mut model = LuaDeviceModel::from_source(
            r#"
                function handle(command, time)
                    error("simulated failure")
                end
                "#,
        )
        .unwrap();

        let result = model.handle_command("read", Duration::ZERO);

        let error = result.unwrap_err().to_string();

        assert!(error.contains("Lua device handler failed",));

        assert!(error.contains("simulated failure",));
    }

    #[test]
    fn interrupts_endless_handler() {
        let mut model = LuaDeviceModel::from_source(
            r#"
            function handle(command, time)
                while true do
                end
            end
            "#,
        )
        .unwrap();

        let error = model
            .handle_command("read", Duration::from_secs(1))
            .unwrap_err();

        assert!(error.to_string().contains("Lua execution exceeded"),);
    }
}
