use super::{
    ControlLoop, ControlLoopDefinition, Controller, ControllerOutput, FurnaceController,
    FurnaceGains, FurnaceModel, FurnaceOutputLimits, PidController, PidGains, PidOutputLimits,
};
use crate::instrument::InstrumentValue;

const AMBIENT_TEMPERATURE: f64 = 20.0;
const MAX_POWER: f64 = 2500.0;
const HEATER_LAG: f64 = 90.0;
const THERMAL_CAPACITY: f64 = 12_000.0;
const LINEAR_LOSS: f64 = 0.35;
const RADIATION_LOSS_1000_C: f64 = 1200.0;
const SAMPLE_INTERVAL: f64 = 1.0;

#[derive(Clone, Copy, Debug)]
enum Strategy {
    Pid,
    Furnace,
}

impl Strategy {
    fn controller(self, setpoint: f64, output_maximum: f64) -> Controller {
        match self {
            Self::Pid => PidController::with_output_limits(
                setpoint,
                PidGains::new(0.4, 0.0005, 5.0).unwrap(),
                PidOutputLimits::new(0.0, output_maximum).unwrap(),
            )
            .unwrap()
            .into(),
            Self::Furnace => FurnaceController::new(
                setpoint,
                FurnaceGains::new(0.1, 0.0005).unwrap(),
                FurnaceModel::new(
                    AMBIENT_TEMPERATURE,
                    MAX_POWER,
                    HEATER_LAG,
                    LINEAR_LOSS,
                    RADIATION_LOSS_1000_C,
                )
                .unwrap(),
                FurnaceOutputLimits::new(0.0, output_maximum).unwrap(),
            )
            .unwrap()
            .into(),
        }
    }
}

#[derive(Clone, Copy)]
struct PlantConfig {
    ambient_temperature: f64,
    max_power: f64,
    heater_lag: f64,
    thermal_capacity: f64,
    linear_loss: f64,
    radiation_loss_1000_c: f64,
}

impl Default for PlantConfig {
    fn default() -> Self {
        Self {
            ambient_temperature: AMBIENT_TEMPERATURE,
            max_power: MAX_POWER,
            heater_lag: HEATER_LAG,
            thermal_capacity: THERMAL_CAPACITY,
            linear_loss: LINEAR_LOSS,
            radiation_loss_1000_c: RADIATION_LOSS_1000_C,
        }
    }
}

struct SimulatedFurnace {
    config: PlantConfig,
    temperature: f64,
    effective_power: f64,
}

impl SimulatedFurnace {
    fn new(config: PlantConfig) -> Self {
        Self {
            temperature: config.ambient_temperature,
            effective_power: 0.0,
            config,
        }
    }

    fn step(&mut self, commanded_power: f64, elapsed: f64) {
        let target_power = self.config.max_power * commanded_power / 100.0;
        let old_power = self.effective_power;
        self.effective_power =
            target_power + (old_power - target_power) * (-elapsed / self.config.heater_lag).exp();

        let kelvin = self.temperature + 273.15;
        let ambient_kelvin = self.config.ambient_temperature + 273.15;
        let radiation_reference =
            (1000.0_f64 + 273.15).powi(4) - (AMBIENT_TEMPERATURE + 273.15).powi(4);
        let heat_loss = self.config.linear_loss
            * (self.temperature - self.config.ambient_temperature)
            + self.config.radiation_loss_1000_c * (kelvin.powi(4) - ambient_kelvin.powi(4))
                / radiation_reference;
        let heating = 0.5 * (old_power + self.effective_power);
        self.temperature += elapsed * (heating - heat_loss) / self.config.thermal_capacity;
    }
}

#[derive(Clone, Copy, Debug)]
struct Metrics {
    overshoot: f64,
    settling_time: Option<f64>,
    integrated_absolute_error: f64,
    maximum_temperature: f64,
    saturated_time: f64,
    energy: f64,
    final_temperature: f64,
}

fn run_scenario(
    strategy: Strategy,
    plant_config: PlantConfig,
    initial_setpoint: f64,
    final_setpoint: f64,
    step_time: f64,
    duration: f64,
    output_maximum: f64,
) -> Metrics {
    let mut plant = SimulatedFurnace::new(plant_config);
    let mut controller = strategy.controller(initial_setpoint, output_maximum);
    let mut timestamp = 0.0;
    let mut changed = false;
    let mut maximum_temperature = plant.temperature;
    let mut integrated_absolute_error = 0.0;
    let mut saturated_time = 0.0;
    let mut energy = 0.0;
    let mut last_outside_tolerance = None;

    while timestamp < duration {
        if !changed && timestamp >= step_time {
            controller
                .configure([("setpoint", InstrumentValue::Number(final_setpoint))])
                .unwrap();
            changed = true;
        }

        let setpoint = if changed {
            final_setpoint
        } else {
            initial_setpoint
        };
        let output = controller.update(timestamp, plant.temperature).unwrap();

        plant.step(output.value(), SAMPLE_INTERVAL);
        timestamp += SAMPLE_INTERVAL;

        if timestamp >= step_time {
            maximum_temperature = maximum_temperature.max(plant.temperature);
            integrated_absolute_error += (setpoint - plant.temperature).abs() * SAMPLE_INTERVAL;
            saturated_time += f64::from(output.saturated().unwrap_or(false)) * SAMPLE_INTERVAL;
            energy += plant.effective_power * SAMPLE_INTERVAL;

            let tolerance = (0.02 * final_setpoint.abs()).max(2.0);
            if (plant.temperature - final_setpoint).abs() > tolerance {
                last_outside_tolerance = Some(timestamp - step_time);
            }
        }
    }

    let settling_time = last_outside_tolerance
        .filter(|last| *last < duration - step_time)
        .map(|last| last + SAMPLE_INTERVAL);

    Metrics {
        overshoot: (maximum_temperature - final_setpoint).max(0.0),
        settling_time,
        integrated_absolute_error,
        maximum_temperature,
        saturated_time,
        energy,
        final_temperature: plant.temperature,
    }
}

fn assert_valid(metrics: Metrics, duration: f64, output_maximum: f64) {
    assert!(metrics.overshoot.is_finite() && metrics.overshoot >= 0.0);
    assert!(metrics.integrated_absolute_error.is_finite());
    assert!(metrics.maximum_temperature.is_finite());
    assert!(metrics.saturated_time >= 0.0 && metrics.saturated_time <= duration);
    assert!(
        metrics.energy >= 0.0 && metrics.energy <= MAX_POWER * output_maximum / 100.0 * duration
    );
    assert!(metrics.final_temperature.is_finite());
}

fn assert_close(actual: f64, expected: f64) {
    let tolerance = 1e-9 * expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected}"
    );
}

#[test]
fn compares_cold_start_and_setpoint_step() {
    let cold_pid = run_scenario(
        Strategy::Pid,
        PlantConfig::default(),
        500.0,
        500.0,
        0.0,
        7_200.0,
        70.0,
    );
    let cold_furnace = run_scenario(
        Strategy::Furnace,
        PlantConfig::default(),
        500.0,
        500.0,
        0.0,
        7_200.0,
        70.0,
    );
    let step_pid = run_scenario(
        Strategy::Pid,
        PlantConfig::default(),
        300.0,
        500.0,
        3_600.0,
        7_200.0,
        70.0,
    );
    let step_furnace = run_scenario(
        Strategy::Furnace,
        PlantConfig::default(),
        300.0,
        500.0,
        3_600.0,
        7_200.0,
        70.0,
    );

    for metrics in [cold_pid, cold_furnace, step_pid, step_furnace] {
        assert_valid(metrics, 7_200.0, 70.0);
    }

    // These example gains favor PID. The comparison deliberately records that
    // result instead of assuming that the model-based controller must win.
    assert!(cold_pid.overshoot < cold_furnace.overshoot);
    assert!(cold_pid.integrated_absolute_error < cold_furnace.integrated_absolute_error);
    assert!(cold_pid.settling_time.is_some());
    assert!(cold_furnace.settling_time.is_none());

    assert!(step_pid.overshoot < step_furnace.overshoot);
    assert!(step_pid.integrated_absolute_error < step_furnace.integrated_absolute_error);
    assert!(step_furnace.saturated_time < step_pid.saturated_time);
    assert!(step_furnace.energy < step_pid.energy);
    assert!(step_pid.settling_time.is_none());
    assert!(step_furnace.settling_time.is_none());
}

#[test]
fn compares_saturation_and_model_mismatch() {
    let mismatched = PlantConfig {
        max_power: 2000.0,
        thermal_capacity: 15_000.0,
        linear_loss: 0.45,
        radiation_loss_1000_c: 1500.0,
        ..PlantConfig::default()
    };

    let saturated_pid = run_scenario(
        Strategy::Pid,
        PlantConfig::default(),
        900.0,
        900.0,
        0.0,
        7_200.0,
        30.0,
    );
    let saturated_furnace = run_scenario(
        Strategy::Furnace,
        PlantConfig::default(),
        900.0,
        900.0,
        0.0,
        7_200.0,
        30.0,
    );
    assert_valid(saturated_pid, 7_200.0, 30.0);
    assert_valid(saturated_furnace, 7_200.0, 30.0);
    assert_eq!(saturated_pid.saturated_time, 7_200.0);
    assert_eq!(saturated_furnace.saturated_time, 7_200.0);
    assert_close(
        saturated_pid.final_temperature,
        saturated_furnace.final_temperature,
    );
    assert_close(saturated_pid.energy, saturated_furnace.energy);
    assert!(saturated_pid.settling_time.is_none());
    assert!(saturated_furnace.settling_time.is_none());

    let mismatched_pid = run_scenario(Strategy::Pid, mismatched, 500.0, 500.0, 0.0, 9_000.0, 70.0);
    let mismatched_furnace = run_scenario(
        Strategy::Furnace,
        mismatched,
        500.0,
        500.0,
        0.0,
        9_000.0,
        70.0,
    );
    assert_valid(mismatched_pid, 9_000.0, 70.0);
    assert_valid(mismatched_furnace, 9_000.0, 70.0);
    assert!(mismatched_pid.overshoot < mismatched_furnace.overshoot);
    assert!(
        mismatched_pid.integrated_absolute_error < mismatched_furnace.integrated_absolute_error
    );
    assert!(mismatched_pid.energy < mismatched_furnace.energy);
    assert!(mismatched_pid.settling_time.is_none());
    assert!(mismatched_furnace.settling_time.is_none());
}

#[test]
fn resynchronizes_after_manual_operation_and_controller_change() {
    let mut plant = SimulatedFurnace::new(PlantConfig::default());
    let mut timestamp = 0.0;

    for _ in 0..300 {
        plant.step(25.0, SAMPLE_INTERVAL);
        timestamp += SAMPLE_INTERVAL;
    }

    let pid_definition =
        ControlLoopDefinition::new("pid", 1_u64, (), Strategy::Pid.controller(500.0, 70.0))
            .unwrap();
    let mut pid = ControlLoop::new(pid_definition);
    let first_pid = pid.process(timestamp, plant.temperature).unwrap().unwrap();
    let ControllerOutput::Pid { output, .. } = first_pid else {
        panic!("expected PID output");
    };
    assert_eq!(output.derivative(), 0.0);

    let mut last_automatic_output = output.value();
    plant.step(last_automatic_output, SAMPLE_INTERVAL);
    timestamp += SAMPLE_INTERVAL;

    for _ in 0..600 {
        let output = pid.process(timestamp, plant.temperature).unwrap().unwrap();
        last_automatic_output = output.value();
        plant.step(last_automatic_output, SAMPLE_INTERVAL);
        timestamp += SAMPLE_INTERVAL;
    }

    pid.pause();
    for _ in 0..300 {
        // Manual takes over at the last applied value, matching the demo's
        // controller-to-manual transition.
        plant.step(last_automatic_output, SAMPLE_INTERVAL);
        timestamp += SAMPLE_INTERVAL;
    }

    pid.resume();
    let resumed_pid = pid.process(timestamp, plant.temperature).unwrap().unwrap();
    let ControllerOutput::Pid { output, .. } = resumed_pid else {
        panic!("expected PID output");
    };
    assert_eq!(output.derivative(), 0.0);

    pid.pause();
    for _ in 0..300 {
        plant.step(25.0, SAMPLE_INTERVAL);
        timestamp += SAMPLE_INTERVAL;
    }

    let furnace_definition = ControlLoopDefinition::new(
        "furnace",
        1_u64,
        (),
        Strategy::Furnace.controller(500.0, 70.0),
    )
    .unwrap();
    let mut furnace = ControlLoop::new(furnace_definition);
    let first_furnace = furnace
        .process(timestamp, plant.temperature)
        .unwrap()
        .unwrap();
    let ControllerOutput::Furnace { output, .. } = first_furnace else {
        panic!("expected Furnace output");
    };
    assert_eq!(output.measurement_rate(), 0.0);
    assert_eq!(output.predicted_measurement(), plant.temperature);
    assert!(output.value().is_finite());
}
