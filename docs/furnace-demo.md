# Furnace control demo

Load `profiles/furnace_manual.lua` directly; it uses the in-memory emulator by
default, so no virtual COM-port pair is required. The profile runs
`emulator_scripts/furnace_plant.lua` and opens the
Manual / PID / Furnace controls. Open **Control panel** in the toolbar. The model
starts recording in Manual with zero heater power; it does not heat until asked.

There are two independent profiles for the same emulator:

| Profile | Best for | First action |
| --- | --- | --- |
| [furnace_manual.lua](../profiles/furnace_manual.lua) | Exploring control modes, filters and model mismatch | Set manual power to 30% |
| [furnace_scenarios.lua](../profiles/furnace_scenarios.lua) | Guided, repeatable automatic experiments | Choose Power steps, then Run selected |

Load one profile at a time; do not run both scripts together in the REPL.
Use the demo's heater-OFF/stop controls before changing profiles.

## Manual exploration

The control window is split into five blocks: operation, temperature smoothing,
PID tuning, predictive Furnace tuning, and the simulated plant.
The selected mode button is disabled. Manual power is disabled during automatic
control; integral reset requires an active automatic controller. After Stop, only
restart and preparation of saved settings remain useful. Disabled controls explain
why on hover. Callback guards also reject inappropriate actions.

Start with 30% manual power and observe the first minute. The original model has a
90-second heater response time: slow heating is intentional. Switch to PID or
Furnace, then adjust **Target temperature** and **Maximum heater command**.
The initial 500 °C target is a long experiment, not a quick warm-up.
Use the scenario profile below for short guided runs.

Temperature and target, heater command (%), delivered power (W), and temperature
change (°C/s) have separate plot panes. Principal controller diagnostics appear
automatically; other terms are initially hidden and can be shown from the signal
list. A smoothing window of five samples is about 2.5 seconds at the default
sampling interval. Larger windows suppress noise but add delay.

- Manual allows direct heater power changes.
- PID uses the filtered temperature and the PID settings in the panel.
- Furnace uses the same filtered temperature, with prediction and model-based
  heat-loss compensation. Its model settings are independent of the simulated
  plant settings, so model mismatch can be explored.
- Heater OFF safely pauses the current controller and writes zero power, retaining
  recording and plotted history. This does not instantly cool the furnace.
- Start / restart resets the plant and clears the plotted history. Controller
  settings, plant settings and the moving-average window are retained.
- Stop demo safely switches heating off, stops acquisition and the emulator, and
  clears history. Use Heater OFF instead if you want to inspect the curves.

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

## Guided scenarios

Run `cargo run -- --config profiles/furnace_scenarios.lua`, then open **Control
panel**. Initially no experiment is running. Choose a preset and press **Run
selected (reset model / history)**. Each run starts from a cold model and clears
the previous plot; completion or stopping keeps the current plot and recording.
The three panel blocks cover selection, operation/results, and observation tips.
Selection and Run are disabled until the current experiment has safely finished.

| Preset | Sequence | Demonstrates |
| --- | --- | --- |
| Power steps | 25% for 15 s → 60% for 15 s → OFF for 20 s | Timed stages, thermal inertia, three physical quantities |
| Ramp and hold | Target 20 → 45 °C at 0.5 °C/s → within ±2 °C for 5 s → hold 10 s → OFF for 10 s | PID, ramp/fixed references, filtered measurement conditions, diagnostics |
| Temperature guard | 80% until temperature exceeds 30 °C or 20 s elapse | A measurement/timer race with safe cleanup |

Power steps lasts 50 seconds. Ramp and hold usually takes 1–2 minutes and has a
150-second overall limit. The temperature guard is an intentional successful
limit demonstration, not a broken demo; its timeout is an alternative stopped
outcome. Every preset also watches for temperature above 80 °C or missing samples
for five seconds. These are software examples, not independent hardware protection.

The model is deliberately faster than the manual profile: thermal capacity
1000 J/K, heater response time 3 s, and heat loss 8 W/K. Both profiles use the
unchanged [furnace emulator](../emulator_scripts/furnace_plant.lua); only its
configuration differs. Demo gains are not suitable instructions for real equipment.

Open **Scenarios** beside the control panel to watch the current stage, pending
conditions and last transition. The panel describes what to look for; the
application log records stage changes and results. Stop experiment requests
graceful scenario finalization. Completion, operator stop, timeout and callback
failure all attempt to pause the controller and write zero. If that fails, Run
stays disabled and **Retry safe heater OFF** becomes available. Inspect the error
and recover before starting another experiment.

Do not use the toolbar's acquisition Stop as an experiment stop: it only stops
sampling; scenario timers still run and the missing-data guard will request
cleanup. Use the experiment's own Stop button. Heater OFF removes the command,
not stored energy, so temperature can continue rising briefly.

For code, read [the scenario script](../lua_scripts/furnace_scenarios_demo.lua)
alongside the [scenario API](scenarios.md). All waiting is handled by scheduled
callbacks; the script has no polling/sleep loop.

The regular `cargo test` suite checks mode changes, preset switching, operator
stop and injected cleanup failures. To run all three complete experiences with
the unmodified model and real timers (about 2.5 minutes), use
`cargo test shipped_furnace_scenarios_complete -- --ignored`.

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
