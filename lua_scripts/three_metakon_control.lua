local SCRIPT_ID = "three_metakon"
local PANEL_ID = "controllers"
local POLL_INTERVAL = 1.0

local addresses = {
    3,
    4,
    5,
}

local parameter_controls = {
    {
        parameter = "setpoint",
        suffix = "setpoint",
        label = "Setpoint, °C",
        initial = 0.0,
        min = -999.0,
        max = 9999.0,
        step = 1.0,
    },

    {
        parameter = "proportional_band",
        suffix = "proportional_band",
        label = "Proportional band",
        initial = 1.0,
        min = 1.0,
        max = 9999.0,
        step = 1.0,
    },

    {
        parameter = "integral_time",
        suffix = "integral_time",
        label = "Integral time",
        initial = 1.0,
        min = 1.0,
        max = 30000.0,
        step = 1.0,
    },

    {
        parameter = "derivative_time",
        suffix = "derivative_time",
        label = "Derivative time",
        initial = 0.0,
        min = 0.0,
        max = 255.0,
        step = 1.0,
    },
}

---@type table<string, any>
local script = {
    id = SCRIPT_ID,

    panels = {
        {
            id = PANEL_ID,
            title = "Metakon controllers — COM5",
            controls = {},
        },
    },
}

local controls = script.panels[1].controls
local devices = {}

local function control_id(
    address,
    suffix
)
    return "metakon_"
        .. address
        .. "_"
        .. suffix
end

local function set_panel_value(
    address,
    suffix,
    value
)
    app.set_control(
        SCRIPT_ID,
        PANEL_ID,
        control_id(address, suffix),
        value
    )
end

local function set_status(message)
    app.set_control(
        SCRIPT_ID,
        PANEL_ID,
        "status",
        message
    )
end

local function write_parameter(
    device,
    parameter,
    suffix,
    value
)
    local actual_value =
        device.controller:write(
            parameter,
            value
        )

    set_panel_value(
        device.address,
        suffix,
        actual_value
    )

    set_status(
        "Metakon "
            .. device.address
            .. ": "
            .. parameter
            .. " = "
            .. tostring(actual_value)
    )
end

local function refresh_device(device)
    for _, definition in ipairs(
        parameter_controls
    ) do
        local value =
            device.controller:read(
                definition.parameter
            )

        set_panel_value(
            device.address,
            definition.suffix,
            value
        )
    end

    set_status(
        "Metakon "
            .. device.address
            .. " parameters refreshed."
    )
end

table.insert(
    controls,
    {
        kind = "readout",
        id = "status",
        label = "Status",
        initial = "Use Refresh to read current PID settings.",
    }
)

for _, address in ipairs(addresses) do
    local device = {
        address = address,

        controller = app.metakon({
            connection = "primary",
            device = address,
            channel = 0,
            scale = 1.0,
        }),
    }

    devices[address] = device

    table.insert(
        controls,
        {
            kind = "readout",
            id = control_id(
                address,
                "identity"
            ),
            label = "Controller",
            initial = "COM5 / address "
                .. address
                .. " / channel 0",
        }
    )

    for _, definition in ipairs(
        parameter_controls
    ) do
        local current_device = device
        local parameter = definition.parameter
        local suffix = definition.suffix

        local callback_name =
            "set_"
            .. address
            .. "_"
            .. suffix

        table.insert(
            controls,
            {
                kind = "number",
                id = control_id(
                    address,
                    suffix
                ),
                label = definition.label,
                initial = definition.initial,
                min = definition.min,
                max = definition.max,
                step = definition.step,
                on_change = callback_name,
            }
        )

        script[callback_name] = function(value)
            write_parameter(
                current_device,
                parameter,
                suffix,
                value
            )
        end
    end

    local current_device = device

    local refresh_callback =
        "refresh_" .. address

    table.insert(
        controls,
        {
            kind = "button",
            id = control_id(
                address,
                "refresh"
            ),
            label = "Refresh Metakon "
                .. address,
            on_click = refresh_callback,
        }
    )

    script[refresh_callback] = function()
        refresh_device(current_device)
    end
end

for _, address in ipairs(addresses) do
    local controller =
        devices[address].controller

    controller:add(
        "measurement",
        {
            name = "metakon_"
                .. address
                .. "_temperature",
            interval = POLL_INTERVAL,
        }
    )

    controller:add(
        "output_power",
        {
            name = "metakon_"
                .. address
                .. "_power",
            interval = POLL_INTERVAL,
        }
    )

    controller:add(
        "setpoint",
        {
            name = "metakon_"
                .. address
                .. "_setpoint",
            interval = POLL_INTERVAL,
        }
    )
end

app.start()

app.register_script(script)
