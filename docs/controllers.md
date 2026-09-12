# Controllers

Controllers consume timestamped raw or filtered samples and produce numeric output requests. They run in the signal-processing service; the output service arbitrates writes and connection workers perform I/O. A loop's `running/paused` state is distinct from its output's `manual/automatic_pending/automatic` ownership.

Follow the [tutorial](lua-tutorial.md) to obtain `plant` and a `temperature` series. Choose one example below at a time because an output cannot belong to two controllers. Constructor commands immediately enqueue installation; there is no subsequent `add()` registration step.

## Common configuration and lifecycle

Every constructor requires `name` (unique controller name), `input` (existing series name) and `setpoint`. Set `safe_output` explicitly; no safe value is inferred. Output targets must be writable numeric parameters. The entire configured output range must fit the target's permitted range; integer/register targets also impose representability constraints. Limits belong to the algorithm; safe output belongs to the actuator target and can be outside the algorithm's operating interval while still fitting the device range.

Controllers use measurement engineering units. PID/on/off do not enforce Celsius; furnace expects Celsius. All numeric configuration is finite. Configuration validates the full candidate before committing it and preserves accumulated algorithm state. It cannot rename a controller or change its physical output.

```lua
return loop:parameters()
```

Descriptors list the concrete controller's keys, types, access and independent bounds. Cross-field requirements, such as ordered limits, are checked on configuration.

```lua
return loop:read("setpoint")
```

This reads the effective setpoint. When an explicit reference is installed, it becomes read-only in the controller parameter table.

| Operation | Example | Consequence |
| --- | --- | --- |
| Inspect | `return loop:state()` | `running` or `paused`; does not report ownership. |
| Pause | `loop:pause()` | Dispatch safe output, pause computation and wait for write result. A write error is still reported after computation pauses. |
| Resume | `loop:resume()` | Request automatic takeover, resynchronize after pause and resume computation; next successful automatic write confirms takeover. |
| Reset all | `loop:reset()` | Reset algorithm and reference elapsed time. State remains running/paused as before; no immediate hardware write. |
| Reset integral | `loop:reset_integral()` | PID/furnace only; preserve other timing state. On/off returns an unsupported-operation error. |
| Switch input | `loop:set_input("temperature_smooth")` | Requires an existing series; resynchronizes algorithm timing. |
| Remove | `loop:remove()` | Wait for safe pause, release ownership, remove loop and diagnostics. Failures are surfaced. |

Pausing preserves integral accumulation and on/off state. Resume resets the time baseline so the first new sample does not integrate the paused duration. Stopping acquisition alone does not perform this lifecycle transition.

## PID

PID is useful when an output can vary continuously and proportional, integral and rate feedback describe the desired response.

```lua
loop = plant:pid("heater_power", {
    name = "heater", input = "temperature", setpoint = 80,
    kp = 1, ki = 0.05, kd = 0.2,
    output_min = 0, output_max = 100, safe_output = 0,
})
```

| Parameter | Creation | Range | Units / role |
| --- | --- | --- | --- |
| `setpoint` | required | finite | Measurement units. |
| `kp` | required | ≥ 0 | Output units / measurement unit. |
| `ki` | optional, 0 | ≥ 0 | Output units / (measurement unit × second). |
| `kd` | optional, 0 | ≥ 0 | Output units × second / measurement unit. |
| `output_min` | required | finite, less than maximum | Minimum output. |
| `output_max` | required | finite, greater than minimum | Maximum output. |

For measurement $y_k$, setpoint $r_k$ and elapsed sample time $\Delta t$:

$$
e_k = r_k-y_k,\qquad P_k=k_p e_k,\qquad
\Delta I_k=k_i e_k\Delta t,\qquad
D_k=-k_d\frac{y_k-y_{k-1}}{\Delta t}.
$$

Derivative acts on measurement, avoiding a derivative kick from changing only the setpoint. The first sample has zero derivative and no integral increment. Later timestamps must strictly increase.

The provisional output is $P_k + I_{k-1} + \Delta I_k + D_k$. Conditional anti-windup prevents an integral increment from driving output farther beyond a limit. If an increment crosses a bound from inside the allowed interval, only the part up to the bound is accumulated. An increment that unwinds existing saturation remains allowed. Final output is clamped to the configured limits. A nonfinite term rejects the sample without committing PID state.

```lua
loop:configure({kp = 1.5, ki = 0.03, kd = 0.1})
```

```lua
return loop:write("kp", 2)
```

```lua
return loop:read("ki")
```

Diagnostics: `setpoint`, `proportional`, `integral`, `derivative`, `output` and `unconstrained_output`.

```lua
loop:add("integral", "heater_integral")
```

`reset_integral` clears only I. `reset` also removes the previous sample baseline. Internal resynchronization preserves I and removes that baseline.

## On/off

On/off is a hysteretic two-level algorithm. The two numeric levels need not be zero and 100, but must be compatible with the target.

```lua
loop = plant:on_off("heater_power", {
    name = "thermostat", input = "temperature", setpoint = 80,
    hysteresis = 2, output_off = 0, output_on = 100, safe_output = 0,
})
```

| Parameter | Creation | Range | Meaning |
| --- | --- | --- | --- |
| `setpoint` | required | finite | Center of the hysteresis band. |
| `hysteresis` | required | ≥ 0, finite thresholds | Half-width in measurement units. |
| `output_off` | required | finite | Output when inactive. |
| `output_on` | required | finite | Output when active. |

With setpoint $r$ and hysteresis $h$, switch on when $y<r-h$ and off when $y>r+h$. At either boundary and throughout the band, retain the previous state. At creation/reset the state is inactive. With `setpoint=80` and `hysteresis=2`, values below 78 activate it and values above 82 deactivate it; exactly 78 or 82 retain state.

The algorithm validates finite timestamps/measurements but has no elapsed-time state of its own. A managed reference still imposes its timestamp requirements.

```lua
loop:configure({hysteresis = 3, output_on = 75})
```

```lua
return loop:write("setpoint", 90)
```

```lua
return loop:read("hysteresis")
```

Diagnostics are `setpoint` and `output`. The algorithm's active/inactive flag is not a separate Lua `state()` value.

```lua
loop:add("output", "thermostat_output")
```

`reset` returns to inactive. Pause/resume and reconfiguration preserve the current on/off state. `reset_integral` is unsupported.

## Furnace

Furnace adds a thermal feed-forward model and lag-based prediction to PI correction. Use it when temperature is in Celsius and output is percent heater power.

```lua
loop = plant:furnace("heater_power", {
    name = "furnace", input = "temperature", setpoint = 500,
    kp = 0.1, ki = 0.0005, output_min = 0, output_max = 100,
    ambient_temperature = 20, max_power = 2500, heater_lag = 90,
    linear_loss = 0.35, radiation_loss_1000c = 1200, safe_output = 0,
})
```

| Parameter | Creation | Range | Units / meaning |
| --- | --- | --- | --- |
| `setpoint` | required | ≥ -273.15 | °C. |
| `kp` | required | ≥ 0 | Percent output / °C. |
| `ki` | optional, 0 | ≥ 0 | Percent output / (°C × second). |
| `output_min`, `output_max` | required | finite, min < max | Percent output, within target range. |
| `ambient_temperature` | required | ≥ -273.15 | °C ambient used for loss prediction. |
| `max_power` | required | > 0 | W at 100% output. |
| `heater_lag` | required | ≥ 0 | Seconds of prediction horizon. |
| `linear_loss` | required | ≥ 0 | W/K linear heat-loss coefficient. |
| `radiation_loss_1000c` | required | ≥ 0 | W radiation coefficient normalized at 1000 °C against 20 °C. |

All values must be finite. Descriptor limits do not encode every physical cross-check; construction/configuration performs those checks.

For setpoint $r$, ambient $a$, maximum power $P_{\max}$ and Kelvin offset 273.15:

$$
Q_{\rm loss}(r)=L(r-a)+R\frac{(r+273.15)^4-(a+273.15)^4}
{1273.15^4-293.15^4},
\qquad
u_{\rm ff}=100\frac{\max(0,Q_{\rm loss}(r))}{P_{\max}}.
$$

The radiation normalization denominator always uses 20 °C, even if the configured ambient differs.

For a later sample, raw rate is $(y_k-y_{k-1})/\Delta t$. The measured rate is exponentially smoothed with $\tau=heater\_lag/3$:

$$
\alpha=1-\exp(-\Delta t/\tau),\qquad
v_k=v_{k-1}+\alpha(v_{\rm raw}-v_{k-1}).
$$

When heater lag is zero, raw rate is used directly. On the first sample/resynchronization the rate is zero. Predicted temperature and feedback error are:

$$
y_{\rm predicted}=y_k+heater\_lag\cdot v_k,\qquad
e_k=r-y_{\rm predicted}.
$$

Final provisional output is $u_{\rm ff}+k_p e_k+I_k$. Integral update, conditional anti-windup and clamping follow the PID approach. There is no separate D gain: measurement-rate prediction provides the anticipatory term. This controller does not simulate the plant's heat capacity or heater energy state; those belong to the emulator/physical process.

```lua
loop:configure({heater_lag = 60, linear_loss = 0.4, ki = 0.001})
```

```lua
return loop:write("max_power", 2400)
```

```lua
return loop:read("ambient_temperature")
```

Diagnostics: `setpoint`, `proportional`, `integral`, `output`, `unconstrained_output`, `feed_forward`, `predicted_measurement`, `measurement_rate`.

```lua
loop:add("predicted_measurement", "predicted_temperature")
```

Resetting the integral preserves rate history; full reset clears integral, rate and sample baseline. Resynchronization clears rate/baseline while preserving the integral. Rejected nonfinite calculations do not commit rate or sample state.

## References and diagnostic interpretation

All three controllers support explicit fixed and ramp references through the common handle:

```lua
loop:set_fixed_reference(80)
```

```lua
loop:set_ramp_reference({start = 20, target = 100, rate = 0.5})
```

A ramp advances using sample timestamps, starts at its declared start on the first sample and stops at its target. Its rate is a positive magnitude; cooling is selected by a lower target. Changing target/rate continues from the current reference; changing start explicitly restarts it. See the full [reference method examples](lua-api.md#references).

```lua
return loop:read("setpoint")
```

Use this to inspect current reference progress. `read_reference("target")` reads the configured destination.

Diagnostic samples are generated by processing, not polled independently. They expose computation even when the output service later rejects a write in manual mode. To determine what was written, inspect `control_outputs.actual_output` and application errors. [Output safety](output-safety.md) explains ownership, safe writes and shutdown.
