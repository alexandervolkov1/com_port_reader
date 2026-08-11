local definition = {
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
}

function definition.setup()
    app.log(
        "Application initialized from startup.lua."
    )
end

return definition
