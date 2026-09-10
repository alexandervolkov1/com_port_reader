local SCRIPT, PANEL = "furnace_manual_demo", "controls"
local RAW, FILTERED = "furnace_temperature", "furnace_temperature_ma"
local POWER, EFFECTIVE = "furnace_heater_power", "furnace_effective_power"
local PID = "furnace_pid"

local plant, pid
local registered, pid_enabled = false, false
local manual_power, ma_window = 0.0, 5

local pv = {
    setpoint = 500.0,
    kp = 0.4,
    ki = 0.0005,
    kd = 5.0,
    output_min = 0.0,
    output_max = 70.0,
}

local model = {
    ambient_temperature = 20.0,
    max_power = 2500.0,
    heater_lag = 90.0,
    thermal_capacity = 12000.0,
    linear_loss = 0.35,
    radiation_loss_1000c = 1200.0,
    noise_amplitude = 0.2,
}

local model_defs = {
    { "ambient_temperature", "Ambient, °C", -50, 100, 1 },
    { "max_power", "Furnace max power, W", 1, 50000, 100 },
    { "heater_lag", "Heater lag, s", 0.1, 10000, 10 },
    { "thermal_capacity", "Heat capacity, J/K", 100, 1e7, 1000 },
    { "linear_loss", "Linear loss, W/K", 0, 100, 0.05 },
    { "radiation_loss_1000c", "Radiation loss @ 1000 °C, W", 0, 50000, 100 },
    { "noise_amplitude", "Noise, °C", 0, 50, 0.1 },
}

local pid_defs = {
    { "setpoint", "Setpoint, °C", 20, 1200, 5 },
    { "kp", "Kp", 0, 10, 0.05 },
    { "ki", "Ki, 1/s", 0, 0.1, 0.0001 },
    { "kd", "Kd, s", 0, 200, 1 },
    { "output_min", "PID min output, %", 0, 100, 1 },
    { "output_max", "PID max output, %", 0, 100, 1 },
}

local controls = {
    { kind = "readout", id = "status", label = "Status", initial = "Starting." },

    { kind = "toggle", id = "pid_enabled", label = "PID control",
      initial = false, on_change = "set_pid_enabled" },

    { kind = "number", id = "heater_power", label = "Manual power, %",
      initial = manual_power, min = 0, max = 100, step = 1,
      on_change = "set_heater_power" },
}

local function number_control(d)
    return {
        kind = "number", id = d[1], label = d[2],
        initial = pv[d[1]] or model[d[1]] or ma_window,
        min = d[3], max = d[4], step = d[5],
        on_change = "set_" .. d[1],
    }
end

for _, d in ipairs(pid_defs) do controls[#controls + 1] = number_control(d) end

controls[#controls + 1] = {
    kind = "number", id = "ma_window", label = "MA window",
    initial = ma_window, min = 1, max = 100, step = 1,
    on_change = "set_ma_window",
}

for _, d in ipairs(model_defs) do controls[#controls + 1] = number_control(d) end

controls[#controls + 1] = {
    kind = "button", id = "power_off", label = "Power OFF",
    on_click = "power_off",
}

controls[#controls + 1] = {
    kind = "button", id = "reset_integral", label = "Reset PID integral",
    on_click = "reset_integral",
}

controls[#controls + 1] = {
    kind = "button", id = "restart", label = "Restart model",
    on_click = "run",
}

controls[#controls + 1] = {
    kind = "button", id = "stop", label = "Stop",
    on_click = "stop",
}

local script = {
    id = SCRIPT,
    panels = {{
        id = PANEL,
        title = "Furnace manual / PID",
        controls = controls,
    }},
}

local function set(id, value)
    if registered then app.set_control(SCRIPT, PANEL, id, value) end
end

local function status(message)
    if not registered then return end

    if message then
        set("status", message)
        return
    end

    local mode = pid_enabled and "PID" or "Manual"
    local state = pid and pid:state() or "stopped"

    set("status", ("%s | controller %s | SP %.0f °C | limits %.0f…%.0f %%")
        :format(mode, state, pv.setpoint, pv.output_min, pv.output_max))
end

local function model_setter(key)
    return function(value)
        if not plant then return end
        model[key] = plant:write(key, value)
        set(key, model[key])
        status()
    end
end

local function pid_setter(key)
    return function(value)
        if not pid then return end
        pv[key] = pid:write(key, value)
        set(key, pv[key])
        status()
    end
end

for _, d in ipairs(model_defs) do
    script["set_" .. d[1]] = model_setter(d[1])
end

for _, d in ipairs(pid_defs) do
    script["set_" .. d[1]] = pid_setter(d[1])
end

function script.set_heater_power(value)
    if not plant then return end

    if pid_enabled then
        set("heater_power", manual_power)
        status("PID controls the heater. Switch to Manual first.")
        return
    end

    manual_power = plant:write("heater_power", value)
    set("heater_power", manual_power)
    status()
end

function script.set_pid_enabled(enabled)
    if type(enabled) ~= "boolean" then error("PID mode must be boolean") end
    if not pid or not plant or enabled == pid_enabled then return end

    if enabled then
        pid:resume()
    else
        local current = plant:read("heater_power")

        pid:pause()
        manual_power = plant:write("heater_power", current)
        set("heater_power", manual_power)
    end

    pid_enabled = enabled
    set("pid_enabled", enabled)
    status()
end

function script.set_ma_window(value)
    local n = math.floor(value + 0.5)
    if n < 1 then error("MA window must be positive") end

    ma_window = n
    app.set_filter(FILTERED, {
        kind = "moving_average",
        window = ma_window,
    })

    set("ma_window", ma_window)
    status()
end

function script.power_off()
    if not plant then return end

    if pid_enabled then
        pid:pause()
        pid_enabled = false
        set("pid_enabled", false)
    end

    manual_power = plant:write("heater_power", 0)
    set("heater_power", 0)
    status()
end

function script.reset_integral()
    if pid then
        pid:reset_integral()
        status("PID integral reset.")
    end
end

local function build_processing()
    plant:add("temperature", {
        name = RAW, interval = 0.5, color = "#808080",
    })

    app.filter(RAW, {
        name = FILTERED,
        kind = "moving_average",
        window = ma_window,
        color = "#D32F2F",
    })

    plant:add("heater_power", {
        name = POWER, interval = 0.5, color = "#1976D2",
    })

    plant:add("effective_power", {
        name = EFFECTIVE, interval = 0.5, color = "#F57C00",
    })

    pid = plant:pid("heater_power", {
        name = PID,
        input = FILTERED,
        setpoint = pv.setpoint,
        kp = pv.kp,
        ki = pv.ki,
        kd = pv.kd,
        output_min = pv.output_min,
        output_max = pv.output_max,
        safe_output = 0.0,
    })

    pid:add("setpoint", { name = "pid_setpoint" })
    pid:add("output", { name = "pid_output" })
    pid:add("unconstrained_output", { name = "pid_unconstrained_output" })

    pid:pause()
    manual_power = plant:write("heater_power", manual_power)
end

function script.run()
    app.stop()
    app.clear()
    app.stop_emu()

    plant, pid = nil, nil
    pid_enabled = false

    app.start_emu()
    plant = app.virtual_instrument({ id = 1 })

    for key in pairs(model) do
        model[key] = plant:read(key)
        set(key, model[key])
    end

    manual_power = plant:read("heater_power")

    build_processing()
    app.start()

    set("pid_enabled", false)
    set("heater_power", manual_power)
    status()
end

function script.stop()
    app.stop()
    app.clear()
    app.stop_emu()

    plant, pid = nil, nil
    pid_enabled = false

    set("pid_enabled", false)
    status("Furnace demo stopped.")
end

app.unregister_script(SCRIPT)
script.run()
app.register_script(script)

registered = true
status()
