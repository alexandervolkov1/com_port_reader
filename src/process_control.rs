mod control_loop;
mod controller;
mod diagnostic;
mod furnace;
mod new_loop;
mod on_off;
mod output_conversion;
mod output_target;
mod pid;
mod reference;
mod reference_runtime;
mod registry;

pub(crate) use control_loop::ControllerInstanceId;
pub use control_loop::{
    ControlLoop, ControlLoopDefinition, ControlLoopDefinitionError, ControlLoopExecutionError,
    ControlLoopParameterError, ControlLoopReferenceError, ControlLoopState,
};
pub use controller::{
    Controller, ControllerDiagnosticError, ControllerError, ControllerKind, ControllerOperation,
    ControllerOperationError, ControllerOutput, ControllerParameter, ControllerParameterError,
};
pub use diagnostic::ControllerDiagnostic;
pub use furnace::{
    FurnaceController, FurnaceControllerError, FurnaceGains, FurnaceModel, FurnaceOutput,
    FurnaceOutputLimits,
};
pub use new_loop::{NewController, NewControllerError};
pub use on_off::{OnOffController, OnOffControllerError, OnOffOutput};
pub use output_conversion::ControlOutputConversionError;
pub use output_target::{ControlOutputParameter, ControlOutputTarget, ControlOutputTargetError};
pub use pid::{
    PidController, PidControllerError, PidGains, PidGainsError, PidOutput, PidOutputLimits,
    PidOutputLimitsError,
};
pub use reference::{
    FixedReference, RampReference, ReferenceKind, ReferenceParameter, ReferenceParameterError,
    ReferenceSource, ReferenceSourceError, ReferenceSourceParameter,
};
pub use reference_runtime::{ReferenceRuntime, ReferenceRuntimeError};
pub use registry::{
    ControlEvent, ControlExecutionError, ControlOutput, ControllerAccessError, ControllerRegistry,
    ControllerRegistryError,
};
