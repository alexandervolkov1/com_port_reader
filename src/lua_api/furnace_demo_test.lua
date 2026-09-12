local owner, script, fail_pause, fail_resume
local power, created, cleared = 0, {}, 0
local series, controls = {}, {}
local plant = {}

function plant:read(key) return key == "heater_power" and power or 20 end
function plant:write(key, value)
    if key == "heater_power" then power = value end
    return value
end
function plant:add(_, options) series[options.name] = true end

local function create(kind, options)
    assert(not owner, "the old controller must release the heater first")
    local controller = { kind = kind, options = options, state_value = "running" }
    owner = controller
    created[#created + 1] = controller
    function controller:state() return self.state_value end
    function controller:pause()
        self.state_value = "paused"
        if fail_pause then error("safe write failed") end
        power = 0
    end
    function controller:resume()
        assert(owner == self)
        if fail_resume then error("resume failed") end
        self.state_value = "running"
        power = 42
    end
    function controller:remove()
        self:pause()
        owner = nil
        self.removed = true
    end
    function controller:write(key, value)
        self.options[key] = value
        return value
    end
    function controller:diagnostics()
        return self.kind == "Furnace"
            and { "output", "feed_forward", "predicted_measurement", "measurement_rate" }
            or { "output", "setpoint" }
    end
    function controller:add(key) series[self.options.name .. "_" .. key] = true end
    function controller:reset_integral() self.integral_reset = true end
    return controller
end

function plant:pid(_, options) return create("PID", options) end
function plant:furnace(_, options) return create("Furnace", options) end

app = {
    stop = function() end, start = function() end,
    stop_emu = function() end, start_emu = function() end,
    unregister_script = function() end,
    clear = function() series = {}; owner = nil; cleared = cleared + 1 end,
    virtual_instrument = function() return plant end,
    filter = function(_, options) series[options.name] = true end,
    set_filter = function(name) assert(series[name]) end,
    set_control = function(_, _, id, value) controls[id] = value end,
    register_script = function(value)
        script = value
        local ids = {}
        for _, control in ipairs(script.panels[1].controls) do
            assert(not ids[control.id], "duplicate control id")
            ids[control.id] = true
            local callback = control.on_click or control.on_change
            if callback then assert(type(script[callback]) == "function") end
        end
    end,
}

function test_furnace_demo()
    assert(power == 0 and #created == 0)
    local initial_clears = cleared
    script.set_heater_power(25)
    assert(power == 25)
    script.pid()
    assert(owner.kind == "PID" and power == 42)
    script.set_heater_power(80)
    assert(power == 42, "manual writes must be blocked in automatic mode")
    script.set_pid_kp(0.6)
    assert(owner.options.kp == 0.6)
    script.manual()
    assert(owner:state() == "paused" and power == 42)
    script.pid()
    assert(#created == 1, "the same paused controller should resume")
    script.furnace()
    assert(created[1].removed and owner.kind == "Furnace")
    assert(owner.options.kp == 0.1 and owner.options.safe_output == 0)
    assert(series.furnace_temperature and series.furnace_temperature_ma)
    assert(series.furnace_pid_1_output and series.furnace_furnace_2_feed_forward)
    assert(cleared == initial_clears, "mode changes must preserve measurements")
    script.set_furnace_kp(0.3)
    assert(owner.options.kp == 0.3)
    script.set_plant_max_power(4000)
    assert(owner.options.max_power == 2500, "plant and controller models are independent")
    script.reset_integral()
    assert(owner.integral_reset)

    fail_pause = true
    assert(not pcall(script.pid))
    assert(owner.kind == "Furnace" and not owner.removed and #created == 2)
    script.set_heater_power(90)
    assert(power == 42, "failed pause must not enable manual writes")
    fail_pause = false
    script.power_off()
    assert(power == 0)
    fail_resume = true
    assert(not pcall(script.pid))
    assert(owner:state() == "paused" and power == 0)
    fail_resume = false
    script.pid()
    assert(owner.options.kp == 0.6)
    assert(not pcall(script.set_pid_output_min, 90))
    assert(owner.options.output_min == 0)
    script.run()
    assert(power == 0 and not owner)
    script.furnace()
    assert(owner.options.kp == 0.3)
    script.stop()
    assert(power == 0 and not owner)
end
