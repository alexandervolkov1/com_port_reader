app.stop()
app.clear()

app.add_metakon({
    device = 15,
    channel = 0,
    register = 0x01,
    value_type = "int",
    scale = 1.0,
    name = "temperature",
})

app.add_metakon({
    device = 15,
    channel = 0,
    register = 0x02,
    value_type = "int",
    scale = 1.0,
    name = "setpoint",
})

app.add_metakon({
    device = 15,
    channel = 0,
    register = 0x06,
    value_type = "byte",
    scale = 1.0,
    name = "power",
})

app.start()
