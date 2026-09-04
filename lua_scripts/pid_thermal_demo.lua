local SCRIPT_ID, PANEL_ID = "pid_thermal_demo", "controls"
local PLANT_ID = 1

local RAW = "temperature"
local FILTERED = "temperature_ma"
local POWER = "heater_power"
local PID = "pid"

local PID_P = "pid_p"
local PID_I = "pid_i"
local PID_D = "pid_d"

local POLL_INTERVAL = 0.5
local OUTPUT_MIN, OUTPUT_MAX = 0.0, 100.0

local setpoint = 150.0
local kp, ki, kd = 1.0, 1.0 / 30.0, 0.0

local moving_average_enabled = true
local moving_average_window = 5

local plant, controller = nil, nil
local panel_registered = false

local script = {
    id = SCRIPT_ID,
    panels = {{
        id = PANEL_ID,
        title = "Thermal PID demo",
        controls = {
            { kind = "readout", id = "status", label = "Status",
              initial = "Starting thermal PID demo." },

            { kind = "number", id = "setpoint", label = "Setpoint, °C",
              initial = setpoint, min = 20.0, max = 220.0, step = 1.0,
              on_change = "set_setpoint" },

            { kind = "number", id = "kp", label = "Kp",
              initial = kp, min = 0.0, max = 10.0, step = 0.1,
              on_change = "set_kp" },

            { kind = "number", id = "ki", label = "Ki, 1/s",
              initial = ki, min = 0.0, max = 1.0, step = 0.005,
              on_change = "set_ki" },

            { kind = "number", id = "kd", label = "Kd, s",
              initial = kd, min = 0.0, max = 30.0, step = 0.1,
              on_change = "set_kd" },

            { kind = "toggle", id = "moving_average_enabled",
              label = "Moving average", initial = moving_average_enabled,
              on_change = "set_moving_average_enabled" },

            { kind = "number", id = "moving_average_window",
              label = "MA window", initial = moving_average_window,
              min = 1.0, max = 1000.0, step = 1.0,
              on_change = "set_moving_average_window" },

            { kind = "button", id = "pause_pid", label = "Pause PID",
              on_click = "pause_pid" },

            { kind = "button", id = "resume_pid", label = "Resume PID",
              on_click = "resume_pid" },

            { kind = "button", id = "reset_integral", label = "Reset integral",
              on_click = "reset_integral" },

            { kind = "button", id = "restart", label = "Restart demo",
              on_click = "run" },

            { kind = "button", id = "stop", label = "Stop demo",
              on_click = "stop" },
        },
    }},
}

local function finite(name, value)
    if type(value) ~= "number" or value ~= value
        or value == math.huge or value == -math.huge
    then
        error(name .. " must be a finite number")
    end
end

local function non_negative(name, value)
    finite(name, value)
    if value < 0.0 then error(name .. " must be non-negative") end
    return value
end

local function window(value)
    finite("moving-average window", value)
    if value < 1.0 or value % 1.0 ~= 0.0 then
        error("moving-average window must be a positive integer")
    end
    return math.floor(value)
end

local function set_status(message)
    if panel_registered then
        app.set_control(SCRIPT_ID, PANEL_ID, "status", message)
    end
end

local function show_status()
    local state = "stopped"

    if controller ~= nil then
        state = controller:state()
    end

    set_status(string.format(
        "PID: %s; SP=%.1f °C, Kp=%.3g, Ki=%.3g, Kd=%.3g; MA=%s, window=%d",
        state,
        setpoint, kp, ki, kd,
        moving_average_enabled and "on" or "off",
        moving_average_window
    ))
end

local function add_diagnostics()
    local diagnostics = {
        { "proportional", PID_P, "#F57C00" },
        { "integral",     PID_I, "#7B1FA2" },
        { "derivative",   PID_D, "#388E3C" },
    }

    for _, d in ipairs(diagnostics) do
        controller:add(d[1], { name = d[2], color = d[3] })
    end
end

local function add_moving_average_filter()
    app.filter(RAW, {
        name = FILTERED,
        kind = "moving_average",
        window = moving_average_window,
        color = "#00897B",
    })
end

local function build_processing()
    app.clear()

    plant:add("temperature", {
        name = RAW,
        interval = POLL_INTERVAL,
        color = "#D32F2F",
    })

    plant:add("heater_power", {
        name = POWER,
        interval = POLL_INTERVAL,
        color = "#1976D2",
    })

    local input = RAW

    if moving_average_enabled then
        add_moving_average_filter()
        input = FILTERED
    end

    controller = plant:pid("heater_power", {
        name = PID,
        input = input,
        setpoint = setpoint,
        kp = kp,
        ki = ki,
        kd = kd,
        output_min = OUTPUT_MIN,
        output_max = OUTPUT_MAX,
        safe_output = 0.0,
    })

    add_diagnostics()
end

local function set_gain(key, name, value)
    value = non_negative(name, value)
    if controller ~= nil then controller:configure({ [key] = value }) end
    return value
end

function script.set_setpoint(value)
    finite("PID setpoint", value)
    if value < 20.0 or value > 220.0 then
        error("PID setpoint must be between 20 and 220 °C")
    end

    setpoint = value
    if controller ~= nil then setpoint = controller:write("setpoint", value) end
    show_status()
end

function script.set_kp(value)
    kp = set_gain("kp", "Kp", value)
    show_status()
end

function script.set_ki(value)
    ki = set_gain("ki", "Ki", value)
    show_status()
end

function script.set_kd(value)
    kd = set_gain("kd", "Kd", value)
    show_status()
end

function script.set_moving_average_enabled(value)
    if type(value) ~= "boolean" then
        error("moving-average enabled must be boolean")
    end

    if value == moving_average_enabled then
        return
    end

    if plant == nil or controller == nil then
        moving_average_enabled = value
        show_status()
        return
    end

    if value then
        add_moving_average_filter()
        controller:set_input(FILTERED)
    else
        controller:set_input(RAW)
        app.delete(FILTERED)
    end

    moving_average_enabled = value
    show_status()
end

function script.set_moving_average_window(value)
    moving_average_window = window(value)

    if plant ~= nil and moving_average_enabled then
        app.set_filter(FILTERED, {
            kind = "moving_average",
            window = moving_average_window,
        })
    end

    show_status()
end

function script.pause_pid()
    if controller == nil then
        set_status("PID is not running.")
        return
    end

    controller:pause()
    show_status()
end

function script.resume_pid()
    if controller == nil then
        set_status("PID is not running.")
        return
    end

    controller:resume()
    show_status()
end

function script.reset_integral()
    if controller == nil then
        set_status("PID is not running.")
        return
    end

    controller:reset_integral()
    show_status()
end

function script.run()
    app.stop()
    app.stop_emu()

    plant, controller = nil, nil

    app.start_emu()
    plant = app.virtual_instrument({ id = PLANT_ID })

    build_processing()
    app.start()

    show_status()
end

function script.stop()
    app.stop()
    app.stop_emu()
    plant, controller = nil, nil
    set_status("Thermal PID demo stopped.")
end

app.unregister_script(SCRIPT_ID)

script.run()

app.register_script(script)
panel_registered = true
show_status()
