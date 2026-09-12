# Virtual instruments and emulator models

Virtual instruments exercise the same acquisition, processing and output-control paths as real instruments, using a model-defined parameter catalog. Normal operation uses an in-process memory byte stream. Serial emulation is an optional integration/debug mode.

## Two Lua environments

Application Lua owns orchestration: `app.start_emu`, handles, plotted series, controllers, scenarios and panels. Model Lua owns simulated device state: the `instruments` catalog and `read/write` functions. They are separate Lua states on separate threads; a model cannot call `app`, and an application script cannot directly access model locals.

The catalog is discovered through the virtual-instrument protocol. Its numeric addresses are translated back into parameter keys before the Rust model adapter calls Lua. Values cross as typed number/integer/boolean values, not as shared Lua objects.

## Minimal complete model

Save this as `emulator_scripts/example.lua` and point a profile at it:

```lua
local value = 20
instruments = {{
    name = "Example",
    parameters = {{
        key = "value", name = "Value", type = "number",
        access = "read_write", series = true,
        unit = "units", min = 0, max = 100,
    }},
}}

function read(instrument_id, parameter, time)
    assert(instrument_id == 1 and parameter == "value")
    return value
end

function write(instrument_id, parameter, requested, time)
    assert(instrument_id == 1 and parameter == "value")
    value = requested
    return value
end
```

A profile saved in the repository root:

```lua
return {
    emulator = {transport = "memory", script = "emulator_scripts/example.lua"},
    setup = function()
        app.start_emu()
        device = app.virtual_instrument({id = 1})
        device:add("value", {name = "example", interval = 0.5})
        app.start()
    end,
}
```

The example paths describe files to create; the ready-made [tutorial profile](../profiles/tutorial.lua) instead uses the supplied furnace model.

## Catalog schema

`instruments` is a global contiguous array. Positions are one-based instrument IDs; parameter array positions become one-based parameter IDs. Each instrument requires a nonempty name and a nonempty parameter array. Keep order stable across model revisions when reusing existing handles/series.

| Parameter field | Type | Required / default | Meaning |
| --- | --- | --- | --- |
| `key` | string | required | Unique key within the instrument; used in Lua reads/writes. |
| `name` | string | optional / key | Display name. |
| `type` | string | required | `number`, `integer` or `boolean`. |
| `access` | string | optional / `read_only` | `read_only`, `write_only` or `read_write`. |
| `series` | boolean | optional / false | Whether the parameter can become a periodic series; requires readable access. |
| `unit` | string | optional | Informational unit label. |
| `min`, `max` | numeric, matching type | optional, both or neither | Inclusive finite ordered range; boolean parameters cannot declare numeric bounds. |

Descriptor validation rejects duplicate/invalid keys and incompatible access/range combinations. The server checks input values before write and validates returned model values against the descriptor; do not return a string for a number or a fractional number for an integer.

`read(instrument_id, parameter, time)` is required if any parameter is readable. `write(instrument_id, parameter, value, time)` is required if any parameter is writable. A write must return the **actual stored value**, which may differ from the request if the model intentionally changes it within allowed bounds.

## Time and state

`time` is elapsed seconds since the emulator session started, not Unix time. Store previous time/model state in Lua locals captured by read/write. Advance the simulation according to elapsed time, not the number of reads. Several parameter reads may occur close together; avoid applying the same elapsed interval repeatedly.

No periodic model callback is scheduled independently. Reads/writes advance the shipped furnace model lazily. Application plotted timestamps are acquisition timestamps, so they serve a different purpose from the model's elapsed argument.

Each model load/read/write uses the Lua instruction-hook execution limit. A model error returns a protocol error to the caller. Native blocking code cannot be forcibly preempted by that hook.

## Transport and lifecycle

`app.start_emu()` loads/validates the model, spawns the server and publishes the session only after startup succeeds. Starting an already running model is idempotent. Memory endpoints require no COM driver and ignore serial baud/parity settings.

`app.stop_emu()` requests stop, joins the server and clears the shared local endpoint. Restart loads a fresh Lua state with initial values and fresh elapsed time. It does not preserve heater power, random-generator state or script locals. Existing application series remain; restart acquisition/retry suspended series as appropriate.

The local source claims virtual operations even while stopped: a stopped local emulator produces `Local emulator is stopped` rather than falling through to a COM source. Real Metakon and text commands still route to serial on the same worker.

The local client allows one outstanding request. After a timeout, it drains the previous complete response before sending another request, avoiding a stale reply being interpreted as the new result. It never automatically replays a timed-out write. Protocol corruption/nonrecoverable transport errors close the session and require restart; a model-level error keeps a usable session.

Profile reload safely pauses controllers before stopping transports and builds a fresh model when requested by the new profile. Normal shutdown also attempts controller safe outputs while the emulator is alive. Explicitly stopping a model yourself does not automatically pause its controllers; pause/remove first.

## Optional serial mode

Serial mode requires an application client port and a different server port connected physically or by a configured virtual pair. Set `emulator.transport = "serial"`, `connection` and `port` as described in [configuration](configuration.md). The model protocol is the same framed protocol used by memory mode. This mode is for compatibility/testing, not the normal first-run path.

The standalone `device_emulator` binary also serves a model over serial; inspect its arguments in [development](development.md) before using it. It is not required for the in-process memory emulator.

## Included models

[The sine generator](../emulator_scripts/sine_generator.lua) exposes eight independent instruments. Each has readable `value` and writable `amplitude`, `noise_amplitude`, `period` and `phase`. Only `value` is series-enabled. Period uses seconds and phase radians; deterministic per-instrument random state supplies noise.

[The furnace](../emulator_scripts/furnace_plant.lua) exposes instrument 1. Readable series include `temperature` (°C), `heater_power` (%) and `effective_power` (W). Model settings include ambient temperature, maximum power, heater lag, thermal capacity, linear/radiative losses and measurement noise.

It integrates first-order heater lag and a heat balance in bounded time steps. The emulator's thermal capacity and effective heater-power state are physical-model state; the furnace controller uses a smaller prediction/feed-forward model and does not share these locals. The [furnace demo](furnace-demo.md) shows how to change model and controller settings independently.
