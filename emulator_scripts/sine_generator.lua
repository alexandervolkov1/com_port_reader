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
        { key = "value", name = "Noisy sine value", type = "number",
          access = "read_only", series = true },
        { key = "amplitude", name = "Sine amplitude", type = "number",
          access = "read_write", series = false, min = 0.0, max = 1000000.0 },
        { key = "noise_amplitude", name = "Noise amplitude", type = "number",
          access = "read_write", series = false, min = 0.0, max = 1000000.0 },
        { key = "period", name = "Period", type = "number", access = "read_write",
          series = false, unit = "s", min = 0.001, max = 1000000.0 },
        { key = "phase", name = "Phase", type = "number", access = "read_write",
          series = false, unit = "rad" },
        { key = "transition_seconds", name = "Amplitude / noise transition", type = "number",
          access = "read_write", series = false, unit = "s", min = 0.0, max = 60.0 },
    }
end

for instrument_id = 1, GENERATOR_COUNT do
    local random_state = (instrument_id * 104729) % RANDOM_MODULUS
    if random_state == 0 then random_state = instrument_id end

    generators[instrument_id] = {
        amplitude = 1.0,
        noise_amplitude = 0.0,
        period = 300.0,
        phase = 0.0,
        phase_origin = 0.0,
        time_origin = 0.0,
        transition_seconds = 0.0,
        ramps = {},
        started = false,
        random_state = random_state,
    }
    instruments[instrument_id] = {
        name = "Sine generator " .. instrument_id,
        parameters = create_parameter_descriptors(),
    }
end

local function get_generator(instrument_id)
    local generator = generators[instrument_id]
    if not generator then error("unknown sine generator: " .. tostring(instrument_id)) end
    return generator
end

local function next_uniform(generator)
    generator.random_state = (generator.random_state * RANDOM_MULTIPLIER) % RANDOM_MODULUS
    return generator.random_state / RANDOM_MODULUS
end

local function effective(generator, key, time)
    local ramp = generator.ramps[key]
    if not ramp then return generator[key] end
    local fraction = math.max(0.0, math.min(1.0, (time - ramp.time) / ramp.duration))
    -- Smoothstep joins both endpoints with zero envelope slope.
    local blend = fraction * fraction * (3.0 - 2.0 * fraction)
    return ramp.from + (generator[key] - ramp.from) * blend
end

local function angle(generator, time)
    local angular_frequency = 2.0 * math.pi / generator.period
    return generator.phase_origin + angular_frequency * (time - generator.time_origin)
end

local function noise_value(generator, time)
    local amplitude = effective(generator, "noise_amplitude", time)
    if amplitude == 0.0 then return 0.0 end

    -- Uniform noise in [-noise_amplitude, +noise_amplitude].
    return amplitude * (2.0 * next_uniform(generator) - 1.0)
end

function read(instrument_id, parameter, time)
    local generator = get_generator(instrument_id)

    if parameter == "value" then
        generator.started = true
        return effective(generator, "amplitude", time) * math.sin(angle(generator, time) + generator.phase)
            + noise_value(generator, time)
    elseif parameter == "amplitude" then
        return generator.amplitude
    elseif parameter == "noise_amplitude" then
        return generator.noise_amplitude
    elseif parameter == "period" then
        return generator.period
    elseif parameter == "phase" then
        return generator.phase
    elseif parameter == "transition_seconds" then
        return generator.transition_seconds
    end

    error("unknown sine parameter: " .. tostring(parameter))
end

function write(instrument_id, parameter, value, time)
    local generator = get_generator(instrument_id)
    if type(value) ~= "number" or value ~= value or math.abs(value) == math.huge then
        error("sine parameter must be a finite number")
    end
    if parameter == "amplitude" or parameter == "noise_amplitude" then
        if value < 0.0 or value > 1000000 then error("amplitude must be between 0 and 1000000") end
        local current = effective(generator, parameter, time)
        generator.ramps[parameter] = generator.started and generator.transition_seconds > 0.0
            and { from = current, time = time, duration = generator.transition_seconds } or nil
        generator[parameter] = value
        return value
    elseif parameter == "period" then
        if value < 0.001 or value > 1000000 then error("period must be between 0.001 and 1000000") end
        -- Preserve the current angle, including after repeated period changes.
        -- Before the first sample, keep the shared time origin for braid setup.
        if generator.started then
            generator.phase_origin = angle(generator, time) % (2.0 * math.pi)
            generator.time_origin = time
        end
        generator.period = value
        return generator.period
    elseif parameter == "phase" then
        generator.phase = value
        return generator.phase
    elseif parameter == "transition_seconds" then
        if value < 0.0 or value > 60.0 then error("transition must be between 0 and 60 seconds") end
        generator.transition_seconds = value
        return value
    end

    error("sine parameter '" .. tostring(parameter) .. "' is not writable")
end
