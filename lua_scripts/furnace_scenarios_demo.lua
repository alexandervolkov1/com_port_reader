-- Guided experiments for the same furnace emulator as furnace_manual_demo.lua.
-- Use the matching profile: it supplies the emulator and named plot panes.
local SCRIPT = "furnace_scenarios_demo"
local RAW, FILTERED = "recipe_temperature", "recipe_temperature_smooth"
local plant, controller, run
local state, selected, registered = "idle", "steps", false
local presets = {
    steps = { title = "1. Power steps", duration = "50 seconds",
        description = "25% for 15 s, 60% for 15 s, then heater OFF for 20 s.",
        lesson = "Compare the immediate command with delayed heating power and temperature." },
    recipe = { title = "2. Ramp and hold", duration = "Usually 1-2 minutes; limit 150 s",
        description = "PID: ramp from 20 to 45 °C, settle within ±2 °C, hold, then switch OFF.",
        lesson = "Watch the target ramp, measured temperature and automatic heater command." },
    guard = { title = "3. Temperature guard", duration = "At most 20 seconds",
        description = "80% power until 30 °C; a 20 s timeout competes with the temperature trigger.",
        lesson = "An intentional limit test: the first race condition wins and switches heating OFF." },
}
local script = { id = SCRIPT, panels = {
    { id = "choose", title = "1. Choose an experiment", controls = {
        { kind = "button", id = "steps", label = "1. Power steps", on_click = "choose_steps" },
        { kind = "button", id = "recipe", label = "2. Ramp and hold", on_click = "choose_recipe" },
        { kind = "button", id = "guard", label = "3. Temperature guard", on_click = "choose_guard" },
        { kind = "readout", id = "selected", label = "Selected" },
        { kind = "readout", id = "duration", label = "Duration" },
    } },
    { id = "operation", title = "2. Run and observe", controls = {
        { kind = "readout", id = "status", label = "State", initial = "Ready" },
        { kind = "readout", id = "stage", label = "Current step", initial = "Not started" },
        { kind = "button", id = "start", label = "Run selected (reset model / history)", on_click = "start" },
        { kind = "button", id = "stop", label = "Stop experiment / heater OFF", on_click = "stop" },
        { kind = "button", id = "recover", label = "Retry safe heater OFF", on_click = "recover" },
        { kind = "readout", id = "result", label = "Last result", initial = "No experiment yet" },
    } },
    { id = "guide", title = "3. What to look for", controls = {
        { kind = "readout", id = "description", label = "Sequence" },
        { kind = "readout", id = "lesson", label = "Observe" },
        { kind = "readout", id = "tools", label = "Explore",
          initial = "Open Scenarios for stages and triggers; use the plot legend to show signals." },
        { kind = "readout", id = "model", label = "Fast demo model",
          initial = "1000 J/K capacity, 3 s heater lag, 8 W/K heat loss. Simulation only." },
        { kind = "readout", id = "cooling", label = "After heater OFF",
          initial = "Stored heat remains: temperature can still rise. Recording continues." },
    } },
} }

local function set(panel, id, value)
    if registered then app.set_control(SCRIPT, panel, id, value) end
end
local function enable(panel, id, value, reason)
    if registered then app.set_control_enabled(SCRIPT, panel, id, value, reason) end
end
local function refresh()
    for key in pairs(presets) do
        enable("choose", key, state == "idle" and key ~= selected,
            "Finish or stop the current run before choosing another experiment.")
    end
    enable("operation", "start", state == "idle", "Wait for safe completion; resolve any heater-OFF error first.")
    enable("operation", "stop", state == "running", "No experiment is running, or stop is already pending.")
    enable("operation", "recover", state == "fault", "Available only after a safe-output failure.")
    set("operation", "status", state)
    set("choose", "selected", presets[selected].title)
    set("choose", "duration", presets[selected].duration)
    set("guide", "description", presets[selected].description)
    set("guide", "lesson", presets[selected].lesson)
end
for key in pairs(presets) do
    script["choose_" .. key] = function()
        if state ~= "idle" then return end
        selected = key
        refresh()
    end
end

local function heater_off()
    -- Attempt a direct zero even if the controller safe write fails.
    local paused, pause_error = true, nil
    if controller then paused, pause_error = pcall(function() controller:pause() end) end
    if plant then plant:write("heater_power", 0) end
    if not paused then error(pause_error) end
end
local function finish(message)
    local ok, err = pcall(heater_off)
    state = ok and "idle" or "fault"
    set("operation", "stage", ok and "Heater OFF; recording continues" or "Safe output needs attention")
    set("operation", "result", ok and message or "Heater OFF failed: " .. tostring(err))
    app.log("Furnace demo: " .. (ok and message or tostring(err)))
    refresh()
    if not ok then error(err) end
end
function script.furnace_demo_finished(event)
    finish((event.status or "stopped") .. ": " .. (event.reason or "operator request"))
end
function script.furnace_demo_failed(event)
    finish("failed: " .. tostring(event.error))
end
function script.stop()
    if state ~= "running" then return end
    state = "stopping"
    refresh()
    run:stop("Stopped by operator")
end
function script.recover()
    if state ~= "fault" then return end
    finish("Safe output restored; choose an experiment and run again.")
end

local function stage(label, action)
    return function()
        if state ~= "running" then return end
        set("operation", "stage", label)
        app.log("Furnace demo: " .. label)
        action()
    end
end
script.furnace_demo_low = stage("Low power: 25% (15 s)", function() plant:write("heater_power", 25) end)
script.furnace_demo_high = stage("High power: 60% (15 s)", function() plant:write("heater_power", 60) end)
script.furnace_demo_coast = stage("Heater OFF: observe stored heat (20 s)", heater_off)
script.furnace_demo_done = stage("Experiment complete", function() run:complete("Sequence finished") end)
script.furnace_demo_ramp = stage("Ramp: 20 → 45 °C at 0.5 °C/s", function()
    controller = plant:pid("heater_power", {
        name = "recipe_pid", input = FILTERED, setpoint = 20,
        kp = 8, ki = 0.8, kd = 0, output_min = 0, output_max = 80, safe_output = 0,
    })
    controller:pause()
    controller:set_ramp_reference({ start = 20, target = 45, rate = 0.5 })
    controller:add("setpoint", { name = "recipe_target", pane = "temperature", color = "#388E3C" })
    controller:add("output", { name = "recipe_requested_power", pane = "command", visible = false })
    controller:resume()
end)
script.furnace_demo_settle = stage("Settle: within 45 ±2 °C for 5 s", function()
    controller:set_fixed_reference(45)
end)
script.furnace_demo_hold = stage("Hold: 45 °C for 10 s", function() end)
script.furnace_demo_cool = stage("Heater OFF: observe stored heat (10 s)", heater_off)
script.furnace_demo_guard_heat = stage("Guard test: 80% until 30 °C or 20 s", function()
    plant:write("heater_power", 80)
    run:race({
        { when = { series = RAW, above = 30 }, callback = "furnace_demo_limit" },
        { after = 20, callback = "furnace_demo_timeout" },
    })
end)
function script.furnace_demo_limit()
    if state == "running" then run:complete("Expected temperature guard reached 30 °C") end
end
function script.furnace_demo_timeout()
    if state == "running" then run:stop("Time limit reached; heating stopped") end
end
function script.furnace_demo_safety()
    if state == "running" then run:stop("Safety guard: temperature above 80 °C or samples missing for 5 s") end
end

local function prepare()
    heater_off()
    app.stop()
    app.clear()
    app.stop_emu()
    plant, controller = nil, nil
    app.start_emu()
    plant = app.virtual_instrument({ id = 1 })
    for key, value in pairs({
        ambient_temperature = 20, max_power = 2500, heater_lag = 3,
        thermal_capacity = 1000, linear_loss = 8, radiation_loss_1000c = 1200, noise_amplitude = 0.1,
    }) do plant:write(key, value) end
    plant:write("heater_power", 0)
    plant:add("temperature", { name = RAW, interval = 0.5, pane = "temperature", color = "#808080" })
    app.filter(RAW, { name = FILTERED, kind = "moving_average", window = 3,
        pane = "temperature", color = "#D32F2F" })
    plant:add("heater_power", { name = "recipe_heater_power", interval = 0.5, pane = "command", color = "#1976D2" })
    plant:add("effective_power", { name = "recipe_effective_power", interval = 0.5, pane = "watts", color = "#F57C00" })
    app.start()
end
function script.start()
    if state ~= "idle" then return end
    state = "starting"
    refresh()
    local ok, err = pcall(function()
        prepare()
        run = app.scenario({ id = "furnace_experiment" })
        run:on_stop("furnace_demo_finished")
        run:on_error("furnace_demo_failed")
        run:when({ any = {
            { series = RAW, above = 80 },
            { series = RAW, stale_for_seconds = 5 },
        } }, "furnace_demo_safety")
        run:after(150, "furnace_demo_timeout")
        if selected == "steps" then
            run:stage("low", { enter = "furnace_demo_low", transitions = {{ after = 15, next = "high" }} })
            run:stage("high", { enter = "furnace_demo_high", transitions = {{ after = 15, next = "coast" }} })
            run:stage("coast", { enter = "furnace_demo_coast", transitions = {{ after = 20, next = "done" }} })
            run:stage("done", { enter = "furnace_demo_done" })
        elseif selected == "recipe" then
            run:stage("ramp", { enter = "furnace_demo_ramp", transitions = {{ after = 52, next = "settle" }} })
            run:stage("settle", { enter = "furnace_demo_settle", transitions = {{
                when = { series = FILTERED, stable = { target = 45, tolerance = 2 }, for_seconds = 5 },
                next = "hold",
            }} })
            run:stage("hold", { enter = "furnace_demo_hold", transitions = {{ after = 10, next = "cool" }} })
            run:stage("cool", { enter = "furnace_demo_cool", transitions = {{ after = 10, next = "done" }} })
            run:stage("done", { enter = "furnace_demo_done" })
        else
            run:stage("guard", { enter = "furnace_demo_guard_heat" })
        end
        state = "running"
        set("operation", "result", "Running: " .. presets[selected].title)
        refresh()
        run:start(selected == "steps" and "low" or selected == "recipe" and "ramp" or "guard")
    end)
    if not ok then
        -- No staged work may survive a partially built start. Explicit cleanup follows cancel.
        if run then run:cancel() end
        finish("Could not start: " .. tostring(err))
        error(err)
    end
end

app.unregister_script(SCRIPT)
app.register_script(script)
registered = true
refresh()
app.log("Furnace scenarios ready. Open Control panel, choose Power steps, then Run selected.")
