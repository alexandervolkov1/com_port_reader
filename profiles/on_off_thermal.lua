local definition = {
    application = {
        fps = 20,
        poll_interval = 0.5,
        plot_window = 300.0,
        max_plot_points_per_series = 2000,
    },

    emulator = {
        transport = "memory",
        script = "../emulator_scripts/pid_thermal_plant.lua",
    },

    scripts = {
        "../lua_scripts/on_off_thermal_demo.lua",
    },
}

function definition.setup()
    app.log("Thermal on/off test profile initialized.")
end

return definition
