# Configuration profiles

A profile is a Lua file that returns a table. `startup.lua` is used by default; pass `--config path/to/profile.lua` to choose another file. Relative `scripts` and `emulator.script` paths resolve from the selected profile's directory.

```lua
return {
  application = { fps = 20, poll_interval = 1.0, plot_window = 3600, max_plot_points_per_series = 1000 },
  emulator = { transport = "memory", script = "emulator_scripts/sine_generator.lua" },
  scripts = { "lua_scripts/demo_virtual_sine.lua" },
}
```

## Root fields

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `application` | table | No | Runtime display and polling settings. |
| `connections` | table | No | Named serial connection definitions. |
| `emulator` | table | No | One virtual-instrument emulator. |
| `scripts` | array | No | Lua application-script paths. |
| `plot_panes` | array | No | Initial plot layout. |
| `setup` | function | No | Runs after the runtime is initialized. |

Unknown fields are rejected. Missing sections retain internal defaults where applicable.

## `application`

| Field | Type | Default | Constraints |
| --- | --- | --- | --- |
| `fps` | integer | 30 | 1 through 240. |
| `poll_interval` | seconds | 1 | Greater than zero. |
| `plot_window` | seconds | 3600 | 1 through 1,209,600. |
| `max_plot_points_per_series` | integer | 4000 | 4 through 100,000. |

## `connections`

The table must include `primary` whenever it is non-empty. Other keys are connection names. Names are case-insensitively unique and COM ports cannot be reused.

```lua
connections = {
  primary = {
    port = "COM5", baud_rate = 9600, data_bits = 8,
    parity = "none", stop_bits = 1, flow_control = "none", timeout = 0.25,
  },
}
```

`port` is required. Defaults are `baud_rate = 9600`, `data_bits = 8`, `parity = "none"`, `stop_bits = 1`, `flow_control = "none"`, and `timeout = 0.25` seconds. Valid parity values are `none`, `odd`, and `even`; valid flow-control values are `none`, `software`, and `hardware`.

## `emulator`

`script` is required. `transport` is `memory` by default when no serial fields are supplied.

```lua
emulator = { transport = "memory", script = "../emulator_scripts/pid_thermal_plant.lua" }
```

For serial integration, set `transport = "serial"`, `connection` to a configured connection name, and `port` to the emulator endpoint. The emulator port must not be used by any connection.

## Scripts and panes

`scripts` is a contiguous array of non-empty paths. Each pane has required `id`, optional `title` (defaults to `id`), and optional positive `weight` (defaults to `1.0`).

## Reloading

The Settings UI can validate and reload the selected profile. The new configuration is fully parsed and validated before it replaces the active runtime. The active profile is remembered in `state/active-profile.txt` under the application directory.
