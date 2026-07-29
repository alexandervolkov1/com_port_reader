local amplitude = 100.0
local period = 300.0
local phase = 0.0

function handle(command, time)
    if command == "read sine" then
        local value =
            amplitude
            * math.sin(
                2.0 * math.pi * time / period
                + phase
            )

        return tostring(value)
    end

    local amplitude_text =
        command:match("^set amplitude ([%+%-]?[%d%.]+)$")

    if amplitude_text ~= nil then
        local value = tonumber(amplitude_text)

        if value == nil then
            return "error invalid amplitude"
        end

        amplitude = value
        return "ok"
    end

    local period_text =
        command:match("^set period ([%+%-]?[%d%.]+)$")

    if period_text ~= nil then
        local value = tonumber(period_text)

        if value == nil then
            return "error invalid period"
        end

        if value <= 0.0 then
            return "error period must be positive"
        end

        period = value
        return "ok"
    end

    local phase_text =
        command:match("^set phase ([%+%-]?[%d%.]+)$")

    if phase_text ~= nil then
        local value = tonumber(phase_text)

        if value == nil then
            return "error invalid phase"
        end

        phase = value
        return "ok"
    end

    return "error unknown command: " .. command
end
