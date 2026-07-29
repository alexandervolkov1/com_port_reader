use mlua::{FromLua, Lua};

pub struct LuaRuntime {
    lua: Lua,
}

impl LuaRuntime {
    pub fn new() -> Self {
        Self { lua: Lua::new() }
    }

    pub fn execute(&self, source: &str) -> mlua::Result<()> {
        self.lua.load(source).exec()
    }

    pub fn evaluate<T>(&self, source: &str) -> mlua::Result<T>
    where
        T: FromLua,
    {
        self.lua.load(source).eval()
    }
}

impl Default for LuaRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::LuaRuntime;

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
}
