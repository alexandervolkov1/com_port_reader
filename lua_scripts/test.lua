app.stop()
app.clear()

app.start_emu()

app.add_serial(
    "read sine",
    "sine"
)

app.start()
