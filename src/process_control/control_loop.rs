use std::{
    error::Error,
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::instrument::{InstrumentValue, ParameterAccess, ParameterDescriptor, ParameterRange};

use super::{
    Controller, ControllerDiagnostic, ControllerDiagnosticError, ControllerError,
    ControllerOperationError, ControllerOutput, ControllerParameterError, ReferenceKind,
    ReferenceParameter, ReferenceParameterError, ReferenceRuntime, ReferenceRuntimeError,
    ReferenceSource, ReferenceSourceError,
};

static NEXT_CONTROLLER_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ControllerInstanceId(u64);

impl ControllerInstanceId {
    fn next() -> Self {
        Self(NEXT_CONTROLLER_INSTANCE_ID.fetch_add(1, Ordering::Relaxed))
    }

    #[cfg(test)]
    pub(crate) const fn for_test(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Display for ControllerInstanceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug)]
pub struct ControlLoopDefinition<SignalId, OutputTarget> {
    instance_id: ControllerInstanceId,
    name: String,
    input: SignalId,
    output_target: OutputTarget,
    controller: Controller,
    reference: Option<ReferenceSource>,
}

impl<SignalId, OutputTarget> ControlLoopDefinition<SignalId, OutputTarget> {
    pub fn new(
        name: impl Into<String>,
        input: SignalId,
        output_target: OutputTarget,
        controller: Controller,
    ) -> Result<Self, ControlLoopDefinitionError> {
        let name = name.into();
        let name = normalize_name(&name)?;

        Ok(Self {
            instance_id: ControllerInstanceId::next(),
            name: name.to_owned(),
            input,
            output_target,
            controller,
            reference: None,
        })
    }

    pub fn with_reference(mut self, reference: ReferenceSource) -> Self {
        self.reference = Some(reference);
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) const fn instance_id(&self) -> ControllerInstanceId {
        self.instance_id
    }

    pub const fn input(&self) -> &SignalId {
        &self.input
    }

    pub const fn output_target(&self) -> &OutputTarget {
        &self.output_target
    }

    pub const fn controller(&self) -> &Controller {
        &self.controller
    }

    pub const fn reference(&self) -> Option<ReferenceSource> {
        self.reference
    }

    fn into_parts(
        self,
    ) -> (
        ControllerInstanceId,
        String,
        SignalId,
        OutputTarget,
        Controller,
        Option<ReferenceSource>,
    ) {
        (
            self.instance_id,
            self.name,
            self.input,
            self.output_target,
            self.controller,
            self.reference,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlLoopState {
    #[default]
    Running,
    Paused,
}

impl ControlLoopState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
        }
    }
}

#[derive(Debug)]
pub struct ControlLoop<SignalId, OutputTarget> {
    instance_id: ControllerInstanceId,
    name: String,
    input: SignalId,
    output_target: OutputTarget,
    controller: Controller,
    reference: Option<ReferenceRuntime>,
    state: ControlLoopState,
}

impl<SignalId, OutputTarget> ControlLoop<SignalId, OutputTarget> {
    pub fn new(definition: ControlLoopDefinition<SignalId, OutputTarget>) -> Self {
        let (instance_id, name, input, output_target, controller, reference) =
            definition.into_parts();

        Self {
            instance_id,
            name,
            input,
            output_target,
            controller,
            reference: reference.map(ReferenceRuntime::new),
            state: ControlLoopState::Running,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) const fn instance_id(&self) -> ControllerInstanceId {
        self.instance_id
    }

    pub const fn input(&self) -> &SignalId {
        &self.input
    }

    pub fn set_input(&mut self, input: SignalId)
    where
        SignalId: PartialEq,
    {
        if self.input == input {
            return;
        }

        self.input = input;
        self.controller.resynchronize();
    }

    pub const fn output_target(&self) -> &OutputTarget {
        &self.output_target
    }

    pub fn reference_source(&self) -> Option<ReferenceSource> {
        self.reference.as_ref().map(ReferenceRuntime::source)
    }

    pub fn reference_kind(&self) -> Option<ReferenceKind> {
        self.reference
            .as_ref()
            .map(|reference| reference.source().kind())
    }

    pub fn reference_parameters(&self) -> Vec<ParameterDescriptor> {
        self.reference
            .as_ref()
            .map(ReferenceRuntime::parameters)
            .unwrap_or_default()
    }

    pub fn read_reference_parameter(
        &self,
        key: &str,
    ) -> Result<InstrumentValue, ControlLoopReferenceError> {
        self.reference
            .as_ref()
            .ok_or(ControlLoopReferenceError::NotConfigured)?
            .read(key)
            .map_err(Into::into)
    }

    pub fn write_reference_parameter(
        &mut self,
        key: &str,
        value: InstrumentValue,
    ) -> Result<InstrumentValue, ControlLoopReferenceError> {
        self.reference
            .as_mut()
            .ok_or(ControlLoopReferenceError::NotConfigured)?
            .write(key, value)
            .map_err(Into::into)
    }

    pub fn configure_reference<I, K>(&mut self, updates: I) -> Result<(), ControlLoopReferenceError>
    where
        I: IntoIterator<Item = (K, InstrumentValue)>,
        K: AsRef<str>,
    {
        self.reference
            .as_mut()
            .ok_or(ControlLoopReferenceError::NotConfigured)?
            .configure(updates)
            .map_err(Into::into)
    }

    pub fn set_reference(&mut self, source: ReferenceSource) {
        match &mut self.reference {
            Some(reference) => {
                reference.set_source(source);
            }

            None => {
                self.reference = Some(ReferenceRuntime::new(source));
            }
        }

        self.controller.resynchronize();
    }

    pub const fn state(&self) -> ControlLoopState {
        self.state
    }

    pub const fn is_running(&self) -> bool {
        matches!(self.state, ControlLoopState::Running)
    }

    pub fn parameters(&self) -> Vec<ParameterDescriptor> {
        let mut setpoint = ReferenceParameter::Setpoint.descriptor();

        if self.reference.is_some() {
            setpoint.access = ParameterAccess::ReadOnly;
        }

        let mut parameters = vec![setpoint];

        parameters.extend(self.controller.parameters());

        parameters
    }

    pub fn read_parameter(&self, key: &str) -> Result<InstrumentValue, ControlLoopParameterError> {
        if ReferenceParameter::from_key(key).is_some() {
            if let Some(reference) = &self.reference {
                let value = reference
                    .current_value()
                    .map_err(ControlLoopParameterError::Reference)?;

                return Ok(InstrumentValue::Number(value));
            }

            return self.controller.read(key).map_err(Into::into);
        }

        self.controller.read(key).map_err(Into::into)
    }

    pub fn write_parameter(
        &mut self,
        key: &str,
        value: InstrumentValue,
    ) -> Result<InstrumentValue, ControlLoopParameterError> {
        if let Some(parameter) = ReferenceParameter::from_key(key) {
            if self.reference.is_some() {
                return Err(ControlLoopParameterError::ManagedByReference(parameter));
            }

            return self.controller.write(key, value).map_err(Into::into);
        }

        self.controller.write(key, value).map_err(Into::into)
    }

    fn validate_configuration_updates(
        &self,
        updates: &[(String, InstrumentValue)],
    ) -> Result<(), ControlLoopParameterError> {
        if self.reference.is_some() {
            for (key, _) in updates {
                if let Some(parameter) = ReferenceParameter::from_key(key) {
                    return Err(ControlLoopParameterError::ManagedByReference(parameter));
                }
            }
        }

        Ok(())
    }

    pub fn configure<I, K>(&mut self, updates: I) -> Result<(), ControlLoopParameterError>
    where
        I: IntoIterator<Item = (K, InstrumentValue)>,
        K: AsRef<str>,
    {
        let updates = updates
            .into_iter()
            .map(|(key, value)| (key.as_ref().to_owned(), value))
            .collect::<Vec<_>>();

        self.validate_configuration_updates(&updates)?;

        self.controller.configure(updates).map_err(Into::into)
    }

    pub(crate) fn output_range_after_configuration(
        &self,
        updates: &[(String, InstrumentValue)],
    ) -> Result<ParameterRange, ControlLoopParameterError> {
        self.validate_configuration_updates(updates)?;

        self.controller
            .output_range_after_configuration(
                updates.iter().map(|(key, value)| (key.as_str(), *value)),
            )
            .map_err(Into::into)
    }

    pub fn update(
        &mut self,
        timestamp: f64,
        measurement: f64,
    ) -> Result<ControllerOutput, ControlLoopExecutionError> {
        if let Some(reference) = &mut self.reference {
            let setpoint = reference
                .update(timestamp)
                .map_err(ControlLoopExecutionError::Reference)?;

            self.controller
                .apply_reference(setpoint)
                .map_err(ControlLoopExecutionError::ReferenceApplication)?;
        }

        self.controller
            .update(timestamp, measurement)
            .map_err(ControlLoopExecutionError::Controller)
    }

    pub fn process(
        &mut self,
        timestamp: f64,
        measurement: f64,
    ) -> Result<Option<ControllerOutput>, ControlLoopExecutionError> {
        if !self.is_running() {
            return Ok(None);
        }

        self.update(timestamp, measurement).map(Some)
    }

    pub fn pause(&mut self) {
        self.state = ControlLoopState::Paused;
    }

    pub fn resume(&mut self) {
        if self.state == ControlLoopState::Paused {
            self.controller.resynchronize();

            if let Some(reference) = &mut self.reference {
                reference.resynchronize();
            }

            self.state = ControlLoopState::Running;
        }
    }

    pub fn reset_integral(&mut self) -> Result<(), ControllerOperationError> {
        self.controller.reset_integral()
    }

    pub fn reset(&mut self) {
        self.controller.reset();

        if let Some(reference) = &mut self.reference {
            reference.reset();
        }
    }

    pub fn diagnostics(&self) -> &'static [ControllerDiagnostic] {
        self.controller.diagnostics()
    }

    pub fn validate_diagnostic(
        &self,
        diagnostic: ControllerDiagnostic,
    ) -> Result<(), ControllerDiagnosticError> {
        self.controller.validate_diagnostic(diagnostic)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlLoopReferenceError {
    NotConfigured,

    Parameter(ReferenceParameterError),
}

impl fmt::Display for ControlLoopReferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConfigured => formatter.write_str(
                "Control loop has no \
                     reference source",
            ),

            Self::Parameter(error) => error.fmt(formatter),
        }
    }
}

impl Error for ControlLoopReferenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NotConfigured => None,

            Self::Parameter(error) => Some(error),
        }
    }
}

impl From<ReferenceParameterError> for ControlLoopReferenceError {
    fn from(error: ReferenceParameterError) -> Self {
        Self::Parameter(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlLoopParameterError {
    ManagedByReference(ReferenceParameter),

    Reference(ReferenceSourceError),

    Controller(ControllerParameterError),
}

impl fmt::Display for ControlLoopParameterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ManagedByReference(parameter) => {
                write!(
                    formatter,
                    "Reference parameter '{}' \
                     is managed by an external \
                     reference source",
                    parameter.key(),
                )
            }

            Self::Reference(error) => error.fmt(formatter),

            Self::Controller(error) => error.fmt(formatter),
        }
    }
}

impl Error for ControlLoopParameterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ManagedByReference(_) => None,

            Self::Reference(error) => Some(error),

            Self::Controller(error) => Some(error),
        }
    }
}

impl From<ControllerParameterError> for ControlLoopParameterError {
    fn from(error: ControllerParameterError) -> Self {
        Self::Controller(error)
    }
}

impl From<ReferenceSourceError> for ControlLoopParameterError {
    fn from(error: ReferenceSourceError) -> Self {
        Self::Reference(error)
    }
}

#[derive(Debug)]
pub enum ControlLoopExecutionError {
    Reference(ReferenceRuntimeError),

    ReferenceApplication(ControllerParameterError),

    Controller(ControllerError),
}

impl fmt::Display for ControlLoopExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reference(error) => {
                write!(
                    formatter,
                    "Reference update failed: \
                     {error}",
                )
            }

            Self::ReferenceApplication(error) => {
                write!(
                    formatter,
                    "Reference value could not \
                     be applied to controller: \
                     {error}",
                )
            }

            Self::Controller(error) => error.fmt(formatter),
        }
    }
}

impl Error for ControlLoopExecutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Reference(error) => Some(error),

            Self::ReferenceApplication(error) => Some(error),

            Self::Controller(error) => Some(error),
        }
    }
}

fn normalize_name(name: &str) -> Result<&str, ControlLoopDefinitionError> {
    let name = name.trim();

    if name.is_empty() {
        return Err(ControlLoopDefinitionError::EmptyName);
    }

    if name.chars().any(char::is_whitespace) {
        return Err(ControlLoopDefinitionError::NameContainsWhitespace);
    }

    Ok(name)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlLoopDefinitionError {
    EmptyName,
    NameContainsWhitespace,
}

impl fmt::Display for ControlLoopDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("Control loop name cannot be empty"),

            Self::NameContainsWhitespace => {
                formatter.write_str("Control loop name cannot contain whitespace")
            }
        }
    }
}

impl Error for ControlLoopDefinitionError {}

#[cfg(test)]
mod tests {
    use super::{
        ControlLoop, ControlLoopDefinition, ControlLoopDefinitionError, ControlLoopParameterError,
        ControlLoopReferenceError, ControlLoopState,
    };

    use crate::instrument::{InstrumentValue, ParameterAccess};

    use crate::process_control::{
        ControllerKind, OnOffController, PidController, PidGains, PidOutputLimits, ReferenceKind,
        ReferenceParameter, ReferenceSource,
    };

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TestOutputTarget {
        connection: u64,
        instrument: u16,
        parameter: u16,
    }

    fn on_off_definition() -> ControlLoopDefinition<u64, TestOutputTarget> {
        let controller = OnOffController::new(100.0, 2.0, 0.0, 100.0).unwrap().into();

        ControlLoopDefinition::new(
            "thermostat",
            1_u64,
            TestOutputTarget {
                connection: 2,
                instrument: 7,
                parameter: 3,
            },
            controller,
        )
        .unwrap()
    }

    #[test]
    fn creates_control_loop_definition() {
        let output_target = TestOutputTarget {
            connection: 2,
            instrument: 7,
            parameter: 3,
        };

        let gains = PidGains::new(2.0, 0.5, 1.0).unwrap();

        let limits = PidOutputLimits::new(0.0, 100.0).unwrap();

        let controller = PidController::with_output_limits(200.0, gains, limits)
            .unwrap()
            .into();

        let definition =
            ControlLoopDefinition::new("  heater  ", 11_u64, output_target, controller).unwrap();

        assert_eq!(definition.name(), "heater",);

        assert_eq!(*definition.input(), 11,);

        assert_eq!(*definition.output_target(), output_target,);
    }

    #[test]
    fn stores_control_loop_reference() {
        let reference = ReferenceSource::ramp(20.0, 100.0, 10.0).unwrap();

        let definition = definition_with(
            100.0,
            PidGains::new(1.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .with_reference(reference);

        assert_eq!(definition.reference().unwrap().kind(), ReferenceKind::Ramp,);
    }

    #[test]
    fn rejects_invalid_control_loop_names() {
        let gains = PidGains::new(1.0, 0.0, 0.0).unwrap();

        let limits = PidOutputLimits::new(0.0, 100.0).unwrap();

        let output_target = TestOutputTarget {
            connection: 1,
            instrument: 1,
            parameter: 1,
        };

        for name in ["", " ", "\t"] {
            let controller = PidController::with_output_limits(100.0, gains, limits)
                .unwrap()
                .into();

            let error =
                ControlLoopDefinition::new(name, 1_u64, output_target, controller).unwrap_err();

            assert_eq!(error, ControlLoopDefinitionError::EmptyName,);
        }

        for name in ["heater one", "heater\tone", "heater\none"] {
            let controller = PidController::with_output_limits(100.0, gains, limits)
                .unwrap()
                .into();

            let error =
                ControlLoopDefinition::new(name, 1_u64, output_target, controller).unwrap_err();

            assert_eq!(error, ControlLoopDefinitionError::NameContainsWhitespace,);
        }
    }

    #[test]
    fn updates_loop_using_its_setpoint() {
        let definition = definition_with(
            80.0,
            PidGains::new(2.0, 1.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        let first = control_loop.update(0.0, 50.0).unwrap();

        assert_close(first.value(), 60.0);

        let second = control_loop.update(1.0, 40.0).unwrap();

        assert_close(second.value(), 100.0);

        assert_close(second.integral().unwrap(), 20.0);

        assert_eq!(second.saturated(), Some(true),);
    }

    #[test]
    fn updates_controller_setpoint_from_ramp_reference() {
        let definition = definition_with(
            999.0,
            PidGains::new(1.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 200.0).unwrap(),
        )
        .with_reference(ReferenceSource::ramp(20.0, 100.0, 10.0).unwrap());

        let mut control_loop = ControlLoop::new(definition);

        let first = control_loop.update(10.0, 0.0).unwrap();

        assert_eq!(first.setpoint(), Some(20.0),);

        let second = control_loop.update(12.0, 0.0).unwrap();

        assert_eq!(second.setpoint(), Some(40.0),);

        let third = control_loop.update(18.0, 0.0).unwrap();

        assert_eq!(third.setpoint(), Some(100.0),);

        assert_eq!(
            control_loop.read_parameter("setpoint"),
            Ok(InstrumentValue::Number(100.0,),),
        );
    }

    #[test]
    fn reset_preserves_loop_configuration() {
        let definition = definition_with(
            100.0,
            PidGains::new(0.0, 1.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        control_loop.update(0.0, 90.0).unwrap();

        let accumulated = control_loop.update(1.0, 90.0).unwrap();

        assert_eq!(accumulated.integral(), Some(10.0),);

        control_loop.reset();

        assert_eq!(control_loop.name(), "heater",);

        assert_eq!(
            control_loop.read_parameter("setpoint",),
            Ok(InstrumentValue::Number(100.0,),),
        );

        let restarted = control_loop.update(0.0, 90.0).unwrap();

        assert_eq!(restarted.integral(), Some(0.0),);
    }

    fn definition_with(
        setpoint: f64,
        gains: PidGains,
        output_limits: PidOutputLimits,
    ) -> ControlLoopDefinition<u64, TestOutputTarget> {
        let controller = PidController::with_output_limits(setpoint, gains, output_limits)
            .unwrap()
            .into();

        ControlLoopDefinition::new(
            "heater",
            1_u64,
            TestOutputTarget {
                connection: 2,
                instrument: 7,
                parameter: 3,
            },
            controller,
        )
        .unwrap()
    }

    fn assert_close(actual: f64, expected: f64) {
        let tolerance = expected.abs().max(1.0) * 1.0e-12;

        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}",
        );
    }

    #[test]
    fn reads_controller_parameter() {
        let definition = definition_with(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let control_loop = ControlLoop::new(definition);

        assert_eq!(
            control_loop.read_parameter("setpoint",),
            Ok(InstrumentValue::Number(100.0,),),
        );

        assert_eq!(
            control_loop.read_parameter("kp",),
            Ok(InstrumentValue::Number(2.0,),),
        );
    }

    #[test]
    fn writes_controller_parameter() {
        let definition = definition_with(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        assert_eq!(
            control_loop.write_parameter("setpoint", InstrumentValue::Number(120.0,),),
            Ok(InstrumentValue::Number(120.0,),),
        );

        assert_eq!(
            control_loop.read_parameter("setpoint",),
            Ok(InstrumentValue::Number(120.0,),),
        );
    }

    #[test]
    fn rejects_setpoint_write_when_reference_is_active() {
        let definition = definition_with(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .with_reference(ReferenceSource::ramp(20.0, 100.0, 10.0).unwrap());

        let mut control_loop = ControlLoop::new(definition);

        assert_eq!(
            control_loop.write_parameter("setpoint", InstrumentValue::Number(120.0,),),
            Err(ControlLoopParameterError::ManagedByReference(
                ReferenceParameter::Setpoint,
            ),),
        );

        assert_eq!(
            control_loop.read_parameter("setpoint",),
            Ok(InstrumentValue::Number(20.0,),),
        );
    }

    #[test]
    fn rejects_mixed_configuration_when_reference_is_active() {
        let definition = definition_with(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .with_reference(ReferenceSource::fixed(150.0).unwrap());

        let mut control_loop = ControlLoop::new(definition);

        assert_eq!(
            control_loop.configure([
                ("kp", InstrumentValue::Number(3.0,),),
                ("setpoint", InstrumentValue::Number(120.0,),),
            ]),
            Err(ControlLoopParameterError::ManagedByReference(
                ReferenceParameter::Setpoint,
            ),),
        );

        assert_eq!(
            control_loop.read_parameter("kp"),
            Ok(InstrumentValue::Number(2.0,),),
        );

        assert_eq!(
            control_loop.read_parameter("setpoint",),
            Ok(InstrumentValue::Number(150.0,),),
        );
    }

    #[test]
    fn configures_controller_parameters_atomically() {
        let definition = definition_with(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        control_loop
            .configure([
                ("output_min", InstrumentValue::Number(200.0)),
                ("output_max", InstrumentValue::Number(300.0)),
            ])
            .unwrap();

        assert_eq!(
            control_loop.read_parameter("output_min",),
            Ok(InstrumentValue::Number(200.0,),),
        );

        assert_eq!(
            control_loop.read_parameter("output_max",),
            Ok(InstrumentValue::Number(300.0,),),
        );
    }

    #[test]
    fn exposes_controller_parameter_descriptors() {
        let control_loop = ControlLoop::new(definition_with(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        ));

        let keys = control_loop
            .parameters()
            .into_iter()
            .map(|parameter| parameter.key)
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec!["setpoint", "kp", "ki", "kd", "output_min", "output_max",],
        );
    }

    #[test]
    fn pauses_and_resumes_without_losing_integral() {
        let definition = definition_with(
            100.0,
            PidGains::new(0.0, 1.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        assert_eq!(control_loop.state(), ControlLoopState::Running,);

        control_loop.process(0.0, 90.0).unwrap();

        let accumulated = control_loop.process(1.0, 90.0).unwrap().unwrap();

        assert_eq!(accumulated.integral(), Some(10.0),);

        control_loop.pause();

        assert_eq!(control_loop.state(), ControlLoopState::Paused,);

        assert_eq!(control_loop.process(100.0, 90.0).unwrap(), None,);

        control_loop.resume();

        assert_eq!(control_loop.state(), ControlLoopState::Running,);

        let resumed = control_loop.process(101.0, 90.0).unwrap().unwrap();

        assert_eq!(resumed.integral(), Some(10.0),);
    }

    #[test]
    fn pause_does_not_advance_ramp_reference() {
        let definition = definition_with(
            100.0,
            PidGains::new(1.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 200.0).unwrap(),
        )
        .with_reference(ReferenceSource::ramp(20.0, 100.0, 10.0).unwrap());

        let mut control_loop = ControlLoop::new(definition);

        let first = control_loop.process(10.0, 0.0).unwrap().unwrap();

        assert_eq!(first.setpoint(), Some(20.0),);

        let second = control_loop.process(12.0, 0.0).unwrap().unwrap();

        assert_eq!(second.setpoint(), Some(40.0),);

        control_loop.pause();

        assert_eq!(control_loop.process(1_000.0, 0.0).unwrap(), None,);

        control_loop.resume();

        let resumed = control_loop.process(2_000.0, 0.0).unwrap().unwrap();

        assert_eq!(resumed.setpoint(), Some(40.0),);

        let next = control_loop.process(2_001.0, 0.0).unwrap().unwrap();

        assert_eq!(next.setpoint(), Some(50.0),);
    }

    #[test]
    fn changes_input_without_losing_integral() {
        let definition = definition_with(
            100.0,
            PidGains::new(0.0, 1.0, 1.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        );

        let mut control_loop = ControlLoop::new(definition);

        control_loop.update(0.0, 90.0).unwrap();

        let accumulated = control_loop.update(1.0, 90.0).unwrap();

        assert_eq!(accumulated.integral(), Some(10.0),);

        control_loop.set_input(2);

        assert_eq!(*control_loop.input(), 2,);

        let switched = control_loop.update(100.0, 80.0).unwrap();

        assert_eq!(switched.integral(), Some(10.0),);

        assert_eq!(switched.derivative(), Some(0.0),);
    }

    #[test]
    fn runs_on_off_controller() {
        let mut control_loop = ControlLoop::new(on_off_definition());

        let on = control_loop.update(0.0, 97.0).unwrap();

        assert_eq!(on.kind(), ControllerKind::OnOff,);

        assert_eq!(on.value(), 100.0);

        let inside = control_loop.update(1.0, 100.0).unwrap();

        assert_eq!(inside.value(), 100.0);

        let off = control_loop.update(2.0, 103.0).unwrap();

        assert_eq!(off.value(), 0.0);
    }

    #[test]
    fn exposes_on_off_parameters() {
        let mut control_loop = ControlLoop::new(on_off_definition());

        let keys = control_loop
            .parameters()
            .into_iter()
            .map(|parameter| parameter.key)
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec!["setpoint", "hysteresis", "output_off", "output_on",],
        );

        assert_eq!(
            control_loop.read_parameter("hysteresis"),
            Ok(InstrumentValue::Number(2.0)),
        );

        assert_eq!(
            control_loop.write_parameter("hysteresis", InstrumentValue::Number(5.0),),
            Ok(InstrumentValue::Number(5.0)),
        );
    }

    #[test]
    fn exposes_control_loop_reference() {
        let definition = definition_with(
            100.0,
            PidGains::new(1.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .with_reference(ReferenceSource::ramp(20.0, 150.0, 2.0).unwrap());

        let control_loop = ControlLoop::new(definition);

        assert_eq!(control_loop.reference_kind(), Some(ReferenceKind::Ramp),);

        let keys = control_loop
            .reference_parameters()
            .into_iter()
            .map(|parameter| parameter.key)
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["start", "target", "rate",],);

        assert_eq!(
            control_loop.read_reference_parameter("target",),
            Ok(InstrumentValue::Number(150.0,),),
        );
    }

    #[test]
    fn reports_missing_reference() {
        let mut control_loop = ControlLoop::new(definition_with(
            100.0,
            PidGains::new(1.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        ));

        assert_eq!(control_loop.reference_kind(), None,);

        assert!(control_loop.reference_parameters().is_empty(),);

        assert_eq!(
            control_loop.read_reference_parameter("target",),
            Err(ControlLoopReferenceError::NotConfigured,),
        );

        assert_eq!(
            control_loop.write_reference_parameter("target", InstrumentValue::Number(150.0,),),
            Err(ControlLoopReferenceError::NotConfigured,),
        );
    }

    #[test]
    fn changes_ramp_reference_through_control_loop() {
        let definition = definition_with(
            100.0,
            PidGains::new(1.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 300.0).unwrap(),
        )
        .with_reference(ReferenceSource::ramp(20.0, 100.0, 10.0).unwrap());

        let mut control_loop = ControlLoop::new(definition);

        let first = control_loop.update(10.0, 0.0).unwrap();

        assert_eq!(first.setpoint(), Some(20.0),);

        let second = control_loop.update(12.0, 0.0).unwrap();

        assert_eq!(second.setpoint(), Some(40.0),);

        assert_eq!(
            control_loop.write_reference_parameter("target", InstrumentValue::Number(150.0,),),
            Ok(InstrumentValue::Number(150.0,),),
        );

        assert_eq!(
            control_loop.read_parameter("setpoint",),
            Ok(InstrumentValue::Number(40.0,),),
        );

        let restarted = control_loop.update(1_000.0, 0.0).unwrap();

        assert_eq!(restarted.setpoint(), Some(40.0),);

        let next = control_loop.update(1_001.0, 0.0).unwrap();

        assert_eq!(next.setpoint(), Some(50.0),);
    }

    #[test]
    fn replaces_control_loop_reference() {
        let definition = definition_with(
            100.0,
            PidGains::new(1.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 300.0).unwrap(),
        )
        .with_reference(ReferenceSource::ramp(20.0, 100.0, 10.0).unwrap());

        let mut control_loop = ControlLoop::new(definition);

        control_loop.update(10.0, 0.0).unwrap();

        control_loop.update(12.0, 0.0).unwrap();

        control_loop.set_reference(ReferenceSource::fixed(175.0).unwrap());

        assert_eq!(control_loop.reference_kind(), Some(ReferenceKind::Fixed),);

        assert_eq!(
            control_loop.read_reference_parameter("value",),
            Ok(InstrumentValue::Number(175.0,),),
        );

        assert_eq!(
            control_loop.read_parameter("setpoint",),
            Ok(InstrumentValue::Number(175.0,),),
        );

        let output = control_loop.update(1_000.0, 0.0).unwrap();

        assert_eq!(output.setpoint(), Some(175.0),);
    }

    #[test]
    fn exposes_legacy_setpoint_as_writable() {
        let control_loop = ControlLoop::new(definition_with(
            100.0,
            PidGains::new(1.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        ));

        let setpoint = control_loop
            .parameters()
            .into_iter()
            .find(|parameter| parameter.key == "setpoint")
            .unwrap();

        assert_eq!(setpoint.access, ParameterAccess::ReadWrite,);
    }

    #[test]
    fn exposes_reference_managed_setpoint_as_read_only() {
        let definition = definition_with(
            100.0,
            PidGains::new(1.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .with_reference(ReferenceSource::ramp(20.0, 150.0, 2.0).unwrap());

        let control_loop = ControlLoop::new(definition);

        let setpoint = control_loop
            .parameters()
            .into_iter()
            .find(|parameter| parameter.key == "setpoint")
            .unwrap();

        assert_eq!(setpoint.access, ParameterAccess::ReadOnly,);
    }

    #[test]
    fn assigns_unique_instance_id_to_each_definition() {
        let gains = PidGains::new(2.0, 0.5, 1.0).unwrap();

        let limits = PidOutputLimits::new(0.0, 100.0).unwrap();

        let first_controller = PidController::with_output_limits(100.0, gains, limits)
            .unwrap()
            .into();

        let second_controller = PidController::with_output_limits(100.0, gains, limits)
            .unwrap()
            .into();

        let first = ControlLoopDefinition::new("heater", 1_u64, (), first_controller).unwrap();

        let second = ControlLoopDefinition::new("heater", 1_u64, (), second_controller).unwrap();

        assert_ne!(first.instance_id(), second.instance_id(),);
    }
}
