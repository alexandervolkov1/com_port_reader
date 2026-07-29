use mlua::{FromLua, Function, Lua, MultiValue};

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

    pub fn evaluate_for_repl(&self, source: &str) -> mlua::Result<Vec<String>> {
        let values: MultiValue = self.lua.load(source).eval()?;

        let tostring: Function = self.lua.globals().get("tostring")?;

        values
            .iter()
            .cloned()
            .map(|value| tostring.call::<String>(value))
            .collect()
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
}
