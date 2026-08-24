use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PidGains {
    proportional: f64,
    integral: f64,
    derivative: f64,
}

impl PidGains {
    pub fn new(proportional: f64, integral: f64, derivative: f64) -> Result<Self, PidGainsError> {
        if !proportional.is_finite() || proportional < 0.0 {
            return Err(PidGainsError::InvalidProportionalGain);
        }

        if !integral.is_finite() || integral < 0.0 {
            return Err(PidGainsError::InvalidIntegralGain);
        }

        if !derivative.is_finite() || derivative < 0.0 {
            return Err(PidGainsError::InvalidDerivativeGain);
        }

        Ok(Self {
            proportional,
            integral,
            derivative,
        })
    }

    pub const fn proportional(self) -> f64 {
        self.proportional
    }

    pub const fn integral(self) -> f64 {
        self.integral
    }

    pub const fn derivative(self) -> f64 {
        self.derivative
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PidGainsError {
    InvalidProportionalGain,
    InvalidIntegralGain,
    InvalidDerivativeGain,
}

impl fmt::Display for PidGainsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProportionalGain => formatter.write_str(
                "PID proportional gain must be finite \
                     and greater than or equal to zero",
            ),

            Self::InvalidIntegralGain => formatter.write_str(
                "PID integral gain must be finite \
                     and greater than or equal to zero",
            ),

            Self::InvalidDerivativeGain => formatter.write_str(
                "PID derivative gain must be finite \
                     and greater than or equal to zero",
            ),
        }
    }
}

impl Error for PidGainsError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PidOutputLimits {
    minimum: f64,
    maximum: f64,
}

impl PidOutputLimits {
    pub const FULL: Self = Self {
        minimum: -f64::MAX,
        maximum: f64::MAX,
    };

    pub fn new(minimum: f64, maximum: f64) -> Result<Self, PidOutputLimitsError> {
        if !minimum.is_finite() {
            return Err(PidOutputLimitsError::InvalidMinimum);
        }

        if !maximum.is_finite() {
            return Err(PidOutputLimitsError::InvalidMaximum);
        }

        if minimum >= maximum {
            return Err(PidOutputLimitsError::InvalidRange);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PidOutputLimitsError {
    InvalidMinimum,
    InvalidMaximum,
    InvalidRange,
}

impl fmt::Display for PidOutputLimitsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMinimum => formatter.write_str("PID output minimum must be finite"),

            Self::InvalidMaximum => formatter.write_str("PID output maximum must be finite"),

            Self::InvalidRange => formatter.write_str(
                "PID output minimum must be less \
                     than its maximum",
            ),
        }
    }
}

impl Error for PidOutputLimitsError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PidOutput {
    value: f64,
    unconstrained_value: f64,
    proportional: f64,
    integral: f64,
    derivative: f64,
    saturated: bool,
}

impl PidOutput {
    pub const fn value(self) -> f64 {
        self.value
    }

    pub const fn unconstrained_value(self) -> f64 {
        self.unconstrained_value
    }

    pub const fn proportional(self) -> f64 {
        self.proportional
    }

    pub const fn integral(self) -> f64 {
        self.integral
    }

    pub const fn derivative(self) -> f64 {
        self.derivative
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
pub struct PidController {
    gains: PidGains,
    output_limits: PidOutputLimits,
    integral: f64,
    previous_sample: Option<PreviousSample>,
}

impl PidController {
    pub const fn new(gains: PidGains) -> Self {
        Self::with_output_limits(gains, PidOutputLimits::FULL)
    }

    pub const fn with_output_limits(gains: PidGains, output_limits: PidOutputLimits) -> Self {
        Self {
            gains,
            output_limits,
            integral: 0.0,
            previous_sample: None,
        }
    }

    pub const fn gains(&self) -> PidGains {
        self.gains
    }

    pub const fn output_limits(&self) -> PidOutputLimits {
        self.output_limits
    }

    pub fn set_gains(&mut self, gains: PidGains) {
        self.gains = gains;
    }

    pub fn set_output_limits(&mut self, output_limits: PidOutputLimits) {
        self.output_limits = output_limits;
    }

    pub const fn integral(&self) -> f64 {
        self.integral
    }

    pub fn update(
        &mut self,
        timestamp: f64,
        setpoint: f64,
        measurement: f64,
    ) -> Result<PidOutput, PidControllerError> {
        if !timestamp.is_finite() {
            return Err(PidControllerError::NonFiniteTimestamp);
        }

        if !setpoint.is_finite() {
            return Err(PidControllerError::NonFiniteSetpoint);
        }

        if !measurement.is_finite() {
            return Err(PidControllerError::NonFiniteMeasurement);
        }

        let error = setpoint - measurement;

        if !error.is_finite() {
            return Err(PidControllerError::NonFiniteOutput);
        }

        let proportional = self.gains.proportional * error;

        let (proposed_integral, integral_change, derivative) = match self.previous_sample {
            Some(previous) => {
                if timestamp <= previous.timestamp {
                    return Err(PidControllerError::NonIncreasingTimestamp {
                        previous: previous.timestamp,
                        current: timestamp,
                    });
                }

                let elapsed_seconds = timestamp - previous.timestamp;

                let integral_change = self.gains.integral * error * elapsed_seconds;

                let proposed_integral = self.integral + integral_change;

                let measurement_rate = (measurement - previous.measurement) / elapsed_seconds;

                let derivative = -self.gains.derivative * measurement_rate;

                (proposed_integral, integral_change, derivative)
            }

            None => (self.integral, 0.0, 0.0),
        };

        let current_value = proportional + self.integral + derivative;

        let proposed_value = proportional + proposed_integral + derivative;

        if !proportional.is_finite()
            || !proposed_integral.is_finite()
            || !integral_change.is_finite()
            || !derivative.is_finite()
            || !current_value.is_finite()
            || !proposed_value.is_finite()
        {
            return Err(PidControllerError::NonFiniteOutput);
        }

        let minimum = self.output_limits.minimum();

        let maximum = self.output_limits.maximum();

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

        let unconstrained_value = proportional + integral + derivative;

        if !integral.is_finite() || !unconstrained_value.is_finite() {
            return Err(PidControllerError::NonFiniteOutput);
        }

        let value = self.output_limits.clamp(unconstrained_value);

        let saturated = integral_limited || value != unconstrained_value;

        let output = PidOutput {
            value,
            unconstrained_value,
            proportional,
            integral,
            derivative,
            saturated,
        };

        self.integral = integral;

        self.previous_sample = Some(PreviousSample {
            timestamp,
            measurement,
        });

        Ok(output)
    }

    pub fn reset(&mut self) {
        self.integral = 0.0;
        self.previous_sample = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PidControllerError {
    NonFiniteTimestamp,
    NonFiniteSetpoint,
    NonFiniteMeasurement,

    NonIncreasingTimestamp { previous: f64, current: f64 },

    NonFiniteOutput,
}

impl fmt::Display for PidControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteTimestamp => formatter.write_str("PID input timestamp must be finite"),

            Self::NonFiniteSetpoint => formatter.write_str("PID setpoint must be finite"),

            Self::NonFiniteMeasurement => formatter.write_str("PID measurement must be finite"),

            Self::NonIncreasingTimestamp { previous, current } => {
                write!(
                    formatter,
                    "PID input timestamps must increase: \
                     previous timestamp is {previous}, \
                     current timestamp is {current}",
                )
            }

            Self::NonFiniteOutput => formatter.write_str("PID output must be finite"),
        }
    }
}

impl Error for PidControllerError {}

#[cfg(test)]
mod tests {
    use super::{
        PidController, PidControllerError, PidGains, PidGainsError, PidOutputLimits,
        PidOutputLimitsError,
    };

    #[test]
    fn creates_pid_gains() {
        let gains = PidGains::new(2.0, 0.5, 1.5).unwrap();

        assert_eq!(gains.proportional(), 2.0);
        assert_eq!(gains.integral(), 0.5);
        assert_eq!(gains.derivative(), 1.5);

        assert!(PidGains::new(0.0, 0.0, 0.0).is_ok(),);
    }

    #[test]
    fn rejects_invalid_pid_gains() {
        let invalid_values = [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY];

        for value in invalid_values {
            assert_eq!(
                PidGains::new(value, 0.0, 0.0),
                Err(PidGainsError::InvalidProportionalGain,),
            );

            assert_eq!(
                PidGains::new(0.0, value, 0.0),
                Err(PidGainsError::InvalidIntegralGain,),
            );

            assert_eq!(
                PidGains::new(0.0, 0.0, value),
                Err(PidGainsError::InvalidDerivativeGain,),
            );
        }
    }

    #[test]
    fn first_sample_only_applies_proportional_term() {
        let gains = PidGains::new(2.0, 3.0, 4.0).unwrap();

        let mut controller = PidController::new(gains);

        let output = controller.update(10.0, 100.0, 90.0).unwrap();

        assert_close(output.proportional(), 20.0);

        assert_close(output.integral(), 0.0);

        assert_close(output.derivative(), 0.0);

        assert_close(output.value(), 20.0);
    }

    #[test]
    fn integrates_using_actual_elapsed_time() {
        let gains = PidGains::new(0.0, 2.0, 0.0).unwrap();

        let mut controller = PidController::new(gains);

        let first = controller.update(10.0, 10.0, 8.0).unwrap();

        assert_close(first.integral(), 0.0);

        let second = controller.update(10.25, 10.0, 8.0).unwrap();

        assert_close(second.integral(), 1.0);

        let third = controller.update(11.0, 10.0, 9.0).unwrap();

        assert_close(third.integral(), 2.5);

        assert_close(controller.integral(), 2.5);
    }

    #[test]
    fn differentiates_measurement_without_setpoint_kick() {
        let gains = PidGains::new(0.0, 0.0, 4.0).unwrap();

        let mut controller = PidController::new(gains);

        let first = controller.update(10.0, 100.0, 20.0).unwrap();

        assert_close(first.derivative(), 0.0);

        let second = controller.update(12.0, 100.0, 23.0).unwrap();

        assert_close(second.derivative(), -6.0);

        let third = controller.update(13.0, 200.0, 23.0).unwrap();

        assert_close(third.derivative(), 0.0);
    }

    #[test]
    fn combines_proportional_integral_and_derivative_terms() {
        let gains = PidGains::new(2.0, 3.0, 4.0).unwrap();

        let mut controller = PidController::new(gains);

        controller.update(0.0, 10.0, 8.0).unwrap();

        let output = controller.update(2.0, 10.0, 7.0).unwrap();

        assert_close(output.proportional(), 6.0);

        assert_close(output.integral(), 18.0);

        assert_close(output.derivative(), 2.0);

        assert_close(output.value(), 26.0);
    }

    #[test]
    fn rejects_invalid_inputs_without_changing_state() {
        let gains = PidGains::new(1.0, 1.0, 1.0).unwrap();

        let mut controller = PidController::new(gains);

        controller.update(1.0, 10.0, 8.0).unwrap();

        assert_eq!(
            controller.update(f64::NAN, 10.0, 7.0,),
            Err(PidControllerError::NonFiniteTimestamp,),
        );

        assert_eq!(
            controller.update(2.0, f64::INFINITY, 7.0,),
            Err(PidControllerError::NonFiniteSetpoint,),
        );

        assert_eq!(
            controller.update(2.0, 10.0, f64::NAN,),
            Err(PidControllerError::NonFiniteMeasurement,),
        );

        assert_eq!(
            controller.update(1.0, 10.0, 7.0,),
            Err(PidControllerError::NonIncreasingTimestamp {
                previous: 1.0,
                current: 1.0,
            },),
        );

        let output = controller.update(2.0, 10.0, 7.0).unwrap();

        assert_close(output.proportional(), 3.0);

        assert_close(output.integral(), 3.0);

        assert_close(output.derivative(), 1.0);

        assert_close(output.value(), 7.0);
    }

    #[test]
    fn rejects_non_finite_output_without_changing_state() {
        let gains = PidGains::new(f64::MAX, 0.0, 0.0).unwrap();

        let mut controller = PidController::new(gains);

        assert_eq!(
            controller.update(1.0, 2.0, 0.0,),
            Err(PidControllerError::NonFiniteOutput,),
        );

        let output = controller.update(1.0, 1.0, 0.0).unwrap();

        assert_eq!(output.value(), f64::MAX,);
    }

    #[test]
    fn reset_clears_integral_and_derivative_history() {
        let gains = PidGains::new(0.0, 1.0, 1.0).unwrap();

        let mut controller = PidController::new(gains);

        controller.update(10.0, 10.0, 5.0).unwrap();

        let accumulated = controller.update(11.0, 10.0, 4.0).unwrap();

        assert_close(accumulated.integral(), 6.0);

        assert_close(accumulated.derivative(), 1.0);

        controller.reset();

        assert_close(controller.integral(), 0.0);

        let restarted = controller.update(0.0, 20.0, 15.0).unwrap();

        assert_close(restarted.integral(), 0.0);

        assert_close(restarted.derivative(), 0.0);
    }

    #[test]
    fn validates_output_limits() {
        let limits = PidOutputLimits::new(0.0, 100.0).unwrap();

        assert_eq!(limits.minimum(), 0.0);
        assert_eq!(limits.maximum(), 100.0);

        assert_eq!(
            PidOutputLimits::new(f64::NAN, 100.0,),
            Err(PidOutputLimitsError::InvalidMinimum,),
        );

        assert_eq!(
            PidOutputLimits::new(0.0, f64::INFINITY,),
            Err(PidOutputLimitsError::InvalidMaximum,),
        );

        assert_eq!(
            PidOutputLimits::new(100.0, 100.0,),
            Err(PidOutputLimitsError::InvalidRange,),
        );

        assert_eq!(
            PidOutputLimits::new(101.0, 100.0,),
            Err(PidOutputLimitsError::InvalidRange,),
        );
    }

    #[test]
    fn clamps_output_to_configured_limits() {
        let gains = PidGains::new(2.0, 0.0, 0.0).unwrap();

        let limits = PidOutputLimits::new(0.0, 100.0).unwrap();

        let mut controller = PidController::with_output_limits(gains, limits);

        let high = controller.update(0.0, 100.0, 0.0).unwrap();

        assert_close(high.unconstrained_value(), 200.0);

        assert_close(high.value(), 100.0);

        assert!(high.saturated());

        let low = controller.update(1.0, 0.0, 10.0).unwrap();

        assert_close(low.unconstrained_value(), -20.0);

        assert_close(low.value(), 0.0);

        assert!(low.saturated());
    }

    #[test]
    fn prevents_integral_windup_at_upper_limit() {
        let gains = PidGains::new(0.0, 10.0, 0.0).unwrap();

        let limits = PidOutputLimits::new(0.0, 100.0).unwrap();

        let mut controller = PidController::with_output_limits(gains, limits);

        controller.update(0.0, 20.0, 0.0).unwrap();

        let first_saturated = controller.update(1.0, 20.0, 0.0).unwrap();

        assert_close(first_saturated.value(), 100.0);

        assert_close(first_saturated.integral(), 100.0);

        assert!(first_saturated.saturated(),);

        let still_saturated = controller.update(2.0, 20.0, 0.0).unwrap();

        assert_close(still_saturated.value(), 100.0);

        assert_close(still_saturated.integral(), 100.0);

        assert!(still_saturated.saturated(),);

        let unwinding = controller.update(3.0, 0.0, 5.0).unwrap();

        assert_close(unwinding.integral(), 50.0);

        assert_close(unwinding.value(), 50.0);

        assert!(!unwinding.saturated(),);
    }

    #[test]
    fn prevents_integral_windup_at_lower_limit() {
        let gains = PidGains::new(0.0, 10.0, 0.0).unwrap();

        let limits = PidOutputLimits::new(-100.0, 0.0).unwrap();

        let mut controller = PidController::with_output_limits(gains, limits);

        controller.update(0.0, 0.0, 20.0).unwrap();

        let saturated = controller.update(1.0, 0.0, 20.0).unwrap();

        assert_close(saturated.integral(), -100.0);

        assert_close(saturated.value(), -100.0);

        assert!(saturated.saturated(),);

        let still_saturated = controller.update(2.0, 0.0, 20.0).unwrap();

        assert_close(still_saturated.integral(), -100.0);

        let unwinding = controller.update(3.0, 0.0, -5.0).unwrap();

        assert_close(unwinding.integral(), -50.0);

        assert_close(unwinding.value(), -50.0);

        assert!(!unwinding.saturated(),);
    }

    #[test]
    fn changes_gains_without_resetting_integral_state() {
        let initial_gains = PidGains::new(0.0, 1.0, 0.0).unwrap();

        let limits = PidOutputLimits::new(-100.0, 100.0).unwrap();

        let mut controller = PidController::with_output_limits(initial_gains, limits);

        controller.update(0.0, 10.0, 8.0).unwrap();

        let accumulated = controller.update(1.0, 10.0, 8.0).unwrap();

        assert_close(accumulated.integral(), 2.0);

        let new_gains = PidGains::new(2.0, 0.0, 0.0).unwrap();

        controller.set_gains(new_gains);

        let updated = controller.update(2.0, 10.0, 8.0).unwrap();

        assert_eq!(controller.gains(), new_gains,);

        assert_close(updated.proportional(), 4.0);

        assert_close(updated.integral(), 2.0);

        assert_close(updated.derivative(), 0.0);

        assert_close(updated.value(), 6.0);
    }

    #[test]
    fn changes_output_limits_without_resetting_controller() {
        let gains = PidGains::new(2.0, 0.0, 0.0).unwrap();

        let mut controller = PidController::new(gains);

        let unrestricted = controller.update(0.0, 100.0, 0.0).unwrap();

        assert_close(unrestricted.value(), 200.0);

        assert!(!unrestricted.saturated(),);

        let narrow_limits = PidOutputLimits::new(0.0, 100.0).unwrap();

        controller.set_output_limits(narrow_limits);

        let limited = controller.update(1.0, 100.0, 0.0).unwrap();

        assert_eq!(controller.output_limits(), narrow_limits,);

        assert_close(limited.value(), 100.0);

        assert!(limited.saturated(),);

        let wide_limits = PidOutputLimits::new(0.0, 300.0).unwrap();

        controller.set_output_limits(wide_limits);

        let unrestricted_again = controller.update(2.0, 100.0, 0.0).unwrap();

        assert_close(unrestricted_again.value(), 200.0);

        assert!(!unrestricted_again.saturated(),);
    }

    #[test]
    fn reset_preserves_gains_and_output_limits() {
        let gains = PidGains::new(2.0, 3.0, 4.0).unwrap();

        let limits = PidOutputLimits::new(0.0, 100.0).unwrap();

        let mut controller = PidController::with_output_limits(gains, limits);

        controller.update(0.0, 10.0, 5.0).unwrap();

        controller.update(1.0, 10.0, 4.0).unwrap();

        controller.reset();

        assert_eq!(controller.gains(), gains,);

        assert_eq!(controller.output_limits(), limits,);

        assert_close(controller.integral(), 0.0);
    }

    fn assert_close(actual: f64, expected: f64) {
        let tolerance = expected.abs().max(1.0) * 1.0e-12;

        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}",
        );
    }
}
