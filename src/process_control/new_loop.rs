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

#[cfg(test)]
mod tests {
    use super::NewPidLoop;

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
}
