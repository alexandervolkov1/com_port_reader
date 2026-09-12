# Development

## Toolchain and checks

Use the Rust toolchain declared by `Cargo.toml` (edition 2024). Before a change is ready:

```powershell
cargo fmt --check
cargo test
cargo test --doc
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps --document-private-items
cargo build --release
```

## Layout and extension points

- Add an instrument driver under `src/instrument/` and its protocol under `src/protocol/`.
- Add a new acquisition backend by implementing `AcquisitionSource`; it is owned by a connection worker.
- Add filters in `src/signal_processing/` and preserve graph cycle validation.
- Add controllers in `src/process_control/`, emitting output intents instead of direct writes.
- Expose Lua functionality through `src/lua_api/`; publish commands to the runtime rather than accessing hardware from Lua.
- Add emulator models under `emulator_scripts/` and use a profile with `transport = "memory"` for local testing.

## Lifecycle rules

Do not move serial I/O onto the GUI or Lua thread. Preserve the existing direction: workers acquire, the processing service calculates, output control arbitrates, and workers write. New background services need explicit shutdown and joining behavior. Profile reload must validate/build a replacement before replacing a healthy runtime.

## Tests

Prefer unit tests alongside the component that owns an invariant. High-value coverage includes protocol framing, worker suspension/retry, controller transitions, output ownership, emulator lifecycle, and profile reload. The existing profiles and Lua demos are integration examples; avoid duplicating their large scripts in documentation.
