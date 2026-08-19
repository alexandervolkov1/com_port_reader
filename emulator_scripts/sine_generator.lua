-- Number of independent virtual sine generators.
local GENERATOR_COUNT = 8

-- Park–Miller deterministic pseudo-random generator.
-- Each virtual instrument has an independent state.
local RANDOM_MODULUS = 2147483647
local RANDOM_MULTIPLIER = 48271

local generators = {}

instruments = {}

local function create_parameter_descriptors()
    return {
        {
            key = "value",
            name = "Noisy sine value",
            type = "number",
            access = "read_only",
            series = true,
        },

        {
            key = "amplitude",
            name = "Sine amplitude",
            type = "number",
            access = "read_write",
            series = false,
            min = 0.0,
            max = 1000000.0,
        },

        {
            key = "noise_amplitude",
            name = "Noise amplitude",
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
    local random_state =
        (instrument_id * 104729)
        % RANDOM_MODULUS

    if random_state == 0 then
        random_state = instrument_id
    end

    generators[instrument_id] = {
        amplitude = 1.0,
        noise_amplitude = 0.0,
        period = 300.0,
        phase = 0.0,
        random_state = random_state,
    }

    instruments[instrument_id] = {
        name = "Sine generator "
            .. instrument_id,

        parameters =
            create_parameter_descriptors(),
    }
end

local function get_generator(instrument_id)
    local generator =
        generators[instrument_id]

    if generator == nil then
        error(
            "unknown sine generator: "
                .. tostring(instrument_id)
        )
    end

    return generator
end

local function next_uniform(generator)
    generator.random_state =
        (
            generator.random_state
            * RANDOM_MULTIPLIER
        )
        % RANDOM_MODULUS

    return generator.random_state
        / RANDOM_MODULUS
end

local function sine_value(
    generator,
    time
)
    local angular_frequency =
        2.0 * math.pi / generator.period

    return generator.amplitude
        * math.sin(
            angular_frequency * time
                + generator.phase
        )
end

local function noise_value(generator)
    if generator.noise_amplitude == 0.0 then
        return 0.0
    end

    -- Uniform noise in the interval
    -- [-noise_amplitude, +noise_amplitude].
    return generator.noise_amplitude
        * (
            2.0 * next_uniform(generator)
            - 1.0
        )
end

function read(
    instrument_id,
    parameter,
    time
)
    local generator =
        get_generator(instrument_id)

    if parameter == "value" then
        return sine_value(generator, time)
            + noise_value(generator)
    end

    if parameter == "amplitude" then
        return generator.amplitude
    end

    if parameter == "noise_amplitude" then
        return generator.noise_amplitude
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
    local generator =
        get_generator(instrument_id)

    if parameter == "amplitude" then
        if value < 0.0 then
            error(
                "sine amplitude must not be negative"
            )
        end

        generator.amplitude = value
        return generator.amplitude
    end

    if parameter == "noise_amplitude" then
        if value < 0.0 then
            error(
                "noise amplitude must not be negative"
            )
        end

        generator.noise_amplitude = value
        return generator.noise_amplitude
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
