use std::time::{Duration, Instant};

use mlua::{Lua, VmState, debug::HookTriggers};

const EXECUTION_TIMEOUT: Duration = Duration::from_millis(500);

const HOOK_INSTRUCTION_INTERVAL: u32 = 10_000;

pub fn run_with_limit<T>(
    lua: &Lua,
    operation: impl FnOnce() -> mlua::Result<T>,
) -> mlua::Result<T> {
    let deadline = Instant::now() + EXECUTION_TIMEOUT;

    lua.set_hook(
        HookTriggers::new().every_nth_instruction(HOOK_INSTRUCTION_INTERVAL),
        move |_lua, _debug| {
            if Instant::now() >= deadline {
                return Err(mlua::Error::RuntimeError(format!(
                    "Lua execution exceeded {} ms",
                    EXECUTION_TIMEOUT.as_millis(),
                )));
            }

            Ok(VmState::Continue)
        },
    )?;

    let guard = HookGuard { lua };

    let result = operation();

    drop(guard);

    result
}

struct HookGuard<'lua> {
    lua: &'lua Lua,
}

impl Drop for HookGuard<'_> {
    fn drop(&mut self) {
        self.lua.remove_hook();
    }
}
