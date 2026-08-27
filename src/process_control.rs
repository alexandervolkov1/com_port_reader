mod control_loop;
mod controller;
mod diagnostic;
mod new_loop;
mod output_conversion;
mod output_target;
mod pid;
mod registry;

pub use control_loop::{ControlLoop, ControlLoopDefinition, ControlLoopDefinitionError};
pub use controller::{
    Controller, ControllerError, ControllerKind, ControllerOutput, ControllerParameter,
    ControllerParameterError,
};
pub use diagnostic::ControllerDiagnostic;
pub use new_loop::{NewPidLoop, NewPidLoopError};
pub use output_conversion::ControlOutputConversionError;
pub use output_target::{ControlOutputParameter, ControlOutputTarget, ControlOutputTargetError};
pub use pid::{
    PidController, PidControllerError, PidGains, PidGainsError, PidOutput, PidOutputLimits,
    PidOutputLimitsError,
};
pub use registry::{
    ControlEvent, ControlExecutionError, ControlOutput, ControllerAccessError, ControllerRegistry,
    ControllerRegistryError,
};
