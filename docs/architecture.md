# Architecture

## Runtime composition

`ApplicationRuntime` is the composition root. It owns Lua execution, acquisition workers, the processing service, output-control service, process recorder, and the optional emulator. GUI code submits commands to this runtime; it does not perform serial I/O.

```text
Lua / GUI --> ApplicationRuntime --> command dispatcher
                                  |       |
                                  |       +--> recorder timeline
                                  v
                    connection workers (one per connection)
                         |                  |
                    AcquisitionSource   worker events
                         |
              serial source + local virtual source

worker samples --> signal-processing service --> controller outputs --> output-control service --> worker writes
```

`CombinedSource` routes each request to the source that supports it. This lets a local virtual instrument and a serial source coexist on a connection worker. Each worker exclusively owns its sources, which prevents concurrent access to a serial transport.

## Workers and processing

Workers schedule polling independently per connection. A series may override the default polling interval. After three consecutive failures only that series is suspended; retrying it resets its failure state. Samples are sent to the processing service, which owns the filter graph and control-loop registry on its own thread.

Controllers do not write devices directly. They produce output intents; output control validates ownership and forwards approved writes to the relevant worker. This separation makes manual override and safe-output transitions explicit.

## Emulator lifecycle

The memory emulator creates an in-process client/server transport for a Lua model. Starting it establishes a new session; stopping it closes that session. A timeout discards the session before a later request, preventing a late response from being applied to a new request. Serial emulator mode instead runs the same protocol against a configured serial port.

## Startup, reload, and shutdown

A profile is parsed and validated into an `ApplicationDefinition` before a replacement runtime is built. A failed rebuild leaves the active runtime intact. Relative profile assets resolve from the profile directory, while application state and process databases resolve from the application directory.

During shutdown the runtime stops dispatching work, requests worker/service shutdown, and joins owned threads. Handles use channels to report service disconnection rather than accessing stopped thread state.
