# Lua scenarios

Scenarios schedule short named callbacks using Rust timers and incoming measurements. They support one-shot events, races and staged workflows. They do not require a Lua polling loop and do not block Lua while waiting.

## Create a scenario and callbacks

```lua
function report(event)
    app.log(event.scenario_id .. ": " .. event.trigger)
end
scenario = app.scenario({id = "experiment"})
```

Reusing an ID starts a new run and cancels old pending tasks. Scenario methods identify the scenario by ID. Give callbacks unique names: a global function takes precedence; otherwise the runtime searches registered script tables and rejects ambiguous names.

```lua
return scenario:id()
```

This returns its stable string ID.

## Timers

```lua
scenario:after(5, "report")
```

Run once after a nonnegative finite relative delay in seconds, using a monotonic clock.

```lua
scenario:at(os.time() + 60, "report")
```

Run once at an absolute Unix timestamp in seconds. Absolute scheduling uses system time; it is distinct from elapsed experiment time.

## Measurement conditions

Each call registers a one-shot condition. These examples assume the tutorial's `temperature` series and the `report` callback above. Values use the series' engineering units.

| Capability | Minimal example | Meaning |
| --- | --- | --- |
| Above | `scenario:when({series = "temperature", above = 80}, "report")` | Threshold predicate. |
| Below | `scenario:when({series = "temperature", below = 30}, "report")` | Lower threshold predicate. |
| Inside | `scenario:when({series = "temperature", inside = {min = 75, max = 85}}, "report")` | Enter the specified range. |
| Outside | `scenario:when({series = "temperature", outside = {min = 10, max = 100}}, "report")` | Leave the specified range. |
| Stable | `scenario:when({series = "temperature", stable = {target = 80, tolerance = 2}, for_seconds = 10}, "report")` | Remain within tolerance for a positive hold duration. |
| Rising rate | `scenario:when({series = "temperature", rate_above = 1, window_seconds = 5}, "report")` | Rate in units/second over a measurement-time window. |
| Falling rate | `scenario:when({series = "temperature", rate_below = -1, window_seconds = 5}, "report")` | Negative rate threshold. |
| Stale data | `scenario:when({series = "temperature", stale_for_seconds = 5}, "report")` | No sufficiently recent sample; also handles a series that has not produced data. |
| All | `scenario:when({all = {{series = "temperature", above = 75}, {series = "temperature", below = 85}}}, "report")` | All child predicates match. |
| Any | `scenario:when({any = {{series = "temperature", above = 100}, {series = "temperature", stale_for_seconds = 5}}}, "report")` | Any child matches. |
| Hold / hysteresis | `scenario:when({series = "temperature", above = 80, hysteresis = 1, for_seconds = 5, edge = "rising"}, "report")` | Require continuous satisfaction; hysteresis reduces boundary chatter. |

Exactly one operator is required in a condition table. `for_seconds` defaults to zero, `hysteresis` to zero, and `edge` to `"rising"`; no falling-edge mode is exposed. One-shot rising semantics can fire on the first matching sample; a prior nonmatching sample is not required.

Ranges require finite `min < max`. Stability requires positive tolerance and positive `for_seconds`. Rate requires positive `window_seconds` and enough samples to establish the window; the rate is computed from sample values/timestamps rather than callback frequency. Window fields are rejected for non-rate operators. Stale conditions require a positive timeout and cannot have hold, hysteresis or a window.

Compositions contain 1–16 children and are bounded to four levels. A composite cannot itself specify `series`, `hysteresis` or `window_seconds`, but children can. Its own hold duration can require the combined predicate to remain true.

## Races

```lua
scenario:race({
    {when = {series = "temperature", above = 80}, callback = "report"},
    {after = 60, callback = "report"},
})
```

The first ready alternative wins; others are cancelled atomically. Each alternative has exactly one of `after`, `at` or `when`, plus a named `callback`. The array must be nonempty. A race is the usual way to combine a measurement target with an experiment timeout.

## Stages

This complete example uses global callbacks so their names resolve unambiguously:

```lua
function begin_heating(event) app.log("Heating") end
function begin_holding(event) app.log("Holding") end
function finish_stage(event) staged:complete("Finished") end

staged = app.scenario({id = "staged"})
staged:stage("heating", {
    enter = "begin_heating",
    transitions = {{after = 5, next = "holding", reason = "Warm-up elapsed"}},
})
staged:stage("holding", {
    enter = "begin_holding",
    transitions = {{after = 10, next = "done"}},
})
staged:stage("done", {enter = "finish_stage"})
staged:start("heating")
```

`stage(name, definition)` declares a unique stage with required `enter` callback and optional transition array. A transition contains one trigger (`after`, `at` or `when`), required `next`, and optional `reason`. Define every target before `start(name)`. A handle can start its stage graph once and cannot add stages afterward.

Stage transitions become active after the enter callback's tracked actions are Applied. An I/O command merely being enqueued is insufficient. Leaving a stage cancels its old transitions.

## Finalization and cancellation

The following are separate examples; register handlers once per handle before finishing it.

```lua
function cleanup(event) app.log("Cleanup: " .. event.reason) end
scenario:on_stop("cleanup")
```

```lua
function failed(event) app.log("Failure: " .. event.error) end
scenario:on_error("failed")
```

```lua
scenario:complete("Target reached")
```

Graceful completion waits for tracked actions and invokes `on_stop` if registered; final status is completed.

```lua
scenario:stop("Operator request")
```

Graceful stop uses the same finalization mechanism; final status is stopped.

```lua
scenario:cancel()
```

Immediate cancellation discards pending tasks and does **not** run cleanup. It does not undo already applied hardware actions. A callback error or a Failed tracked action follows the error finalization path. Cleanup should remain short and must not depend on scheduling another long-running scenario phase.

## Callback event

All callbacks receive one table. Fields depend on trigger type:

| Trigger | Fields |
| --- | --- |
| All | `scenario_id`, `callback`, `trigger`, `fired_at` (Unix seconds). |
| `timer` | `delay_seconds`. |
| `absolute_time` | `scheduled_at`. |
| `measurement` | `series`, `value`, `timestamp` when a triggering sample exists; `condition`, `for_seconds`, `definition` and operator-specific fields. |
| `stage` | `stage`, optional `previous_stage`, `reason`, optional `transition_trigger`. |
| `stop` | `status`, `reason`. |
| `error` | `status = "failed"`, `error`. |

Operator-specific measurement fields include `threshold`, `hysteresis`, `min`, `max`, `target`, `tolerance` or `stale_for_seconds`. Composite/stale timer events need not contain a sample value. Use optional-field checks.

```lua
function inspect_event(event)
    app.log(event.trigger .. " value=" .. tostring(event.value))
end
```

Callbacks are serialized per scenario. Actions carry scenario/run identity so stale completion events cannot advance a replacement run. In-app Scenarios shows status, stage, pending triggers, last transition and errors. It is read-only. Profile replacement discards the old runtime's scenarios; it is not graceful scenario completion and should not be used as a cleanup callback mechanism.
