local SCRIPT_ID, PANEL_ID = "three_metakon", "controllers"
local POLL_INTERVAL = 1.0

local addresses = { 1, 2, 5 }
local device_colors = { "#D32F2F", "#1976D2", "#388E3C" }

---@type table<string, any>[]
local parameter_controls = {
    { parameter = "setpoint", suffix = "setpoint", label = "Setpoint, °C",
      initial = 0.0, min = -999.0, max = 9999.0, step = 1.0 },
    { parameter = "proportional_band", suffix = "proportional_band",
      label = "Proportional band", initial = 1.0, min = 1.0, max = 9999.0, step = 1.0 },
    { parameter = "integral_time", suffix = "integral_time", label = "Integral time, min",
      initial = 0.1, min = 0.1, max = 500.0, step = 0.1 },
    { parameter = "derivative_time", suffix = "derivative_time", label = "Derivative time",
      initial = 0.0, min = 0.0, max = 255.0, step = 1.0 },
}

-- These parameters are read during Refresh to restore their suspended series, but are
-- not shown in the panel.
---@type table<string, any>[]
local recovery_parameters = {
    { parameter = "measurement" },
    { parameter = "output_power" },
}

---@type table<string, any>
local script = {
    id = SCRIPT_ID,
    panels = {{
        id = PANEL_ID,
        title = "Metakon controllers — COM5",
        controls = {},
    }},
}

local controls = script.panels[1].controls
local devices = {}

local function control_id(address, suffix)
    return "metakon_" .. address .. "_" .. suffix
end

local function set_panel_value(address, suffix, value)
    app.set_control(SCRIPT_ID, PANEL_ID, control_id(address, suffix), value)
end

local function set_status(message)
    app.set_control(SCRIPT_ID, PANEL_ID, "status", message)
end

local function write_parameter(device, parameter, suffix, value)
    local actual_value = device.controller:write(parameter, value)
    set_panel_value(device.address, suffix, actual_value)
    set_status("Metakon " .. device.address .. ": " .. parameter .. " = " .. tostring(actual_value))
end

local function try_read_parameter(device, definition)
    local ok, value_or_error = pcall(function()
        return device.controller:read(definition.parameter)
    end)
    if not ok then return false, value_or_error end

    -- Only PID parameters have a corresponding panel control.
    if definition.suffix then
        set_panel_value(device.address, definition.suffix, value_or_error)
    end
    return true, nil
end

local function refresh_device(device)
    local failures = {}

    -- Setpoint is the connectivity probe. Avoid waiting for all remaining parameters
    -- to time out when the device is disconnected.
    local connected, connection_error = try_read_parameter(device, parameter_controls[1])
    if not connected then
        set_status("Metakon " .. device.address .. " is unavailable. See Application log.")
        return false, connection_error
    end

    -- Successful reads restore suspended series without exposing values in the panel.
    for _, definition in ipairs(recovery_parameters) do
        local ok = try_read_parameter(device, definition)
        if not ok then table.insert(failures, definition.parameter) end
    end

    -- Setpoint was already read; continue with the other PID parameters even if a
    -- measurement failed because of a thermocouple alarm.
    for index = 2, #parameter_controls do
        local definition = parameter_controls[index]
        local ok = try_read_parameter(device, definition)
        if not ok then table.insert(failures, definition.parameter) end
    end

    if #failures == 0 then
        set_status("Metakon " .. device.address .. " parameters refreshed.")
        return true, nil
    end

    local failed_parameters = table.concat(failures, ", ")
    set_status("Metakon " .. device.address .. " refresh failed for: "
        .. failed_parameters .. ". See Application log.")
    return false, failed_parameters
end

controls[#controls + 1] = {
    kind = "readout",
    id = "status",
    label = "Status",
    initial = "Waiting for initial Metakon read.",
}

for index, address in ipairs(addresses) do
    local device = {
        address = address,
        color = device_colors[index],
        controller = app.metakon({ connection = "primary", device = address, channel = 0, scale = 1.0 }),
    }
    devices[address] = device

    controls[#controls + 1] = {
        kind = "readout",
        id = control_id(address, "identity"),
        label = "Controller",
        initial = "COM5 / address " .. address .. " / channel 0",
    }

    for _, definition in ipairs(parameter_controls) do
        local current_device = device
        local parameter, suffix = definition.parameter, definition.suffix
        local callback_name = "set_" .. address .. "_" .. suffix

        controls[#controls + 1] = {
            kind = "number",
            id = control_id(address, suffix),
            label = definition.label,
            initial = definition.initial,
            min = definition.min,
            max = definition.max,
            step = definition.step,
            on_change = callback_name,
        }
        script[callback_name] = function(value)
            write_parameter(current_device, parameter, suffix, value)
        end
    end

    local current_device = device
    local refresh_callback = "refresh_" .. address
    controls[#controls + 1] = {
        kind = "button",
        id = control_id(address, "refresh"),
        label = "Refresh Metakon " .. address,
        on_click = refresh_callback,
    }
    script[refresh_callback] = function() refresh_device(current_device) end
end

for _, address in ipairs(addresses) do
    local device = devices[address]
    local controller = device.controller
    local prefix = "metakon_" .. address

    controller:add("measurement", {
        name = prefix .. "_temperature",
        interval = POLL_INTERVAL,
        color = device.color,
    })
    controller:add("output_power", {
        name = prefix .. "_power",
        interval = POLL_INTERVAL,
        color = device.color,
    })
    controller:add("setpoint", {
        name = prefix .. "_setpoint",
        interval = POLL_INTERVAL,
        color = device.color,
    })
end

app.register_script(script)

local initial_refresh_failures = 0
for _, address in ipairs(addresses) do
    local ok, refreshed_or_error = pcall(refresh_device, devices[address])
    if not ok or not refreshed_or_error then
        initial_refresh_failures = initial_refresh_failures + 1
        if not ok then
            app.log("Unexpected initial refresh failure for Metakon " .. address
                .. ": " .. tostring(refreshed_or_error))
        end
    end
end

if initial_refresh_failures == 0 then
    set_status("Initial Metakon refresh completed.")
else
    set_status("Initial refresh completed with " .. initial_refresh_failures
        .. " error(s). Use Refresh to retry.")
end

app.start()
