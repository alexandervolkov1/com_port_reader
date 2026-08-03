---@class SineSignal
---@field amplitude number
---@field period number
---@field phase number

---@type table<string, SineSignal>
local signals = {}

---@param value number?
---@return boolean
local function is_finite(value)
    return value ~= nil
        and value == value
        and value ~= math.huge
        and value ~= -math.huge
end

---@param value number
---@return string
local function format_number(value)
    return string.format("%.17g", value)
end

---@param command string
---@return string?
local function define_signal(command)
    local name,
        amplitude_text,
        period_text,
        phase_text =
        command:match(
            "^define%s+"
                .. "(%S+)%s+"
                .. "(%S+)%s+"
                .. "(%S+)%s+"
                .. "(%S+)%s*$"
        )

    if name == nil then
        return nil
    end

    local amplitude = tonumber(amplitude_text)
    local period = tonumber(period_text)
    local phase = tonumber(phase_text)

    if not is_finite(amplitude) then
        return "error invalid amplitude"
    end

    if not is_finite(period) then
        return "error invalid period"
    end

    if not is_finite(phase) then
        return "error invalid phase"
    end

    ---@cast amplitude number
    ---@cast period number
    ---@cast phase number

    if period <= 0.0 then
        return "error period must be positive"
    end

    signals[name] = {
        amplitude = amplitude,
        period = period,
        phase = phase,
    }

    return "ok"
end

---@param signal SineSignal
---@param parameter string
---@return number?
local function get_parameter(
    signal,
    parameter
)
    if parameter == "amplitude" then
        return signal.amplitude
    end

    if parameter == "period" then
        return signal.period
    end

    if parameter == "phase" then
        return signal.phase
    end

    return nil
end

---@param signal SineSignal
---@param parameter string
---@param value number
---@return string
local function set_parameter(
    signal,
    parameter,
    value
)
    if parameter == "amplitude" then
        signal.amplitude = value

        return "ok"
    end

    if parameter == "period" then
        if value <= 0.0 then
            return "error period must be positive"
        end

        signal.period = value

        return "ok"
    end

    if parameter == "phase" then
        signal.phase = value

        return "ok"
    end

    return "error unknown parameter: "
        .. parameter
end

---@param command string
---@return string?
local function get_signal_parameter(command)
    local name, parameter =
        command:match(
            "^get%s+(%S+)%s+(%S+)%s*$"
        )

    if name == nil then
        return nil
    end

    local signal = signals[name]

    if signal == nil then
        return "error unknown signal: " .. name
    end

    local value =
        get_parameter(signal, parameter)

    if value == nil then
        return "error unknown parameter: "
            .. parameter
    end

    return format_number(value)
end

---@param command string
---@return string?
local function set_signal_parameter(command)
    local name,
        parameter,
        value_text =
        command:match(
            "^set%s+"
                .. "(%S+)%s+"
                .. "(%S+)%s+"
                .. "(%S+)%s*$"
        )

    if name == nil then
        return nil
    end

    local signal = signals[name]

    if signal == nil then
        return "error unknown signal: " .. name
    end

    local value = tonumber(value_text)

    if not is_finite(value) then
        return "error invalid value"
    end

    ---@cast value number

    return set_parameter(
        signal,
        parameter,
        value
    )
end

---@param command string
---@param time number
---@return string?
local function read_signal(command, time)
    local name = command:match(
        "^read%s+(%S+)%s*$"
    )

    if name == nil then
        return nil
    end

    local signal = signals[name]

    if signal == nil then
        return "error unknown signal: " .. name
    end

    local value =
        signal.amplitude
        * math.sin(
            2.0
                * math.pi
                * time
                / signal.period
                + signal.phase
        )

    return format_number(value)
end

---@param command string
---@return string?
local function delete_signal(command)
    local name = command:match(
        "^delete%s+(%S+)%s*$"
    )

    if name == nil then
        return nil
    end

    if signals[name] == nil then
        return "error unknown signal: " .. name
    end

    signals[name] = nil

    return "ok"
end

---@param command string
---@param time number
---@return string
function handle(command, time)
    if command:match("^clear%s*$") then
        signals = {}

        return "ok"
    end

    local response = define_signal(command)

    if response ~= nil then
        return response
    end

    response = get_signal_parameter(command)

    if response ~= nil then
        return response
    end

    response = set_signal_parameter(command)

    if response ~= nil then
        return response
    end

    response = read_signal(command, time)

    if response ~= nil then
        return response
    end

    response = delete_signal(command)

    if response ~= nil then
        return response
    end

    return "error unknown command: "
        .. command
end
