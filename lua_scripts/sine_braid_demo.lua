local braid = {
    {
        name = "braid_1",
        amplitude = 100.0,
        period = 300.0,
        phase = 0.000000,
    },
    {
        name = "braid_2",
        amplitude = 90.0,
        period = 300.0,
        phase = 0.785398,
    },
    {
        name = "braid_3",
        amplitude = 80.0,
        period = 300.0,
        phase = 1.570796,
    },
    {
        name = "braid_4",
        amplitude = 70.0,
        period = 300.0,
        phase = 2.356194,
    },
    {
        name = "braid_5",
        amplitude = 70.0,
        period = 300.0,
        phase = 3.141593,
    },
    {
        name = "braid_6",
        amplitude = 80.0,
        period = 300.0,
        phase = 3.926991,
    },
    {
        name = "braid_7",
        amplitude = 90.0,
        period = 300.0,
        phase = 4.712389,
    },
    {
        name = "braid_8",
        amplitude = 100.0,
        period = 300.0,
        phase = 5.497787,
    },
}

local function define_sine(signal)
    local command = string.format(
        "define %s %.17g %.17g %.17g",
        signal.name,
        signal.amplitude,
        signal.period,
        signal.phase
    )

    app.send_serial(command)

    app.add_serial(
        "read " .. signal.name,
        signal.name
    )
end

app.stop()
app.stop_emu()
app.clear()

app.start_emu()
app.send_serial("clear")

for _, signal in ipairs(braid) do
    define_sine(signal)
end

app.start()
