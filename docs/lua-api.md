# Lua API

This reference describes the application API registered by Rust, not the Lua standard library. Lua 5.4 runs in two independent environments: application scripts/console have `app`; [emulator models](virtual-instruments.md) have `instruments`, `read` and `write` instead.

Start with the [tutorial](lua-tutorial.md). For algorithms and parameter ranges, see [controllers](controllers.md); for event automation, see [scenarios](scenarios.md).

## Execution and results

The console, Run script, profile `setup` and application scripts share one persistent application Lua state. Use a global such as `plant = ...` when a handle must survive separate console submissions; a `local` only survives inside its chunk or captured closures. Ctrl+Enter submits console input. Use `return expression` to inspect a result in console history; use `app.log` for a recorded message. Results use Lua `tostring`, not a table pretty-printer: inspect a returned descriptor array with `ipairs` and log its fields, as shown below.

Commands such as `app.start`, series changes, controller construction and scenario scheduling enqueue work and return no Lua values. Successful enqueueing does not mean the action succeeded: inspect the application log. Instrument discovery/read/write and controller inspection/configuration/lifecycle methods wait for a runtime reply and raise a Lua error on failure. Their bindings wait up to 10 seconds; serial transport timeout is configured separately. A reply timeout does not cancel an already dispatched device write.

Application execution has a 500 ms instruction-hook limit, checked every 10,000 Lua instructions. This interrupts busy Lua loops but cannot preempt a blocking Rust call. Keep callbacks short and use scenarios for delays. Errors do not roll back earlier commands. An ordinary console error leaves the Lua runtime usable; a startup setup/script failure disconnects that worker.

## Application lifecycle

Each example is a separate operation.

| Call / minimal example | Result and effect |
| --- | --- |
| `app.start()` | Enqueue periodic acquisition on all connection workers. Starting also re-enables suspended polling on a newly started worker. |
| `app.stop()` | Enqueue stopping periodic acquisition. Series and recorded history remain. This is not a controller pause or an actuator reset. |
| `app.clear()` | Remove series, filters and controllers through the coordinated clear path. A controller safe-output failure can prevent completion; check the log. |
| `app.log("Heating stage started")` | Informational application-log message; also stored when recording is available. |
| `app.start_emu()` | Start the profile's model. Repeated start while running is harmless; no model path can be passed here. |
| `app.stop_emu()` | Stop/join the emulator and detach its local session. Existing series remain and can fail polling until restart/retry. |

Stop controllers safely before deliberately stopping their emulator. Restart creates fresh model state; obtain new handles if the model's descriptor catalog changed.

## Series and plotting

A series is identified by its unique name in Lua. Instrument `add` and `app.add_serial` accept no options, a name string, or a table:

```lua
plant:add("temperature", {
    name = "temperature", interval = 0.5,
    color = "#D32F2F", visible = true,
})
```

Here and below, `plant` is the furnace handle obtained in the tutorial.

| Option | Type / default | Meaning |
| --- | --- | --- |
| `name` | optional string, generated when absent | Unique series name. |
| `interval` | optional positive finite seconds | Override the application's default poll interval; raw series only. |
| `color` | optional `"#RRGGBB"` | Initial line color; otherwise automatic. |
| `visible` | optional boolean, `true` | Initial visibility; hidden series still acquire and record data. |
| `pane` | optional string, first configured pane | A key from profile `plot_panes`. Unknown explicit keys fail. |
| `connection` | optional string, `"primary"` | Only raw text serial series accept this field; instrument handles already select a connection. |

Filters and controller diagnostic series are driven by input events and do not accept a polling interval. There is no Lua setter for an existing raw series interval or visibility; configure it on creation and use the GUI for visibility.

| Call / minimal example | Effect |
| --- | --- |
| `app.rename("temperature", "furnace_temperature")` | Rename the existing series; its identity/history remain. Later Lua lookups must use the new name. |
| `app.set_color("temperature", "#FF8800")` | Change color. |
| `app.set_color("temperature", nil)` | Restore automatic color. |
| `app.set_series_pane("temperature", "thermal")` | Move to a profile-declared pane named `thermal`. Declare that pane first. |
| `app.retry("temperature")` | Re-enable this suspended raw series; normal scheduling supplies the next poll. |
| `app.retry_all()` | Re-enable all suspended series. |
| `app.delete("temperature")` | Remove the named series and dependent processing/controller state through the safety-aware removal path. |

A raw series becomes Offline after three consecutive failed polls. Successful manual read/write of the same parameter also restores matching polling. One failed series does not suspend other parameters.

## Raw serial commands

These examples require a configured `primary` serial connection and a device that understands the command.

```lua
app.add_serial("read_temperature", {name = "serial_temperature", interval = 1})
```

A periodic text response must parse as one finite number. Text transactions use the application's text protocol; choose commands appropriate to the attached instrument.

```lua
app.send_serial("status", {connection = "primary"})
```

The response or error appears in the application log, not as a Lua return value. Omitting options selects `primary`. Only `connection` is accepted by `send_serial`. Work is serialized with polling on the selected worker; due polls take priority. Timeout/settings come from `connections` in the profile. A missing port, protocol error or timeout is reported as an action failure.

## Filters

Each example assumes an existing `temperature` series; names are deliberately distinct.

### Moving average

```lua
app.filter("temperature", {name = "temperature_mean", kind = "moving_average", window = 5})
```

`window` is an integer sample count from 1 to 100,000. During warm-up it averages available samples, so the first output does not require five samples.

### Median

```lua
app.filter("temperature", {name = "temperature_median", kind = "median", window = 5})
```

`window` must be positive, odd and at most 100,000. It removes isolated spikes; it is a sample-count window, not seconds.

### Exponential

```lua
app.filter("temperature", {name = "temperature_smooth", kind = "exponential", time_constant = 2})
```

`time_constant` is positive finite seconds. The first sample initializes the filter; subsequent weights depend on elapsed input time.

### Change a filter

```lua
app.set_filter("temperature_smooth", {kind = "exponential", time_constant = 5})
```

Changing an exponential filter's time constant preserves its current output,
sample clock and downstream state. Other replacements reset this filter and its
downstream filters and resynchronize downstream controller timing. Input and
presentation do not change. Creation also accepts `color`, `visible` and `pane`.
Unknown keys are errors.

## Virtual instruments

Start the emulator first, then discover the catalog:

```lua
plant = app.virtual_instrument({id = 1})
```

Options are optional `connection` (default `primary`) and `id` (default `1`, integer 1–65535). Memory emulation is attached to the primary worker. In a memory-only profile without a `connections` map, omit `connection`: explicit names are looked up in that map, while omission selects the primary worker directly. Discovery returns a handle for an existing instrument; it does not create a model instrument.

| Method / minimal example | Result |
| --- | --- |
| `return plant:id()` | One-based numeric model instrument ID. |
| `return plant:name()` | Model instrument name. |
| `return plant:parameters()` | Descriptor array, in model order. |
| `plant:add("temperature", "temperature")` | Enqueue a periodic series for a readable, series-enabled parameter. |
| `return plant:read("temperature")` | Number, integer or boolean according to its descriptor. |
| `return plant:write("heater_power", 0)` | The actual value returned by the model's write function. |
| `loop = plant:pid("heater_power", pid_options)` | Create/register a PID using the complete options below. |
| `loop = plant:on_off("heater_power", on_off_options)` | Create/register an on/off controller. |
| `loop = plant:furnace("heater_power", furnace_options)` | Create/register a furnace controller. |

`parameters()` entries contain `key`, `name`, `access`, `value_type`, `series` and optional `unit`, `minimum`, `maximum`. Access is `read_only`, `write_only` or `read_write`; value type is `number`, `integer` or `boolean`. Do not confuse these returned fields with model declaration fields `type`, `min` and `max`.

```lua
for _, parameter in ipairs(plant:parameters()) do
    app.log(parameter.key .. ": " .. parameter.value_type .. " / " .. parameter.access)
end
```

A controller target must be writable and satisfy numeric output conversion/range requirements. See [output safety](output-safety.md). Writing to a controller-owned parameter takes manual control.

## Metakon 5X3

Constructing a Metakon handle validates options; it does not perform discovery:

```lua
meter = app.metakon({connection = "primary", device = 1, channel = 0, scale = 1})
```

All options are optional. `device` and `channel` are unsigned bytes; `scale` must be positive and finite. Defaults are shown above. The driver verifies the channel type during actual communication.

| Method / minimal example | Result |
| --- | --- |
| `return meter:parameters()` | Descriptors with effective scaling and engineering limits. |
| `meter:add("measurement", "measured_temperature")` | Periodically read a supported register. |
| `return meter:read("measurement")` | Engineering value; sensor alarm is an error. |
| `return meter:write("setpoint", 100)` | Write then read back and return the actual value. |
| `loop = meter:pid("output_power", pid_options)` | Use the PID options below, with input set to `measured_temperature`. |
| `loop = meter:on_off("output_power", on_off_options)` | Use the on/off options below with that input. |
| `loop = meter:furnace("output_power", furnace_options)` | Use the furnace options below with that input and a suitable physical model. |

There are no Metakon `id()` or `name()` methods. The local variable name `meter` does not name a software controller.

### Register table

Ranges below are raw register limits. For scaled registers multiply by `scale` to get engineering limits. Integral time always uses its own 1/60 scale.

| Key | Meaning | Access | Unit / range | Special behavior |
| --- | --- | --- | --- | --- |
| `channel_type` | Hardware channel type | read | integer 0–255 | Expected type code 3. |
| `measurement` | Process measurement | read | raw -999–9999 × scale | Raw -32768 means sensor alarm; no sample is stored. |
| `setpoint` | Hardware setpoint | read/write | raw -999–9999 × scale | Independent of a software controller's setpoint. |
| `proportional_band` | Hardware proportional band | read/write | raw 1–9999 × scale | Engineering measurement units. |
| `integral_time` | Hardware integral time | read/write | 1/60–500 minutes | Register seconds converted to minutes; ignores user scale. |
| `derivative_time` | Hardware derivative time | read/write | 0–255 seconds | Unscaled. |
| `output_power` | Hardware output command | read/write | -100–100 percent | Unscaled numeric actuator target. |
| `pwm_positive` | Positive PWM state | read | boolean | Unscaled. |
| `pwm_negative` | Negative PWM state | read | boolean | Unscaled. |
| `upper_setpoint` | Upper threshold | read/write | raw -999–9999 × scale | Scaled. |
| `upper_hysteresis` | Upper hysteresis | read/write | raw 0–255 × scale | Scaled. |
| `upper_output` | Upper output state | read/write | boolean | Unscaled. |
| `lower_setpoint` | Lower threshold | read/write | raw -999–9999 × scale | Scaled. |
| `lower_hysteresis` | Lower hysteresis | read/write | raw 0–255 × scale | Scaled. |
| `lower_output` | Lower output state | read/write | boolean | Unscaled. |

Writes must be representable in register increments; arbitrary fractional raw values are rejected. `parameters()` exposes `key`, `name`, `access`, `value_type`, `minimum`, `maximum` and `scale`. Read/write values use engineering scaling even when the underlying descriptor is an integer register.

```lua
for _, p in ipairs(meter:parameters()) do
    app.log(p.key .. " scale=" .. tostring(p.scale))
end
```

Three consecutive measurement alarms suspend that series. Repair the sensor and call `app.retry("measured_temperature")` or successfully read `measurement`. Other parameters continue polling. The hardware front-panel integral OFF state is not encoded in the numeric `integral_time` register; reading it can return the last stored time.

## Controller construction

Run **one** construction example at a time: all target the same furnace output. Before creating another controller, remove the previous one successfully. The tutorial provides `plant` and `temperature`.

### PID

```lua
pid_options = {
    name = "heater", input = "temperature", setpoint = 80,
    kp = 1, ki = 0.05, kd = 0,
    output_min = 0, output_max = 100, safe_output = 0,
}
loop = plant:pid("heater_power", pid_options)
```

Required: `name`, `input`, `setpoint`, `kp`, `output_min`, `output_max`. `ki` and `kd` default to zero.

### On/off

```lua
on_off_options = {
    name = "thermostat", input = "temperature", setpoint = 80,
    hysteresis = 2, output_off = 0, output_on = 100, safe_output = 0,
}
loop = plant:on_off("heater_power", on_off_options)
```

All fields except `safe_output` are required. The output retains its state inside the hysteresis band.

### Furnace

```lua
furnace_options = {
    name = "furnace", input = "temperature", setpoint = 80,
    kp = 1, ki = 0.02, output_min = 0, output_max = 100,
    ambient_temperature = 20, max_power = 2500, heater_lag = 90,
    linear_loss = 0.35, radiation_loss_1000c = 1200, safe_output = 0,
}
loop = plant:furnace("heater_power", furnace_options)
```

All fields except `ki` (default zero) and `safe_output` are required. The model terms are explained in [controllers](controllers.md).

Construction enqueues installation immediately. There is no second registration call. `loop:add("output")` plots a diagnostic; it does not start the controller. New loops are running and initially own their output automatically. Output occurs when acquisition supplies measurements.

`safe_output` is optional, but there is no implicit safe value. Omitting it means safe pause/removal cannot complete successfully. Set it deliberately for every actuator. It is creation-time output configuration, not a controller parameter accepted by `configure`.

## Controller methods

These operations work on the `loop` created above, except `reset_integral` is unsupported for on/off.

| Method / minimal example | Result / effect |
| --- | --- |
| `return loop:name()` | Unique controller name. |
| `return loop:parameters()` | Parameter descriptor array. |
| `return loop:read("setpoint")` | Current effective setpoint. |
| `return loop:write("kp", 2)` | Set one PID/furnace parameter; return actual value. For on/off use `loop:write("hysteresis", 2)`. |
| `loop:configure({kp = 2, ki = 0.1})` | Atomically validate/apply PID/furnace parameter updates; no result. |
| `loop:configure({hysteresis = 3, output_on = 80})` | On/off configuration example. |
| `loop:configure({heater_lag = 60, linear_loss = 0.4})` | Furnace model configuration example. |
| `loop:set_input("temperature_smooth")` | Switch to an existing series and resynchronize controller input timing. |
| `return loop:state()` | `"running"` or `"paused"`, not output ownership. |
| `loop:pause()` | Attempt safe output and pause computation; hardware failure is returned even if computation was paused. |
| `loop:resume()` | Request automatic takeover and resume computation; failed resume attempts rollback. Does not wait for the next automatic output. |
| `loop:reset_integral()` | Clear PID/furnace integral only. |
| `loop:reset()` | Reset algorithm state and reference elapsed time; does not change pause/running state or directly write hardware. |
| `loop:remove()` | Pause safely, wait for safe write, release ownership and remove controller/diagnostics. Failure retains state for recovery where possible. |
| `return loop:diagnostics()` | Array of diagnostic key strings. |
| `loop:add("output", {name = "controller_output", color = "#0088FF"})` | Add an event-driven diagnostic series; optional options follow series presentation rules. |

Returned controller/reference descriptors contain `key`, `name`, `access`, `value_type`, `minimum` and `maximum`. Semantic checks can be stricter than those independent descriptor limits, for example `output_min < output_max`.

```lua
for _, p in ipairs(loop:parameters()) do
    app.log(p.key .. " " .. p.access .. " " .. tostring(loop:read(p.key)))
end
```

Configuration preserves accumulated state unless the operation explicitly resets/resynchronizes it. Changing limits validates compatibility with the physical output target before applying the update.

### Diagnostic keys

| Controller | Keys |
| --- | --- |
| PID | `setpoint`, `proportional`, `integral`, `derivative`, `output`, `unconstrained_output` |
| On/off | `setpoint`, `output` |
| Furnace | `setpoint`, `proportional`, `integral`, `output`, `unconstrained_output`, `feed_forward`, `predicted_measurement`, `measurement_rate` |

Each returned key can be plotted separately:

```lua
for _, key in ipairs(loop:diagnostics()) do
    loop:add(key, {name = loop:name() .. "_" .. key})
end
```

Diagnostics describe computed values, including requested output, not proof of a hardware write. Inspect logs/process output records for actual write results.

## References

Without an explicit reference, `setpoint` is directly writable. `reference_kind()` returns nil and `reference_parameters()` returns an empty array. Once fixed/ramp reference management is enabled, controller `setpoint` becomes read-only; use reference operations to change it. There is no Lua operation to remove reference management.

| Method / minimal example | Result / effect |
| --- | --- |
| `loop:set_fixed_reference(80)` | Install a constant reference in measurement units. |
| `return loop:reference_kind()` | `"fixed"`, `"ramp"` or nil. |
| `return loop:reference_parameters()` | Descriptor array for the active reference. |
| `return loop:read_reference("value")` | Fixed reference value. |
| `return loop:write_reference("value", 90)` | Change fixed reference and return its actual value. |
| `loop:configure_reference({value = 100})` | Atomically update a fixed reference. |
| `loop:set_ramp_reference({start = 20, target = 100, rate = 1})` | Install a ramp; rate is a positive magnitude in measurement units/second. |
| `return loop:read_reference("target")` | Configured ramp target. |
| `return loop:write_reference("target", 120)` | Change ramp target, continuing from its current value. |
| `loop:configure_reference({target = 150, rate = 0.5})` | Change target/rate without jumping back to the original start. |
| `loop:configure_reference({start = 30, target = 100, rate = 1})` | Explicit start restarts the ramp from that value. |
| `return loop:read("setpoint")` | Current effective reference value, rather than the target. |

The first accepted sample establishes the ramp time baseline; subsequent sample timestamps advance it. The direction follows target minus start, so cooling still uses a positive rate. A controller pause freezes the reference; resume resets its timestamp baseline while retaining progress. Stopping acquisition alone is different: elapsed time between the last and next sample can include the stopped interval. `reset()` resets reference elapsed time to zero. Replacing the reference resynchronizes controller timing.

```lua
for _, p in ipairs(loop:reference_parameters()) do
    app.log(p.key .. "=" .. tostring(loop:read_reference(p.key)))
end
```

## Application scripts and control panels

`app.register_script` stores a script table and publishes its panels. Callback fields contain **names of functions in that table**, not function values. A number/toggle callback receives the submitted value; a button callback receives no arguments. Use closures to access a local handle.

The smallest wrapper for a widget is:

```lua
app.register_script({
    id = "status",
    panels = {{
        id = "main", title = "Status",
        controls = {{kind = "readout", id = "message", label = "Message", initial = "Ready"}},
    }},
})
```

Registering an existing script ID replaces its retained table and panel definitions. Panel IDs must be unique within a script, and control IDs within a panel. Use nonempty stable identifiers without surrounding whitespace. Unregistering explicitly removes a script and its panels. A script may omit `panels` and register only callbacks.

Each following widget is an independent expression intended for the wrapper's `controls` array. Define the named callback in the script table.

### Readout

```lua
local widget = {kind = "readout", id = "temperature", label = "Temperature", initial = "20 °C"}
```

`initial` is optional text (default `"—"`). Updates require a string: convert numeric measurements with `tostring` or `string.format`.

### Number

```lua
local widget = {kind = "number", id = "target", label = "Target",
    initial = 20, min = 0, max = 200, step = 1, on_change = "set_target"}
```

`on_change` is required; `initial` defaults to 0 and `step` to 1. Bounds are optional; supplied values must be finite, bounds ordered, initial in range and step positive. Submission occurs after a drag ends or keyboard editing loses focus.

### Toggle

```lua
local widget = {kind = "toggle", id = "enabled", label = "Enabled",
    initial = false, on_change = "set_enabled"}
```

`initial` defaults to false; `on_change` is required and receives a boolean.

### Button

```lua
local widget = {kind = "button", id = "refresh", label = "Refresh", on_click = "refresh"}
```

`on_click` is required. Buttons have no value and reject `app.set_control`.

### Complete small panel

This example uses the tutorial's furnace handle and does not require a controller.

```lua
local script = {id = "furnace_panel"}
script.refresh = function()
    app.set_control("furnace_panel", "main", "temperature",
        string.format("%.1f °C", plant:read("temperature")))
end
script.set_power = function(value)
    local actual = plant:write("heater_power", value)
    app.set_control("furnace_panel", "main", "power", actual)
end
script.set_acquisition = function(value)
    if value then app.start() else app.stop() end
end
script.panels = {{
    id = "main", title = "Furnace",
    controls = {
        {kind = "readout", id = "temperature", label = "Temperature"},
        {kind = "number", id = "power", label = "Power (%)",
            initial = 0, min = 0, max = 100, step = 1, on_change = "set_power"},
        {kind = "toggle", id = "acquisition", label = "Acquisition",
            initial = true, on_change = "set_acquisition"},
        {kind = "button", id = "refresh", label = "Refresh", on_click = "refresh"},
    },
}}
app.register_script(script)
```

The registration retains the script table/closures. Closing the native Control panel window leaves the registration and experiment running. Reloading the profile replaces the Lua state and removes registrations.

| Function / minimal example | Effect |
| --- | --- |
| `app.set_control("furnace_panel", "main", "temperature", "25 °C")` | Update readout text. |
| `app.set_control("furnace_panel", "main", "power", 0)` | Update numeric UI value; does not itself write equipment. |
| `app.set_control("furnace_panel", "main", "acquisition", false)` | Update toggle UI value; does not call its callback. |
| `app.set_control_enabled("furnace_panel", "main", "power", false, "Automatic control")` | Disable a widget with optional displayed reason. GUI guard only. |
| `app.set_control_enabled("furnace_panel", "main", "power", true)` | Enable the widget again. |
| `app.unregister_script("furnace_panel")` | Remove retained registration and all its panels; later callback lookups fail. |

## Profile setup

A complete profile saved in the repository root:

```lua
return {
    emulator = {transport = "memory", script = "emulator_scripts/furnace_plant.lua"},
    setup = function()
        app.start_emu()
        plant = app.virtual_instrument()
        plant:add("temperature", "temperature")
        app.start()
    end,
}
```

The profile top level is evaluated during validation without `app`, then again in the application runtime. Put actions in `setup`. Setup runs after API installation, then profile `scripts` run in listed order. Relative script/model paths are resolved from the profile directory. See [configuration](configuration.md) for reload behavior.

## Completeness index

The tables above provide separate examples for every application, instrument and controller operation. [Scenarios](scenarios.md) supplies the remaining userdata methods and predicate examples. This index is checked against the bindings by the documentation tests.

| API family | Exposed members | Examples / detailed section |
| --- | --- | --- |
| Application | `app.start`, `app.stop`, `app.clear`, `app.log`, `app.start_emu`, `app.stop_emu` | [Lifecycle](#application-lifecycle) |
| Series / serial | `app.add_serial`, `app.send_serial`, `app.delete`, `app.rename`, `app.set_color`, `app.set_series_pane`, `app.retry`, `app.retry_all` | [Series](#series-and-plotting), [serial](#raw-serial-commands) |
| Filters | `app.filter`, `app.set_filter` | [Filters](#filters) |
| Instruments | `app.metakon`, `app.virtual_instrument` | [Metakon](#metakon-5x3), [virtual instruments](#virtual-instruments) |
| Metakon methods | `parameters`, `add`, `read`, `write`, `pid`, `on_off`, `furnace` | [Metakon](#metakon-5x3), [creation](#controller-construction) |
| Virtual methods | `id`, `name`, `parameters`, `add`, `read`, `write`, `pid`, `on_off`, `furnace` | [Virtual instruments](#virtual-instruments), [creation](#controller-construction) |
| Controller methods | `name`, `add`, `parameters`, `diagnostics`, `read`, `write`, `configure`, `set_input`, `state`, `pause`, `resume`, `reset_integral`, `reset`, `remove` | [Methods](#controller-methods) |
| Reference methods | `reference_kind`, `reference_parameters`, `read_reference`, `write_reference`, `configure_reference`, `set_fixed_reference`, `set_ramp_reference` | [References](#references) |
| Panels | `app.register_script`, `app.unregister_script`, `app.set_control`, `app.set_control_enabled` | [Panels](#application-scripts-and-control-panels) |
| Scenarios | `app.scenario`; `id`, `after`, `at`, `when`, `race`, `stage`, `start`, `on_stop`, `on_error`, `complete`, `stop`, `cancel` | [Scenarios](scenarios.md) |
| Profile / model | `setup`; `instruments`, `read`, `write` | [Setup](#profile-setup), [models](virtual-instruments.md) |
