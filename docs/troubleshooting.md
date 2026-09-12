# Troubleshooting

## COM port cannot open

**Likely cause:** another program owns the port, its name is wrong, or profile settings do not match the device.

**Diagnosis and recovery:** check the application log, close competing software, verify `port`, baud rate, data bits, parity, stop bits, flow control, and timeout in the profile. Do not assign an emulator port to a normal connection.

## Instrument is offline or polling is suspended

**Likely cause:** repeated request failures, cabling/device power, or an incorrect device/channel.

**Diagnosis and recovery:** the worker suspends only the failing series after three consecutive failures. Correct the cause, then use the UI or `app.retry(name)` / `app.retry_all()` to resume polling.

## Metakon sensor fault or invalid value

**Likely cause:** the device reports a fault, an unsupported register is requested, or the configured scale is wrong.

**Diagnosis and recovery:** inspect the log for the instrument response, confirm device/channel and parameter key, and check sensor wiring and Metakon configuration before resuming control.

## Emulator does not start

**Likely cause:** the profile has no emulator, its script path is invalid, or the Lua model fails validation.

**Diagnosis and recovery:** use a memory profile such as `startup.lua`; verify the model defines `instruments`, `read`, and `write`. Check the script path relative to the profile. For serial mode, also verify the configured port.

## Lua error

**Likely cause:** an invalid API option, an exception in a callback, or a long-running Lua operation.

**Diagnosis and recovery:** read the Lua console/application log, compare calls with [Lua API](lua-api.md), and keep callbacks bounded. A Lua execution failure is reported without intentionally terminating the runtime.

## Invalid profile or reload failure

**Likely cause:** an unknown field, invalid numeric range, duplicated connection/port, or a missing asset.

**Diagnosis and recovery:** validate the profile in Settings, then use the exact error message with [configuration](configuration.md). The previous runtime remains active after a failed rebuild.

## Controller ownership or safe-output failure

**Likely cause:** the safe write failed, the output is manually owned, or the controller target/range is invalid.

**Diagnosis and recovery:** stop or correct manual activity, confirm the target can accept the configured `safe_output`, inspect connection errors, then resume the controller only after the safe transition succeeds.

## SQLite recorder failure

**Likely cause:** the `processes/` directory is not writable, disk space is exhausted, or the database is locked/damaged.

**Diagnosis and recovery:** preserve the reported path/log, fix filesystem access or choose a writable application directory, and restart. Acquisition and control continue even while recording is disabled.
