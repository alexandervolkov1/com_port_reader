local definition = {
    application = {
        fps = 20,
        poll_interval = 0.5,
        plot_window = 3600.0,
        max_plot_points_per_series = 2000,
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
        script =
            "../emulator_scripts/pid_thermal_plant.lua",
    },

    scripts = {
        "../lua_scripts/pid_thermal_demo.lua",
    },
}

function definition.setup()
    app.log(
        "Thermal PID test profile initialized."
    )
end

return definition
