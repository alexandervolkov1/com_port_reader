# Configuration profiles

A profile is a Lua file that returns one table. The parser lives in `src/lua_application_definition.rs`; validated domain constraints live in `application_definition.rs` and `presentation.rs`. Unknown keys in defined configuration tables are rejected.

## Selection and paths

Selection priority is explicit `--config path` (or `--config=path`), an existing remembered profile in `state/active-profile.txt`, then `startup.lua` in the application directory. Relative command-line paths resolve from the working directory.

In debug builds, the application directory is the Cargo manifest directory. In release builds, it is the executable's directory. Model paths and `scripts` entries resolve from the **selected profile's directory** in both builds; logs, process databases and remembered selection resolve from the **application directory**.

For a release executable built in the repository, use:

```powershell
cargo run --release -- --config startup.lua
```

Without `--config` it looks beside `target/release/com_port_reader.exe`, not necessarily in the repository root. A packaged distribution includes `startup.lua` beside its executable.

## Root table

| Field | Type | Required / default | Meaning and example |
| --- | --- | --- | --- |
| `application` | table | optional; internal runtime defaults | Display/poll settings: `application = {fps = 30}`. |
| `connections` | map of named tables | optional; no configured serial ports | `connections = {primary = {port = "COM5"}}`. |
| `emulator` | table | optional; no emulator | `emulator = {script = "model.lua"}`. |
| `scripts` | contiguous array of nonempty path strings | optional; empty | `scripts = {"experiment.lua", "panel.lua"}`, executed in order. |
| `plot_panes` | nonempty contiguous array of pane tables | optional; one default pane | `plot_panes = {{id = "thermal", title = "Temperature"}}`. |
| `setup` | function | optional; no action | `setup = function() app.log("Ready") end`. |

`return {}` is a complete valid profile. It does not start acquisition. A missing profile at initial launch falls back to internal defaults; unreadable/invalid profiles also fall back with a warning. Reload reports errors instead of silently using defaults.

## Application settings

All fields are optional.

| Field | Type | Default | Allowed range | Meaning / example |
| --- | --- | --- | --- | --- |
| `fps` | integer | 30 | 1–240 | GUI repaint frequency: `fps = 20`. |
| `poll_interval` | seconds, number | 1.0 | positive finite representable nonzero duration | Default raw-series polling: `poll_interval = 0.5`. |
| `plot_window` | seconds, number | 3600 | 1–1,209,600 | Live plot time window: `plot_window = 300`. |
| `max_plot_points_per_series` | integer | 4000 | 4–100,000 | Prepared plot points per visible series: `max_plot_points_per_series = 2000`. |

The point limit controls rendering, not database retention. A series' `interval` overrides the default poll interval. Polling is scheduled per series; communication time and other due polls affect actual timing.

## Serial connections

A nonempty map must include a connection named `primary`. Names and COM-port reuse are checked case-insensitively; IDs are assigned deterministically with primary first. Other connection names are used by Lua instrument options.

| Field | Type | Required / default | Allowed values | Meaning / example |
| --- | --- | --- | --- | --- |
| `port` | string | required | nonempty port name, not shared by connections | `port = "COM5"`. |
| `baud_rate` | integer | optional / 9600 | positive unsigned 32-bit value; driver must support it | `baud_rate = 19200`. |
| `data_bits` | integer | optional / 8 | 5, 6, 7, 8 | `data_bits = 8`. |
| `parity` | string | optional / `"none"` | none, even, odd; case-insensitive | `parity = "even"`. |
| `stop_bits` | integer | optional / 1 | 1, 2 | `stop_bits = 1`. |
| `flow_control` | string | optional / `"none"` | none, software, hardware; case-insensitive | `flow_control = "none"`. |
| `timeout` | seconds, number | optional / 0.25 | finite, at least 0.001; representable duration | `timeout = 0.5`; stored as whole milliseconds (fraction truncated). |

Each connection has a worker; instruments sharing a connection execute transactions sequentially. A memory emulator can coexist with real serial acquisition on primary: routing selects local virtual requests and serial Metakon/text requests.

Complete two-connection profile:

```lua
return {
    connections = {
        primary = {port = "COM5", baud_rate = 9600, parity = "none"},
        auxiliary = {port = "COM6", baud_rate = 19200, parity = "even", timeout = 0.5},
    },
    setup = function()
        local meter = app.metakon({connection = "primary", device = 1, channel = 0})
        meter:add("measurement", {name = "temperature", interval = 1})
        app.start()
    end,
}
```

Replace port/settings/device addresses with the actual equipment configuration.

## Emulator settings

| Field | Type | Required / default | Values and meaning |
| --- | --- | --- | --- |
| `script` | nonempty path string | required when emulator exists | Model file, e.g. `"../emulator_scripts/furnace_plant.lua"`. |
| `transport` | string | optional / inferred | `"memory"` normally; `"serial"` if `connection` or `port` is supplied and transport omitted. Case-insensitive. |
| `connection` | string | required for serial; unused for explicit memory | Existing connection whose serial settings are copied. |
| `port` | string | required for serial; unused for explicit memory | Emulator server endpoint; must differ from every application connection port. |

Memory mode is local and attaches to primary. Omit serial fields for clarity; explicit memory currently ignores them. Declaring an emulator does not start it.

Complete memory profile, saved under `profiles/`:

```lua
return {
    application = {poll_interval = 0.5},
    emulator = {transport = "memory", script = "../emulator_scripts/furnace_plant.lua"},
    setup = function()
        app.start_emu()
        plant = app.virtual_instrument()
        plant:add("temperature", "temperature")
        app.start()
    end,
}
```

Optional serial integration/debug profile (requires an actual linked pair of ports):

```lua
return {
    connections = {primary = {port = "COM5", baud_rate = 9600}},
    emulator = {
        transport = "serial", connection = "primary",
        port = "COM6", script = "../emulator_scripts/sine_generator.lua",
    },
    setup = function() app.start_emu() end,
}
```

Normal demos require no virtual COM software.

## Plot layout

Each pane has:

| Field | Type | Required / default | Constraint / example |
| --- | --- | --- | --- |
| `id` | string | required | Unique nonempty stable key: `"thermal"`. |
| `title` | string | optional / id | Nonempty displayed title: `"Temperature"`. |
| `weight` | number | optional / 1.0 | Positive finite relative height: `2`. |

```lua
return {
    plot_panes = {
        {id = "thermal", title = "Temperature", weight = 2},
        {id = "power", title = "Power", weight = 1},
    },
}
```

The first pane is the default. Series creation accepts `pane = "power"`; `app.set_series_pane` moves an existing series. GUI pane creation is a presentation feature; Lua validates keys against the configured layout.

## Setup and scripts

The top level is evaluated during validation without the `app` API and again in the runtime. Keep it declarative: no file writes, hardware actions or other side effects. Validation and application execution use an instruction-hook time limit; native calls are not preemptible.

The worker reads application script files, installs the API, executes the profile's `setup()`, then runs `scripts` in order. A failed setup prevents scripts from running; a failed script prevents later scripts. Commands already dispatched can have effects. Profile initialization is therefore not a hardware transaction.

Ready-to-run examples are [sine braid](../profiles/sine_braid.lua), [furnace panel](../profiles/furnace_manual.lua), [guided furnace scenarios](../profiles/furnace_scenarios.lua), and [tutorial](../profiles/tutorial.lua). The furnace demo's detailed controls are described in [furnace demo](furnace-demo.md).

## Reload and recovery

Settings lets you select, validate and load a profile. The reload path parses the candidate before changing the active experiment. It then attempts safe pause on every controller while its transport is still available. A safe-output failure aborts replacement and leaves the existing runtime available for recovery, although controllers may already be paused.

After successful safe pause, acquisition and emulator stop; the replacement runtime is built and its setup/scripts run. Successful replacement clears series/plot history, panels, Lua globals and scenarios. The process database belongs to the application session and is reused.

If candidate setup fails after the old runtime was stopped, the old runtime/series remain, but automatic operation is not restored. Resolve the error and restart deliberately. Earlier actions from a failing setup are not rolled back. A retired runtime never sends another safe-output write when dropped after its replacement starts.

The GUI remembers successfully selected profiles in `state/active-profile.txt`. Use explicit `--config startup.lua` to override a remembered choice.
