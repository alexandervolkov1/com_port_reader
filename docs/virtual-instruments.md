# Virtual instruments

Virtual instruments are Lua models exposed through the same acquisition and controller paths as real instruments. Memory transport is the normal development and demonstration mode; it runs entirely in process and does not need a virtual COM driver.

## Model contract

An emulator script defines a global `instruments` array and global `read` and `write` functions. Instrument and parameter identifiers are assigned from their array positions, starting at 1. Parameter keys are stable strings used by the callbacks.

```lua
instruments = {
  {
    name = "demo source",
    parameters = {
      { key = "value", name = "Value", type = "number", access = "read_only", series = true, unit = "V", min = -10, max = 10 },
      { key = "enabled", type = "boolean", access = "read_write" },
    },
  },
}

local enabled = true

function read(instrument_id, key, elapsed_seconds)
  if key == "value" then return enabled and math.sin(elapsed_seconds) or 0 end
  if key == "enabled" then return enabled end
  error("unknown key: " .. key)
end

function write(instrument_id, key, value, elapsed_seconds)
  if key == "enabled" then enabled = value; return enabled end
  error("parameter is not writable")
end
```

`type` is `number`, `integer`, or `boolean`. `access` is `read_only`, `write_only`, or `read_write` and defaults to `read_only`. `series` defaults to `false`; only readable series parameters can be sampled. `name`, `unit`, `min`, and `max` are optional, but range bounds must match the value type.

The elapsed time passed to `read` and `write` is measured from emulator start. Model calls are bounded by the Lua execution limit; an error is reported to the caller instead of terminating the application.

## Starting and using an emulator

```lua
app.start_emu()
local source = app.virtual_instrument({ id = 1 })
source:add("value", { name = "virtual_value", interval = 0.5 })
app.start()
```

`app.virtual_instrument` accepts optional `connection` and `id` (default `1`). The returned handle supports `parameters()`, `read(key)`, `write(key, value)`, `add(key, options)`, and controller constructors. Consult `emulator_scripts/sine_generator.lua` for a minimal model and `emulator_scripts/pid_thermal_plant.lua` or `furnace_plant.lua` for stateful plants.

## Lifecycle and transport

`app.stop_emu()` ends the active session. Starting again creates a fresh model/session. In memory mode, the local source opens a request session only while the emulator is running. After a timeout, it closes and discards that session so delayed bytes cannot be reused by the next request.

Serial transport uses the same framed virtual-instrument protocol but requires a configured emulator COM port and a separate application connection. It is optional and is intended for external serial integrations, not the normal quick-start path.
