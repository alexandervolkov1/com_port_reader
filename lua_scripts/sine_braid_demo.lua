-- Eight phase-shifted sine waves form a symmetric braid around zero.
local AMPLITUDES = { 100.0, 90.0, 80.0, 70.0, 70.0, 80.0, 90.0, 100.0 }
local RAW_COLORS = { "#E63946", "#F4A261", "#E9C46A", "#2A9D8F", "#00B4D8", "#4361EE", "#8338EC", "#FF4D9D" }
local FILTER_COLORS = { "#FF8A8F", "#FFC28A", "#FFF0A3", "#78D6C8", "#72D9EE", "#91A7FF", "#C7A4FF", "#FF9BC4" }

local SCRIPT_ID, PANEL_ID = "sine_braid", "controls"
local period, noise_amplitude, amplitude, filter_time_constant = 300.0, 2.0, 100.0, 6.0
local generators, filter_enabled = {}, {}
local panel_registered = false

for index = 1, #AMPLITUDES do
    filter_enabled[index] = true
end

local function raw_name(index) return "braid_" .. index end
local function filter_name(index) return raw_name(index) .. "_ema" end

local function require_finite_positive(name, value)
    if type(value) ~= "number" or value ~= value
        or value == math.huge or value == -math.huge or value <= 0.0
    then error(name .. " must be a positive finite number") end
end

local function set_status(message)
    if not panel_registered then return end
    app.set_control(SCRIPT_ID, PANEL_ID, "status", message)
    local running = #generators > 0
    app.set_control_enabled(SCRIPT_ID, PANEL_ID, "stop", running, "The demo is stopped.")
    for index = 1, #AMPLITUDES do
        app.set_control_enabled(SCRIPT_ID, "filters", "filter_enabled_" .. index, running,
            "Restart the demo to change visible filters.")
    end
end

local function add_filter(index)
    app.filter(raw_name(index), {
        name = filter_name(index), kind = "exponential",
        time_constant = filter_time_constant, color = FILTER_COLORS[index],
    })
end

local function apply_filter(index)
    if filter_enabled[index] then
        app.set_filter(filter_name(index), {
            kind = "exponential", time_constant = filter_time_constant,
        })
    end
end

local filter_controls = {}
local controls = {
    { kind = "readout", id = "status", label = "Status", initial = "Starting sine braid." },
    { kind = "readout", id = "hint", label = "All eight curves",
      initial = "Amplitude keeps braid proportions; amplitude/noise blend over 2 s. Period keeps phase; EMA keeps history." },
    { kind = "number", id = "amplitude", label = "Outer amplitude (all waves)", initial = amplitude,
      min = 0.0, max = 1000.0, step = 1.0, on_change = "set_amplitude" },
    { kind = "number", id = "filter_time_constant", label = "Filter time constant, s (all)", initial = filter_time_constant,
      min = 0.1, max = 3600.0, step = 0.5, on_change = "set_filter_time_constant" },
    { kind = "number", id = "period", label = "Period, s", initial = period,
      min = 1.0, max = 86400.0, step = 1.0, on_change = "set_period" },
    { kind = "number", id = "noise", label = "Noise amplitude", initial = noise_amplitude,
      min = 0.0, max = 1000.0, step = 0.1, on_change = "set_noise" },
    { kind = "button", id = "restart", label = "Restart demo", on_click = "run" },
    { kind = "button", id = "stop", label = "Stop demo", on_click = "stop" },
}

for index = 1, #AMPLITUDES do
    filter_controls[#filter_controls + 1] = {
        kind = "toggle", id = "filter_enabled_" .. index,
        label = "Wave " .. index .. " exponential filter",
        initial = filter_enabled[index], on_change = "set_filter_enabled_" .. index,
    }
end

local script = {
    id = SCRIPT_ID,
    panels = {
        { id = PANEL_ID, title = "1. All eight curves", controls = controls },
        { id = "filters", title = "2. Show filtered curves", controls = filter_controls },
    },
}

function script.set_period(value)
    require_finite_positive("period", value)
    period = value
    for _, generator in ipairs(generators) do generator:write("period", period) end
    set_status("Period updated.")
end

function script.set_noise(value)
    if type(value) ~= "number" or value ~= value
        or value == math.huge or value == -math.huge or value < 0.0
    then error("noise amplitude must be a non-negative finite number") end
    noise_amplitude = value
    for _, generator in ipairs(generators) do generator:write("noise_amplitude", value) end
    set_status("Noise amplitude updated for all curves (2 s transition).")
end

for index = 1, #AMPLITUDES do
    script["set_filter_enabled_" .. index] = function(enabled)
        if #generators == 0 or enabled == filter_enabled[index] then return end
        filter_enabled[index] = enabled
        if enabled then
            add_filter(index)
            set_status("Wave " .. index .. " filter enabled.")
        else
            app.delete(filter_name(index))
            set_status("Wave " .. index .. " filter disabled.")
        end
    end
end

function script.set_amplitude(value)
    if type(value) ~= "number" or value ~= value or value < 0 or value > 1000 then
        error("amplitude must be between 0 and 1000")
    end
    for index, generator in ipairs(generators) do
        generator:write("amplitude", value * AMPLITUDES[index] / 100.0)
    end
    amplitude = value
    set_status("Amplitude updated for all curves (2 s transition).")
end

function script.set_filter_time_constant(value)
    require_finite_positive("filter time constant", value)
    filter_time_constant = value
    for index = 1, #AMPLITUDES do apply_filter(index) end
    set_status("All EMA time constants updated without resetting history.")
end

function script.stop()
    app.stop()
    app.stop_emu()
    generators = {}
    set_status("Sine braid stopped.")
end

function script.run()
    app.stop()
    app.stop_emu()
    app.clear()
    generators = {}

    app.start_emu()
    for index, base_amplitude in ipairs(AMPLITUDES) do
        local generator = app.virtual_instrument({ id = index })
        generator:write("transition_seconds", 2.0)
        generator:write("amplitude", amplitude * base_amplitude / 100.0)
        generator:write("noise_amplitude", noise_amplitude)
        generator:write("period", period)
        generator:write("phase", (index - 1) * math.pi / 4.0)
        generator:add("value", { name = raw_name(index), interval = 0.5, color = RAW_COLORS[index] })
        if filter_enabled[index] then add_filter(index) end
        generators[index] = generator
    end
    app.start()
    set_status("Eight-wave sine braid running.")
end

app.unregister_script(SCRIPT_ID)
script.run()
app.register_script(script)
panel_registered = true
set_status("Eight-wave sine braid running.")
