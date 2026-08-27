local SCRIPT_ID = "pid_thermal_demo"
local PANEL_ID = "controls"

local PLANT_ID = 1

local TEMPERATURE_SERIES_NAME = "thermal_temperature"
local POWER_SERIES_NAME = "thermal_heater_power"
local PID_NAME = "thermal_temperature_pid"

local POLL_INTERVAL = 0.5

-- Controller parameters.
local setpoint = 150.0
local kp = 1.0
local ki = 1.0 / 30.0
local kd = 0.0

local OUTPUT_MIN = 0.0
local OUTPUT_MAX = 100.0

local plant = nil
local controller = nil
local panel_registered = false


---@type table<string, any>
local script = {
    id = SCRIPT_ID,

    panels = {
        {
            id = PANEL_ID,
            title = "Thermal PID demo",

            controls = {
                {
                    kind = "readout",
                    id = "status",
                    label = "Status",
                    initial = "Starting thermal PID demo.",
                },

                {
                    kind = "number",
                    id = "setpoint",
                    label = "Setpoint, °C",
                    initial = setpoint,
                    min = 20.0,
                    max = 220.0,
                    step = 1.0,
                    on_change = "set_setpoint",
                },

                {
                    kind = "number",
                    id = "kp",
                    label = "Kp",
                    initial = kp,
                    min = 0.0,
                    max = 10.0,
                    step = 0.1,
                    on_change = "set_kp",
                },

                {
                    kind = "number",
                    id = "ki",
                    label = "Ki, 1/s",
                    initial = ki,
                    min = 0.0,
                    max = 1.0,
                    step = 0.005,
                    on_change = "set_ki",
                },

                {
                    kind = "number",
                    id = "kd",
                    label = "Kd, s",
                    initial = kd,
                    min = 0.0,
                    max = 30.0,
                    step = 0.1,
                    on_change = "set_kd",
                },

                {
                    kind = "button",
                    id = "restart",
                    label = "Restart demo",
                    on_click = "run",
                },

                {
                    kind = "button",
                    id = "stop",
                    label = "Stop demo",
                    on_click = "stop",
                },
            },
        },
    },
}


local function require_finite_number(name, value)
    if type(value) ~= "number"
        or value ~= value
        or value == math.huge
        or value == -math.huge
    then
        error(name .. " must be a finite number")
    end
end


local function require_non_negative(name, value)
    require_finite_number(name, value)

    if value < 0.0 then
        error(name .. " must be non-negative")
    end
end


local function set_status(message)
    if panel_registered then
        app.set_control(SCRIPT_ID, PANEL_ID, "status", message)
    end
end


local function set_pid_status()
    set_status(
        string.format(
            "PID: setpoint = %.1f °C, Kp = %.3g, Ki = %.3g, Kd = %.3g",
            setpoint,
            kp,
            ki,
            kd
        )
    )
end


local function configure_plant()
    plant = app.virtual_instrument({
        id = PLANT_ID,
    })

    plant:add("temperature", {
        name = TEMPERATURE_SERIES_NAME,
        interval = POLL_INTERVAL,
        color = "#D32F2F",
    })

    plant:add("heater_power", {
        name = POWER_SERIES_NAME,
        interval = POLL_INTERVAL,
        color = "#1976D2",
    })

    controller = plant:pid("heater_power", {
        name = PID_NAME,
        input = TEMPERATURE_SERIES_NAME,

        setpoint = setpoint,

        kp = kp,
        ki = ki,
        kd = kd,

        output_min = OUTPUT_MIN,
        output_max = OUTPUT_MAX,
    })
end


function script.set_setpoint(value)
    require_finite_number("PID setpoint", value)

    if value < 20.0 or value > 220.0 then
        error("PID setpoint must be between 20 and 220 °C")
    end

    if controller == nil then
        error("PID controller is not configured")
    end

    setpoint = controller:write("setpoint", value)

    set_pid_status()
end


function script.set_kp(value)
    require_non_negative("Kp", value)

    if controller ~= nil then
        controller:configure({
            kp = value,
        })
    end

    kp = value

    set_pid_status()
end


function script.set_ki(value)
    require_non_negative("Ki", value)

    if controller ~= nil then
        controller:configure({
            ki = value,
        })
    end

    ki = value

    set_pid_status()
end


function script.set_kd(value)
    require_non_negative("Kd", value)

    if controller ~= nil then
        controller:configure({
            kd = value,
        })
    end

    kd = value

    set_pid_status()
end


function script.run()
    app.stop()
    app.stop_emu()
    app.clear()

    plant = nil
    controller = nil

    app.start_emu()

    configure_plant()

    app.start()

    set_status(
        string.format(
            "Thermal PID demo running. Setpoint = %.1f °C, Kp = %.3g, Ki = %.3g, Kd = %.3g",
            setpoint,
            kp,
            ki,
            kd
        )
    )
end


function script.stop()
    app.stop()
    app.stop_emu()

    set_status("Thermal PID demo stopped.")
end


-- Make manual re-running safe in the persistent Lua runtime.
app.unregister_script(SCRIPT_ID)

-- Configure the application before publishing the panel.
-- A startup failure therefore does not leave a dead panel behind.
script.run()

app.register_script(script)
panel_registered = true

set_pid_status()
