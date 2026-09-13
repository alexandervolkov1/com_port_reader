# com_port_reader

## Overview

A desktop laboratory application for acquiring serial measurements, plotting and filtering signals, running feedback controllers, and recording experiments. Lua profiles and scripts connect these pieces: you can start with a simulated instrument and later replace its measurements and outputs with real hardware.

Typical uses include logging a serial sensor, comparing raw and filtered signals, experimenting with PID control, and automating a heating experiment with a control panel and timed or measurement-driven stages.

## Download and run (Windows x64)

No Rust, C++ build tools, separate Lua installation or physical instrument is
needed to try the ready-made application.

1. Open [GitHub Releases](https://github.com/alexandervolkov1/com_port_reader/releases)
   and choose the newest published release.
2. Under **Assets**, download `com_port_reader-<version>-windows-x86_64.zip`.
   **Source code (zip)** and **Source code (tar.gz)** contain source files, not
   the ready-to-run application.
3. Extract the **entire ZIP** into a writable folder, for example
   `Documents\COM Port Reader`. Do not run the executable inside the ZIP or move
   it away from its accompanying folders.
4. Open the extracted folder and double-click `com_port_reader.exe`.
   The first launch starts the simulated sine-braid experiment automatically.
   No COM-port configuration is required.
5. Open **Control panel** to adjust all eight waves, or **Help → Lua reference**
   to explore the scripting functions.

The application needs an OpenGL-capable graphics driver and the
[Microsoft Visual C++ x64 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).
If Windows reports a missing `VCRUNTIME140.dll`, install that runtime.
The separate `device_emulator.exe` is an optional serial-testing tool; it is not
needed for the built-in demos.

For an optional integrity check, also download the matching `.zip.sha256` file
and compare its hash with `Get-FileHash -Algorithm SHA256 <archive.zip>` in
PowerShell. Logs and recordings are saved beside the application; keep them when
upgrading by extracting a newer release into a separate folder.

To try the guided furnace experiments, open PowerShell in the extracted folder:

```powershell
.\com_port_reader.exe --config profiles/furnace_scenarios.lua
```

Then open **Control panel → Run selected**. See the [furnace guide](docs/furnace-demo.md)
for the experiments and safe stopping.

## Main capabilities

- Raw line-oriented serial requests and a typed Metakon 5X3 driver.
- Lua-defined virtual instruments; the normal emulator uses in-process memory transport.
- Multiple connections, independently polled series, plot panes, and acquisition failure/retry tracking.
- Moving-average, median and time-based exponential filters.
- PID, on/off and model-assisted Furnace controllers with fixed or ramp references.
- Manual/automatic output arbitration and explicitly configured safe-output writes.
- Lua console, script-defined panels, scenarios and bilingual in-app Lua reference.
- SQLite measurements, actions, output results, configuration source and application logs.

## Architecture at a glance

```text
GUI / Lua commands --> ApplicationRuntime --> acquisition workers --> instruments
                              |                       |
                              |                 timestamped samples
                              |                       v
                              +--------------> processing worker
                              |                filters / controllers
                              |                       |
                              |                 output arbitration --> instrument writes
                              v
                       SQLite writer
```

Hardware connections belong to acquisition workers; controller state belongs to the processing worker. Lua uses commands and bounded request/reply waits. The GUI coordinates their lifetimes. See [architecture](docs/architecture.md) for ownership and shutdown details.

## Requirements and building

Use a stable Rust toolchain supporting edition 2024 and the native C/C++ build tools for your platform. On Windows this normally means the MSVC Rust target and Visual Studio C++ Build Tools. Lua 5.4 and SQLite are built from bundled sources; no separate Lua or SQLite installation is needed.

The main supported workflow is Windows. Real serial acquisition needs an accessible port and matching instrument settings. The memory emulator needs neither hardware nor a virtual COM driver.

From the repository root:

```powershell
cargo build --release
```

This builds the GUI `com_port_reader` and the optional standalone `device_emulator` binary. The GUI is the Cargo default-run target.

## First run: no hardware required

Use an explicit profile when running the release executable from the source tree:

```powershell
cargo run --release -- --config startup.lua
```

The supplied startup profile loads `emulator_scripts/sine_generator.lua` and `lua_scripts/sine_braid_demo.lua`. The script starts the memory emulator and acquisition, adds eight sine waves and eight exponential filters, and registers a control panel. Wait for samples to appear; open the series list to change visibility and Control panel to adjust shared amplitude, noise, period and filter time constant. Each filtered curve can be enabled separately.

In the Lua console, submit with Ctrl+Enter:

```lua
local generator = app.virtual_instrument({id = 1})
app.log(generator:name())
for _, parameter in ipairs(generator:parameters()) do
    app.log(parameter.key .. ": " .. parameter.value_type)
end
```

This discovers the running model and prints its parameter keys/types in the application log. For a guided exercise starting from an empty experiment, use:

```powershell
cargo run -- --config profiles/tutorial.lua
```

Follow the [Lua tutorial](docs/lua-tutorial.md): start the model, read and write a value, plot a series, add a filter and controller, and build a small panel.

The packaged executable searches beside itself for `startup.lua`. An unpackaged release executable searches in `target/release`, not the repository root; this is why the command above passes `--config`. Without an explicit argument, a remembered profile selection can also take precedence. See [path selection](docs/configuration.md#selection-and-paths).

## Working with real instruments

Create `profiles/hardware.lua` with a connection matching your device. This example uses a Metakon channel; change the port, address, channel and scale for your installation:

```lua
return {
    connections = {
        primary = {port = "COM3", baud_rate = 9600, timeout = 0.25},
    },
    setup = function()
        local meter = app.metakon({connection = "primary", device = 1, channel = 0, scale = 1})
        meter:add("measurement", {name = "temperature", interval = 1})
        app.start()
    end,
}
```

Launch `cargo run -- --config profiles/hardware.lua`. This example only reads; adding an actuator requires explicit output configuration and safety review. A generic line-oriented device can use `app.add_serial("READ?", "temperature")` instead. Unsupported binary protocols require a Rust driver; `send_serial` is not a general binary protocol API.

Check the [Metakon parameter reference](docs/lua-api.md#metakon-5x3) and [serial troubleshooting](docs/troubleshooting.md). Physical-port behavior cannot be verified by the emulator tests.

## Configuration and Lua automation

Profiles return a table with `application`, `connections`, optional `emulator`, `plot_panes`, `scripts` and `setup`. Relative model/script paths are based on the profile directory. Put side effects in `setup`, not at the profile top level: validation evaluates the profile before the runtime installs `app`. Setup runs before the listed scripts.

Scripts can retain instrument/controller handles, register panels and schedule scenarios. Functions that enqueue actions return before completion; operations such as instrument reads wait for a result. Error and execution-limit behavior is described in the [Lua reference](docs/lua-api.md). Editor annotations are in `lua_types/app.d.lua`.

Use Settings to select/reload a profile. Reload validates first, attempts safe controller outputs, stops the old runtime, then initializes the replacement. It is not a transaction: a failure during replacement setup does not restart the previous experiment automatically.

## Signal processing and controllers

Filters add derived series without changing the input. Moving-average and median windows count samples; the exponential filter uses elapsed seconds.

Controller constructors install a running loop immediately. They consume an existing raw or filtered numeric series and write a compatible actuator parameter. Use `controller:add("output")` to plot a diagnostic; `add` does not install the controller.

PID uses derivative on measurement and conditional-integration anti-windup. On/off uses hysteresis. Furnace combines a heat-loss model with lag prediction and PI correction. [Controller documentation](docs/controllers.md) gives equations, units, validation, reference and reset semantics.

## Output safety

Set `safe_output` explicitly for each controller. There is no inferred safe value. Pause and removal attempt this write; reload and normal application destruction also attempt to make registered controller outputs safe while transports are still available.

Manual writes change output ownership. Resuming requests automatic ownership; the first successful automatic write completes takeover. A queued or failed write is not proof of physical actuator state. `app.stop()` stops polling; it is **not** an emergency stop or a safe-output command.

Use hardware interlocks and independent shutdown mechanisms for hazardous equipment. Abrupt termination, unavailable hardware and failed writes cannot be made safe by this application. Read [output safety](docs/output-safety.md) before driving an actuator.

## Process recording

Each application run attempts to create a session database beneath `processes/` in the application directory. Debug builds use the repository directory; release builds use the executable directory. The recorder persists timestamped measurements, configuration snapshots, actions, output requests/results and logs on a background thread.

Profile reloads share the application recording session. A writer failure disables recording and reports an error but does not stop acquisition or controllers. See [schema and example SQL](docs/process-recording.md).

## Included examples

| Entry point | Purpose |
| --- | --- |
| `startup.lua` / `profiles/sine_braid.lua` | Automatic eight-wave signal/filter demonstration |
| `profiles/tutorial.lua` | Empty memory furnace experiment for the progressive tutorial |
| `profiles/furnace_manual.lua` | Grouped Manual / PID / Furnace controls and diagnostic plots |
| `profiles/furnace_scenarios.lua` | Guided power steps, ramp/hold and temperature-guard experiments |
| `emulator_scripts/sine_generator.lua` | Multi-instrument sine model |
| `emulator_scripts/furnace_plant.lua` | Thermal plant with heater lag, heat losses and configurable noise |
| `lua_scripts/` | Application scripts loaded by the supplied profiles |

Run `cargo run -- --config profiles/furnace_manual.lua` for the [furnace demo](docs/furnace-demo.md). Its tuning is illustrative: the [deterministic comparison](docs/furnace-controller-comparison.md) does not establish Furnace as superior to PID.

New to the application? Run `cargo run -- --config profiles/furnace_scenarios.lua`,
open Control panel, and press Run selected for a 50-second guided experiment.
The [tutorial](docs/lua-tutorial.md#try-the-furnace-first-no-lua-typing) starts with
this no-code tour before introducing Lua.

## Repository and documentation

`src/` contains the GUI, application runtime, worker services, protocols, controllers and colocated tests. `profiles/` configures experiments; `lua_scripts/` automates the application; `emulator_scripts/` runs in the separate model-side Lua environment.

- [Configuration](docs/configuration.md): fields, defaults, ranges, profiles and paths.
- [Lua tutorial](docs/lua-tutorial.md) and [complete API reference](docs/lua-api.md): progressive workflow and per-operation examples.
- [Scenarios](docs/scenarios.md): timers, conditions, races, stages and cleanup.
- [Virtual instruments](docs/virtual-instruments.md): model contract and memory/serial transports.
- [Controllers](docs/controllers.md) and [output safety](docs/output-safety.md).
- [Process recording](docs/process-recording.md) and [troubleshooting](docs/troubleshooting.md).
- [Architecture](docs/architecture.md) and [Rust extension guides](docs/development.md).

Help → Lua reference is the concise English/Russian function lookup; the guides above explain the longer workflows.

## Development, testing and packaging

```powershell
cargo fmt --check
cargo test
cargo test --doc
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps --document-private-items
cargo build --release
git diff --check
```

Tests cover protocols, virtual instruments, processing/control, arbitration, scenarios, profile/runtime lifecycle and recording without real COM hardware. Documentation tests check Lua syntax and binding coverage; runtime tests exercise representative documented workflows.

`tools/package-release.ps1` builds both binaries with the locked dependencies,
packages profiles, scripts, types and documentation, and creates a ZIP plus
SHA-256 checksum. See the [release checklist](docs/development.md#github-release-checklist)
and [changelog](CHANGELOG.md). Previous local packages are preserved under `dist`.

The sine-braid Control panel adjusts noise, outer amplitude, period and EMA time
constant for all eight waves. Amplitude keeps their original proportions and
blends over twenty seconds, as does the noise envelope. Period changes preserve the
current phase; EMA retuning preserves accumulated output. Random noise still
varies sample by sample. Individual toggles show/hide each filtered curve.

## Known limitations

This is not a real-time or safety-certified control system. GUI stalls, OS scheduling, transport failures and blocking Lua/native calls affect timing. Lua instruction limits do not interrupt native calls. Plot history can grow during a long experiment; display downsampling is not storage truncation.

SQLite failure is nonfatal, and failed physical output writes require operator recovery. The optional serial emulator is an integration/debug path, not a quick-start prerequisite. Automated tests do not replace a real-hardware acceptance test.
