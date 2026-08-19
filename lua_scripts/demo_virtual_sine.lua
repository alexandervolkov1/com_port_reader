local SCRIPT_ID = "virtual_sine_demo"
local PANEL_ID = "controls"

local RAW_SERIES_NAME = "virtual_sine"

local POLL_INTERVAL = 0.5

local amplitude = 100.0
local noise_amplitude = 20.0
local period = 300.0
local phase = 0.0

local exponential_enabled = true
local exponential_time_constant = 5.0

local moving_average_enabled = true
local moving_average_window = 10

local median_enabled = true
local median_window = 9

local generator = nil
local panel_registered = false

---@type table<string, any>
local script = {
    id = SCRIPT_ID,

    panels = {
        {
            id = PANEL_ID,
            title = "Noisy sine and filters",

            controls = {
                {
                    kind = "readout",
                    id = "status",
                    label = "Status",
                    initial = "Starting demo.",
                },

                {
                    kind = "number",
                    id = "amplitude",
                    label = "Sine amplitude",
                    initial = amplitude,
                    min = 0.0,
                    max = 1000000.0,
                    step = 1.0,
                    on_change = "set_amplitude",
                },

                {
                    kind = "number",
                    id = "noise_amplitude",
                    label = "Noise amplitude",
                    initial = noise_amplitude,
                    min = 0.0,
                    max = 1000000.0,
                    step = 1.0,
                    on_change = "set_noise_amplitude",
                },

                {
                    kind = "number",
                    id = "period",
                    label = "Period, s",
                    initial = period,
                    min = 0.001,
                    max = 1000000.0,
                    step = 1.0,
                    on_change = "set_period",
                },

                {
                    kind = "number",
                    id = "phase",
                    label = "Phase, rad",
                    initial = phase,
                    min = -1000000.0,
                    max = 1000000.0,
                    step = 0.1,
                    on_change = "set_phase",
                },

                {
                    kind = "toggle",
                    id = "exponential_enabled",
                    label = "Exponential filter",
                    initial = exponential_enabled,
                    on_change =
                        "set_exponential_enabled",
                },

                {
                    kind = "number",
                    id = "exponential_time_constant",
                    label = "EMA time constant, s",
                    initial =
                        exponential_time_constant,
                    min = 0.001,
                    max = 1000000.0,
                    step = 0.5,
                    on_change =
                        "set_exponential_time_constant",
                },

                {
                    kind = "toggle",
                    id = "moving_average_enabled",
                    label = "Moving-average filter",
                    initial =
                        moving_average_enabled,
                    on_change =
                        "set_moving_average_enabled",
                },

                {
                    kind = "number",
                    id = "moving_average_window",
                    label = "Moving-average window",
                    initial =
                        moving_average_window,
                    min = 1.0,
                    max = 100000.0,
                    step = 1.0,
                    on_change =
                        "set_moving_average_window",
                },

                {
                    kind = "toggle",
                    id = "median_enabled",
                    label = "Median filter",
                    initial = median_enabled,
                    on_change =
                        "set_median_enabled",
                },

                {
                    kind = "number",
                    id = "median_window",
                    label = "Median window (odd)",
                    initial = median_window,
                    min = 1.0,
                    max = 99999.0,
                    step = 2.0,
                    on_change =
                        "set_median_window",
                },

                {
                    kind = "button",
                    id = "rebuild",
                    label = "Rebuild series and filters",
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
            name .. " must be a finite number"
        )
    end
end

local function require_non_negative(
    name,
    value
)
    require_finite_number(name, value)

    if value < 0.0 then
        error(
            name .. " must not be negative"
        )
    end
end

local function require_positive(
    name,
    value
)
    require_finite_number(name, value)

    if value <= 0.0 then
        error(
            name .. " must be greater than zero"
        )
    end
end

local function require_window(
    name,
    value
)
    require_positive(name, value)

    if value % 1.0 ~= 0.0 then
        error(
            name .. " must be an integer"
        )
    end

    if value > 100000.0 then
        error(
            name
                .. " must not exceed 100000"
        )
    end

    return math.floor(value)
end

local function require_median_window(value)
    local window =
        require_window(
            "median window",
            value
        )

    if window % 2 == 0 then
        error(
            "median window must be odd"
        )
    end

    return window
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

local function write_generator_parameter(
    parameter,
    value
)
    if generator ~= nil then
        generator:write(
            parameter,
            value
        )
    end
end

function script.set_amplitude(value)
    require_non_negative(
        "sine amplitude",
        value
    )

    amplitude = value

    write_generator_parameter(
        "amplitude",
        amplitude
    )

    set_status(
        "Sine amplitude updated."
    )
end

function script.set_noise_amplitude(value)
    require_non_negative(
        "noise amplitude",
        value
    )

    noise_amplitude = value

    write_generator_parameter(
        "noise_amplitude",
        noise_amplitude
    )

    set_status(
        "Noise amplitude updated."
    )
end

function script.set_period(value)
    require_positive("period", value)

    period = value

    write_generator_parameter(
        "period",
        period
    )

    set_status("Period updated.")
end

function script.set_phase(value)
    require_finite_number("phase", value)

    phase = value

    write_generator_parameter(
        "phase",
        phase
    )

    set_status("Phase updated.")
end

function script.set_exponential_enabled(value)
    exponential_enabled = value

    set_status(
        "Press Rebuild to apply filter changes."
    )
end

function script.set_exponential_time_constant(
    value
)
    require_positive(
        "exponential time constant",
        value
    )

    exponential_time_constant = value

    set_status(
        "Press Rebuild to apply filter changes."
    )
end

function script.set_moving_average_enabled(
    value
)
    moving_average_enabled = value

    set_status(
        "Press Rebuild to apply filter changes."
    )
end

function script.set_moving_average_window(
    value
)
    moving_average_window =
        require_window(
            "moving-average window",
            value
        )

    set_status(
        "Press Rebuild to apply filter changes."
    )
end

function script.set_median_enabled(value)
    median_enabled = value

    set_status(
        "Press Rebuild to apply filter changes."
    )
end

function script.set_median_window(value)
    median_window =
        require_median_window(value)

    set_status(
        "Press Rebuild to apply filter changes."
    )
end

local function configure_generator()
    generator =
        app.virtual_instrument({
            id = 1,
        })

    generator:write(
        "amplitude",
        amplitude
    )

    generator:write(
        "noise_amplitude",
        noise_amplitude
    )

    generator:write(
        "period",
        period
    )

    generator:write(
        "phase",
        phase
    )
end

local function add_raw_series()
    generator:add(
        "value",
        {
            name = RAW_SERIES_NAME,
            interval = POLL_INTERVAL,
            color = "#808080",
        }
    )
end

local function add_filters()
    if exponential_enabled then
        app.filter(
            RAW_SERIES_NAME,
            {
                name = "virtual_sine_exponential",
                kind = "exponential",
                time_constant =
                    exponential_time_constant,
                color = "#1976D2",
            }
        )
    end

    if moving_average_enabled then
        app.filter(
            RAW_SERIES_NAME,
            {
                name =
                    "virtual_sine_moving_average",
                kind = "moving_average",
                window =
                    moving_average_window,
                color = "#388E3C",
            }
        )
    end

    if median_enabled then
        app.filter(
            RAW_SERIES_NAME,
            {
                name = "virtual_sine_median",
                kind = "median",
                window = median_window,
                color = "#F57C00",
            }
        )
    end
end

function script.run()
    app.stop()
    app.stop_emu()
    app.clear()

    generator = nil

    app.start_emu()

    configure_generator()
    add_raw_series()
    add_filters()

    app.start()

    set_status(
        "Demo running. Filter topology rebuilt."
    )
end

function script.stop()
    app.stop()
    app.stop_emu()

    generator = nil

    set_status("Demo stopped.")
end

-- Initialize before registering the panel so that a
-- startup error does not leave a broken panel behind.
script.run()

app.register_script(script)

panel_registered = true

set_status(
    "Demo running. Noise can be changed live."
)
