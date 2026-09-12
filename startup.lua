local definition = {
    application = {
        fps = 20,
        poll_interval = 1.0,
        plot_window = 3600.0,
        max_plot_points_per_series = 1000,
    },

    emulator = {
        transport = "memory",
        script = "emulator_scripts/sine_generator.lua",
    },

    scripts = {
        "lua_scripts/sine_braid_demo.lua",
    },
}

function definition.setup()
    app.log("Application initialized from startup.lua.")
end

return definition
