local signals = {}

local function is_finite(value)
    return value ~= nil
        and value == value
        and value ~= math.huge
        and value ~= -math.huge
end

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

    if period <= 0.0 then
        return "error period must be positive"
    end

    if not is_finite(phase) then
        return "error invalid phase"
    end

    signals[name] = {
        amplitude = amplitude,
        period = period,
        phase = phase,
    }

    return "ok"
end

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

    return tostring(value)
end

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

function handle(command, time)
    if command:match("^clear%s*$") then
        signals = {}

        return "ok"
    end

    local response = define_signal(command)

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
