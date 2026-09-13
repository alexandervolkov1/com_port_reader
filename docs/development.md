# Development and extension guide

Read [architecture](architecture.md) first. This project has a library plus two binaries. `com_port_reader` is the default GUI binary; `device_emulator` is the optional standalone serial server. Edition 2024 is declared in Cargo.toml; an exact minimum supported compiler version is not currently pinned. Use a working stable Rust toolchain and the committed Cargo.lock.

## Build, tests and distribution

From the repository root:

```powershell
cargo fmt --check
cargo test
cargo test --doc
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps --document-private-items
cargo build --release
git diff --check
```

Tests live alongside owning modules; protocol/model tests do not require hardware. Runtime integration tests exercise memory emulation, output safety, profiles and Lua. Documentation tests compile Lua blocks and check API coverage; the tutorial is exercised through the runtime.

Generate private-item rustdoc because much of the application's important API is internal. Start at `target/doc/com_port_reader/index.html` and follow module pages. Keep rustdoc on important private functions as well as public types.

`tools/package-release.ps1` builds both release binaries with `--locked` and copies
startup, profiles, scripts, models, editor declarations, changelog and docs into
`dist`. It requires an x86-64 MSVC host toolchain. The optional `-Version` must
match Cargo.toml; otherwise Cargo metadata supplies the version. `-SkipChecks`
skips its pre-build checks. An existing matching package/archive/checksum is moved
into a `.previous-*` directory under `dist`, preserving its measurements and logs.
The new ZIP excludes runtime data and has a companion SHA-256 file.

### GitHub release checklist

1. Keep Cargo.toml, Cargo.lock and [CHANGELOG](../CHANGELOG.md) on the same version.
   Review licensing and the hardware limitations before publishing.
2. Run the checks above and the real-time furnace acceptance test:
   `cargo test --locked shipped_furnace_scenarios_complete -- --ignored`.
3. Run `powershell -File tools/package-release.ps1`. Do not use `-SkipChecks`
   for the final release unless the same source has already passed the checks.
4. Extract the ZIP into a new writable folder. Launch `com_port_reader.exe`
   without arguments from a different working directory; verify the startup
   signals, Help links and normal closing. Try both furnace profiles via
   `--config` with an absolute profile path.
5. Commit the verified source. On GitHub, create a release for that commit using
   the matching `v<version>` tag, use the changelog as release notes, and attach
   the ZIP and `.zip.sha256`. The packaging script does not push, tag or publish.

After downloading, compare `Get-FileHash -Algorithm SHA256 <archive.zip>` with
the companion checksum. Keep the entire extracted directory together; the
executables need the supplied Lua resources and documentation.

Standalone serial emulator example:

```powershell
cargo run --release --bin device_emulator -- COM6 emulator_scripts/sine_generator.lua 9600
```

Arguments are port, model path, optional baud (default 9600); framing is 8N1 with no flow control. Press Enter to stop. A separate application connection must use the linked client port. This workflow requires port infrastructure; memory tutorials do not.

## Add a real instrument

The following steps describe adding a second real device family, using Metakon as the existing reference. The compiler's exhaustive-match errors help locate enum dispatch points, but they do not replace semantic tests.

### 1. Identity, parameters and descriptors

Create `src/instrument/my_device.rs` and register/re-export it from `src/instrument.rs` as needed. Define a small value type containing protocol address fields and a parameter enum. Keep parameter keys, access, type, engineering scale, range and unit semantics with this driver.

Study `Metakon5x3`, `Metakon5x3Register::descriptor`, `engineering_scale` and `Metakon5x3Write::validate`. A model fault code must be translated into an error before becoming a plotted measurement. Decide explicitly which parameters can be periodically sampled and which are actuator candidates.

Do not reuse “temperature” assumptions in common `InstrumentValue`. It represents boolean, integer and number values shared by all devices.

### 2. Protocol transactions

Add `src/protocol/my_device.rs` and declare it in `src/protocol.rs`. Encode requests and decode responses here: address matching, function/register codes, lengths, checksum and error responses. Use `protocol/metakon.rs` as a pattern for request types and typed errors.

Keep pure codecs testable without opening a port. A serial transaction can use `SerialConnection::exchange_exact` for fixed-length replies or its `Read/Write` implementation for another framing protocol. Do not force binary traffic through the text-line interface.

Choose retry semantics consciously. Reads and idempotent register assignments may tolerate retries; commands with effects such as “advance one step” may not. A timeout is not proof that a device did nothing.

### 3. Application-level addresses and requests

In `src/instrument.rs` extend `InstrumentParameterAddress`, `InstrumentReadRequest` and `InstrumentWriteRequest` with the new family. Supply constructors and update `parameter_address`, `corresponding_read_request`, naming/display and same-parameter matching.

A `ConnectedParameterAddress` adds the connection identity. Equivalent reads/writes must yield the same parameter address or manual override, retry restoration and ownership will disagree. Keep display strings separate from identity.

Return `InstrumentValue` in engineering units. Test forward and inverse conversion, representability, limits and any sentinel handling.

### 4. Existing serial acquisition path

Extend `read_instrument_value` and `write_instrument_value` in `src/acquisition/serial_command_source.rs`. Reuse its lazily opened connection and device verification cache pattern where suitable. Map driver errors into `AcquisitionError` with useful device context.

Review `src/acquisition/local_virtual_instrument_source.rs`: unrelated device variants must return `Ok(None)` so `CombinedSource` reaches serial. Review `src/worker.rs` one-shot read/write helpers and the output service's serial-port requirement. They must recognize that the new physical instrument needs a selected COM port.

No new worker or `AcquisitionSource` is needed merely for a new serial protocol. Add a backend only for a distinct acquisition/transport responsibility. If you do, implement unsupported-operation semantics and transactional start/reverse stop behavior, then inject it through `worker/serial.rs` or runtime composition.

### 5. Periodic series

Use `NewSeries::named_instrument` or `unnamed_instrument` with the new read request. `SeriesSource::Instrument` already carries typed requests, so another top-level series variant may be unnecessary.

Check `src/data/series.rs` naming and `series_store.rs` source normalization. Preserve connection ID, interval, color/visibility/pane and stable series identity. Add tests that failed reads suspend only the affected series and successful manual read/write restores its polling.

### 6. Writable output targets and conversion

Extend `ControlOutputParameter` and `ControlOutputTarget` in `src/process_control/output_target.rs` for numeric writable parameters. Expose the proper engineering range and address. Reject read-only and boolean controller targets unless you deliberately add a new supported conversion path.

Implement conversion in `output_conversion.rs`, including `write_request` and safe-value handling. Validate rounding/quantization and overflow. The same physical address must be used in the write request and arbitration target.

Route explicit writes through `InstrumentCommand::Write` and `OutputHandle`. Automatic writes must use `AutomaticOutputIntent` with controller instance identity. Never write a controlled output directly from a driver-facing Lua binding.

### 7. Lua handle and registration

Add `src/lua_api/my_device.rs`, declare it in `lua_api/mod.rs` and register the constructor in `install`. Follow `metakon.rs` for options validation, userdata, `parameters`, `add`, `read` and `write`. Bindings send `UserCommand`; they do not open ports.

The binding pattern is:

```text
Lua options -> validated device identity
parameter key -> driver parameter descriptor
read -> InstrumentCommand::Read + bounded response sender
write -> value conversion -> InstrumentCommand::Write + response sender
add -> NewSeries instrument request -> SeriesCommand::Add
pid/on_off/furnace -> ControlOutputTarget -> existing controller constructor helper
```

Reuse `LuaControllerHandle` and its generic methods. Only the new device's output-target construction is device-specific. Preserve the distinction between asynchronous installation and synchronous I/O completion.

### 8. Usable profile and editor declarations

Add a complete small profile under `profiles/` and, when useful, its application script under `lua_scripts/`. Use an obvious placeholder COM port and document how to replace it. Include at least one read, one series and one write where supported.

Update `lua_types/app.d.lua` with constructor/options/handle declarations and correct return types. This is editor assistance, not the runtime implementation; tests/documentation should catch drift.

### 9. Test the whole route

Add pure protocol encoding/decoding tests, invalid checksum/length/address cases, engineering conversion limits and sentinel faults. Add source-routing tests for supported and unsupported requests. Test periodic reads, manual writes, write readback/failure, restoration of polling and Lua option validation.

For actuator support, test ownership collisions, safe-value conversion, manual override, failed writes and a new controller instance reusing an address. Use simulated sources/protocol peers; keep real hardware checks separate.

### 10. Documentation checklist

- [ ] Protocol and driver contracts, including retries and side effects.
- [ ] Typed read/write requests and acquisition routing.
- [ ] Series behavior and output-target conversion.
- [ ] Lua constructor and every method with a minimal example.
- [ ] Editor declarations, complete profile and tests.
- [ ] README capability list, configuration/operating instructions, troubleshooting and both Help languages.

## Add a fourth controller type

### 1. Algorithm module and contract

Create `src/process_control/my_controller.rs`. Define configuration, runtime state, output structure and error type. Provide a constructor, validated configuration, `update(timestamp, measurement)`, `reset` and `resynchronize`. Keep it independent of COM, Lua, egui and channel ownership.

Decide what timestamp ordering is required, what the first sample does and which state survives reconfiguration/pause. Reject invalid numeric results without partially poisoning runtime state. PID and Furnace are useful examples of delayed state commit.

### 2. Parameter ownership and atomic updates

Keep controller-specific keys and descriptors inside the algorithm module, as `PidParameter`, `OnOffParameter` and `FurnaceParameter` do. Implement `parameters`, `parameter_values`, `read_parameter` and `configure_parameters`.

Build a complete candidate, validate all fields/cross-field constraints, then commit configuration while preserving the intended runtime state. Common dispatch should not learn a new controller's gain names. Setpoint is integrated with generic reference handling, so follow existing treatment of its descriptor.

### 3. Generic enum integration

Register the module/re-exports in `src/process_control.rs`. Extend `ControllerKind`, `Controller`, `ControllerOutput` and error variants in `controller.rs`. Add dispatch for construction conversions, parameter operations, update, reset/resynchronize and diagnostics.

Implement both current `output_range` and `output_range_after_configuration`. The latter constructs a configuration candidate without mutating the live loop; registry validation uses it before committing a change. Omitting it can allow incompatible output limits to reach hardware.

### 4. Output semantics

State the output's units and bounds. Return numeric output in the target's engineering units, not raw protocol bytes. Use existing `ControlOutputTarget` conversion when compatible; only extend it for an actual new output representation.

The controller algorithm does not choose ownership or dispatch safe writes. `safe_output` belongs to target construction; keep that distinction in Lua options and documentation.

### 5. ControlLoop integration

`ControlLoop` binds input ID, controller, target, reference and running/paused state. Its `process` skips paused loops; `update` applies a managed reference before calculating output. Ensure the new controller accepts reference setpoints through `Controller::apply_reference`.

Use the existing lifecycle unless a real algorithm requirement demands an extension. Resynchronization must not accidentally reset accumulated state meant to survive pause or input-filter replacement. Full reset and integral-only reset are different operations; report unsupported operations explicitly.

### 6. Registry and process-control services

`ControllerRegistry::add` validates output compatibility and uniqueness. `configure` validates the candidate range against the target. `process` emits `ControlEvent::Output` or errors.

The processing service commands/handle in `src/signal_processing/service/` already dispatch generic controller operations. The application `ControllerCommandHandler` resolves the input name, registers output ownership, installs the loop and rolls registration back if installation fails. Reuse this route; do not create an alternate installation path.

### 7. Ownership and safe lifecycle

The dispatcher emits `AutomaticOutputIntent` with the loop instance ID. `OutputService` owns Manual/AutomaticPending/Automatic transitions. New algorithms must not bypass this arbitration even if their calculations run in a separate helper.

Verify safe pause, resume rollback, removal, clear, profile reload and normal shutdown. A failed safe write must remain visible; a stale output from the previous instance must be rejected after replacement. [Output safety](output-safety.md) describes the exact guarantees.

### 8. Lua exposure

Add a constructor helper in `src/lua_api/controllers.rs`: validate allowed keys, construct the new algorithm, configure the safe target, enqueue `ControllerCommand::Add` and return `LuaControllerHandle`. Expose the constructor on appropriate instrument userdata in `metakon.rs` and `virtual_instrument.rs`.

Generic `parameters/read/write/configure`, references, state, pause/resume/reset, removal and diagnostic addition already belong to the common handle. Do not copy that userdata implementation. Update editor declarations and per-operation examples.

### 9. Diagnostics

Choose stable diagnostic keys. Add genuinely new keys to `ControllerDiagnostic` in `diagnostic.rs` and map them in `ControllerOutput::diagnostic`. Define the controller's supported diagnostic slice in `controller.rs`.

`ControllerCommandHandler::add_controller_diagnostic` creates the series; processing bindings produce values after computation. Test unsupported keys, removal cleanup and filtering/plotting diagnostics. Requested output is not confirmed hardware output.

### 10. Tests and documentation

Test first sample, irregular timestamps, invalid/nonfinite input, saturation, limits, configuration atomicity, state preservation, reset/resynchronize and every diagnostic. Then test registry output-range validation, processing integration, Lua constructor/options and safe lifecycle behavior.

Add a small executable example, equations checked against implementation, units/ranges, tuning assumptions and known limits in `controllers.md`. Update the Lua completeness index and bilingual Help.

- [ ] Algorithm/configuration and delayed state commit.
- [ ] Generic kind/output/error dispatch and candidate range validation.
- [ ] Reference and lifecycle semantics.
- [ ] Output arbitration, safe writes, rollback and stale-instance tests.
- [ ] Lua creation and generic handle compatibility.
- [ ] Diagnostics, editor types, examples, rustdoc and user guides.
