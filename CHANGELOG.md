# Changelog

## 0.1.0

Initial Windows x86-64 release.

- Serial acquisition, Metakon 5X3 support and Lua-defined virtual instruments.
- Hardware-free startup demo with raw and filtered signal plots.
- PID, on/off and predictive Furnace controllers with references and diagnostics.
- Grouped furnace controls plus guided power-step, ramp/hold and temperature-guard experiments.
- Lua scenarios with timers, measurement conditions, races and staged workflows.
- SQLite measurement/action recording, application logs and configurable plot panes.
- Searchable English/Russian Lua Help, tutorials and API/development guides.
- Safer controller pause/removal, runtime shutdown and profile replacement.

This release is not safety-certified or hard real-time. Real COM/Metakon equipment
has not been acceptance-tested as part of this release preparation. Use independent
hardware protection for physical heating or other hazardous actuators.
