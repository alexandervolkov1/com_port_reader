local definition = {
    application = {
        fps = 20,
        poll_interval = 0.5,
        plot_window = 300.0,
        max_plot_points_per_series = 2000,
    },

    emulator = {
        transport = "memory",
        script = "../emulator_scripts/sine_generator.lua",
    },

    scripts = {
        "../lua_scripts/sine_braid_demo.lua",
    },
}

function definition.setup()
    app.log("Eight-wave sine braid initialized.")
end

return definition
