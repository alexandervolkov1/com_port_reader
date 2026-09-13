local SCRIPT = "furnace_manual_demo"
local RAW, FILTERED = "furnace_temperature", "furnace_temperature_ma"
local POWER, EFFECTIVE = "furnace_heater_power", "furnace_effective_power"

local plant, controller, controller_kind
local registered, generation = false, 0
local mode, manual_power, ma_window = "Manual", 0.0, 5

local settings = {
    PID = { setpoint = 500, kp = 0.4, ki = 0.0005, kd = 5, output_min = 0, output_max = 70 },
    Furnace = {
        setpoint = 500, kp = 0.1, ki = 0.0005, output_min = 0, output_max = 70,
        ambient_temperature = 20, max_power = 2500, heater_lag = 90,
        linear_loss = 0.35, radiation_loss_1000c = 1200,
    },
}

local model = {
    ambient_temperature = 20, max_power = 2500, heater_lag = 90,
    thermal_capacity = 4000, linear_loss = 0.35,
    radiation_loss_1000c = 1200, noise_amplitude = 0.2,
}

local model_defs = {
    { "ambient_temperature", "Room temperature, °C", -50, 100, 1 },
    { "max_power", "Heater rating, W", 1, 50000, 100 },
    { "heater_lag", "Heater response time, s", 0.1, 10000, 10 },
    { "thermal_capacity", "Thermal capacity, J/K", 100, 1e7, 1000 },
    { "linear_loss", "Heat loss per degree, W/K", 0, 100, 0.05 },
    { "radiation_loss_1000c", "Radiation at 1000 °C, W", 0, 50000, 100 },
    { "noise_amplitude", "Sensor noise amplitude, °C", 0, 50, 0.1 },
}

local common_defs = {
    { "setpoint", "Target temperature, °C", 20, 1200, 5 },
    { "kp", "Proportional gain (Kp)", 0, 10, 0.05 },
    { "ki", "Integral gain (Ki)", 0, 0.1, 0.0001 },
    { "output_min", "Minimum heater command, %", 0, 100, 1 },
    { "output_max", "Maximum heater command, %", 0, 100, 1 },
}

local controls = {
    { kind = "readout", id = "status", label = "Status", initial = "Starting." },
    { kind = "button", id = "manual", label = "Use manual control", on_click = "manual" },
    { kind = "button", id = "pid", label = "Use PID controller", on_click = "pid" },
    { kind = "button", id = "furnace", label = "Use predictive Furnace", on_click = "furnace" },
    { kind = "number", id = "heater_power", label = "Manual power, %",
      initial = 0, min = 0, max = 100, step = 1, on_change = "set_heater_power" },
    { kind = "number", id = "ma_window", label = "Smoothing window, samples",
      initial = ma_window, min = 1, max = 100, step = 1, on_change = "set_ma_window" },
}

-- Panels are separate blocks; control IDs remain stable for callbacks and tests.
local groups = {
    { id = "controls", title = "1. Operation", controls = controls },
    { id = "filter", title = "2. Temperature smoothing", controls = { table.remove(controls) } },
    { id = "pid_settings", title = "3. PID tuning", controls = {} },
    { id = "furnace_settings", title = "4. Predictive Furnace tuning", controls = {} },
    { id = "plant_settings", title = "5. Simulated furnace", controls = {
        { kind = "readout", id = "model_hint", label = "Experiment",
          initial = "Plant and controller models are independent." },
    } },
}
table.insert(controls, 1, { kind = "readout", id = "guide", label = "First steps",
    initial = "Set manual power to 30%; observe heating, then choose a controller." })
local script = { id = SCRIPT, panels = groups }
local locations = {}
local function locate()
    for _, panel in ipairs(groups) do
        for _, control in ipairs(panel.controls) do locations[control.id] = panel.id end
    end
end
local function set(id, value)
    if registered then app.set_control(SCRIPT, locations[id], id, value) end
end
local function enable(id, enabled, reason)
    if registered then app.set_control_enabled(SCRIPT, locations[id], id, enabled, reason) end
end
local function status(message)
    local active = plant ~= nil
    for id, kind in pairs({ manual = "Manual", pid = "PID", furnace = "Furnace" }) do
        enable(id, active and mode ~= kind and mode ~= "Fault", "Start the model; choose a different mode. On failure, use Heater OFF.")
    end
    enable("heater_power", active and mode == "Manual", "Manual power is available only in Manual mode.")
    enable("ma_window", active, "Start the model first.")
    enable("power_off", active, "The model is stopped.")
    enable("stop", active, "The demo is already stopped.")
    enable("reset_integral", active and controller ~= nil and mode ~= "Manual" and mode ~= "Fault",
        "Select an automatic controller first.")
    -- Settings can be prepared before switching modes; inactive controller tuning is intentional.
    local state = controller and controller:state() or "not installed"
    set("status", message or (active and ("%s | controller %s"):format(mode, state) or "Stopped"))
end

local function add_setting(prefix, label, values, definition, apply)
    local key, id = definition[1], prefix .. "_" .. definition[1]
    local target = groups[prefix == "pid" and 3 or prefix == "furnace" and 4 or 5].controls
    target[#target + 1] = {
        kind = "number", id = id, label = (label ~= "" and label .. " " or "") .. definition[2],
        initial = values[key], min = definition[3], max = definition[4], step = definition[5],
        on_change = "set_" .. id,
    }
    script["set_" .. id] = function(value)
        local ok, actual = pcall(apply, key, value)
        if not ok then set(id, values[key]); error(actual) end
        values[key] = actual
        set(id, actual)
        status()
    end
end

for _, kind in ipairs({ "PID", "Furnace" }) do
    local function configure(key, value)
        local values = settings[kind]
        local low = key == "output_min" and value or values.output_min
        local high = key == "output_max" and value or values.output_max
        if low >= high then error("Output minimum must be below maximum") end
        if controller and controller_kind == kind then return controller:write(key, value) end
        return value
    end
    for _, definition in ipairs(common_defs) do
        add_setting(kind:lower(), "", settings[kind], definition, configure)
    end
    if kind == "PID" then
        add_setting("pid", "", settings.PID, { "kd", "Derivative gain (Kd)", 0, 200, 1 }, configure)
    else
        for _, definition in ipairs(model_defs) do
            if settings.Furnace[definition[1]] ~= nil then
                add_setting("furnace", "Model:", settings.Furnace, definition, configure)
            end
        end
    end
end

for _, definition in ipairs(model_defs) do
    add_setting("plant", "", model, definition, function(key, value)
        if not plant then return value end
        return plant:write(key, value)
    end)
end

for _, button in ipairs({
    { "power_off", "Heater OFF (keep recording)" }, { "reset_integral", "Reset controller integral" },
    { "run", "Start / restart (clear history)" }, { "stop", "Stop demo (clear history)" },
}) do
    controls[#controls + 1] = {
        kind = "button", id = button[1], label = button[2], on_click = button[1],
    }
end

local function create_controller(kind)
    generation = generation + 1
    local options = { name = "furnace_" .. kind:lower() .. "_" .. generation,
                      input = FILTERED, safe_output = 0 }
    for key, value in pairs(settings[kind]) do options[key] = value end
    controller = plant[kind:lower()](plant, "heater_power", options)
    controller_kind = kind
    controller:pause()
    for _, diagnostic in ipairs(controller:diagnostics()) do
        local thermal = diagnostic == "setpoint" or diagnostic == "predicted_measurement"
        controller:add(diagnostic, { pane = thermal and "temperature"
            or diagnostic == "measurement_rate" and "rate" or "command",
            visible = diagnostic == "setpoint" or diagnostic == "output"
                or diagnostic == "feed_forward" or diagnostic == "predicted_measurement" })
    end
end

local function switch_mode(requested)
    if not plant or mode == "Fault" then return end
    local ok, err = pcall(function()
        if mode == requested then return end
        local current = plant:read("heater_power")
        if controller then controller:pause() end
        mode = "Manual"
        manual_power = current
        if requested ~= "Manual" and controller_kind ~= requested then
            if controller then controller:remove() end
            controller, controller_kind = nil, nil
            create_controller(requested)
        end
        if requested == "Manual" then
            manual_power = plant:write("heater_power", current)
            set("heater_power", manual_power)
        else
            controller:resume()
        end
        mode = requested
    end)
    if not ok then
        mode = "Fault"
        if not controller or pcall(function() controller:pause() end) then mode = "Manual" end
        status("Mode change failed: " .. tostring(err))
        error(err)
    end
    status()
end

function script.manual() switch_mode("Manual") end
function script.pid() switch_mode("PID") end
function script.furnace() switch_mode("Furnace") end

function script.set_heater_power(value)
    if not plant then return end
    if mode ~= "Manual" then
        set("heater_power", manual_power)
        status("Switch to Manual before changing heater power.")
        return
    end
    manual_power = plant:write("heater_power", value)
    set("heater_power", manual_power)
    status()
end

function script.set_ma_window(value)
    local n = math.floor(value + 0.5)
    if not plant then return end
    if n < 1 or n > 100 then error("Smoothing window must be between 1 and 100 samples") end
    app.set_filter(FILTERED, { kind = "moving_average", window = n })
    ma_window = n
    set("ma_window", n)
end

function script.power_off()
    if not plant then return end
    if controller then controller:pause() end
    manual_power = plant:write("heater_power", 0)
    mode = "Manual"
    set("heater_power", manual_power)
    status()
end

function script.reset_integral()
    if controller and mode ~= "Manual" and mode ~= "Fault" then controller:reset_integral(); status() end
end

function script.stop()
    script.power_off()
    app.stop()
    app.clear()
    app.stop_emu()
    plant, controller, controller_kind = nil, nil, nil
    mode = "Manual"
    status("Furnace demo stopped.")
end

function script.run()
    script.stop()
    generation = 0
    app.start_emu()
    plant = app.virtual_instrument({ id = 1 })
    for key in pairs(model) do
        model[key] = plant:write(key, model[key])
        set("plant_" .. key, model[key])
    end
    manual_power = plant:write("heater_power", 0)
    plant:add("temperature", { name = RAW, interval = 0.5, color = "#808080", pane = "temperature" })
    app.filter(RAW, { name = FILTERED, kind = "moving_average", window = ma_window, color = "#D32F2F", pane = "temperature" })
    plant:add("heater_power", { name = POWER, interval = 0.5, color = "#1976D2", pane = "command" })
    plant:add("effective_power", { name = EFFECTIVE, interval = 0.5, color = "#F57C00", pane = "watts" })
    app.start()
    set("heater_power", manual_power)
    status()
end

-- A failed safe write must leave the controls in a recoverable, non-manual state.
for _, name in ipairs({ "power_off", "stop", "run" }) do
    local action = script[name]
    script[name] = function()
        local ok, err = pcall(action)
        if not ok then
            mode = "Fault"
            status("Action failed; retry Heater OFF: " .. tostring(err))
            error(err)
        end
    end
end
locate()
app.unregister_script(SCRIPT)
script.run()
app.register_script(script)
registered = true
status()
