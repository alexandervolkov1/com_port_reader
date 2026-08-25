local SCRIPT_ID =
    "pid_thermal_demo"

local PANEL_ID =
    "controls"


local PLANT_ID = 1

local TEMPERATURE_SERIES_NAME =
    "thermal_temperature"

local POWER_SERIES_NAME =
    "thermal_heater_power"

local PID_NAME =
    "thermal_temperature_pid"

local POLL_INTERVAL = 0.5


-- Current controller setpoint.
local setpoint = 150.0


-- PI controller parameters.
--
-- The virtual plant has:
--   time constant = 30 s
--   heater gain   = 2 °C / %
--
-- These deliberately conservative values should
-- produce a smooth approach to the setpoint.
local KP = 1.0
local KI = 1.0 / 30.0
local KD = 0.0

local OUTPUT_MIN = 0.0
local OUTPUT_MAX = 100.0


local plant = nil
local panel_registered = false


---@type table<string, any>
local script = {
    id = SCRIPT_ID,

    panels = {
        {
            id = PANEL_ID,

            title =
                "Thermal PID demo",

            controls = {
                {
                    kind = "readout",

                    id = "status",

                    label = "Status",

                    initial =
                        "Starting thermal PID demo.",
                },

                {
                    kind = "number",

                    id = "setpoint",

                    label = "Setpoint, °C",

                    initial = setpoint,

                    min = 20.0,

                    max = 220.0,

                    step = 1.0,

                    on_change =
                        "set_setpoint",
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


local function require_finite_number(
    name,
    value
)
    if type(value) ~= "number"
        or value ~= value
        or value == math.huge
        or value == -math.huge
    then
        error(
            name
                .. " must be a finite number"
        )
    end
end


local function set_status(message)
    if not panel_registered then
        return
    end

    app.set_control(
        SCRIPT_ID,
        PANEL_ID,
        "status",
        message
    )
end


local function configure_plant()
    plant =
        app.virtual_instrument({
            id = PLANT_ID,
        })

    plant:add(
        "temperature",
        {
            name =
                TEMPERATURE_SERIES_NAME,

            interval =
                POLL_INTERVAL,

            color =
                "#D32F2F",
        }
    )

    plant:add(
        "heater_power",
        {
            name =
                POWER_SERIES_NAME,

            interval =
                POLL_INTERVAL,

            color =
                "#1976D2",
        }
    )

    plant:pid(
        "heater_power",
        {
            name =
                PID_NAME,

            input =
                TEMPERATURE_SERIES_NAME,

            setpoint =
                setpoint,

            kp = KP,
            ki = KI,
            kd = KD,

            output_min =
                OUTPUT_MIN,

            output_max =
                OUTPUT_MAX,
        }
    )
end


function script.set_setpoint(value)
    require_finite_number(
        "PID setpoint",
        value
    )

    if value < 20.0
        or value > 220.0
    then
        error(
            "PID setpoint must be "
                .. "between 20 and 220 °C"
        )
    end

    setpoint = value

    app.set_pid_setpoint(
        PID_NAME,
        setpoint
    )

    set_status(
        "PID setpoint updated to "
            .. tostring(setpoint)
            .. " °C."
    )
end


function script.run()
    app.stop()
    app.stop_emu()
    app.clear()

    plant = nil

    app.start_emu()

    configure_plant()

    app.start()

    set_status(
        "Thermal PID demo running. "
            .. "Setpoint = "
            .. tostring(setpoint)
            .. " °C."
    )
end


function script.stop()
    app.stop()
    app.stop_emu()

    plant = nil

    set_status(
        "Thermal PID demo stopped."
    )
end


-- Make manual re-running of this script safe in the
-- persistent Lua runtime.
app.unregister_script(
    SCRIPT_ID
)


-- Configure the application before publishing the panel.
-- A startup failure therefore does not leave a dead panel
-- behind.
script.run()


app.register_script(script)

panel_registered = true


set_status(
    "Thermal PID demo running. "
        .. "Setpoint = "
        .. tostring(setpoint)
        .. " °C."
)
