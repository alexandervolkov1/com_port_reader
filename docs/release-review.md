# Release review

This is the working audit record for the review requested in `PLAN.md`.
The baseline is `1276db0`; the reviewed branch initially ended at `ca25185`.
The user's pre-existing changes to `PLAN.md` are preserved.

## Baseline

- `cargo test`: 718 passed, no failures; no existing doctests.
- Documentation/source cleanup, language changes, demo consolidation and release packaging all fall within the comparison range.
- There are 124 Rust source files at the start of the review.

## Coverage inventory

| Area | Implementation / contract to review | Documentation destination |
| --- | --- | --- |
| Composition and lifecycle | `application_runtime`, `app`, paths, profile parsing, Lua initialization, reload and destruction order | architecture, configuration |
| Acquisition | `AcquisitionSource`, combined/local/serial sources, per-connection workers, poll health and retry | architecture, troubleshooting |
| Instruments | typed requests/addresses/values, Metakon registers and protocol, virtual descriptors and transports | Lua API, virtual instruments, development |
| Processing | signal graph, filters, service commands, dependent controller/diagnostic removal | architecture, controllers |
| Control | PID, on/off, furnace, references, registry, instance identity, output conversion | controllers, development |
| Output arbitration | registration, manual/pending/automatic, tracked completions, safe writes, rollback, removal | output safety |
| Lua application | app functions, instrument/controller/scenario userdata, callbacks, validation and execution limits | Lua API, tutorials |
| UI declarations | script registry, four widgets, enabled state, plot panes and series presentation | Lua API, configuration |
| Scenarios | timers, predicates, races, stages, action completion, finalization, cancellation | scenarios |
| Recording | event publication, writer ownership, schema, failure and session lifetime | process recording |
| Presentation | GUI views/models, plots/downsampling, console, Help languages | README, Help |
| Distribution | both binaries, profiles, models, scripts, editor types, packaging | README, development |

The binding inventory includes `src/lua_api/*.rs` **and** `src/lua_application_script.rs`.
Model-side globals come from `src/lua_virtual_instrument_model.rs`.
Editor declarations in `lua_types/app.d.lua` must agree with those implementations.

## Findings being addressed

1. Metakon documentation uses the nonexistent `process_value` key instead of `measurement`.
2. Controller examples call `add()` without its required diagnostic key and describe it as installation.
3. Documentation exposes an `add_diagnostic` Lua method that is not registered; the actual method is `add`.
4. README release quick start omits the explicit profile needed when running an unpackaged executable from `target/release`.
5. Output-arbitration rustdoc incorrectly describes safe writes during automatic takeover and overstates failure guarantees.
6. Profile-reload documentation overstates transactional behavior: the current runtime is stopped before replacement validation/initialization.
7. Application runtime destruction has no explicit controller-safe-output phase; lifecycle behavior needs correction or an explicit release limitation supported by tests.
8. Lua API coverage is incomplete for scenarios, widgets, references, diagnostics and per-operation examples.

## Completion status

Review and implementation are in progress. This record does not yet assert release readiness.
