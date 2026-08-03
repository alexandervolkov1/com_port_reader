local state = {
    [1] = {
        amplitude = 1.0,
        period = 10.0,
        phase = 0.0,
    },
}

instruments = {
    {
        name = "Sine generator",

        parameters = {
            {
                key = "value",
                name = "Signal value",
                type = "number",
                access = "read_only",
                series = true,
            },

            {
                key = "amplitude",
                name = "Amplitude",
                type = "number",
                access = "read_write",
            },

            {
                key = "period",
                name = "Period",
                type = "number",
                access = "read_write",
                min = 0.001,
                max = 86400.0,
                unit = "s",
            },

            {
                key = "phase",
                name = "Phase",
                type = "number",
                access = "read_write",
                unit = "rad",
            },
        },
    },
}

local function instrument_state(instrument)
    local value = state[instrument]

    if value == nil then
        error(
            "unknown instrument: "
                .. tostring(instrument)
        )
    end

    return value
end

function read(instrument, parameter, time)
    local value =
        instrument_state(instrument)

    if parameter == "value" then
        return value.amplitude
            * math.sin(
                2.0
                    * math.pi
                    * time
                    / value.period
                    + value.phase
            )
    end

    local parameter_value =
        value[parameter]

    if parameter_value == nil then
        error(
            "unknown readable parameter: "
                .. tostring(parameter)
        )
    end

    return parameter_value
end

function write(
    instrument,
    parameter,
    value,
    time
)
    local current =
        instrument_state(instrument)

    if parameter == "value" then
        error("value is read-only")
    end

    if current[parameter] == nil then
        error(
            "unknown writable parameter: "
                .. tostring(parameter)
        )
    end

    current[parameter] = value

    return current[parameter]
end
