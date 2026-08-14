local definition = {
    application = {
        fps = 20,
        poll_interval = 1.0,
        plot_window = 3600.0,
        max_plot_points_per_series = 2000,
    },

    connections = {
        primary = {
            port = "COM5",
            baud_rate = 9600,
            data_bits = 8,
            parity = "none",
            stop_bits = 1,
            flow_control = "none",
            timeout = 0.25,
        },
    },

    scripts = {
        "../lua_scripts/three_metakon_control.lua",
    },
}

function definition.setup()
    app.log(
        "Three Metakon controllers configured on COM5."
    )
end

return definition
