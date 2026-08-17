local SCRIPT_ID = "three_metakon"
local PANEL_ID = "controllers"
local POLL_INTERVAL = 1.0

local addresses = {
    1,
    2,
    5,
}

local device_colors = {
    "#D32F2F",
    "#1976D2",
    "#388E3C",
}

---@type table<string, any>[]
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
        label = "Integral time, min",
        initial = 0.1,
        min = 0.1,
        max = 500.0,
        step = 0.1,
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

-- These parameters are read during Refresh to restore
-- their suspended series, but are not shown in the panel.
---@type table<string, any>[]
local recovery_parameters = {
    {
        parameter = "measurement",
    },

    {
        parameter = "output_power",
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

local function try_read_parameter(
    device,
    definition
)
    local ok, value_or_error = pcall(
        function()
            return device.controller:read(
                definition.parameter
            )
        end
    )

    if not ok then
        return false, value_or_error
    end

    -- Only PID parameters have a corresponding panel
    -- control. Recovery parameters are read silently.
    if definition.suffix ~= nil then
        set_panel_value(
            device.address,
            definition.suffix,
            value_or_error
        )
    end

    return true, nil
end

local function refresh_device(device)
    local failures = {}

    -- Setpoint is used as a connectivity probe. If it
    -- cannot be read, avoid waiting for every remaining
    -- parameter to time out on a disconnected device.
    local connected, connection_error =
        try_read_parameter(
            device,
            parameter_controls[1]
        )

    if not connected then
        set_status(
            "Metakon "
                .. device.address
                .. " is unavailable. See Application log."
        )

        return false, connection_error
    end

    -- Successful reads restore the suspended temperature
    -- and output-power series without showing the values
    -- in the control panel.
    for _, definition in ipairs(
        recovery_parameters
    ) do
        local ok =
            try_read_parameter(
                device,
                definition
            )

        if not ok then
            table.insert(
                failures,
                definition.parameter
            )
        end
    end

    -- Setpoint has already been read as the connectivity
    -- probe. Read the remaining PID parameters even if
    -- measurement failed because of a thermocouple alarm.
    for index = 2, #parameter_controls do
        local definition =
            parameter_controls[index]

        local ok =
            try_read_parameter(
                device,
                definition
            )

        if not ok then
            table.insert(
                failures,
                definition.parameter
            )
        end
    end

    if #failures == 0 then
        set_status(
            "Metakon "
                .. device.address
                .. " parameters refreshed."
        )

        return true, nil
    end

    set_status(
        "Metakon "
            .. device.address
            .. " refresh failed for: "
            .. table.concat(failures, ", ")
            .. ". See Application log."
    )

    return false, table.concat(failures, ", ")
end

table.insert(
    controls,
    {
        kind = "readout",
        id = "status",
        label = "Status",
        initial = "Waiting for initial Metakon read.",
    }
)

for index, address in ipairs(addresses) do
    local device = {
        address = address,
        color = device_colors[index],

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
    local device = devices[address]
    local controller = device.controller

    controller:add(
        "measurement",
        {
            name = "metakon_"
                .. address
                .. "_temperature",
            interval = POLL_INTERVAL,
            color = device.color,
        }
    )

    controller:add(
        "output_power",
        {
            name = "metakon_"
                .. address
                .. "_power",
            interval = POLL_INTERVAL,
            color = device.color,
        }
    )

    controller:add(
        "setpoint",
        {
            name = "metakon_"
                .. address
                .. "_setpoint",
            interval = POLL_INTERVAL,
            color = device.color,
        }
    )
end

app.register_script(script)

local initial_refresh_failures = 0

for _, address in ipairs(addresses) do
    local ok, refreshed_or_error =
        pcall(
            refresh_device,
            devices[address]
        )

    if not ok or not refreshed_or_error then
        initial_refresh_failures =
            initial_refresh_failures + 1

        if not ok then
            app.log(
                "Unexpected initial refresh failure "
                    .. "for Metakon "
                    .. address
                    .. ": "
                    .. tostring(refreshed_or_error)
            )
        end
    end
end

if initial_refresh_failures == 0 then
    set_status(
        "Initial Metakon refresh completed."
    )
else
    set_status(
        "Initial refresh completed with "
            .. initial_refresh_failures
            .. " error(s). Use Refresh to retry."
    )
end

app.start()
