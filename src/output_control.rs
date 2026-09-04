use std::{collections::HashMap, error::Error, fmt};

use crate::{
    instrument::{ConnectedParameterAddress, InstrumentWriteRequest},
    process_control::ControllerInstanceId,
};

mod service;

pub(crate) use service::{OutputHandle, OutputRequestError, OutputService};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OutputMode {
    Manual,
    AutomaticPending,

    #[default]
    Automatic,
}

impl OutputMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AutomaticPending => "automatic_pending",
            Self::Automatic => "automatic",
        }
    }
}

impl fmt::Display for OutputMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OutputSource {
    Manual,
    Controller {
        name: String,
        instance_id: ControllerInstanceId,
    },
    Safety,
}

impl OutputSource {
    pub(crate) fn controller(name: impl Into<String>, instance_id: ControllerInstanceId) -> Self {
        Self::Controller {
            name: name.into(),
            instance_id,
        }
    }

    const fn kind(&self) -> OutputSourceKind {
        match self {
            Self::Manual => OutputSourceKind::Manual,

            Self::Controller { .. } => OutputSourceKind::Controller,

            Self::Safety => OutputSourceKind::Safety,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputSourceKind {
    Manual,
    Controller,
    Safety,
}

impl OutputSourceKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Controller => "controller",
            Self::Safety => "safety",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct OutputState {
    mode: OutputMode,
    controller: String,
    instance_id: ControllerInstanceId,
    safe_request: Option<InstrumentWriteRequest>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AutomaticOutputIntent {
    target: ConnectedParameterAddress,
    controller: String,
    instance_id: ControllerInstanceId,
    request: InstrumentWriteRequest,
}

impl AutomaticOutputIntent {
    pub(crate) fn new(
        target: ConnectedParameterAddress,
        controller: impl Into<String>,
        instance_id: ControllerInstanceId,
        request: InstrumentWriteRequest,
    ) -> Self {
        Self {
            target,
            controller: controller.into(),
            instance_id,
            request,
        }
    }

    fn into_parts(
        self,
    ) -> (
        ConnectedParameterAddress,
        String,
        ControllerInstanceId,
        InstrumentWriteRequest,
    ) {
        (self.target, self.controller, self.instance_id, self.request)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ManualOutputIntent {
    target: ConnectedParameterAddress,
    request: InstrumentWriteRequest,
}

impl ManualOutputIntent {
    pub(crate) fn new(target: ConnectedParameterAddress, request: InstrumentWriteRequest) -> Self {
        Self { target, request }
    }

    fn into_parts(self) -> (ConnectedParameterAddress, InstrumentWriteRequest) {
        (self.target, self.request)
    }
}

#[derive(Default)]
pub(crate) struct OutputArbiter {
    outputs: HashMap<ConnectedParameterAddress, OutputState>,
}

impl OutputArbiter {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn contains(&self, target: ConnectedParameterAddress) -> bool {
        self.outputs.contains_key(&target)
    }

    pub(crate) fn request_automatic(&mut self, controller: &str) -> Result<(), OutputArbiterError> {
        let state = self
            .outputs
            .values_mut()
            .find(|state| state.controller == controller)
            .ok_or_else(|| OutputArbiterError::ControllerNotRegistered(controller.to_owned()))?;

        if state.mode == OutputMode::Manual {
            state.mode = OutputMode::AutomaticPending;
        }

        Ok(())
    }

    pub(crate) fn complete_automatic_transition(
        &mut self,
        target: ConnectedParameterAddress,
        controller: &str,
    ) -> Result<(), OutputArbiterError> {
        let state = self
            .outputs
            .get_mut(&target)
            .ok_or(OutputArbiterError::NotRegistered)?;

        if state.controller != controller {
            return Err(OutputArbiterError::ControllerMismatch {
                expected: state.controller.clone(),
                actual: controller.to_owned(),
            });
        }

        if state.mode == OutputMode::AutomaticPending {
            state.mode = OutputMode::Automatic;
        }

        Ok(())
    }

    pub(crate) fn register_controller(
        &mut self,
        target: ConnectedParameterAddress,
        controller: impl Into<String>,
        instance_id: ControllerInstanceId,
        safe_request: Option<InstrumentWriteRequest>,
    ) -> Result<(), OutputArbiterError> {
        if let Some(existing) = self.outputs.get(&target) {
            return Err(OutputArbiterError::AlreadyRegistered {
                controller: existing.controller.clone(),
            });
        }

        self.outputs.insert(
            target,
            OutputState {
                mode: OutputMode::Automatic,
                controller: controller.into(),
                instance_id,
                safe_request,
            },
        );

        Ok(())
    }

    pub(crate) fn unregister_controller(
        &mut self,
        target: ConnectedParameterAddress,
        controller: &str,
    ) -> Result<(), OutputArbiterError> {
        let state = self
            .outputs
            .get(&target)
            .ok_or(OutputArbiterError::NotRegistered)?;

        if state.controller != controller {
            return Err(OutputArbiterError::ControllerMismatch {
                expected: state.controller.clone(),
                actual: controller.to_owned(),
            });
        }

        self.outputs.remove(&target);

        Ok(())
    }

    pub(crate) fn release_controller(
        &mut self,
        controller: &str,
    ) -> Result<(), OutputArbiterError> {
        let (target, mode) = self
            .outputs
            .iter()
            .find(|(_, state)| state.controller == controller)
            .map(|(target, state)| (*target, state.mode))
            .ok_or_else(|| OutputArbiterError::ControllerNotRegistered(controller.to_owned()))?;

        if mode != OutputMode::Manual {
            return Err(OutputArbiterError::UnsafeControllerRelease {
                controller: controller.to_owned(),
                mode,
            });
        }

        self.outputs.remove(&target);

        Ok(())
    }

    pub(crate) fn mode(
        &self,
        target: ConnectedParameterAddress,
    ) -> Result<OutputMode, OutputArbiterError> {
        self.outputs
            .get(&target)
            .map(|state| state.mode)
            .ok_or(OutputArbiterError::NotRegistered)
    }

    pub(crate) fn set_mode(
        &mut self,
        target: ConnectedParameterAddress,
        mode: OutputMode,
    ) -> Result<(), OutputArbiterError> {
        let state = self
            .outputs
            .get_mut(&target)
            .ok_or(OutputArbiterError::NotRegistered)?;

        state.mode = mode;

        Ok(())
    }

    pub(crate) fn authorize(
        &self,
        target: ConnectedParameterAddress,
        source: &OutputSource,
    ) -> Result<(), OutputArbiterError> {
        let state = self
            .outputs
            .get(&target)
            .ok_or(OutputArbiterError::NotRegistered)?;

        match source {
            OutputSource::Safety => Ok(()),

            OutputSource::Manual => {
                if state.mode == OutputMode::Manual {
                    Ok(())
                } else {
                    Err(OutputArbiterError::SourceNotAllowed {
                        mode: state.mode,
                        source: source.kind(),
                    })
                }
            }

            OutputSource::Controller { name, instance_id } => {
                if name != &state.controller {
                    return Err(OutputArbiterError::ControllerMismatch {
                        expected: state.controller.clone(),
                        actual: name.clone(),
                    });
                }

                if instance_id != &state.instance_id {
                    return Err(OutputArbiterError::ControllerInstanceMismatch {
                        controller: name.clone(),
                        expected: state.instance_id,
                        actual: *instance_id,
                    });
                }

                if matches!(
                    state.mode,
                    OutputMode::Automatic | OutputMode::AutomaticPending
                ) {
                    Ok(())
                } else {
                    Err(OutputArbiterError::SourceNotAllowed {
                        mode: state.mode,
                        source: source.kind(),
                    })
                }
            }
        }
    }

    fn safe_output(
        &self,
        controller: &str,
    ) -> Result<(ConnectedParameterAddress, InstrumentWriteRequest), OutputArbiterError> {
        let (target, state) = self
            .outputs
            .iter()
            .find(|(_, state)| state.controller == controller)
            .ok_or_else(|| OutputArbiterError::ControllerNotRegistered(controller.to_owned()))?;

        let request = state
            .safe_request
            .ok_or_else(|| OutputArbiterError::SafeOutputNotConfigured(controller.to_owned()))?;

        Ok((*target, request))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OutputArbiterError {
    AlreadyRegistered {
        controller: String,
    },

    NotRegistered,
    ControllerNotRegistered(String),

    ControllerMismatch {
        expected: String,
        actual: String,
    },

    ControllerInstanceMismatch {
        controller: String,
        expected: ControllerInstanceId,
        actual: ControllerInstanceId,
    },

    SourceNotAllowed {
        mode: OutputMode,
        source: OutputSourceKind,
    },

    UnsafeControllerRelease {
        controller: String,
        mode: OutputMode,
    },

    SafeOutputNotConfigured(String),
}

impl fmt::Display for OutputArbiterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRegistered { controller } => {
                write!(
                    formatter,
                    "Output is already registered \
                     for controller '{controller}'",
                )
            }

            Self::NotRegistered => formatter.write_str("Output is not registered"),

            Self::ControllerNotRegistered(controller) => {
                write!(
                    formatter,
                    "Controller '{controller}' \
                     does not own a registered output",
                )
            }

            Self::ControllerMismatch { expected, actual } => {
                write!(
                    formatter,
                    "Output belongs to controller \
                     '{expected}', not '{actual}'",
                )
            }

            Self::ControllerInstanceMismatch {
                controller,
                expected,
                actual,
            } => {
                write!(
                    formatter,
                    "Output belongs to controller \
                     '{controller}' instance {expected}, \
                     not instance {actual}",
                )
            }

            Self::SourceNotAllowed { mode, source } => {
                write!(
                    formatter,
                    "Output source '{}' is not \
                     allowed in {mode} mode",
                    source.as_str(),
                )
            }

            Self::UnsafeControllerRelease { controller, mode } => {
                write!(
                    formatter,
                    "Controller '{controller}' \
                     cannot release its output \
                     while it is in {mode} mode",
                )
            }

            Self::SafeOutputNotConfigured(controller) => {
                write!(
                    formatter,
                    "Controller '{controller}' \
                     does not have a configured \
                     safe output",
                )
            }
        }
    }
}

impl Error for OutputArbiterError {}

#[cfg(test)]
mod tests {
    use crate::{
        connection::ConnectionId,
        instrument::{
            ConnectedParameterAddress, InstrumentParameterAddress,
            virtual_instrument::{VirtualInstrumentId, VirtualParameterId},
        },
        process_control::ControllerInstanceId,
    };

    use super::{OutputArbiter, OutputArbiterError, OutputMode, OutputSource, OutputSourceKind};

    fn target() -> ConnectedParameterAddress {
        ConnectedParameterAddress::new(
            ConnectionId::new(2),
            InstrumentParameterAddress::virtual_instrument(
                VirtualInstrumentId::new(7),
                VirtualParameterId::new(4),
            ),
        )
    }

    fn instance_id() -> ControllerInstanceId {
        ControllerInstanceId::for_test(1)
    }

    fn other_instance_id() -> ControllerInstanceId {
        ControllerInstanceId::for_test(2)
    }

    #[test]
    fn registers_controller_in_automatic_mode() {
        let mut arbiter = OutputArbiter::new();

        let target = target();

        arbiter
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        assert_eq!(arbiter.mode(target), Ok(OutputMode::Automatic),);

        assert_eq!(
            arbiter.authorize(target, &OutputSource::controller("heater", instance_id(),),),
            Ok(()),
        );
    }

    #[test]
    fn switches_authority_between_automatic_and_manual() {
        let mut arbiter = OutputArbiter::new();

        let target = target();

        arbiter
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        assert_eq!(
            arbiter.authorize(target, &OutputSource::Manual,),
            Err(OutputArbiterError::SourceNotAllowed {
                mode: OutputMode::Automatic,
                source: OutputSourceKind::Manual,
            },),
        );

        arbiter.set_mode(target, OutputMode::Manual).unwrap();

        assert_eq!(arbiter.authorize(target, &OutputSource::Manual,), Ok(()),);

        assert_eq!(
            arbiter.authorize(target, &OutputSource::controller("heater", instance_id(),),),
            Err(OutputArbiterError::SourceNotAllowed {
                mode: OutputMode::Manual,
                source: OutputSourceKind::Controller,
            },),
        );
    }

    #[test]
    fn rejects_another_controller() {
        let mut arbiter = OutputArbiter::new();

        let target = target();

        arbiter
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        assert_eq!(
            arbiter.authorize(target, &OutputSource::controller("other", instance_id(),),),
            Err(OutputArbiterError::ControllerMismatch {
                expected: "heater".to_owned(),
                actual: "other".to_owned(),
            },),
        );
    }

    #[test]
    fn safety_overrides_manual_and_automatic_modes() {
        let mut arbiter = OutputArbiter::new();

        let target = target();

        arbiter
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        assert_eq!(arbiter.authorize(target, &OutputSource::Safety,), Ok(()),);

        arbiter.set_mode(target, OutputMode::Manual).unwrap();

        assert_eq!(arbiter.authorize(target, &OutputSource::Safety,), Ok(()),);
    }

    #[test]
    fn rejects_duplicate_registration() {
        let mut arbiter = OutputArbiter::new();

        let target = target();

        arbiter
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        assert_eq!(
            arbiter.register_controller(target, "other", instance_id(), None),
            Err(OutputArbiterError::AlreadyRegistered {
                controller: "heater".to_owned(),
            },),
        );
    }

    #[test]
    fn rejects_unregistered_output() {
        let arbiter = OutputArbiter::new();

        assert_eq!(
            arbiter.authorize(target(), &OutputSource::Manual,),
            Err(OutputArbiterError::NotRegistered,),
        );
    }

    #[test]
    fn unregisters_matching_controller() {
        let mut arbiter = OutputArbiter::new();

        let target = target();

        arbiter
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        assert_eq!(arbiter.unregister_controller(target, "heater",), Ok(()),);

        assert_eq!(
            arbiter.mode(target),
            Err(OutputArbiterError::NotRegistered,),
        );
    }

    #[test]
    fn does_not_unregister_another_controller() {
        let mut arbiter = OutputArbiter::new();

        let target = target();

        arbiter
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        assert_eq!(
            arbiter.unregister_controller(target, "other",),
            Err(OutputArbiterError::ControllerMismatch {
                expected: "heater".to_owned(),
                actual: "other".to_owned(),
            },),
        );

        assert_eq!(arbiter.mode(target), Ok(OutputMode::Automatic),);
    }

    #[test]
    fn requests_automatic_takeover_without_completing_it() {
        let mut arbiter = OutputArbiter::new();

        let target = target();

        arbiter
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        arbiter.set_mode(target, OutputMode::Manual).unwrap();

        arbiter.request_automatic("heater").unwrap();

        assert_eq!(arbiter.mode(target), Ok(OutputMode::AutomaticPending,),);

        assert_eq!(
            arbiter.authorize(target, &OutputSource::controller("heater", instance_id(),),),
            Ok(()),
        );
    }

    #[test]
    fn rejects_controller_release_while_automatic() {
        let mut arbiter = OutputArbiter::new();

        let target = target();

        arbiter
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        assert_eq!(
            arbiter.release_controller("heater",),
            Err(OutputArbiterError::UnsafeControllerRelease {
                controller: "heater".to_owned(),
                mode: OutputMode::Automatic,
            },),
        );

        assert_eq!(arbiter.mode(target), Ok(OutputMode::Automatic),);
    }

    #[test]
    fn releases_controller_after_entering_manual_mode() {
        let mut arbiter = OutputArbiter::new();

        let target = target();

        arbiter
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        arbiter.set_mode(target, OutputMode::Manual).unwrap();

        arbiter.release_controller("heater").unwrap();

        assert_eq!(
            arbiter.mode(target),
            Err(OutputArbiterError::NotRegistered,),
        );
    }

    #[test]
    fn rejects_stale_controller_instance() {
        let mut arbiter = OutputArbiter::new();

        let target = target();

        arbiter
            .register_controller(target, "heater", instance_id(), None)
            .unwrap();

        assert_eq!(
            arbiter.authorize(
                target,
                &OutputSource::controller("heater", other_instance_id(),),
            ),
            Err(OutputArbiterError::ControllerInstanceMismatch {
                controller: "heater".to_owned(),
                expected: instance_id(),
                actual: other_instance_id(),
            },),
        );

        assert_eq!(arbiter.mode(target), Ok(OutputMode::Automatic),);
    }
}
