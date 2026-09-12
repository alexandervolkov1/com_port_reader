pub(super) const STARTUP_EXAMPLE: &str = r#"local definition = {
    application = {
        fps = 20,
        poll_interval = 1.0,
        plot_window = 3600.0,
        max_plot_points_per_series = 1000,
    },

    connections = {
        primary = {
            port = "COM3",
            baud_rate = 9600,
            data_bits = 8,
            parity = "none",
            stop_bits = 1,
            flow_control = "none",
            timeout = 0.25,
        },
    },

    emulator = {
        connection = "primary",
        port = "COM4",
        script = "emulator_scripts/sine_generator.lua",
    },

    plot_panes = {
        { id = "temperature", title = "Temperature", weight = 2.0 },
        { id = "control", title = "Control" },
    },

    scripts = {
        "lua_scripts/experiment.lua",
    },
}

function definition.setup()
    app.log("Application initialized.")
end

return definition"#;

pub(super) const SERIAL_SERIES_EXAMPLE: &str = r##"app.add_serial(
    "read temperature",
    {
        name = "temperature",
        connection = "primary",
        interval = 0.5,
        color = "#1976D2",
        visible = true,
        pane = "temperature",
    }
)"##;

pub(super) const FILTER_EXAMPLE: &str = r##"app.filter(
    "temperature",
    {
        name = "temperature_filtered",
        kind = "moving_average",
        window = 5,
        color = "#7B1FA2",
        pane = "temperature",
    }
)

app.set_filter(
    "temperature_filtered",
    {
        kind = "exponential",
        time_constant = 3.0,
    }
)"##;

pub(super) const PID_EXAMPLE: &str = r#"heater = controller:pid(
    "output_power",
    {
        name = "heater_pid",
        input = "temperature_filtered",
        setpoint = 150.0,
        kp = 2.0,
        ki = 0.1,
        kd = 0.0,
        output_min = 0.0,
        output_max = 100.0,
        safe_output = 0.0,
    }
)

heater:add("setpoint")
heater:add("output", "heater_output")
heater:add("unconstrained_output")"#;

pub(super) const ON_OFF_EXAMPLE: &str = r#"thermostat = controller:on_off(
    "output_power",
    {
        name = "heater_on_off",
        input = "temperature",
        setpoint = 150.0,
        hysteresis = 2.0,
        output_off = 0.0,
        output_on = 100.0,
        safe_output = 0.0,
    }
)"#;

pub(super) const METAKON_EXAMPLE: &str = r##"controller = app.metakon({
    connection = "primary",
    device = 15,
    channel = 0,
    scale = 1.0,
})

controller:add(
    "measurement",
    {
        name = "temperature",
        interval = 1.0,
        color = "#D32F2F",
    }
)

controller:add("setpoint", "setpoint")
controller:add("output_power", "power")

app.start()"##;

pub(super) const METAKON_REPL_EXAMPLE: &str = r#"controller:read("measurement")
controller:write("setpoint", 150)
controller:write("proportional_band", 20)"#;

pub(super) const VIRTUAL_INSTRUMENT_EXAMPLE: &str = r##"app.start_emu()

generator = app.virtual_instrument({
    connection = "primary",
    id = 1,
})

generator:write("amplitude", 100.0)
generator:write("period", 300.0)
generator:write("phase", 0.0)

generator:add(
    "value",
    {
        name = "virtual_sine",
        interval = 0.25,
        color = "#35B779",
    }
)

app.start()"##;

pub(super) const VIRTUAL_MODEL_EXAMPLE: &str = r#"local amplitude = 1.0

instruments = {
    {
        name = "Generator",

        parameters = {
            {
                key = "value",
                name = "Signal value",
                type = "number",
                access = "read_only",
                series = true,
                unit = "V",
                min = -1000.0,
                max = 1000.0,
            },

            {
                key = "amplitude",
                name = "Amplitude",
                type = "number",
                access = "read_write",
                min = 0.0,
                max = 1000.0,
            },
        },
    },
}

function read(
    instrument_id,
    parameter,
    time
)
    if parameter == "value" then
        return amplitude * math.sin(time)
    end

    if parameter == "amplitude" then
        return amplitude
    end

    error("unknown parameter: " .. parameter)
end

function write(
    instrument_id,
    parameter,
    value,
    time
)
    if parameter == "amplitude" then
        amplitude = value
        return amplitude
    end

    error("parameter is not writable: " .. parameter)
end"#;

pub(super) const SERIES_COLOR_EXAMPLE: &str = r##"app.set_color("temperature", "#D32F2F")
app.set_color("temperature", nil) -- restore automatic color
app.set_series_pane("heater_output", "control")"##;

pub(super) const SCENARIO_EXAMPLE: &str = r#"local process = app.scenario({ id = "heat_cycle" })

function safe_stop(event)
    app.stop()
    app.log("Scenario cleanup: " .. event.reason)
end

function failed(event)
    app.stop()
    app.log("Scenario failed: " .. event.error)
end

function enter_heating(event)
    app.start()
    app.add_serial("heater", { name = "heater_command", interval = 1.0 })
end

function enter_holding(event)
    process:complete("Target temperature reached")
end

process:on_stop("safe_stop")
process:on_error("failed")
process:stage("heating", {
    enter = "enter_heating",
    transitions = {{
        when = {
            series = "temperature",
            above = 150.0,
            for_seconds = 5.0,
            hysteresis = 2.0,
        },
        next = "holding",
    }, {
        after = 3600.0,
        next = "holding",
        reason = "Heating timeout",
    }},
})
process:stage("holding", { enter = "enter_holding" })
process:start("heating")

-- stop() runs cleanup; cancel() immediately discards pending work."#;

pub(super) const CONTROL_PANEL_EXAMPLE: &str = r#"local controller = app.metakon({
    connection = "primary",
    device = 15,
    channel = 0,
})

local script = {
    id = "heater_control",

    panels = {
        {
            id = "heater",
            title = "Heater",
            controls = {
                {
                    kind = "readout",
                    id = "temperature",
                    label = "Temperature",
                    initial = "—",
                },
                {
                    kind = "number",
                    id = "setpoint",
                    label = "Setpoint",
                    initial = 20.0,
                    min = 0.0,
                    max = 400.0,
                    step = 1.0,
                    on_change = "set_setpoint",
                },
                {
                    kind = "button",
                    id = "refresh",
                    label = "Refresh",
                    on_click = "refresh",
                },
            },
        },
    },
}

function script.set_setpoint(value)
    local actual = controller:write("setpoint", value)
    app.set_control(script.id, "heater", "setpoint", actual)
end

function script.refresh()
    local value = controller:read("measurement")
    app.set_control(
        script.id,
        "heater",
        "temperature",
        string.format("%.1f °C", value)
    )
end

app.register_script(script)"#;
