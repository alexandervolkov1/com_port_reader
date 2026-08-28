local ID = 1
local NOISE_AMPLITUDE = 0.4 -- measurement noise, °C

local RNG_MOD, RNG_MUL = 2147483647, 48271

local state = {
    temperature = 20.0,
    heater_power = 0.0,
    ambient_temperature = 20.0,
    time_constant = 30.0,
    heater_gain = 2.0,
    last_time = nil,
    rng = 104729,
}

instruments = {
    [ID] = {
        name = "Virtual thermal plant",
        parameters = {
            { key = "temperature", name = "Temperature", type = "number",
              access = "read_only", series = true, unit = "°C",
              min = -273.15, max = 2000.0 },

            { key = "heater_power", name = "Heater power", type = "number",
              access = "read_write", series = true, unit = "%",
              min = 0.0, max = 100.0 },

            { key = "ambient_temperature", name = "Ambient temperature", type = "number",
              access = "read_write", series = false, unit = "°C",
              min = -100.0, max = 1000.0 },

            { key = "time_constant", name = "Thermal time constant", type = "number",
              access = "read_write", series = false, unit = "s",
              min = 0.1, max = 100000.0 },

            { key = "heater_gain", name = "Heater gain", type = "number",
              access = "read_write", series = false, unit = "°C/%",
              min = 0.0, max = 1000.0 },
        },
    },
}

local function get_plant(id)
    if id ~= ID then error("unknown thermal plant: " .. tostring(id)) end
    return state
end

local function finite(name, value)
    if type(value) ~= "number" or value ~= value
        or value == math.huge or value == -math.huge
    then
        error(name .. " must be a finite number")
    end
end

local function ranged(name, value, min, max)
    finite(name, value)
    if value < min or value > max then
        error(string.format("%s must be between %g and %g", name, min, max))
    end
    return value
end

local function update(p, time)
    finite("time", time)

    if p.last_time == nil then
        p.last_time = time
        return
    end

    local dt = time - p.last_time
    if dt <= 0.0 then return end

    local equilibrium = p.ambient_temperature + p.heater_gain * p.heater_power
    local decay = math.exp(-dt / p.time_constant)

    p.temperature = equilibrium + (p.temperature - equilibrium) * decay
    p.last_time = time
end

local function measurement_noise(p)
    p.rng = (p.rng * RNG_MUL) % RNG_MOD
    return NOISE_AMPLITUDE * (2.0 * p.rng / RNG_MOD - 1.0)
end

function read(id, parameter, time)
    local p = get_plant(id)
    update(p, time)

    if parameter == "temperature" then
        return p.temperature + measurement_noise(p)
    elseif parameter == "heater_power" then
        return p.heater_power
    elseif parameter == "ambient_temperature" then
        return p.ambient_temperature
    elseif parameter == "time_constant" then
        return p.time_constant
    elseif parameter == "heater_gain" then
        return p.heater_gain
    end

    error("unknown thermal plant parameter: " .. tostring(parameter))
end

function write(id, parameter, value, time)
    local p = get_plant(id)
    update(p, time)

    if parameter == "heater_power" then
        p.heater_power = ranged(parameter, value, 0.0, 100.0)
    elseif parameter == "ambient_temperature" then
        p.ambient_temperature = ranged(parameter, value, -100.0, 1000.0)
    elseif parameter == "time_constant" then
        p.time_constant = ranged(parameter, value, 0.1, 100000.0)
    elseif parameter == "heater_gain" then
        p.heater_gain = ranged(parameter, value, 0.0, 1000.0)
    else
        error("thermal plant parameter '" .. tostring(parameter) .. "' is not writable")
    end

    return p[parameter]
end
