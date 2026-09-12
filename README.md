# com_port_reader

`com_port_reader` is a desktop application for acquiring measurements from serial instruments, processing signals, running control loops, and recording an experiment. It includes Lua automation and a built-in virtual-instrument emulator, so the default demo needs no physical hardware or virtual COM driver.

## Overview

Profiles configure the runtime and load Lua application scripts. Scripts define measurements, filters, controllers, control panels, and scenarios. The GUI visualizes series, logs actions, and provides a Lua console.

## Features

- Serial request/response acquisition and Metakon 5x3 support.
- Lua-backed virtual instruments over the default in-memory transport.
- Moving-average, median, and exponential filters.
- PID, on/off, and model-assisted furnace controllers.
- Safety-aware manual and automatic output ownership.
- SQLite process history for measurements, actions, outputs, and logs.

## Architecture

```text
GUI and Lua scripts
        |
        v
ApplicationRuntime ----> process recorder (SQLite)
        |
        +--> per-connection acquisition workers --> serial / virtual instruments
        |
        +--> signal-processing worker --> controllers --> output-control service --> instrument writes
```

See [architecture](docs/architecture.md) for lifecycle and ownership details.

## Requirements

- A current stable Rust toolchain.
- Windows for the usual COM-port workflow. The built-in memory emulator works without a port.

## Building

```powershell
cargo build --release
```

## Quick start

From the repository root, launch the supplied memory-emulator profile:

```powershell
cargo run --release
```

`startup.lua` selects `emulator_scripts/sine_generator.lua` and `lua_scripts/sine_braid_demo.lua`. It starts an eight-wave memory-emulator braid with independent configurable exponential filters. No virtual COM configuration is required.

To launch a specific profile, pass its path explicitly:

```powershell
cargo run --release -- --config profiles/furnace_manual.lua
```

## Built-in emulator

Profiles with `emulator = { transport = "memory", ... }` run a Lua model in process. The optional `serial` transport is for integrations that require a serial endpoint. See [virtual instruments](docs/virtual-instruments.md).

## Real instruments

Define a `connections` table in a profile, then use `app.metakon` or `app.send_serial` from a script. Do not configure the same COM port for both a connection and a serial emulator. See [configuration](docs/configuration.md).

## Configuration profiles

Profiles are Lua tables. The shipped examples in `profiles/` are the canonical starting points. Relative profile paths resolve from the profile file; process data and application state resolve from the application directory.

## Lua scripting

The global `app` table starts and stops acquisition, defines series and filters, accesses instruments, and creates controllers. The API is asynchronous for hardware operations: commands are sent to the runtime rather than performed on the Lua thread. See [Lua API](docs/lua-api.md).

## Controllers

PID, on/off, and furnace controllers process timestamped measurements in the background. Output control separates manual writes from automatic ownership and uses a configured safe value when entering automatic control. See [controllers](docs/controllers.md).

## Process recording

Each run attempts to create an SQLite database under `processes/`. Recording failures are visible in the application log and do not stop acquisition or control. See [process recording](docs/process-recording.md).

## Project structure

- `src/` — application, runtime, device protocols, UI, and tests.
- `profiles/` — ready-to-run profile configurations.
- `lua_scripts/` — application scripts and demos.
- `emulator_scripts/` — virtual-instrument models.
- `docs/` — architecture and operating guides.

## Development

```powershell
cargo fmt --check
cargo test
cargo test --doc
cargo clippy --all-targets -- -D warnings
```

See [development](docs/development.md) for extension boundaries and [troubleshooting](docs/troubleshooting.md) for operational failures.

## Release status

The project includes an in-memory demo path and tested protocol/controller components. Real serial operation still requires matching hardware settings and is not exercised by automated hardware tests.
