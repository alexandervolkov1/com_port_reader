# Troubleshooting

Start with the application log, console error and selected profile path in Settings. Distinguish an enqueued command from an applied action. A green controller state alone does not confirm output ownership or a successful hardware write.

| Symptom | Likely cause | Diagnose | Recovery |
| --- | --- | --- | --- |
| `COM port is not selected` | No connection configured for a serial operation | Check `connections.primary` and the handle's connection option | Select a valid profile or configure the named connection. Memory virtual instruments do not require a port. |
| `Failed to open COM port` | Port absent, busy or inaccessible | Check Device Manager, port name and whether another program owns it | Close the competing application, reconnect and retry/reload. |
| Serial timeout or invalid frame | Wrong baud/parity/stop bits, address, wiring or protocol | Compare all settings with the instrument; first try one explicit read | Correct configuration; increase timeout only when expected device latency warrants it. |
| Metakon channel identification failure | Wrong device/channel or incompatible hardware | Check device address, channel and expected channel type 3 | Correct address/model selection. Handle construction alone does not identify hardware. |
| Series marked Offline | Three failed polling cycles | Read the preceding errors; other series may still be working | Repair the underlying cause then `app.retry("series_name")` or `app.retry_all()`. |
| Metakon `alarm value -32768` | Thermocouple/sensor fault | Inspect sensor and wiring; read `measurement` | Repair the sensor, then a successful manual read or explicit retry restores polling. Do not interpret -32768 as temperature. |
| Wrong values by a constant factor | Incorrect engineering scale/unit assumption | Inspect `meter:parameters()`; integral time is always minutes | Set measurement scale correctly; do not apply that scale to integral time. |
| `Unknown ... option` / profile validation error | Typo, misplaced field, wrong type or invalid bounds | Compare exact key and section in [configuration](configuration.md) | Correct the profile and Validate again. Top-level code must return a table. |
| Profile validation stalls then times out | Busy Lua at profile top level | Inspect loops/computation; the instruction hook reports exceeded execution time | Keep profiles declarative; use setup for bounded application actions. Native calls are not preemptible. |
| Lua syntax/runtime error | Invalid syntax, unknown method, missing handle or failed operation | Read console history and action log; check `local` scope between submissions | Correct syntax/API; use global handles across console chunks. Ordinary execution errors do not require restart. |
| `Lua setup failed` / application script initialization failure | Startup script failed after API installation | Inspect setup and scripts in execution order | Correct and reload. Prior commands may already have acted; no hardware rollback is implied. |
| Demo does not appear in release mode | Unpackaged executable searches its own directory, or remembered profile overrides default | Check Settings profile path | Run `cargo run --release -- --config startup.lua` from the repository root. |
| Emulator startup failure | Missing model, invalid catalog/handlers or occupied serial endpoint | Check profile-relative model path and first startup error | Fix model schema/path; use `transport = "memory"` for local use. |
| `Local emulator is stopped` | Model not started, explicitly stopped or disconnected | Check emulator log and selected transport | `app.start_emu()`, rediscover if catalog changed, then retry polling. |
| Local timeout / restart-required transport error | Slow model, late response or corrupted stream | Inspect model execution time and full error text | A recoverable timeout drains the old reply; never blindly repeat actuator writes. Restart for a closed session. |
| `Output is already registered` | Two controllers target the same physical parameter | Check existing controller names and targets | Safely remove the existing loop before creating another. Pausing alone retains its registration. |
| Controller `running` but output rejected | Manual override changed ownership | Check output-service log and latest manual write | Use `loop:resume()` when automatic operation is intended. |
| Missing configured safe output | Constructor omitted `safe_output` | Review controller creation | Recreate with an appropriate explicit safe value after placing the equipment in its intended state. |
| Safe-output write failed | Transport/device rejected or did not acknowledge the value | Controller can already be paused; inspect device and error | Restore communication and retry pause/removal. Do not assume hardware reached the safe value. |
| Reload aborted on safe output | Existing actuator could not be safely paused | The previous runtime and transport remain available | Resolve the failed write and retry reload. Other controllers may already be paused. |
| Reload setup failed after stopping old runtime | Candidate validates but fails during initialization | Read setup/script error; old series can still be present | Correct the candidate. Restart the old experiment deliberately if needed; automatic state is not restored. |
| Control panel unavailable | No script registered a panel | Inspect `app.register_script` execution and duplicate-ID errors | Register a valid panel; unregister before rerunning the same ID. |
| Callback missing/ambiguous | Name not in script table, or duplicated scenario callback names | Check callback strings and registered functions | Use unique names and retain the script registration. |
| Plot pane error | Lua uses a key absent from profile layout | Compare `pane` with `plot_panes[].id` | Declare the pane in the profile or use an existing key. |
| SQLite recording disabled | Directory permissions, disk space, locked/unwritable storage | Read database path and the first recorder error in the log | Fix storage and restart for a new session; missing records cannot be reconstructed automatically. |
| Slow close | In-flight I/O, queued Lua work or slow model/native call | Identify the last device action and configured timeout | Let bounded operations finish; fix blocking model code/settings before the next run. |

## Useful console checks

```lua
return plant:parameters()
```

Lists model keys and access; requires a discovered handle.

```lua
return loop:state()
```

Reports computation state only.

```lua
return loop:read("setpoint")
```

Reads the current effective setpoint; a managed ramp's target is separate.

```lua
app.retry_all()
```

Resumes suspended polls without replacing the experiment.

## Reporting a problem

Include the exact error, active profile, relevant small Lua snippet, whether transport is memory or serial, steps to reproduce, and build/commit information. For serial faults include device/channel and baud/parity/stop bits. For recording faults include the failing path and available disk space. Avoid sending unrelated experiment data.
