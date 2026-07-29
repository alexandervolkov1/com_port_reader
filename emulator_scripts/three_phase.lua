local amplitude = 100.0
local period = 300.0

local phase_offsets = {
    phase_a = 0.0,
    phase_b = 2.0 * math.pi / 3.0,
    phase_c = 4.0 * math.pi / 3.0,
}

local function sine_value(
    time,
    phase_offset
)
    return amplitude
        * math.sin(
            2.0 * math.pi * time / period
            + phase_offset
        )
end

local function parameter_value(
    command,
    parameter
)
    local text = command:match(
        "^set "
            .. parameter
            .. " (.+)$"
    )

    if text == nil then
        return nil, false
    end

    return tonumber(text), true
end

function handle(command, time)
    local channel = command:match(
        "^read (phase_[abc])$"
    )

    if channel ~= nil then
        local phase_offset =
            phase_offsets[channel]

        if phase_offset ~= nil then
            return tostring(
                sine_value(
                    time,
                    phase_offset
                )
            )
        end
    end

    local value, matched =
        parameter_value(
            command,
            "amplitude"
        )

    if matched then
        if value == nil then
            return "error invalid amplitude"
        end

        amplitude = value
        return "ok"
    end

    value, matched =
        parameter_value(
            command,
            "period"
        )

    if matched then
        if value == nil then
            return "error invalid period"
        end

        if value <= 0.0 then
            return "error period must be positive"
        end

        period = value
        return "ok"
    end

    return "error unknown command: "
        .. command
end
