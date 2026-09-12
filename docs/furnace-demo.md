# Furnace control demo

Load `profiles/furnace_manual.lua` directly; it uses the in-memory emulator by
default, so no virtual COM-port pair is required. The profile runs
`emulator_scripts/furnace_plant.lua` and opens the
Manual / PID / Furnace panel. It starts in Manual with zero heater power.

- Manual allows direct heater power changes.
- PID uses the filtered temperature and the PID settings in the panel.
- Furnace uses the same filtered temperature, with prediction and model-based
  heat-loss compensation. Its model settings are independent of the simulated
  plant settings, so model mismatch can be explored.
- Power OFF safely pauses the current controller and writes zero power.
- Restart model resets the plant and clears the plotted history. Controller
  settings, plant settings and the moving-average window are retained.

Switches use the existing safe pause/resume and output arbitration. Switching
to Manual reads the current power before pausing, then restores that power after
the safe write. A safe-output pulse can therefore occur during the transition.
Only one controller owns the heater at a time. Switching controller type removes
the old instance and creates a new one with the saved settings and fresh dynamic
state. Returning from Manual to the same type resumes its existing instance.
Temperature and power histories remain intact. Each new instance gets separate
diagnostic series; old diagnostic samples remain visible but receive no new data.

The example gains and furnace model are synthetic starting values, not settings
for real equipment. See the [deterministic comparison](furnace-controller-comparison.md)
for the current measured baseline and its limitations.

## Lua API

```lua
local controller = plant:furnace("heater_power", {
    name = "furnace_controller",
    input = "furnace_temperature_ma",
    setpoint = 500,
    kp = 0.1,
    ki = 0.0005,
    output_min = 0,
    output_max = 70,
    ambient_temperature = 20,
    max_power = 2500,
    heater_lag = 90,
    linear_loss = 0.35,
    radiation_loss_1000c = 1200,
    safe_output = 0,
})
controller:add("feed_forward")
controller:add("predicted_measurement")
controller:add("measurement_rate")
```

`furnace()` is available on both virtual instruments and Metakon handles, using
the same output-target validation as `pid()` and `on_off()`. All options shown
are required except `ki` (default zero) and `safe_output`. Configure `safe_output`
for safe pause/removal. Constructors follow the existing API and start Running.

The returned handle supports the existing `parameters`, `read`, `write`,
`configure`, reference, input, state, pause/resume and reset methods.
`setpoint` is directly writable until a fixed/ramp reference is installed; after
that, use the reference API. See [controller methods](lua-api.md#controller-methods)
and [output safety](output-safety.md) for the complete contract.
`thermal_capacity` is a plant parameter, not a Furnace controller parameter.

Furnace diagnostics are `setpoint`, `proportional`, `integral`, `output`,
`unconstrained_output`, `feed_forward` (output units), `predicted_measurement`
(temperature units) and `measurement_rate` (smoothed temperature change per second).

`controller:remove()` waits for safe pause, releases output ownership, removes
the controller and detaches its diagnostic streams. It preserves existing series
and their samples. A failed safe write leaves the controller registered and
paused; it does not release the output. Removed handles must no longer be used.
