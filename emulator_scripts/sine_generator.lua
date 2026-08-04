-- Number of independent virtual sine generators.
-- Change this value without recompiling the Rust application.
local GENERATOR_COUNT = 8

local generators = {}

instruments = {}

local function create_parameter_descriptors()
    return {
        {
            key = "value",
            name = "Value",
            type = "number",
            access = "read_only",
            series = true,
        },

        {
            key = "amplitude",
            name = "Amplitude",
            type = "number",
            access = "read_write",
            series = false,
            min = 0.0,
            max = 1000000.0,
        },

        {
            key = "period",
            name = "Period",
            type = "number",
            access = "read_write",
            series = false,
            unit = "s",
            min = 0.001,
            max = 1000000.0,
        },

        {
            key = "phase",
            name = "Phase",
            type = "number",
            access = "read_write",
            series = false,
            unit = "rad",
        },
    }
end

for instrument_id = 1, GENERATOR_COUNT do
    generators[instrument_id] = {
        amplitude = 1.0,
        period = 300.0,
        phase = 0.0,
    }

    instruments[instrument_id] = {
        name = "Sine generator " .. instrument_id,
        parameters = create_parameter_descriptors(),
    }
end

local function get_generator(instrument_id)
    local generator = generators[instrument_id]

    if generator == nil then
        error(
            "unknown sine generator: "
                .. tostring(instrument_id)
        )
    end

    return generator
end

function read(instrument_id, parameter, time)
    local generator = get_generator(instrument_id)

    if parameter == "value" then
        local angular_frequency =
            2.0 * math.pi / generator.period

        return generator.amplitude
            * math.sin(
                angular_frequency * time
                    + generator.phase
            )
    end

    if parameter == "amplitude" then
        return generator.amplitude
    end

    if parameter == "period" then
        return generator.period
    end

    if parameter == "phase" then
        return generator.phase
    end

    error(
        "unknown sine parameter: "
            .. tostring(parameter)
    )
end

function write(
    instrument_id,
    parameter,
    value,
    _time
)
    local generator = get_generator(instrument_id)

    if parameter == "amplitude" then
        generator.amplitude = value
        return generator.amplitude
    end

    if parameter == "period" then
        if value <= 0.0 then
            error(
                "sine period must be greater than zero"
            )
        end

        generator.period = value
        return generator.period
    end

    if parameter == "phase" then
        generator.phase = value
        return generator.phase
    end

    error(
        "sine parameter '"
            .. tostring(parameter)
            .. "' is not writable"
    )
end
