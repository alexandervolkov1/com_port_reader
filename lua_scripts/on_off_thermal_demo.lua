local SCRIPT_ID =
    "on_off_thermal_demo"

local PANEL_ID = "controls"
local PLANT_ID = 1

local RAW =
    "on_off_temperature"

local FILTERED =
    "on_off_temperature_ma"

local POWER =
    "on_off_heater_power"

local CONTROLLER =
    "thermostat"

local SETPOINT_DIAGNOSTIC =
    "thermostat_setpoint"

local OUTPUT_DIAGNOSTIC =
    "thermostat_output"

local POLL_INTERVAL = 0.5

local setpoint = 150.0
local hysteresis = 2.0

local output_off = 0.0
local output_on = 100.0

local moving_average_enabled = true
local moving_average_window = 5

local plant = nil
local controller = nil

local panel_registered = false

local script = {
    id = SCRIPT_ID,

    panels = {
        {
            id = PANEL_ID,
            title = "Thermal on/off demo",

            controls = {
                {
                    kind = "readout",
                    id = "status",
                    label = "Status",
                    initial =
                        "Starting thermal on/off demo.",
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
                    id = "hysteresis",
                    label = "Hysteresis, °C",
                    initial = hysteresis,
                    min = 0.0,
                    max = 100.0,
                    step = 0.5,
                    on_change = "set_hysteresis",
                },

                {
                    kind = "number",
                    id = "output_off",
                    label = "Output OFF, %",
                    initial = output_off,
                    min = 0.0,
                    max = 100.0,
                    step = 1.0,
                    on_change = "set_output_off",
                },

                {
                    kind = "number",
                    id = "output_on",
                    label = "Output ON, %",
                    initial = output_on,
                    min = 0.0,
                    max = 100.0,
                    step = 1.0,
                    on_change = "set_output_on",
                },

                {
                    kind = "toggle",
                    id = "moving_average_enabled",
                    label = "Moving average",
                    initial =
                        moving_average_enabled,
                    on_change =
                        "set_moving_average_enabled",
                },

                {
                    kind = "number",
                    id = "moving_average_window",
                    label = "MA window",
                    initial =
                        moving_average_window,
                    min = 1.0,
                    max = 1000.0,
                    step = 1.0,
                    on_change =
                        "set_moving_average_window",
                },

                {
                    kind = "button",
                    id = "pause_controller",
                    label = "Pause controller",
                    on_click = "pause_controller",
                },

                {
                    kind = "button",
                    id = "resume_controller",
                    label = "Resume controller",
                    on_click = "resume_controller",
                },

                {
                    kind = "button",
                    id = "reset_controller",
                    label = "Reset controller",
                    on_click = "reset_controller",
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

local function finite(
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

local function ranged(
    name,
    value,
    minimum,
    maximum
)
    finite(name, value)

    if value < minimum
        or value > maximum
    then
        error(
            string.format(
                "%s must be between %g and %g",
                name,
                minimum,
                maximum
            )
        )
    end

    return value
end

local function window(value)
    finite(
        "moving-average window",
        value
    )

    if value < 1.0
        or value % 1.0 ~= 0.0
    then
        error(
            "moving-average window "
                .. "must be a positive integer"
        )
    end

    return math.floor(value)
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

local function show_status()
    local state = "stopped"

    if controller ~= nil then
        state = controller:state()
    end

    set_status(
        string.format(
            "On/off: %s; "
                .. "SP=%.1f °C, H=%.1f °C; "
                .. "OFF=%.1f%%, ON=%.1f%%; "
                .. "MA=%s, window=%d",
            state,
            setpoint,
            hysteresis,
            output_off,
            output_on,
            moving_average_enabled
                and "on"
                or "off",
            moving_average_window
        )
    )
end

local function add_moving_average_filter()
    app.filter(
        RAW,
        {
            name = FILTERED,
            kind = "moving_average",
            window =
                moving_average_window,
            color = "#00897B",
        }
    )
end

local function add_diagnostics()
    controller:add(
        "setpoint",
        {
            name =
                SETPOINT_DIAGNOSTIC,
            color = "#7B1FA2",
        }
    )

    controller:add(
        "output",
        {
            name =
                OUTPUT_DIAGNOSTIC,
            color = "#F57C00",
        }
    )
end

local function build_processing()
    app.clear()

    plant:add(
        "temperature",
        {
            name = RAW,
            interval = POLL_INTERVAL,
            color = "#D32F2F",
        }
    )

    plant:add(
        "heater_power",
        {
            name = POWER,
            interval = POLL_INTERVAL,
            color = "#1976D2",
        }
    )

    local input = RAW

    if moving_average_enabled then
        add_moving_average_filter()

        input = FILTERED
    end

    controller =
        plant:on_off(
            "heater_power",
            {
                name = CONTROLLER,
                input = input,

                setpoint = setpoint,
                hysteresis =
                    hysteresis,

                output_off =
                    output_off,
                output_on =
                    output_on,
            }
        )

    add_diagnostics()
end

function script.set_setpoint(value)
    setpoint =
        ranged(
            "setpoint",
            value,
            20.0,
            220.0
        )

    if controller ~= nil then
        setpoint =
            controller:write(
                "setpoint",
                setpoint
            )
    end

    show_status()
end

function script.set_hysteresis(value)
    hysteresis =
        ranged(
            "hysteresis",
            value,
            0.0,
            100.0
        )

    if controller ~= nil then
        controller:configure({
            hysteresis =
                hysteresis,
        })
    end

    show_status()
end

function script.set_output_off(value)
    value =
        ranged(
            "output OFF",
            value,
            0.0,
            100.0
        )

    if value > output_on then
        error(
            "output OFF must not "
                .. "exceed output ON"
        )
    end

    output_off = value

    if controller ~= nil then
        controller:configure({
            output_off =
                output_off,
        })
    end

    show_status()
end

function script.set_output_on(value)
    value =
        ranged(
            "output ON",
            value,
            0.0,
            100.0
        )

    if value < output_off then
        error(
            "output ON must not "
                .. "be less than output OFF"
        )
    end

    output_on = value

    if controller ~= nil then
        controller:configure({
            output_on =
                output_on,
        })
    end

    show_status()
end

function script.set_moving_average_enabled(
    value
)
    if type(value) ~= "boolean" then
        error(
            "moving-average enabled "
                .. "must be boolean"
        )
    end

    if value
        == moving_average_enabled
    then
        return
    end

    if plant == nil
        or controller == nil
    then
        moving_average_enabled =
            value

        show_status()

        return
    end

    if value then
        add_moving_average_filter()

        controller:set_input(
            FILTERED
        )
    else
        controller:set_input(
            RAW
        )

        app.delete(
            FILTERED
        )
    end

    moving_average_enabled = value

    show_status()
end

function script.set_moving_average_window(
    value
)
    moving_average_window =
        window(value)

    if plant ~= nil
        and moving_average_enabled
    then
        app.set_filter(
            FILTERED,
            {
                kind =
                    "moving_average",
                window =
                    moving_average_window,
            }
        )
    end

    show_status()
end

function script.pause_controller()
    if controller == nil then
        set_status(
            "Controller is not running."
        )

        return
    end

    controller:pause()

    show_status()
end

function script.resume_controller()
    if controller == nil then
        set_status(
            "Controller is not running."
        )

        return
    end

    controller:resume()

    show_status()
end

function script.reset_controller()
    if controller == nil then
        set_status(
            "Controller is not running."
        )

        return
    end

    controller:reset()

    show_status()
end

function script.run()
    app.stop()
    app.stop_emu()

    plant = nil
    controller = nil

    app.start_emu()

    plant =
        app.virtual_instrument({
            id = PLANT_ID,
        })

    build_processing()

    app.start()

    show_status()
end

function script.stop()
    app.stop()
    app.stop_emu()

    plant = nil
    controller = nil

    set_status(
        "Thermal on/off demo stopped."
    )
end

app.unregister_script(
    SCRIPT_ID
)

script.run()

app.register_script(
    script
)

panel_registered = true

show_status()
