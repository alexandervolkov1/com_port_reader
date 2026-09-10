local ID = 1
local K0 = 273.15
local RAD_REF = (1000 + K0)^4 - (20 + K0)^4
local RNG_MOD, RNG_MUL = 2147483647, 48271

local s = {
    temperature = 20.0,
    heater_power = 0.0,
    effective_power = 0.0,
    ambient_temperature = 20.0,

    max_power = 2500.0,
    heater_lag = 90.0,
    thermal_capacity = 12000.0,
    linear_loss = 0.35,
    radiation_loss_1000c = 1200.0,

    noise_amplitude = 0.2,
    last_time = nil,
    rng = 104729,
}

local function p(key, name, access, series, unit, min, max)
    return {
        key = key, name = name, type = "number",
        access = access, series = series,
        unit = unit, min = min, max = max,
    }
end

instruments = {
    [ID] = {
        name = "Virtual furnace",
        parameters = {
            p("temperature", "Temperature", "read_only", true, "°C", -100, 2000),
            p("heater_power", "Heater power", "read_write", true, "%", 0, 100),
            p("effective_power", "Effective heating power", "read_only", true, "W", 0, 50000),

            p("ambient_temperature", "Ambient temperature", "read_write", false, "°C", -50, 100),
            p("max_power", "Maximum heater power", "read_write", false, "W", 1, 50000),
            p("heater_lag", "Heater lag", "read_write", false, "s", 0.1, 10000),
            p("thermal_capacity", "Effective heat capacity", "read_write", false, "J/K", 100, 1e7),
            p("linear_loss", "Linear heat loss", "read_write", false, "W/K", 0, 100),
            p("radiation_loss_1000c", "Radiation loss at 1000 °C",
              "read_write", false, "W", 0, 50000),
            p("noise_amplitude", "Measurement noise", "read_write", false, "°C", 0, 50),
        },
    },
}

local limits = {
    heater_power = { 0, 100 },
    ambient_temperature = { -50, 100 },
    max_power = { 1, 50000 },
    heater_lag = { 0.1, 10000 },
    thermal_capacity = { 100, 1e7 },
    linear_loss = { 0, 100 },
    radiation_loss_1000c = { 0, 50000 },
    noise_amplitude = { 0, 50 },
}

local function check_id(id)
    if id ~= ID then error("unknown furnace: " .. tostring(id)) end
end

local function finite(x)
    return type(x) == "number"
        and x == x
        and x ~= math.huge
        and x ~= -math.huge
end

local function checked(key, value)
    local r = limits[key]
    if not r then error(("parameter '%s' is not writable"):format(key)) end
    if not finite(value) or value < r[1] or value > r[2] then
        error(("%s must be between %g and %g"):format(key, r[1], r[2]))
    end
    return value
end

local function step(dt)
    local target = s.max_power * s.heater_power / 100
    local old_power = s.effective_power

    s.effective_power =
        target + (old_power - target) * math.exp(-dt / s.heater_lag)

    local tk = s.temperature + K0
    local ak = s.ambient_temperature + K0

    local loss =
        s.linear_loss * (s.temperature - s.ambient_temperature)
        + s.radiation_loss_1000c * (tk^4 - ak^4) / RAD_REF

    local heating = 0.5 * (old_power + s.effective_power)
    s.temperature = s.temperature
        + dt * (heating - loss) / s.thermal_capacity
end

local function update(time)
    if not finite(time) then error("time must be finite") end

    if not s.last_time then
        s.last_time = time
        return
    end

    local dt = time - s.last_time
    if dt <= 0 then return end

    while dt > 0 do
        local h = math.min(dt, 1.0)
        step(h)
        dt = dt - h
    end

    s.last_time = time
end

local function noise()
    if s.noise_amplitude == 0 then return 0 end

    s.rng = (s.rng * RNG_MUL) % RNG_MOD
    return s.noise_amplitude * (2 * s.rng / RNG_MOD - 1)
end

function read(id, key, time)
    check_id(id)
    update(time)

    if key == "temperature" then
        return s.temperature + noise()
    end

    local value = s[key]
    if value == nil then
        error("unknown furnace parameter: " .. tostring(key))
    end

    return value
end

function write(id, key, value, time)
    check_id(id)
    update(time)

    s[key] = checked(key, value)
    return s[key]
end
