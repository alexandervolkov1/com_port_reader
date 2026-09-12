# Controllers

Controllers consume timestamped series samples in the processing service and issue output intents. A controller is attached to a writable Metakon or virtual-instrument parameter through a handle returned by `app.metakon(...)` or `app.virtual_instrument(...)`.

## Output ownership and safe output

Manual writes own an output until automatic control is requested. If `safe_output` is configured, the runtime writes it before granting automatic ownership. The controller is in a pending transition until that write succeeds. A failure pauses or rolls back the automatic transition rather than assuming that the device reached a safe state.

Pausing removes active automatic output ownership without losing controller state. Resuming requests automatic ownership again. Removing a controller waits for its safe-output write when configured; a failed safety write retains the controller. Always set and validate a meaningful `safe_output` for outputs where an uncontrolled value is hazardous.

## PID

Use PID for continuous processes with a measured value and a controllable continuous output.

```lua
local heater = app.virtual_instrument({ id = 1 })
local loop = heater:pid("power", {
  name = "temperature_pid", input = "temperature", setpoint = 200,
  kp = 2.0, ki = 0.05, kd = 0.2,
  output_min = 0, output_max = 100, safe_output = 0,
})
loop:add()
```

`kp` is required; `ki` and `kd` default to zero. All gains are non-negative and output limits are finite with `output_min < output_max`.

The derivative term is calculated from measurement rate, not error rate:

```text
D = -kd × (measurement - previous_measurement) / elapsed_seconds
```

This avoids a derivative kick on a setpoint change. Conditional integration limits integral growth that would drive an already saturated output farther into saturation. Timestamps must increase. `reset_integral()` clears only the integral; `reset()` also clears the previous-sample baseline. `resynchronize` is used internally after topology changes to avoid treating an old sample as adjacent to a new one.

## On/off

Use on/off control for actuators with discrete off/on operating levels.

```lua
local loop = heater:on_off("power", {
  name = "thermostat", input = "temperature", setpoint = 70,
  hysteresis = 1.5, output_off = 0, output_on = 100, safe_output = 0,
})
loop:add()
```

The hysteresis band prevents output chatter around the setpoint. `reset()` returns the controller to its initial inactive state; `resynchronize` preserves the current state while accepting a new input baseline.

## Furnace

Use the furnace controller for a process adequately represented by its thermal model. It combines proportional/integral feedback with predicted measurement, heater lag, and feed-forward terms.

```lua
local loop = heater:furnace("power", {
  name = "furnace", input = "temperature", setpoint = 900,
  kp = 1.0, ki = 0.02, output_min = 0, output_max = 100,
  ambient_temperature = 20, max_power = 1000, heater_lag = 15,
  linear_loss = 1.2, radiation_loss_1000c = 80, safe_output = 0,
})
loop:add()
```

`heater_lag` controls the predicted heater response; loss parameters define the model's heat-loss estimate. Model mismatch can reduce performance, so tune from diagnostics rather than treating the values as universal constants. The controller exposes diagnostic series through `loop:diagnostics()` and `loop:add_diagnostic(...)`.

## Common handle operations

After `add()`, a controller handle provides `parameters()`, `read(key)`, `write(key, value)`, `configure(updates)`, `set_fixed_reference(value)`, `set_ramp_reference({ start, target, rate })`, `pause()`, `resume()`, `reset_integral()`, `reset()`, and `remove()`. Configuration updates are validated as a complete candidate before controller state changes, preventing a partially applied invalid update.
