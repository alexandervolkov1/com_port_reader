-- Stop the previous session.
app.stop()
app.stop_emu()
app.clear()

-- Start the emulator configured in Settings.
app.start_emu()

-- Discover instrument 1 through the virtual COM pair.
generator = app.virtual_instrument({
    id = 1,
})

-- Configure the virtual sine generator.
generator:write(
    "amplitude",
    100.0
)

generator:write(
    "period",
    300.0
)

generator:write(
    "phase",
    0.0
)

-- Add its output to periodic acquisition.
generator:add(
    "value",
    "virtual_sine"
)

app.start()
