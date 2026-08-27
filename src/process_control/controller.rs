use std::{error::Error, fmt};

use crate::instrument::ParameterRange;

use super::{PidController, PidControllerError, PidOutput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerKind {
    Pid,
}

impl ControllerKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pid => "pid",
        }
    }
}

impl fmt::Display for ControllerKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug)]
pub enum Controller {
    Pid(PidController),
}

impl Controller {
    pub const fn kind(&self) -> ControllerKind {
        match self {
            Self::Pid(_) => ControllerKind::Pid,
        }
    }

    pub fn output_range(&self) -> ParameterRange {
        match self {
            Self::Pid(controller) => {
                let limits = controller.output_limits();

                ParameterRange::Number {
                    minimum: limits.minimum(),
                    maximum: limits.maximum(),
                }
            }
        }
    }

    pub fn update(
        &mut self,
        timestamp: f64,
        measurement: f64,
    ) -> Result<ControllerOutput, ControllerError> {
        match self {
            Self::Pid(controller) => {
                let setpoint = controller.setpoint();

                let output = controller
                    .update(timestamp, measurement)
                    .map_err(ControllerError::Pid)?;

                Ok(ControllerOutput::Pid { setpoint, output })
            }
        }
    }

    pub fn reset(&mut self) {
        match self {
            Self::Pid(controller) => {
                controller.reset();
            }
        }
    }
}

impl From<PidController> for Controller {
    fn from(controller: PidController) -> Self {
        Self::Pid(controller)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControllerOutput {
    Pid { setpoint: f64, output: PidOutput },
}

impl ControllerOutput {
    pub const fn kind(&self) -> ControllerKind {
        match self {
            Self::Pid { .. } => ControllerKind::Pid,
        }
    }

    pub const fn value(&self) -> f64 {
        match self {
            Self::Pid { output, .. } => output.value(),
        }
    }

    pub const fn setpoint(&self) -> Option<f64> {
        match self {
            Self::Pid { setpoint, .. } => Some(*setpoint),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControllerError {
    Pid(PidControllerError),
}

impl fmt::Display for ControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pid(error) => error.fmt(formatter),
        }
    }
}

impl Error for ControllerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Pid(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Controller, ControllerError, ControllerKind, ControllerOutput};

    use crate::{
        instrument::ParameterRange,
        process_control::{PidController, PidControllerError, PidGains, PidOutputLimits},
    };

    fn controller() -> Controller {
        PidController::with_output_limits(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap()
        .into()
    }

    #[test]
    fn exposes_controller_kind() {
        let controller = controller();

        assert_eq!(controller.kind(), ControllerKind::Pid,);

        assert_eq!(controller.kind().as_str(), "pid",);
    }

    #[test]
    fn exposes_controller_output_range() {
        let controller = controller();

        assert_eq!(
            controller.output_range(),
            ParameterRange::Number {
                minimum: 0.0,
                maximum: 100.0,
            },
        );
    }

    #[test]
    fn updates_pid_through_controller_api() {
        let mut controller = controller();

        let output = controller.update(1_000.0, 80.0).unwrap();

        assert_eq!(output.kind(), ControllerKind::Pid,);

        assert_eq!(output.setpoint(), Some(100.0),);

        assert_eq!(output.value(), 40.0,);

        let ControllerOutput::Pid { setpoint, output } = output;

        assert_eq!(setpoint, 100.0,);

        assert_eq!(output.proportional(), 40.0,);
    }

    #[test]
    fn wraps_pid_controller_error() {
        let mut controller = controller();

        assert_eq!(
            controller.update(f64::NAN, 80.0,),
            Err(ControllerError::Pid(PidControllerError::NonFiniteTimestamp,),),
        );
    }

    #[test]
    fn resets_pid_through_controller_api() {
        let mut controller = PidController::with_output_limits(
            100.0,
            PidGains::new(0.0, 1.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap();

        controller.update(0.0, 90.0).unwrap();

        controller.update(1.0, 90.0).unwrap();

        assert_eq!(controller.integral(), 10.0,);

        let mut controller: Controller = controller.into();

        controller.reset();

        let output = controller.update(0.0, 90.0).unwrap();

        let ControllerOutput::Pid { output, .. } = output;

        assert_eq!(output.integral(), 0.0,);
    }
}
