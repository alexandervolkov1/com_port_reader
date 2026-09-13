local definition = {
    plot_panes = {
        { id = "temperature", title = "Temperature / target, °C", weight = 2 },
        { id = "command", title = "Heater command / controller terms, %" },
        { id = "watts", title = "Delivered heating power, W" },
        { id = "rate", title = "Temperature change, °C/s" },
    },
    application = {
        fps = 20,
        poll_interval = 0.5,
        plot_window = 7200,
        max_plot_points_per_series = 4000,
    },

    emulator = {
        transport = "memory",
        script = "../emulator_scripts/furnace_plant.lua",
    },

    scripts = {
        "../lua_scripts/furnace_manual_demo.lua",
    },
}

function definition.setup()
    app.log("Furnace Manual / PID / Furnace demo initialized.")
end

return definition
