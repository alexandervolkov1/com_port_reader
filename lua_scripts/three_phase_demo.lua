app.stop()
app.stop_emu()
app.clear()

app.start_emu()

app.add_serial(
    "read phase_a",
    "phase_A"
)

app.add_serial(
    "read phase_b",
    "phase_B"
)

app.add_serial(
    "read phase_c",
    "phase_C"
)

app.start()
