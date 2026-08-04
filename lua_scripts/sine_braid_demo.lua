-- Symmetric sine-wave braid.
-- Eight virtual sine generators.
-- Phase step: 45 degrees = pi / 4.

app.stop()
app.stop_rec()
app.stop_emu()
app.clear()

app.start_emu()

local amplitudes = {
    100.0,
    90.0,
    80.0,
    70.0,
    70.0,
    80.0,
    90.0,
    100.0,
}

local period = 300.0
local phase_step = math.pi / 4.0

-- Keep controllers available from the REPL.
braid = {}

for index, amplitude in ipairs(amplitudes) do
    local generator = app.virtual_instrument({
        id = index,
    })

    generator:write(
        "amplitude",
        amplitude
    )

    generator:write(
        "period",
        period
    )

    generator:write(
        "phase",
        (index - 1) * phase_step
    )

    generator:add(
        "value",
        "braid_" .. index
    )

    braid[index] = generator
end

app.start()
