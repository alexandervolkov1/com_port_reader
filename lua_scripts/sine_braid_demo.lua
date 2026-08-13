-- Symmetric sine-wave braid.
-- Eight virtual sine generators.
-- Phase step: 45 degrees = pi / 4.

local script = {
    id = "sine_braid",

    panels = {
        {
            id = "controls",
            title = "Sine braid demo",

            controls = {
                {
                    kind = "number",
                    id = "period",
                    label = "Period, s",
                    initial = 300.0,
                    min = 0.1,
                    max = 86400.0,
                    step = 1.0,
                    on_change = "set_period",
                },

                {
                    kind = "number",
                    id = "phase_step",
                    label = "Phase step, rad",
                    initial = math.pi / 4.0,
                    min = 0.0,
                    max = 2.0 * math.pi,
                    step = 0.01,
                    on_change = "set_phase_step",
                },

                {
                    kind = "number",
                    id = "amplitude_1",
                    label = "Amplitude 1",
                    initial = 100.0,
                    min = 0.0,
                    max = 1000000.0,
                    step = 1.0,
                    on_change = "set_amplitude_1",
                },

                {
                    kind = "number",
                    id = "amplitude_2",
                    label = "Amplitude 2",
                    initial = 90.0,
                    min = 0.0,
                    max = 1000000.0,
                    step = 1.0,
                    on_change = "set_amplitude_2",
                },

                {
                    kind = "number",
                    id = "amplitude_3",
                    label = "Amplitude 3",
                    initial = 80.0,
                    min = 0.0,
                    max = 1000000.0,
                    step = 1.0,
                    on_change = "set_amplitude_3",
                },

                {
                    kind = "number",
                    id = "amplitude_4",
                    label = "Amplitude 4",
                    initial = 70.0,
                    min = 0.0,
                    max = 1000000.0,
                    step = 1.0,
                    on_change = "set_amplitude_4",
                },

                {
                    kind = "number",
                    id = "amplitude_5",
                    label = "Amplitude 5",
                    initial = 70.0,
                    min = 0.0,
                    max = 1000000.0,
                    step = 1.0,
                    on_change = "set_amplitude_5",
                },

                {
                    kind = "number",
                    id = "amplitude_6",
                    label = "Amplitude 6",
                    initial = 80.0,
                    min = 0.0,
                    max = 1000000.0,
                    step = 1.0,
                    on_change = "set_amplitude_6",
                },

                {
                    kind = "number",
                    id = "amplitude_7",
                    label = "Amplitude 7",
                    initial = 90.0,
                    min = 0.0,
                    max = 1000000.0,
                    step = 1.0,
                    on_change = "set_amplitude_7",
                },

                {
                    kind = "number",
                    id = "amplitude_8",
                    label = "Amplitude 8",
                    initial = 100.0,
                    min = 0.0,
                    max = 1000000.0,
                    step = 1.0,
                    on_change = "set_amplitude_8",
                },

                {
                    kind = "button",
                    id = "start",
                    label = "Start braid",
                    on_click = "run",
                },

                {
                    kind = "button",
                    id = "stop",
                    label = "Stop braid",
                    on_click = "stop",
                },
            },
        },
    },
}

local amplitudes = {
    100.0,
    90.0,
    80.0,
    70.0,
    70.0,
    80.0,
    90.0,
    100.0,
}

local period = 300.0
local phase_step = math.pi / 4.0
local generators = {}

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

local function set_amplitude(
    index,
    value
)
    require_finite_number(
        "amplitude",
        value
    )

    if value < 0.0 then
        error(
            "amplitude must not be negative"
        )
    end

    amplitudes[index] = value

    local generator = generators[index]

    if generator ~= nil then
        generator:write(
            "amplitude",
            value
        )
    end
end

function script.set_period(value)
    require_finite_number(
        "period",
        value
    )

    if value <= 0.0 then
        error(
            "period must be greater than zero"
        )
    end

    period = value

    for _, generator in ipairs(generators) do
        generator:write(
            "period",
            period
        )
    end
end

function script.set_phase_step(value)
    require_finite_number(
        "phase step",
        value
    )

    phase_step = value

    for index, generator in ipairs(generators) do
        generator:write(
            "phase",
            (index - 1) * phase_step
        )
    end
end

function script.set_amplitude_1(value)
    set_amplitude(1, value)
end

function script.set_amplitude_2(value)
    set_amplitude(2, value)
end

function script.set_amplitude_3(value)
    set_amplitude(3, value)
end

function script.set_amplitude_4(value)
    set_amplitude(4, value)
end

function script.set_amplitude_5(value)
    set_amplitude(5, value)
end

function script.set_amplitude_6(value)
    set_amplitude(6, value)
end

function script.set_amplitude_7(value)
    set_amplitude(7, value)
end

function script.set_amplitude_8(value)
    set_amplitude(8, value)
end

function script.stop()
    app.stop()
    app.stop_emu()

    generators = {}
end

function script.run()
    app.stop()
    app.stop_rec()
    app.stop_emu()
    app.clear()

    generators = {}

    app.start_emu()

    for index, amplitude in ipairs(amplitudes) do
        local generator =
            app.virtual_instrument({
                id = index,
            })

        generator:write(
            "amplitude",
            amplitude
        )

        generator:write(
            "period",
            period
        )

        generator:write(
            "phase",
            (index - 1) * phase_step
        )

        generator:add(
            "value",
            {
                name = "braid_" .. index,
                interval = 1.0,
            }
        )

        generators[index] = generator
    end

    app.start()
end

-- Run first so a failed initialization does not
-- leave a non-working panel in the application.
script.run()

-- Registration makes the panel appear in the GUI
-- and keeps this table alive in the Lua runtime.
app.register_script(script)
