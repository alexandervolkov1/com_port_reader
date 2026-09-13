# Lua tutorial: a first experiment

This path needs no physical instrument or virtual COM driver.

## Try the furnace first (no Lua typing)

1. From the repository root, run `cargo run -- --config profiles/furnace_scenarios.lua`.
2. Open **Control panel**. Power steps is already selected; press **Run selected**.
3. Open **Scenarios**. Watch low power change to high power after 15 seconds, then
   heater OFF after another 15 seconds. Compare the command (%), delivered power
   (W), and temperature (°C) plots. Their different shapes show thermal inertia.
4. After 50 seconds the result reports completion. Recording continues, so you
   can inspect cooling. Choose **Ramp and hold**, then Run; this resets the model
   and plot. Watch the green target ramp and the red smoothed temperature.
5. After completion, try **Temperature guard**. Heating stops when the measured
   temperature exceeds 30 °C (or the 20-second timeout wins). This is intentional.
6. Start any preset again and use **Stop experiment / heater OFF**. Run and preset
   selection become available only after cleanup. A disabled button's tooltip
   explains why it cannot be used.

The [furnace guide](furnace-demo.md) explains settings, failure recovery, and a
second profile for Manual / PID / Furnace comparison. The automatic demo uses a
faster configuration of the same emulator, so it does not require a long warm-up.
For the following coding exercises, close the demo and launch the empty tutorial
profile instead; do not paste these exercises into an active demo.

## Build the experiment yourself

Start from the repository root. Save this as `profiles/tutorial.lua` (the supplied file already contains it):

```lua
return {
    application = {poll_interval = 0.5},
    emulator = {transport = "memory", script = "../emulator_scripts/furnace_plant.lua"},
}
```

Launch `cargo run -- --config profiles/tutorial.lua`. This profile starts with no scripted actions, so the following console steps are visible. Submit each block separately with Ctrl+Enter. Global handles deliberately survive between submissions.

## 1. Write a log message

```lua
app.log("Tutorial started")
```

Open the application log. Informational messages are also recorded in SQLite when available.

## 2. Start the model

```lua
app.start_emu()
```

This starts the profile's furnace model over memory transport. It does not start polling.

## 3. Discover the virtual instrument

```lua
plant = app.virtual_instrument({id = 1})
return plant:name()
```

The console should show `Virtual furnace`. `plant:parameters()` lists the actual keys, types and access modes.

## 4. Read one value

```lua
return plant:read("temperature")
```

The initial furnace temperature is near 20 °C. A manual read returns a value but does not create a plotted series.

## 5. Plot periodic measurements

```lua
plant:add("temperature", {name = "temperature", interval = 0.5})
app.start()
```

Wait for the first sample. Use the series list to change visibility; double-click the plot to resume following new data.

## 6. Write a parameter

```lua
return plant:write("heater_power", 10)
```

The model returns the stored percent output. Its temperature responds slowly because the model includes heater lag and heat capacity.

## 7. Add a filtered signal

```lua
app.filter("temperature", {
    name = "temperature_smooth", kind = "exponential", time_constant = 2,
})
```

Both raw and filtered signals remain available. This filter uses elapsed sample time.

## 8. Create a controller

```lua
loop = plant:pid("heater_power", {
    name = "heater", input = "temperature_smooth", setpoint = 80,
    kp = 1, ki = 0.02, kd = 0,
    output_min = 0, output_max = 100, safe_output = 0,
})
```

The command registers a running controller. Acquisition drives it. Read `loop:state()` to verify installation; a failed asynchronous construction appears in the application log. These gains are a demonstration, not a tuning prescription for real hardware.

## 9. Plot a diagnostic

```lua
loop:add("output", "requested_power")
```

This is the controller's requested output. Actual instrument results and failures are tracked separately. `loop:diagnostics()` lists all supported keys.

## 10. Add a small control panel

```lua
local script = {id = "tutorial"}
script.pause = function() loop:pause() end
script.resume = function() loop:resume() end
script.refresh = function()
    app.set_control("tutorial", "main", "state", loop:state())
end
script.panels = {{
    id = "main", title = "Tutorial",
    controls = {
        {kind = "readout", id = "state", label = "Controller", initial = "running"},
        {kind = "button", id = "pause", label = "Pause", on_click = "pause"},
        {kind = "button", id = "resume", label = "Resume", on_click = "resume"},
        {kind = "button", id = "refresh", label = "Refresh", on_click = "refresh"},
    },
}}
app.register_script(script)
```

Open Control panel. Pause attempts to write zero and then leaves the controller paused. Refresh shows its computation state. Resume requests automatic ownership; the next successful automatic write completes takeover.

## 11. Finish

```lua
loop:remove()
app.unregister_script("tutorial")
app.stop()
app.stop_emu()
```

Removal waits for safe output; if it fails, resolve the error before stopping the model. Series/history remain until `app.clear()` or a profile replacement. The database remains in the application's `processes` directory.

Next, compare [controller types](controllers.md), configure [references](lua-api.md#references), or schedule [scenarios](scenarios.md). The [API reference](lua-api.md) covers every supported operation.
