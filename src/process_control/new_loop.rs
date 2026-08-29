use std::{error::Error, fmt};

use super::{PidGains, PidOutputLimits};

#[derive(Clone, Debug, PartialEq)]
pub struct NewPidLoop<OutputTarget> {
    name: String,
    input_name: String,
    output_target: OutputTarget,
    setpoint: f64,
    gains: PidGains,
    output_limits: PidOutputLimits,
}

impl<OutputTarget> NewPidLoop<OutputTarget> {
    pub fn new(
        name: impl Into<String>,
        input_name: impl Into<String>,
        output_target: OutputTarget,
        setpoint: f64,
        gains: PidGains,
        output_limits: PidOutputLimits,
    ) -> Result<Self, NewPidLoopError> {
        let name = name.into();
        let input_name = input_name.into();

        if name.trim().is_empty() {
            return Err(NewPidLoopError::new("PID loop name cannot be empty"));
        }

        if input_name.trim().is_empty() {
            return Err(NewPidLoopError::new(
                "PID loop input series name \
                     cannot be empty",
            ));
        }

        if !setpoint.is_finite() {
            return Err(NewPidLoopError::new("PID loop setpoint must be finite"));
        }

        Ok(Self {
            name,
            input_name,
            output_target,
            setpoint,
            gains,
            output_limits,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn input_name(&self) -> &str {
        &self.input_name
    }

    pub const fn output_target(&self) -> &OutputTarget {
        &self.output_target
    }

    pub const fn setpoint(&self) -> f64 {
        self.setpoint
    }

    pub const fn gains(&self) -> PidGains {
        self.gains
    }

    pub const fn output_limits(&self) -> PidOutputLimits {
        self.output_limits
    }

    pub fn into_parts(self) -> (String, String, OutputTarget, f64, PidGains, PidOutputLimits) {
        (
            self.name,
            self.input_name,
            self.output_target,
            self.setpoint,
            self.gains,
            self.output_limits,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewPidLoopError {
    message: String,
}

impl NewPidLoopError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for NewPidLoopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for NewPidLoopError {}

#[derive(Clone, Debug, PartialEq)]
pub struct NewOnOffLoop<OutputTarget> {
    name: String,
    input_name: String,
    output_target: OutputTarget,
    setpoint: f64,
    hysteresis: f64,
    output_off: f64,
    output_on: f64,
}

impl<OutputTarget> NewOnOffLoop<OutputTarget> {
    pub fn new(
        name: impl Into<String>,
        input_name: impl Into<String>,
        output_target: OutputTarget,
        setpoint: f64,
        hysteresis: f64,
        output_off: f64,
        output_on: f64,
    ) -> Result<Self, NewOnOffLoopError> {
        let name = name.into();
        let input_name = input_name.into();

        if name.trim().is_empty() {
            return Err(NewOnOffLoopError::new("On/off loop name cannot be empty"));
        }

        if input_name.trim().is_empty() {
            return Err(NewOnOffLoopError::new(
                "On/off loop input series name cannot be empty",
            ));
        }

        if !setpoint.is_finite() {
            return Err(NewOnOffLoopError::new("On/off setpoint must be finite"));
        }

        if !hysteresis.is_finite() {
            return Err(NewOnOffLoopError::new("On/off hysteresis must be finite"));
        }

        if hysteresis < 0.0 {
            return Err(NewOnOffLoopError::new(
                "On/off hysteresis must be non-negative",
            ));
        }

        if !output_off.is_finite() {
            return Err(NewOnOffLoopError::new(
                "On/off output-off value must be finite",
            ));
        }

        if !output_on.is_finite() {
            return Err(NewOnOffLoopError::new(
                "On/off output-on value must be finite",
            ));
        }

        let lower = setpoint - hysteresis;
        let upper = setpoint + hysteresis;

        if !lower.is_finite() || !upper.is_finite() {
            return Err(NewOnOffLoopError::new(
                "On/off hysteresis thresholds must be finite",
            ));
        }

        Ok(Self {
            name,
            input_name,
            output_target,
            setpoint,
            hysteresis,
            output_off,
            output_on,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn input_name(&self) -> &str {
        &self.input_name
    }

    pub const fn output_target(&self) -> &OutputTarget {
        &self.output_target
    }

    pub const fn setpoint(&self) -> f64 {
        self.setpoint
    }

    pub const fn hysteresis(&self) -> f64 {
        self.hysteresis
    }

    pub const fn output_off(&self) -> f64 {
        self.output_off
    }

    pub const fn output_on(&self) -> f64 {
        self.output_on
    }

    pub fn into_parts(self) -> (String, String, OutputTarget, f64, f64, f64, f64) {
        (
            self.name,
            self.input_name,
            self.output_target,
            self.setpoint,
            self.hysteresis,
            self.output_off,
            self.output_on,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewOnOffLoopError {
    message: String,
}

impl NewOnOffLoopError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for NewOnOffLoopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for NewOnOffLoopError {}

#[cfg(test)]
mod tests {
    use super::{NewOnOffLoop, NewPidLoop};

    use crate::process_control::{PidGains, PidOutputLimits};

    fn gains() -> PidGains {
        PidGains::new(2.0, 0.5, 0.1).unwrap()
    }

    fn limits() -> PidOutputLimits {
        PidOutputLimits::new(0.0, 100.0).unwrap()
    }

    #[test]
    fn stores_named_pid_loop_request() {
        let request = NewPidLoop::new(
            "heater",
            "temperature_filtered",
            17_u64,
            200.0,
            gains(),
            limits(),
        )
        .unwrap();

        assert_eq!(request.name(), "heater");

        assert_eq!(request.input_name(), "temperature_filtered",);

        assert_eq!(request.output_target(), &17,);

        assert_eq!(request.setpoint(), 200.0);

        assert_eq!(request.gains(), gains());

        assert_eq!(request.output_limits(), limits(),);

        assert_eq!(
            request.into_parts(),
            (
                "heater".to_owned(),
                "temperature_filtered".to_owned(),
                17,
                200.0,
                gains(),
                limits(),
            ),
        );
    }

    #[test]
    fn rejects_empty_loop_name() {
        let error = NewPidLoop::new("  ", "temperature", (), 100.0, gains(), limits()).unwrap_err();

        assert_eq!(error.to_string(), "PID loop name cannot be empty",);
    }

    #[test]
    fn rejects_empty_input_name() {
        let error = NewPidLoop::new("heater", "", (), 100.0, gains(), limits()).unwrap_err();

        assert_eq!(
            error.to_string(),
            "PID loop input series name \
             cannot be empty",
        );
    }

    #[test]
    fn rejects_non_finite_setpoint() {
        let error =
            NewPidLoop::new("heater", "temperature", (), f64::NAN, gains(), limits()).unwrap_err();

        assert_eq!(error.to_string(), "PID loop setpoint must be finite",);
    }

    #[test]
    fn stores_named_on_off_loop_request() {
        let request = NewOnOffLoop::new(
            "thermostat",
            "temperature_filtered",
            17_u64,
            150.0,
            2.0,
            0.0,
            100.0,
        )
        .unwrap();

        assert_eq!(request.name(), "thermostat");
        assert_eq!(request.input_name(), "temperature_filtered",);
        assert_eq!(request.output_target(), &17);
        assert_eq!(request.setpoint(), 150.0);
        assert_eq!(request.hysteresis(), 2.0);
        assert_eq!(request.output_off(), 0.0);
        assert_eq!(request.output_on(), 100.0);

        assert_eq!(
            request.into_parts(),
            (
                "thermostat".to_owned(),
                "temperature_filtered".to_owned(),
                17,
                150.0,
                2.0,
                0.0,
                100.0,
            ),
        );
    }

    #[test]
    fn rejects_invalid_on_off_loop_request() {
        assert!(
            NewOnOffLoop::new("thermostat", "temperature", (), 100.0, -1.0, 0.0, 100.0,).is_err()
        );

        assert!(NewOnOffLoop::new("", "temperature", (), 100.0, 2.0, 0.0, 100.0,).is_err());

        assert!(NewOnOffLoop::new("thermostat", "", (), 100.0, 2.0, 0.0, 100.0,).is_err());
    }
}
