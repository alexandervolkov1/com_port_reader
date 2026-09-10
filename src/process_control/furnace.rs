use std::{error::Error, fmt};

const ABSOLUTE_ZERO_C: f64 = -273.15;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FurnaceGains {
    proportional: f64,
    integral: f64,
}

impl FurnaceGains {
    pub fn new(proportional: f64, integral: f64) -> Result<Self, FurnaceControllerError> {
        if !proportional.is_finite() || proportional < 0.0 {
            return Err(FurnaceControllerError::InvalidProportionalGain);
        }

        if !integral.is_finite() || integral < 0.0 {
            return Err(FurnaceControllerError::InvalidIntegralGain);
        }

        Ok(Self {
            proportional,
            integral,
        })
    }

    pub const fn proportional(self) -> f64 {
        self.proportional
    }

    pub const fn integral(self) -> f64 {
        self.integral
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FurnaceModel {
    ambient_temperature: f64,
    max_power: f64,
    heater_lag: f64,
    linear_loss: f64,
    radiation_loss_1000c: f64,
}

impl FurnaceModel {
    pub fn new(
        ambient_temperature: f64,
        max_power: f64,
        heater_lag: f64,
        linear_loss: f64,
        radiation_loss_1000c: f64,
    ) -> Result<Self, FurnaceControllerError> {
        if !ambient_temperature.is_finite() || ambient_temperature < ABSOLUTE_ZERO_C {
            return Err(FurnaceControllerError::InvalidAmbientTemperature);
        }

        if !max_power.is_finite() || max_power <= 0.0 {
            return Err(FurnaceControllerError::InvalidMaximumPower);
        }

        if !heater_lag.is_finite() || heater_lag < 0.0 {
            return Err(FurnaceControllerError::InvalidHeaterLag);
        }

        if !linear_loss.is_finite() || linear_loss < 0.0 {
            return Err(FurnaceControllerError::InvalidLinearLoss);
        }

        if !radiation_loss_1000c.is_finite() || radiation_loss_1000c < 0.0 {
            return Err(FurnaceControllerError::InvalidRadiationLoss);
        }

        Ok(Self {
            ambient_temperature,
            max_power,
            heater_lag,
            linear_loss,
            radiation_loss_1000c,
        })
    }

    pub const fn ambient_temperature(self) -> f64 {
        self.ambient_temperature
    }

    pub const fn max_power(self) -> f64 {
        self.max_power
    }

    pub const fn heater_lag(self) -> f64 {
        self.heater_lag
    }

    pub const fn linear_loss(self) -> f64 {
        self.linear_loss
    }

    pub const fn radiation_loss_1000c(self) -> f64 {
        self.radiation_loss_1000c
    }

    fn feed_forward(self, temperature: f64) -> f64 {
        let kelvin = temperature + 273.15;
        let ambient_kelvin = self.ambient_temperature + 273.15;

        let reference = 1273.15_f64.powi(4) - 293.15_f64.powi(4);

        let radiation =
            self.radiation_loss_1000c * (kelvin.powi(4) - ambient_kelvin.powi(4)) / reference;

        let linear = self.linear_loss * (temperature - self.ambient_temperature);

        100.0 * (linear + radiation).max(0.0) / self.max_power
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FurnaceOutputLimits {
    minimum: f64,
    maximum: f64,
}

impl FurnaceOutputLimits {
    pub fn new(minimum: f64, maximum: f64) -> Result<Self, FurnaceControllerError> {
        if !minimum.is_finite() {
            return Err(FurnaceControllerError::InvalidOutputMinimum);
        }

        if !maximum.is_finite() {
            return Err(FurnaceControllerError::InvalidOutputMaximum);
        }

        if minimum >= maximum {
            return Err(FurnaceControllerError::InvalidOutputRange);
        }

        Ok(Self { minimum, maximum })
    }

    pub const fn minimum(self) -> f64 {
        self.minimum
    }

    pub const fn maximum(self) -> f64 {
        self.maximum
    }

    fn clamp(self, value: f64) -> f64 {
        value.clamp(self.minimum, self.maximum)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FurnaceOutput {
    value: f64,
    unconstrained_value: f64,
    feed_forward: f64,
    proportional: f64,
    integral: f64,
    predicted_measurement: f64,
    measurement_rate: f64,
    saturated: bool,
}

impl FurnaceOutput {
    pub const fn value(self) -> f64 {
        self.value
    }

    pub const fn unconstrained_value(self) -> f64 {
        self.unconstrained_value
    }

    pub const fn feed_forward(self) -> f64 {
        self.feed_forward
    }

    pub const fn proportional(self) -> f64 {
        self.proportional
    }

    pub const fn integral(self) -> f64 {
        self.integral
    }

    pub const fn predicted_measurement(self) -> f64 {
        self.predicted_measurement
    }

    pub const fn measurement_rate(self) -> f64 {
        self.measurement_rate
    }

    pub const fn saturated(self) -> bool {
        self.saturated
    }
}

#[derive(Clone, Copy, Debug)]
struct PreviousSample {
    timestamp: f64,
    measurement: f64,
}

#[derive(Debug)]
pub struct FurnaceController {
    setpoint: f64,
    gains: FurnaceGains,
    model: FurnaceModel,
    output_limits: FurnaceOutputLimits,

    integral: f64,
    measurement_rate: f64,
    previous_sample: Option<PreviousSample>,
}

impl FurnaceController {
    pub fn new(
        setpoint: f64,
        gains: FurnaceGains,
        model: FurnaceModel,
        output_limits: FurnaceOutputLimits,
    ) -> Result<Self, FurnaceControllerError> {
        validate_setpoint(setpoint)?;

        Ok(Self {
            setpoint,
            gains,
            model,
            output_limits,
            integral: 0.0,
            measurement_rate: 0.0,
            previous_sample: None,
        })
    }

    pub fn configure(
        &mut self,
        setpoint: f64,
        gains: FurnaceGains,
        model: FurnaceModel,
        output_limits: FurnaceOutputLimits,
    ) -> Result<(), FurnaceControllerError> {
        validate_setpoint(setpoint)?;

        self.setpoint = setpoint;
        self.gains = gains;
        self.model = model;
        self.output_limits = output_limits;

        Ok(())
    }

    pub const fn setpoint(&self) -> f64 {
        self.setpoint
    }

    pub const fn gains(&self) -> FurnaceGains {
        self.gains
    }

    pub const fn model(&self) -> FurnaceModel {
        self.model
    }

    pub const fn output_limits(&self) -> FurnaceOutputLimits {
        self.output_limits
    }

    pub const fn integral(&self) -> f64 {
        self.integral
    }

    pub fn update(
        &mut self,
        timestamp: f64,
        measurement: f64,
    ) -> Result<FurnaceOutput, FurnaceControllerError> {
        if !timestamp.is_finite() {
            return Err(FurnaceControllerError::NonFiniteTimestamp);
        }

        if !measurement.is_finite() {
            return Err(FurnaceControllerError::NonFiniteMeasurement);
        }

        let elapsed = match self.previous_sample {
            Some(previous) => {
                if timestamp <= previous.timestamp {
                    return Err(FurnaceControllerError::NonIncreasingTimestamp {
                        previous: previous.timestamp,
                        current: timestamp,
                    });
                }

                let dt = timestamp - previous.timestamp;
                let raw_rate = (measurement - previous.measurement) / dt;

                let tau = self.model.heater_lag / 3.0;

                self.measurement_rate = if tau > 0.0 {
                    let alpha = 1.0 - (-dt / tau).exp();

                    self.measurement_rate + alpha * (raw_rate - self.measurement_rate)
                } else {
                    raw_rate
                };

                Some(dt)
            }

            None => None,
        };

        let predicted_measurement = measurement + self.model.heater_lag * self.measurement_rate;

        let error = self.setpoint - predicted_measurement;

        let feed_forward = self.model.feed_forward(self.setpoint);

        let proportional = self.gains.proportional * error;

        let integral_change = elapsed
            .map(|dt| self.gains.integral * error * dt)
            .unwrap_or(0.0);

        let proposed_integral = self.integral + integral_change;

        let current_value = feed_forward + proportional + self.integral;

        let proposed_value = feed_forward + proportional + proposed_integral;

        if !predicted_measurement.is_finite()
            || !self.measurement_rate.is_finite()
            || !error.is_finite()
            || !feed_forward.is_finite()
            || !proportional.is_finite()
            || !proposed_integral.is_finite()
            || !proposed_value.is_finite()
        {
            return Err(FurnaceControllerError::NonFiniteOutput);
        }

        let minimum = self.output_limits.minimum;
        let maximum = self.output_limits.maximum;

        let mut integral = proposed_integral;
        let mut integral_limited = false;

        if proposed_value > maximum && integral_change > 0.0 {
            integral = if current_value < maximum {
                self.integral + maximum - current_value
            } else {
                self.integral
            };

            integral_limited = true;
        } else if proposed_value < minimum && integral_change < 0.0 {
            integral = if current_value > minimum {
                self.integral + minimum - current_value
            } else {
                self.integral
            };

            integral_limited = true;
        }

        let unconstrained_value = feed_forward + proportional + integral;

        if !integral.is_finite() || !unconstrained_value.is_finite() {
            return Err(FurnaceControllerError::NonFiniteOutput);
        }

        let value = self.output_limits.clamp(unconstrained_value);

        let output = FurnaceOutput {
            value,
            unconstrained_value,
            feed_forward,
            proportional,
            integral,
            predicted_measurement,
            measurement_rate: self.measurement_rate,
            saturated: integral_limited || value != unconstrained_value,
        };

        self.integral = integral;
        self.previous_sample = Some(PreviousSample {
            timestamp,
            measurement,
        });

        Ok(output)
    }

    pub fn reset_integral(&mut self) {
        self.integral = 0.0;
    }

    pub fn resynchronize(&mut self) {
        self.previous_sample = None;
        self.measurement_rate = 0.0;
    }

    pub fn reset(&mut self) {
        self.reset_integral();
        self.resynchronize();
    }
}

fn validate_setpoint(setpoint: f64) -> Result<(), FurnaceControllerError> {
    if !setpoint.is_finite() || setpoint < ABSOLUTE_ZERO_C {
        return Err(FurnaceControllerError::InvalidSetpoint);
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FurnaceControllerError {
    InvalidSetpoint,
    InvalidProportionalGain,
    InvalidIntegralGain,

    InvalidAmbientTemperature,
    InvalidMaximumPower,
    InvalidHeaterLag,
    InvalidLinearLoss,
    InvalidRadiationLoss,

    InvalidOutputMinimum,
    InvalidOutputMaximum,
    InvalidOutputRange,

    NonFiniteTimestamp,
    NonFiniteMeasurement,

    NonIncreasingTimestamp { previous: f64, current: f64 },

    NonFiniteOutput,
}

impl fmt::Display for FurnaceControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidSetpoint => "Furnace setpoint must be finite and physically valid",

            Self::InvalidProportionalGain => {
                "Furnace proportional gain must be finite and non-negative"
            }

            Self::InvalidIntegralGain => "Furnace integral gain must be finite and non-negative",

            Self::InvalidAmbientTemperature => {
                "Furnace ambient temperature must be finite and physically valid"
            }

            Self::InvalidMaximumPower => {
                "Furnace maximum power must be finite and greater than zero"
            }

            Self::InvalidHeaterLag => "Furnace heater lag must be finite and non-negative",

            Self::InvalidLinearLoss => "Furnace linear loss must be finite and non-negative",

            Self::InvalidRadiationLoss => "Furnace radiation loss must be finite and non-negative",

            Self::InvalidOutputMinimum => "Furnace output minimum must be finite",

            Self::InvalidOutputMaximum => "Furnace output maximum must be finite",

            Self::InvalidOutputRange => "Furnace output minimum must be less than its maximum",

            Self::NonFiniteTimestamp => "Furnace input timestamp must be finite",

            Self::NonFiniteMeasurement => "Furnace measurement must be finite",

            Self::NonIncreasingTimestamp { .. } => {
                return match self {
                    Self::NonIncreasingTimestamp { previous, current } => write!(
                        formatter,
                        "Furnace input timestamps must increase: \
                         previous timestamp is {previous}, \
                         current timestamp is {current}",
                    ),
                    _ => unreachable!(),
                };
            }

            Self::NonFiniteOutput => "Furnace controller produced a non-finite output",
        };

        formatter.write_str(message)
    }
}

impl Error for FurnaceControllerError {}

#[cfg(test)]
mod tests {
    use super::{FurnaceController, FurnaceGains, FurnaceModel, FurnaceOutputLimits};

    fn controller(heater_lag: f64) -> FurnaceController {
        FurnaceController::new(
            500.0,
            FurnaceGains::new(0.1, 0.0005).unwrap(),
            FurnaceModel::new(20.0, 2500.0, heater_lag, 0.35, 1200.0).unwrap(),
            FurnaceOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn computes_model_feed_forward() {
        let mut controller = controller(0.0);

        let output = controller.update(0.0, 500.0).unwrap();

        assert!(output.feed_forward() > 10.0 && output.feed_forward() < 20.0);

        assert_eq!(output.proportional(), 0.0,);
    }

    #[test]
    fn anticipates_rising_temperature() {
        let mut predictive = controller(90.0);
        let mut immediate = controller(0.0);

        predictive.update(0.0, 400.0).unwrap();
        immediate.update(0.0, 400.0).unwrap();

        let predictive = predictive.update(10.0, 410.0).unwrap();

        let immediate = immediate.update(10.0, 410.0).unwrap();

        assert!(predictive.predicted_measurement() > 410.0);

        assert!(predictive.value() < immediate.value());
    }

    #[test]
    fn prevents_integral_windup_at_upper_limit() {
        let mut controller = FurnaceController::new(
            500.0,
            FurnaceGains::new(1.0, 1.0).unwrap(),
            FurnaceModel::new(20.0, 2500.0, 0.0, 0.0, 0.0).unwrap(),
            FurnaceOutputLimits::new(0.0, 50.0).unwrap(),
        )
        .unwrap();

        controller.update(0.0, 20.0).unwrap();

        let output = controller.update(1.0, 20.0).unwrap();

        assert_eq!(output.value(), 50.0);
        assert_eq!(output.integral(), 0.0);
        assert!(output.saturated());
    }

    #[test]
    fn reset_clears_integral_and_rate_state() {
        let mut controller = controller(90.0);

        controller.update(0.0, 400.0).unwrap();
        controller.update(10.0, 410.0).unwrap();

        controller.reset();

        assert_eq!(controller.integral(), 0.0);

        let output = controller.update(20.0, 420.0).unwrap();

        assert_eq!(output.measurement_rate(), 0.0,);

        assert_eq!(output.predicted_measurement(), 420.0,);
    }
}
