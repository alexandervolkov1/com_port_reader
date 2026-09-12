# Lua API

Application scripts receive a global `app` table. Commands are delivered to the runtime; hardware work happens on owned background workers. Errors from synchronous controller operations are returned as Lua errors.

## Runtime and logging

```lua
app.log("starting experiment")
app.start_emu()
app.start()
-- later
app.stop()
app.stop_emu()
app.clear()
```

`start`, `stop`, `clear`, `start_emu`, and `stop_emu` control acquisition and the configured emulator. `log(message)` adds an application/process log entry.

## Instruments and series

```lua
local plant = app.virtual_instrument({ id = 1 })
plant:add("temperature", { name = "temperature", interval = 0.5, color = "#D32F2F", pane = "thermal" })

local controller = app.metakon({ connection = "primary", device = 1, channel = 0, scale = 1.0 })
controller:add("process_value", { name = "pv" })
controller:write("setpoint", 100)
```

`app.virtual_instrument({ connection?, id? })` and `app.metakon({ connection?, device?, channel?, scale? })` return handles. A connection defaults to `primary`; virtual id defaults to `1`. Handles provide `parameters()`, `read(key)`, `write(key, value)`, and `add(key, options)`.

Series options are `name`, `interval` (seconds), `color` (`#RRGGBB`), `visible` (default `true`), and `pane`. `connection` is accepted where a raw serial series needs an explicit connection. Use `app.add_serial(command, options)` for a text serial command, `app.delete(name)`, `app.rename(old, new)`, `app.set_color(name, color_or_nil)`, `app.set_series_pane(name, pane)`, `app.retry(name)`, and `app.retry_all()` for series management. `app.send_serial(command, { connection = "primary" })` sends a one-off serial request.

## Filters

```lua
app.filter("temperature", { name = "temperature_avg", kind = "moving_average", window = 5 })
app.set_filter("temperature_avg", { kind = "exponential", time_constant = 10.0 })
```

Filter kinds are `moving_average`, `median`, and `exponential`. Moving-average and median filters need `window`; a median window must be odd. Exponential filters need positive `time_constant` in seconds. `filter` accepts the normal series presentation options; `set_filter` replaces only the filter definition.

## Controllers and references

Instrument handles create controller handles with `:pid(parameter, options)`, `:on_off(parameter, options)`, or `:furnace(parameter, options)`. See [controllers](controllers.md) for parameters and safety behavior.

```lua
local loop = plant:pid("power", {
  name = "heater", input = "temperature", setpoint = 80,
  kp = 1, ki = 0.1, kd = 0, output_min = 0, output_max = 100, safe_output = 0,
})
loop:add()
loop:set_ramp_reference({ start = 80, target = 120, rate = 1.0 })
loop:pause()
loop:resume()
```

Controller handles provide `name`, `add`, `parameters`, `reference_kind`, `reference_parameters`, `diagnostics`, `read`, `write`, `read_reference`, `write_reference`, `configure`, `configure_reference`, `set_fixed_reference`, `set_ramp_reference`, `set_input`, `state`, `pause`, `resume`, `reset_integral`, `reset`, and `remove`. A diagnostic can be added as a series with `add_diagnostic(key, options)`.

## Control panels and scripts

Scripts can register UI panels and callbacks with `app.register_script(definition)`, update a registered control using `app.set_control(script_id, panel_id, control_id, value)`, and remove an old registration with `app.unregister_script(script_id)`. The shipped scripts under `lua_scripts/` are the authoritative examples of panel structure and callbacks.

## Scenarios

`app.scenario(...)` defines staged, measurement-driven automation. Scenario callbacks execute through the same command API and are recorded with their origin. See the existing demo scripts and in-app help for the complete scenario syntax; use bounded callbacks and let completion events determine transitions.
