# PART 2 — Astra

The first documentation/source-quality pass is complete.

Now perform a deep release review of the entire current branch.

This is not only a code review.

The existing documentation has been reviewed by the user and is considered **too shallow**.

This stage must therefore perform two jobs:

```text
1. deep technical and release review
2. substantial documentation improvement
```

The final documentation must be detailed enough that:

```text
a new user can operate the application without knowing its history

a Lua user can discover and use every supported Lua capability from the docs

a Rust developer can understand the important architecture and extend the project
without reconstructing it from source code and git history
```

Do NOT assume the previous documentation pass is sufficient merely because all checks are green.

Do NOT rewrite working architecture unnecessarily.

---

# 1. Review the entire previous documentation/source-quality diff first

Start with:

```powershell
git status
git log --oneline -20
```

Identify the commit before the documentation pass and inspect:

```powershell
git diff <commit-before-documentation-pass>..HEAD
```

Review:

```text
source moves
visibility changes
renames
rustdoc
ordinary comments
README
docs/
Lua documentation
examples
in-app help
```

Pay particular attention to accidental behavior changes introduced during source cleanup.

Do not preserve a previous change merely because it was intentional.

Keep it only if it improves the current codebase.

---

# 2. Build a documentation coverage inventory before rewriting docs

Before editing documentation, inspect the current repository and create an internal inventory.

At minimum inspect:

```text
src/
profiles/
lua_scripts/
emulator_scripts/
README.md
docs/
Cargo.toml
```

For Rust, identify:

```text
major modules
important public types
important internal types
important functions
critical state machines
thread boundaries
safety invariants
extension points
```

For Lua, build an exhaustive list of the currently exposed Lua API from the actual implementation.

Do not derive this list only from existing documentation.

Inspect the actual Lua API registration/bindings.

The inventory must cover every supported Lua feature, including all important:

```text
global app functions
series operations
filters
serial commands
instrument APIs
Metakon APIs
virtual instrument APIs
controller creation
controller operations
controller configuration
references
diagnostics
control panels
application scripts
emulator commands
logging
profile setup
```

If an API exists in code but is absent from documentation, document it.

If documentation describes an API that no longer exists, remove or correct it.

---

# 3. Language policy

All repository documentation should be in English:

```text
README
docs/
Rust rustdoc
Rust source comments
developer documentation
Lua examples
configuration examples
architecture documentation
troubleshooting
test comments where comments are useful
```

The existing Russian in-application help is the one intentional exception.

Preserve it.

The Russian and English in-application help must describe the same current behavior.

Do not introduce Russian developer comments.

---

# 4. Deepen the README substantially

The README must be a real entry point to the project, not merely a project summary.

It should answer:

```text
What is this application?
What problem does it solve?
What can it control?
What can it record?
How is it structured?
How do I run it without hardware?
How do I connect real hardware?
Where do I configure it?
Where do I write automation logic?
Where is process data stored?
Where is the full documentation?
```

Recommended structure:

```markdown
# com_port_reader

## Overview

## Main capabilities

## Typical use cases

## Architecture at a glance

## Requirements

## Building

## First run

## Quick start with the built-in emulator

## Working with real instruments

## Configuration profiles

## Lua automation

## Signal processing

## Controllers

## Output safety

## Process recording

## Included examples

## Repository structure

## Documentation

## Development

## Testing

## Troubleshooting

## Known limitations
```

Keep README readable.

Detailed API reference belongs in `docs/`, but README must be complete enough to orient a new user.

---

# 5. Verify the README Quick Start from scratch

The normal quick-start path must require no external virtual COM driver.

Verify the actual commands and files.

The documented path should allow a fresh user to:

```text
build the project
launch it
load/use a local emulator
start acquisition
see a signal
interact with a virtual instrument
```

Do not invent paths or commands.

Verify them against the repository.

---

# 6. Make the Lua documentation comprehensive

The Lua documentation must become significantly more detailed.

The user specifically requires:

> every Lua capability should have a simple example.

Treat this as a release requirement.

Create or expand:

```text
docs/lua-api.md
```

so that the Lua API can be learned without reading Rust code.

Organize it by concepts and workflows.

---

# 7. Every Lua capability must have a minimal example

For every user-facing Lua operation or method, provide the simplest useful example.

Examples should be deliberately small.

Do not use one huge furnace script as the explanation for twenty different API operations.

Prefer:

```lua
app.start()
```

with a short explanation.

Then:

```lua
app.stop()
```

Then:

```lua
app.log("Experiment started")
```

etc.

If an operation needs setup context, provide the smallest valid surrounding example.

The goal is:

```text
one capability
one obvious example
one short explanation
```

Larger complete examples may be included separately.

---

# 8. Lua API documentation coverage

Verify and document the actual current API.

At minimum inspect whether the following categories exist and document all supported members.

## Application lifecycle

Examples for operations such as:

```lua
app.start()
app.stop()
app.clear()
```

Explain exactly what each operation affects.

---

## Logging

Example:

```lua
app.log("Heating stage started")
```

Explain where the message appears and whether it is recorded.

---

## Series management

Document all supported operations such as:

```text
adding a series
deleting
renaming
visibility if exposed
color
retry
retry_all
polling interval
```

Provide simple examples for each.

---

## Filters

Document:

```text
adding filtered signals
available filter kinds
changing an existing filter
filter parameters
```

Provide a minimal example for every supported filter type.

Do not merely provide one generic table if different filters have different semantics.

---

## Raw serial commands

Document the currently supported serial-command interface.

Include:

```lua
-- minimal example
```

and explain:

```text
connection selection
response
errors
timeouts if relevant
```

---

## Virtual instruments

Document the complete virtual-instrument handle API.

Provide separate examples for:

```text
discover/create handle
id
name
parameters
add series
read
write
create PID controller
create On/Off controller
create Furnace controller if supported through this API
```

Use the actual current methods.

Do not document speculative methods.

---

## Metakon 5X3

Document the complete currently exposed Metakon API.

Include:

```text
construction/discovery
parameters()
add()
read()
write()
controller creation
scaling
special measurement fault behavior
integral-time unit behavior
```

Provide the simplest possible example for every operation.

Also include a parameter table with:

```text
key
meaning
access
unit
special behavior
```

derived from actual code.

---

## Controllers

Document all supported controller kinds:

```text
PID
On/Off
Furnace
```

For each, provide:

```text
minimal creation example
minimal configuration example
read parameter example
write parameter example
pause
resume
state
reset
controller-specific reset operation
reference configuration
diagnostic series
```

Only include operations that the actual current API supports.

---

## Controller parameter inspection

Document:

```lua
controller:parameters()
```

or the actual equivalent.

Explain the returned descriptors.

Provide an example that prints or accesses them.

---

## Controller state

Document the state values returned by the API.

Provide an example:

```lua
local state = controller:state()
```

Explain what each possible state means.

---

## References

Document every currently supported reference mode.

For example, if current code supports:

```text
fixed reference
ramp reference
```

give separate examples for each.

Explain:

```text
units
rate semantics
start behavior
configuration
reading current reference
```

---

## Diagnostics

Document how to discover and add controller diagnostics.

For PID, examples may include actual supported diagnostics such as:

```text
P
I
D
output
unconstrained output
```

but verify names from implementation.

Do the same for other controllers.

---

## Control panels

Document the declarative panel system completely.

Provide minimal examples for every supported control kind:

```text
readout
number
toggle
button
```

Each must have its own short example.

Explain:

```text
script registration
panel IDs
control IDs
callbacks
on_change
on_click
app.set_control()
callback lifetime
unregistering
```

Also provide one complete small panel example combining several controls.

---

## Emulator control

Document:

```lua
app.start_emu()
app.stop_emu()
```

with simple examples.

Explain lifecycle behavior and local/in-memory transport.

If serial emulator mode remains supported, clearly mark it as an optional integration/debug path.

---

## Profile setup

Document:

```lua
setup = function()
    ...
end
```

with a minimal complete startup profile.

Explain:

```text
when setup executes
side effects
scripts order
relative paths
runtime reload
```

---

# 9. Create a Lua API completeness table

At the end of the Lua API review, maintain a table or equivalent checklist covering every exposed API member.

For example:

```markdown
| API | Purpose | Example | Detailed section |
|---|---|---|---|
| `app.start()` | Start acquisition | Yes | ... |
| `app.stop()` | Stop acquisition | Yes | ... |
| ... | ... | ... | ... |
```

This is intended to catch missing documentation.

Do not ship a Lua feature without at least a simple example.

---

# 10. Provide progressive Lua tutorials, not only reference material

In addition to the complete API reference, add a small learning path.

For example:

```text
1. log a message
2. start the emulator
3. obtain a virtual instrument
4. read one value
5. add a plotted series
6. write one parameter
7. add a filter
8. create a controller
9. expose a diagnostic
10. create a small control panel
```

This may be a section of `docs/lua-api.md` or a separate guide if it becomes large.

The examples must build gradually.

---

# 11. Improve `docs/configuration.md`

Configuration documentation must be detailed enough that users do not need to inspect the parser.

For every field include:

```text
field
type
required/optional
default
allowed range or values
meaning
example
```

Cover:

```text
application
connections
emulator
scripts
setup
memory emulator
serial emulator if retained
multiple connections
relative paths
profile selection
profile reload
```

Include complete minimal profiles.

Also include realistic complete profiles based on repository examples.

---

# 12. Improve `docs/virtual-instruments.md`

Explain the full virtual-instrument architecture.

Cover:

```text
application-side Lua environment
model-side Lua environment
why they are separate
instrument descriptors
parameter descriptors
read()
write()
time argument
model state
local memory transport
optional serial transport
start
stop
restart
profile reload
```

Provide a complete minimal virtual instrument model.

For example, something conceptually as small as:

```lua
instruments = {
    {
        name = "Example",
        parameters = {
            ...
        }
    }
}

function read(...)
    ...
end

function write(...)
    ...
end
```

Use actual required syntax.

Also explain how larger models such as the furnace emulator are structured.

---

# 13. Improve `docs/controllers.md`

Make this document substantially detailed.

For each controller:

```text
PID
On/Off
Furnace
```

explain:

```text
purpose
appropriate use cases
input
output
parameters
units
valid ranges
algorithm
diagnostics
references
reset behavior
pause/resume
output ownership
safe output
configuration examples
Lua examples
```

For PID explain at least:

```text
proportional term
integral term
derivative on measurement
anti-windup
elapsed dt
output limits
```

For On/Off explain:

```text
setpoint
hysteresis
switching thresholds
output_off
output_on
state
```

For Furnace explain:

```text
feed-forward
heat-loss model
temperature prediction
heater lag
measurement-rate filtering
PI correction
output limits
```

Include the important equations in readable Markdown.

Verify every equation against current implementation.

---

# 14. Document output safety in depth

This deserves a dedicated major section, either in `controllers.md`, `architecture.md`, or its own document.

Explain:

```text
Manual
AutomaticPending
Automatic
```

Explain transitions between them.

Document:

```text
manual writes
controller ownership
safe output
pause
resume
failed resume
rollback
controller removal
shutdown
tracked writes
```

Use a state diagram.

For example:

```text
Manual
  |
  | resume controller
  v
AutomaticPending
  |
  | first successful automatic write
  v
Automatic
```

Adjust to actual implementation.

Do not make claims that are stronger than code guarantees.

---

# 15. Improve `docs/process-recording.md`

Explain:

```text
when database is created
where it is created
session lifetime
measurements
actions
application log
configuration source
failure policy
shutdown
```

If schema is stable, document important tables and columns.

Provide useful example SQL queries where appropriate.

Examples might include:

```text
latest measurements
measurements for one series
actions during a time interval
errors from the application log
```

Only use actual schema.

---

# 16. Improve troubleshooting documentation

`docs/troubleshooting.md` should be practical.

For each problem include:

```text
symptom
likely causes
diagnostic steps
recovery
```

Cover at least:

```text
COM port unavailable
wrong serial settings
instrument read timeout
polling suspended
retry
Metakon sensor fault
Lua syntax/runtime error
profile validation error
emulator startup failure
controller cannot own output
safe-output failure
SQLite recording disabled
runtime/profile reload problem
```

Use actual error terminology from the application where useful.

---

# 17. Add detailed Rust extension documentation

The user specifically requires detailed instructions for two developer workflows:

```text
1. adding a new instrument
2. adding a new controller type
```

These should be excellent.

Put them in:

```text
docs/development.md
```

or separate documents if needed.

---

# 18. Rust guide: adding a new instrument

Document the complete sequence from concept to usable application feature.

Do not write a vague list such as:

```text
create driver
register it
add Lua bindings
```

Explain the actual project-specific path.

The guide should identify actual files/modules and explain why each step exists.

At minimum cover:

## Step 1 — Define instrument identity and parameters

Explain where:

```text
instrument type
parameter enum/type
parameter descriptors
access metadata
units
ranges
series capability
```

belong.

Use the existing Metakon or virtual-instrument driver as a reference.

---

## Step 2 — Implement protocol communication

Explain where protocol encoding/decoding belongs.

Cover:

```text
request
response
validation
error types
retry behavior if appropriate
```

Keep hardware protocol separate from application-level logic.

---

## Step 3 — Define application-level read/write requests

Explain how the new instrument becomes represented through:

```text
InstrumentReadRequest
InstrumentWriteRequest
InstrumentValue
```

or the current equivalent.

Explain required enum updates.

---

## Step 4 — Integrate with acquisition

Explain how:

```text
SerialCommandSource
AcquisitionSource
CombinedSource
worker
```

route operations.

State clearly whether the new instrument needs:

```text
existing serial source
new AcquisitionSource
new transport
```

Do not encourage a new source when an existing one is appropriate.

---

## Step 5 — Support periodic series

Explain how an instrument parameter becomes a series.

Cover:

```text
SeriesSource
series metadata
sampling interval
polling failure behavior
retry/suspension
```

---

## Step 6 — Support writes/output control

If the instrument has writable actuator parameters, explain how to create the corresponding:

```text
ControlOutputTarget
```

or current equivalent.

Explain why writes controlled by controllers must pass through output control rather than bypassing it.

---

## Step 7 — Expose it to Lua

Explain where Lua bindings belong.

Cover:

```text
handle/userdata
parameters()
add()
read()
write()
controller creation if applicable
value conversion
validation
```

Give a minimal binding-oriented example or pseudocode based on current code.

---

## Step 8 — Add profile/example

Explain how to create a small usable Lua/profile example.

---

## Step 9 — Tests

Specify required test layers:

```text
protocol tests
parameter conversion tests
read tests
write tests
source routing tests
failure tests
Lua API tests
```

---

## Step 10 — Documentation

List which docs must be updated.

Include a final checklist:

```text
[ ] protocol
[ ] driver
[ ] read/write requests
[ ] acquisition
[ ] series
[ ] output target
[ ] Lua API
[ ] tests
[ ] examples
[ ] docs
```

This section should be detailed enough that a developer can add a second real instrument without asking where functionality belongs.

---

# 19. Rust guide: adding a new controller type

Document the complete project-specific workflow.

Use PID, On/Off and Furnace as current examples.

At minimum cover:

## Step 1 — Define the controller algorithm

Explain where the controller-specific module belongs.

Document expected responsibilities:

```text
configuration
state
update(timestamp, measurement)
output
reset/resynchronize
diagnostics
```

---

## Step 2 — Define parameters

Explain where controller-specific parameter knowledge belongs.

Important architecture rule:

```text
PID parameter knowledge stays in PID
OnOff parameter knowledge stays in OnOff
Furnace parameter knowledge stays in Furnace
```

A new controller should follow the same principle.

Do not push controller-specific keys into generic dispatch code.

---

## Step 3 — Add controller kind/enum integration

Explain the generic:

```text
Controller
ControllerKind
```

integration points.

Explain what generic dispatch should and should not know.

---

## Step 4 — Define output semantics

Explain:

```text
controller output units
output limits
ControlOutputTarget
safe output
```

---

## Step 5 — Integrate with `ControlLoop`

Explain:

```text
input series
timestamp
controller update
output forwarding
controller lifecycle
diagnostics
```

---

## Step 6 — Integrate with registry/process-control service

Explain where creation/removal/configuration is handled.

---

## Step 7 — Integrate output ownership and safety

This step is mandatory.

Explain interaction with:

```text
Manual
AutomaticPending
Automatic
pause
resume
safe output
rollback
```

A new controller must not bypass output-control arbitration.

---

## Step 8 — Expose to Lua

Explain:

```text
creation method
parameter descriptors
read
write/configure
reference
state
pause/resume
reset
diagnostics
```

Document whether generic controller userdata already covers most methods.

Avoid duplicating bindings unnecessarily.

---

## Step 9 — Add diagnostics

Explain:

```text
diagnostic keys
descriptor exposure
series creation
signal processing integration
```

---

## Step 10 — Tests

At minimum:

```text
algorithm tests
parameter validation
atomic configuration
reset
state preservation
output limits
diagnostics
control-loop integration
pause/resume
safe-output integration
Lua API
```

---

## Step 11 — Docs/examples

Add:

```text
Lua creation example
configuration example
diagnostics example
controllers.md section
help updates
```

Provide a final checklist.

This guide should make it possible for a developer to implement a fourth controller type without reverse-engineering all three existing ones.

---

# 20. Add rustdoc to important functions across the ENTIRE project

This is an explicit requirement.

Perform a project-wide `.rs` file review.

Do not restrict rustdoc to public API.

Add documentation comments to **important functions and methods throughout the codebase**, including important private functions.

Use:

```rust
/// ...
```

where the function has an important contract, lifecycle role, architectural responsibility or non-obvious behavior.

Use:

```rust
//! ...
```

for important modules.

The goal is not that literally every three-line helper receives documentation.

The goal is that important code paths throughout the entire project are documented.

---

# 21. Define what counts as an important function

At minimum consider functions important when they:

```text
start or stop a subsystem
spawn or join a thread
process commands/events
mutate major state
perform hardware I/O
perform protocol transactions
schedule polling
handle retries
route requests
configure controllers
compute controller outputs
change output ownership
apply safe output
record process data
load/reload configuration
execute Lua
parse major configuration structures
perform value conversion with domain meaning
handle virtual-instrument requests
perform lifecycle transitions
```

Review every Rust file and identify such functions.

Do not wait for `pub` visibility to decide whether documentation is needed.

---

# 22. Rustdoc must explain contracts and consequences

For important functions, document things such as:

```text
what the function owns
what state it mutates
important preconditions
what success means
what failure means
whether failure is recoverable
threading assumptions
ordering requirements
lifecycle effects
safety effects
```

For example, a shutdown function should explain whether it:

```text
signals a thread
waits for it
flushes state
drops channels
applies safe output
```

Do not merely say:

```rust
/// Stops the worker.
```

if the actual semantics are richer.

---

# 23. Add ordinary comments in complex implementation blocks

In addition to rustdoc, add normal comments:

```rust
// ...
```

where local implementation reasoning deserves explanation.

Particularly review:

```text
acquisition scheduling
worker event loops
output control
controller lifecycle
PID
Furnace
protocol framing
serial retries
memory transport
profile reload
process recorder
Lua runtime synchronization
```

Comments should explain:

```text
WHY this code has this shape
```

not narrate obvious syntax.

Bad:

```rust
// Check if the value is greater than zero.
if value > 0.0 {
```

Good:

```rust
// Validate the complete update before replacing controller state so a
// partially invalid configuration cannot leave the running loop half-updated.
```

---

# 24. Review ALL Rust files for missing important documentation

Explicitly inspect every `.rs` file.

Maintain an internal checklist.

For each file check:

```text
module documentation
important types
important functions
important invariants
comments around complex code
source ordering
visibility
naming
tests
```

Do not assume files that were already visited during the first pass are complete.

The user specifically wants a whole-project pass.

---

# 25. Preserve sensible source organization

Review the source ordering produced during the previous pass.

Preferred default:

```text
//! module documentation

imports

constants

supporting types

main types

associated impls

trait impls

free functions

private helpers

tests
```

But:

```text
logical story > rigid ordering
```

Revert moves that made code harder to understand.

Keep strongly related types and implementations together.

---

# 26. Review architectural boundaries

Inspect whether current ownership remains coherent:

```text
ApplicationRuntime
AcquisitionSource
CombinedSource
workers
SerialCommandSource
local virtual-instrument source
signal processing
process control
output control
process recorder
Lua API
emulator
```

Reject abstractions introduced only for stylistic cleanup.

Check for:

```text
conceptual cycles
leaked implementation details
duplicated responsibility
unexpected public APIs
over-generalized abstractions
```

Do not redesign the system without a demonstrated benefit.

---

# 27. Review concurrency carefully

Inspect:

```text
worker ownership
channels
shutdown
Drop implementations
thread joins
profile reload
runtime replacement
emulator start/stop
```

Look for:

```text
deadlocks
blocking on the wrong thread
lost events
shutdown races
orphaned threads
stale endpoints
incorrect disconnect handling
```

Verify new rustdoc/comments describe actual behavior.

---

# 28. Review output safety

This is a release-critical check.

Verify behavior and documentation for:

```text
Manual
AutomaticPending
Automatic
```

and:

```text
controller ownership
manual override
safe output
pause
resume
removal
failed writes
rollback
shutdown
```

Any correction to safety-sensitive behavior must have focused tests.

Documentation must not overstate guarantees.

---

# 29. Review controllers

Inspect:

```text
PID
On/Off
Furnace
```

Verify documentation matches:

```text
parameters
units
valid ranges
diagnostics
reset
pause/resume
output limits
anti-windup
state preservation
reference behavior
```

For Furnace verify equations against implementation.

For PID verify derivative-on-measurement description.

---

# 30. Review emulator / virtual instruments

Verify normal emulator operation no longer requires external virtual COM software.

Check:

```text
memory transport
local source routing
start
stop
restart
profile reload
shutdown
read
write
periodic acquisition
controller writes
```

If serial emulator mode remains:

```text
normal mode = memory
integration/debug mode = serial
```

Documentation must make this distinction obvious.

---

# 31. Review real serial / Metakon path

Ensure changes have not altered behavior of:

```text
SerialCommandSource
SerialConnection
Metakon protocol
read
write
polling
failure suspension
retry
sensor fault handling
```

Do not require hardware for automated tests, but inspect this path carefully.

---

# 32. Validate every documentation statement against current code

Treat documentation as an API.

Verify:

```text
method names
parameter keys
diagnostic keys
default values
units
paths
state names
configuration fields
Lua callbacks
error behavior
controller behavior
process recorder behavior
```

Do not trust previous docs.

Do not trust examples copied from old scripts.

---

# 33. Validate all Lua examples

Every Lua example added to documentation must be checked against the current API.

Where possible, turn representative examples into tests.

At minimum manually verify syntax and actual method names.

A documentation example that cannot run is a documentation bug.

---

# 34. Review rustdoc output manually

Run:

```powershell
cargo doc --no-deps --document-private-items
```

and inspect generated documentation.

If practical:

```powershell
cargo doc --no-deps --document-private-items --open
```

Check:

```text
module landing pages
important private functions
public APIs
broken links
formatting
code blocks
equations
duplicate prose
missing contracts
```

---

# 35. Review doctests

Run:

```powershell
cargo test --doc
```

Fix examples that are intended to compile.

Do not force unrealistic runtime examples into doctests.

Use `text`, `ignore`, or non-compiling explanatory snippets appropriately.

---

# 36. Release configuration audit

Review:

```text
Cargo.toml
binary names
default-run
dependencies
features
release build
application paths
startup/profile defaults
```

Do not update dependencies merely because newer versions exist.

---

# 37. Test coverage audit

Prioritize critical invariants:

```text
runtime shutdown
profile reload
emulator restart
memory virtual instruments
poll retry/suspension
output ownership
safe output
controller reconfiguration
pause/resume
recorder failure
Lua execution failure
```

Add focused tests where real release behavior is currently insufficiently protected.

Avoid test proliferation without purpose.

---

# 38. Manual release workflow audit

Verify a realistic emulator workflow:

```text
launch
load profile
start emulator
discover instrument
read
write
add series
start acquisition
add filter
create controller
inspect diagnostic
manual override
pause
resume
process recording
profile reload
clean shutdown
```

Also inspect/document a realistic real-hardware workflow.

---

# 39. Final documentation completeness questions

Before release, a Lua user must be able to answer from documentation alone:

```text
How do I start acquisition?
How do I stop it?
How do I log something?
How do I add a signal?
How do I rename/delete/retry it?
How do I add every supported filter?
How do I send a serial command?
How do I use Metakon?
How do I use a virtual instrument?
How do I read/write a parameter?
How do I create every controller type?
How do I configure a controller?
How do references work?
How do diagnostics work?
How do I pause/resume/reset?
How do I build every type of control-panel widget?
How do callbacks work?
How do I start/stop an emulator?
How do startup profiles work?
```

Each capability must have a minimal example.

---

# 40. Final Rust developer completeness questions

A Rust developer must be able to answer:

```text
What owns application runtime state?
How do workers communicate?
How does AcquisitionSource routing work?
How does a real serial instrument flow through the system?
How does a virtual instrument flow through the system?
How does signal processing fit in?
How does a controller produce an output?
How does output arbitration protect actuators?
How does process recording work?
How does Lua reach Rust?
How do I add a new instrument?
How do I add a new controller type?
Where do I add tests?
What documentation must I update?
```

---

# 41. Final cleanup

Only after review:

```text
remove stale comments
fix incorrect docs
remove useless comments
tighten obvious visibility
correct genuinely misleading names
```

No speculative architecture rewrite.

This branch is preparing for release.

---

# 42. Final automated checks

Run:

```powershell
cargo fmt --check
cargo test
cargo test --doc
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps --document-private-items
cargo build --release
git diff --check
```

Also run any repository-specific tests or checks discovered during inspection.

---

# 43. Final documentation review

Manually inspect:

```text
README
architecture guide
configuration guide
Lua API guide
virtual instrument guide
controller guide
process recording guide
development guide
troubleshooting guide
in-app English help
in-app Russian help
generated rustdoc
```

Check all Markdown links.

Check referenced files exist.

Check all documented examples against current repository paths.

---

# 44. Final release-readiness report

Produce a concise report containing:

```text
1. Code areas reviewed
2. Documentation areas rewritten/expanded
3. Lua API coverage
4. Rustdoc coverage
5. Complex-code comments added
6. Instrument-extension guide status
7. Controller-extension guide status
8. Issues found
9. Issues fixed
10. Remaining known limitations
11. Tests/checks run
12. Manual workflows verified
13. Remaining release blockers
```

Do not claim release readiness while known safety, lifecycle, documentation or API-consistency problems remain.

---

# Definition of done

The release candidate is ready only when:

```text
all automated checks pass

release build succeeds

README provides a clear entry point

documentation is substantially detailed rather than summary-level

every Lua capability has a simple working example

configuration fields are fully documented

PID, On/Off and Furnace controllers are explained in depth

output safety is documented clearly

virtual instrument models are documented in depth

process recording is documented

troubleshooting is practical

adding a new Rust instrument is documented step by step

adding a new Rust controller type is documented step by step

important functions throughout the entire Rust project have meaningful
rustdoc where appropriate

complex implementation decisions have explanatory comments where useful

generated rustdoc is readable

memory emulator workflow works without external virtual COM software

real serial/Metakon path remains intact

shutdown/reload are clean

documentation matches current implementation
```

The standard is not:

```text
"there is documentation"
```

The standard is:

```text
"a competent new user or developer can actually learn the system from it."
```
