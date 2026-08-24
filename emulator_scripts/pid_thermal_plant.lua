local INSTRUMENT_ID = 1

local state = {
    temperature = 20.0,
    heater_power = 0.0,

    ambient_temperature = 20.0,

    -- Time constant of the thermal object, seconds.
    time_constant = 30.0,

    -- Steady-state temperature rise above ambient
    -- for one percent of heater power, °C / %.
    heater_gain = 2.0,

    last_time = nil,
}

instruments = {
    [INSTRUMENT_ID] = {
        name = "Virtual thermal plant",

        parameters = {
            {
                key = "temperature",
                name = "Temperature",
                type = "number",
                access = "read_only",
                series = true,
                unit = "°C",
                min = -273.15,
                max = 2000.0,
            },

            {
                key = "heater_power",
                name = "Heater power",
                type = "number",
                access = "read_write",
                series = true,
                unit = "%",
                min = 0.0,
                max = 100.0,
            },

            {
                key = "ambient_temperature",
                name = "Ambient temperature",
                type = "number",
                access = "read_write",
                series = false,
                unit = "°C",
                min = -100.0,
                max = 1000.0,
            },

            {
                key = "time_constant",
                name = "Thermal time constant",
                type = "number",
                access = "read_write",
                series = false,
                unit = "s",
                min = 0.1,
                max = 100000.0,
            },

            {
                key = "heater_gain",
                name = "Heater gain",
                type = "number",
                access = "read_write",
                series = false,
                unit = "°C/%",
                min = 0.0,
                max = 1000.0,
            },
        },
    },
}

local function get_instrument(instrument_id)
    if instrument_id ~= INSTRUMENT_ID then
        error(
            "unknown thermal plant: "
                .. tostring(instrument_id)
        )
    end

    return state
end

local function require_finite(
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

local function update_state(
    plant,
    time
)
    require_finite("time", time)

    if plant.last_time == nil then
        plant.last_time = time
        return
    end

    local dt =
        time - plant.last_time

    if dt <= 0.0 then
        return
    end

    local equilibrium_temperature =
        plant.ambient_temperature
        + plant.heater_gain
            * plant.heater_power

    local decay =
        math.exp(
            -dt / plant.time_constant
        )

    plant.temperature =
        equilibrium_temperature
        + (
            plant.temperature
            - equilibrium_temperature
        ) * decay

    plant.last_time = time
end

function read(
    instrument_id,
    parameter,
    time
)
    local plant =
        get_instrument(instrument_id)

    update_state(
        plant,
        time
    )

    if parameter == "temperature" then
        return plant.temperature
    end

    if parameter == "heater_power" then
        return plant.heater_power
    end

    if parameter == "ambient_temperature" then
        return plant.ambient_temperature
    end

    if parameter == "time_constant" then
        return plant.time_constant
    end

    if parameter == "heater_gain" then
        return plant.heater_gain
    end

    error(
        "unknown thermal plant parameter: "
            .. tostring(parameter)
    )
end

function write(
    instrument_id,
    parameter,
    value,
    time
)
    local plant =
        get_instrument(instrument_id)

    require_finite(
        parameter,
        value
    )

    update_state(
        plant,
        time
    )

    if parameter == "heater_power" then
        if value < 0.0
            or value > 100.0
        then
            error(
                "heater power must be "
                    .. "between 0 and 100"
            )
        end

        plant.heater_power = value

        return plant.heater_power
    end

    if parameter == "ambient_temperature" then
        if value < -100.0
            or value > 1000.0
        then
            error(
                "ambient temperature must be "
                    .. "between -100 and 1000"
            )
        end

        plant.ambient_temperature = value

        return plant.ambient_temperature
    end

    if parameter == "time_constant" then
        if value < 0.1
            or value > 100000.0
        then
            error(
                "time constant must be "
                    .. "between 0.1 and 100000"
            )
        end

        plant.time_constant = value

        return plant.time_constant
    end

    if parameter == "heater_gain" then
        if value < 0.0
            or value > 1000.0
        then
            error(
                "heater gain must be "
                    .. "between 0 and 1000"
            )
        end

        plant.heater_gain = value

        return plant.heater_gain
    end

    error(
        "thermal plant parameter '"
            .. tostring(parameter)
            .. "' is not writable"
    )
end
