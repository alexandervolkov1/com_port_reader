local script, run, owner, power, resets, fail_zero = nil, nil, nil, 0, 0, false
local values, enabled, signals = {}, {}, {}
local plant = {}
function plant:write(key, value)
    if key == "heater_power" then
        if value == 0 and fail_zero then error("zero rejected") end
        power = value
    end
    return value
end
function plant:add(_, options) signals[options.name] = options end
function plant:pid(_, options)
    assert(not owner)
    owner = { options = options }
    function owner:pause() plant:write("heater_power", 0) end
    function owner:resume() power = 40 end
    function owner:set_ramp_reference(value) self.reference = value end
    function owner:set_fixed_reference(value) self.reference = value end
    function owner:add(_, value) signals[value.name] = value end
    return owner
end
app = {
    log = function() end, start = function() end, stop = function() end,
    start_emu = function() end, stop_emu = function() end,
    clear = function() resets = resets + 1; owner = nil; signals = {} end,
    virtual_instrument = function() return plant end,
    filter = function(_, options) signals[options.name] = options end,
    unregister_script = function() end,
    set_control = function(_, _, id, value) values[id] = value end,
    set_control_enabled = function(_, _, id, value) enabled[id] = value end,
    register_script = function(value)
        script = value
        for _, panel in ipairs(script.panels) do
            for _, control in ipairs(panel.controls) do
                local callback = control.on_click or control.on_change
                if callback then assert(type(script[callback]) == "function") end
            end
        end
    end,
    scenario = function()
        run = { stages = {}, conditions = {}, timers = {} }
        function run:on_stop(name) self.cleanup = name end
        function run:on_error(name) self.failed = name end
        function run:when(condition, callback) self.conditions[#self.conditions + 1] = {condition, callback} end
        function run:after(delay, callback) self.timers[delay] = callback end
        function run:race(alternatives) self.alternatives = alternatives end
        function run:stage(name, definition)
            assert(not self.stages[name] and script[definition.enter])
            self.stages[name] = definition
        end
        function run:start(name)
            for _, definition in pairs(self.stages) do
                for _, transition in ipairs(definition.transitions or {}) do
                    assert(self.stages[transition.next], "missing stage target")
                end
            end
            self:enter(name)
        end
        function run:enter(name) script[self.stages[name].enter]() end
        function run:complete(reason) script[self.cleanup]({status = "completed", reason = reason}) end
        function run:stop(reason) script[self.cleanup]({status = "stopped", reason = reason}) end
        function run:cancel() self.cancelled = true end
        return run
    end,
}
function test_furnace_scenarios()
    assert(#script.panels == 3 and power == 0 and resets == 0)
    assert(enabled.start and not enabled.steps and not enabled.stop and not enabled.recover)
    script.start()
    assert(power == 25 and not enabled.start and enabled.stop and not enabled.recipe)
    assert(run.timers[150] and #run.conditions[1][1].any == 2)
    script.choose_recipe()
    script.start()
    assert(resets == 1 and values.selected == "1. Power steps")
    run:enter("high"); assert(power == 60)
    run:enter("coast"); assert(power == 0)
    run:enter("done")
    assert(enabled.start and not enabled.stop and values.result:match("completed"))
    assert(signals.recipe_temperature and signals.recipe_effective_power, "completion keeps plots")
    script.choose_recipe()
    script.start()
    assert(owner and owner.reference.rate == 0.5 and power == 40)
    assert(signals.recipe_target.pane == "temperature")
    assert(run.stages.settle.transitions[1].when.for_seconds == 5)
    run:enter("settle"); assert(owner.reference == 45)
    run:enter("hold"); run:enter("cool"); run:enter("done")
    assert(power == 0 and enabled.start)
    script.choose_guard()
    script.start()
    assert(not owner and power == 80 and #run.alternatives == 2)
    assert(run.alternatives[1].when.above == 30 and run.alternatives[2].after == 20)
    script.furnace_demo_limit()
    assert(power == 0 and values.result:match("Expected temperature guard"))
    script.start()
    script.furnace_demo_timeout()
    assert(power == 0 and values.result:match("Time limit"))
    script.start()
    script.furnace_demo_safety()
    assert(power == 0 and values.result:match("Safety guard"))
    script.start()
    script.stop()
    assert(power == 0 and values.result:match("operator"))
    script.start()
    script.furnace_demo_failed({error = "injected callback failure"})
    assert(power == 0 and enabled.start and values.result:match("injected"))
    script.start()
    fail_zero = true
    assert(not pcall(script.stop))
    assert(not enabled.start and enabled.recover and not enabled.steps)
    local before = resets
    script.start()
    script.choose_steps()
    assert(resets == before and values.selected == "3. Temperature guard")
    fail_zero = false
    script.recover()
    assert(power == 0 and enabled.start and not enabled.recover)
    script.start()
    assert(power == 80)
    script.stop()
end
