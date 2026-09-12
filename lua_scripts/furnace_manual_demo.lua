local SCRIPT, PANEL = "furnace_manual_demo", "controls"
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
    { "ambient_temperature", "Ambient, °C", -50, 100, 1 },
    { "max_power", "Max power, W", 1, 50000, 100 },
    { "heater_lag", "Heater lag, s", 0.1, 10000, 10 },
    { "thermal_capacity", "Heat capacity, J/K", 100, 1e7, 1000 },
    { "linear_loss", "Linear loss, W/K", 0, 100, 0.05 },
    { "radiation_loss_1000c", "Radiation loss @ 1000 °C, W", 0, 50000, 100 },
    { "noise_amplitude", "Noise, °C", 0, 50, 0.1 },
}

local common_defs = {
    { "setpoint", "Setpoint, °C", 20, 1200, 5 },
    { "kp", "Kp", 0, 10, 0.05 },
    { "ki", "Ki, 1/s", 0, 0.1, 0.0001 },
    { "output_min", "Min output, %", 0, 100, 1 },
    { "output_max", "Max output, %", 0, 100, 1 },
}

local controls = {
    { kind = "readout", id = "status", label = "Status", initial = "Starting." },
    { kind = "button", id = "manual", label = "Manual", on_click = "manual" },
    { kind = "button", id = "pid", label = "PID", on_click = "pid" },
    { kind = "button", id = "furnace", label = "Furnace", on_click = "furnace" },
    { kind = "number", id = "heater_power", label = "Manual power, %",
      initial = 0, min = 0, max = 100, step = 1, on_change = "set_heater_power" },
    { kind = "number", id = "ma_window", label = "MA window",
      initial = ma_window, min = 1, max = 100, step = 1, on_change = "set_ma_window" },
}

local script = {
    id = SCRIPT,
    panels = {{ id = PANEL, title = "Furnace: Manual / PID / Furnace", controls = controls }},
}

local function set(id, value)
    if registered then app.set_control(SCRIPT, PANEL, id, value) end
end

local function status(message)
    if message then set("status", message); return end
    local state = controller and controller:state() or "none"
    set("status", ("%s | %s controller: %s"):format(mode, controller_kind or "No", state))
end

local function add_setting(prefix, label, values, definition, apply)
    local key, id = definition[1], prefix .. "_" .. definition[1]
    controls[#controls + 1] = {
        kind = "number", id = id, label = label .. " " .. definition[2],
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
        add_setting(kind:lower(), kind, settings[kind], definition, configure)
    end
    if kind == "PID" then
        add_setting("pid", "PID", settings.PID, { "kd", "Kd, s", 0, 200, 1 }, configure)
    else
        for _, definition in ipairs(model_defs) do
            if settings.Furnace[definition[1]] ~= nil then
                add_setting("furnace", "Controller model", settings.Furnace, definition, configure)
            end
        end
    end
end

for _, definition in ipairs(model_defs) do
    add_setting("plant", "Plant", model, definition, function(key, value)
        if not plant then return value end
        return plant:write(key, value)
    end)
end

for _, button in ipairs({
    { "power_off", "Power OFF" }, { "reset_integral", "Reset active integral" },
    { "run", "Restart model" }, { "stop", "Stop" },
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
    for _, diagnostic in ipairs(controller:diagnostics()) do controller:add(diagnostic) end
end

local function switch_mode(requested)
    if not plant then return end
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
    if n < 1 then error("MA window must be positive") end
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
    if controller then controller:reset_integral(); status() end
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
    plant:add("temperature", { name = RAW, interval = 0.5, color = "#808080" })
    app.filter(RAW, { name = FILTERED, kind = "moving_average", window = ma_window, color = "#D32F2F" })
    plant:add("heater_power", { name = POWER, interval = 0.5, color = "#1976D2" })
    plant:add("effective_power", { name = EFFECTIVE, interval = 0.5, color = "#F57C00" })
    app.start()
    set("heater_power", manual_power)
    status()
end

app.unregister_script(SCRIPT)
script.run()
app.register_script(script)
registered = true
status()
