local definition = {
    application = {
        fps = 20,
        poll_interval = 1.0,
        plot_window = 3600.0,
        max_plot_points_per_series = 1000,
    },

    connections = {
        primary = {
            port = "COM10",
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
        port = "COM11",
        script = "emulator_scripts/sine_generator.lua",
    },

    scripts = {
        "lua_scripts/sine_braid_demo.lua",
    },
}

function definition.setup()
    app.log(
        "Application initialized from startup.lua."
    )
end

return definition
