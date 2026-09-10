# Furnace controller comparison

`process_control::furnace_comparison_tests` runs PID and FurnaceController
against the same deterministic version of `emulator_scripts/furnace_plant.lua`.
It uses a one-second sample interval and the initial gains from the demo. The
tests measure overshoot, two-percent settling time, integrated absolute error
(IAE), maximum temperature, time at an output limit and integrated heater
energy. Noise is disabled so regressions are reproducible.

The current baseline is:

| Scenario | Strategy | Overshoot, °C | Settling | IAE, °C·s | Saturated, s | Energy, J |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 20→500 °C, 2 h | PID | 47.6 | 6609 s | 968134 | 2838 | 7497747 |
| 20→500 °C, 2 h | Furnace | 90.3 | >2 h | 1083830 | 5132 | 8004171 |
| 300→500 °C after 1 h | PID | 47.7 | >1 h | 180882 | 449 | 3263061 |
| 300→500 °C after 1 h | Furnace | 65.5 | >1 h | 182848 | 394 | 2935129 |
| 20→900 °C, output limited to 30% | Both | 0 | >2 h | 4894691 | 7200 | 5332874 |
| Mismatched plant, 20→500 °C | PID | 40.6 | >2.5 h | 1470170 | 5167 | 9980837 |
| Mismatched plant, 20→500 °C | Furnace | 63.8 | >2.5 h | 1522011 | 6045 | 10309054 |

The example Furnace tuning does not currently outperform the example PID
tuning. It uses less energy and spends less time saturated on the setpoint step,
but has more overshoot and does not settle within the observation windows. This
is a useful baseline, not a claim about the controller design: the gains are
starting values and have not been optimized independently for the scenarios.

The saturation case is intentionally unreachable. Both controllers command the
same 30% limit for the whole run and therefore produce the same plant trajectory.
The mismatch case gives the real plant 20% less heater power, 25% more thermal
capacity and stronger heat losses while FurnaceController retains its nominal
model.

The transition test also covers Manual → PID, PID → Manual → PID and
PID → Manual → Furnace. Resuming an existing controller and starting a new one
must clear rate/derivative history, while the simulated plant state remains
continuous. Output-arbitration safety itself is covered by the runtime and
output-control tests.
