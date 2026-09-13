return {
    application = {
        fps = 20, poll_interval = 0.5, plot_window = 300,
        max_plot_points_per_series = 2000,
    },
    plot_panes = {
        { id = "temperature", title = "Measured temperature / recipe target, °C", weight = 2 },
        { id = "command", title = "Heater command, %" },
        { id = "watts", title = "Delivered heating power, W" },
    },
    emulator = {
        transport = "memory", script = "../emulator_scripts/furnace_plant.lua",
    },
    scripts = { "../lua_scripts/furnace_scenarios_demo.lua" },
}
