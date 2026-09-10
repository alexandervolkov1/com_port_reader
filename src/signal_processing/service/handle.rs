use crossbeam_channel::{Sender, bounded};

use super::{
    AddControlLoopError, AddControllerDiagnosticError, AddSignalFilterError,
    ControllerRequestError, ProcessingInput, ProcessingServiceDisconnected,
    ReplaceSignalFilterError, command::ProcessingCommand,
};
use crate::{
    instrument::{InstrumentValue, ParameterDescriptor},
    process_control::{
        ControlLoopDefinition, ControlLoopState, ControlOutputTarget, ControllerDiagnostic,
        ReferenceKind, ReferenceSource,
    },
    signal_processing::SignalFilterDefinition,
};

pub struct ProcessingHandle<SignalId> {
    pub(super) command_sender: Sender<ProcessingCommand<SignalId>>,
}

impl<SignalId> Clone for ProcessingHandle<SignalId> {
    fn clone(&self) -> Self {
        Self {
            command_sender: self.command_sender.clone(),
        }
    }
}

impl<SignalId> ProcessingHandle<SignalId> {
    pub fn add_filter(
        &self,
        input: SignalId,
        output: SignalId,
        definition: SignalFilterDefinition,
    ) -> Result<(), AddSignalFilterError<SignalId>> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::AddFilter {
                input,
                output,
                definition,
                response_sender,
            })
            .map_err(|_| AddSignalFilterError::Disconnected)?;

        let result = response_receiver
            .recv()
            .map_err(|_| AddSignalFilterError::Disconnected)?;

        result.map_err(AddSignalFilterError::Definition)
    }

    pub fn replace_filter(
        &self,
        output: SignalId,
        definition: SignalFilterDefinition,
    ) -> Result<(), ReplaceSignalFilterError<SignalId>> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ReplaceFilter {
                output,
                definition,
                response_sender,
            })
            .map_err(|_| ReplaceSignalFilterError::Disconnected)?;

        let result = response_receiver
            .recv()
            .map_err(|_| ReplaceSignalFilterError::Disconnected)?;

        result.map_err(ReplaceSignalFilterError::Definition)
    }

    pub fn add_control_loop(
        &self,
        definition: ControlLoopDefinition<SignalId, ControlOutputTarget>,
    ) -> Result<(), AddControlLoopError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::AddControlLoop {
                definition,
                response_sender,
            })
            .map_err(|_| AddControlLoopError::Disconnected)?;

        let result = response_receiver
            .recv()
            .map_err(|_| AddControlLoopError::Disconnected)?;

        result.map_err(AddControlLoopError::Definition)
    }

    pub fn add_controller_diagnostic(
        &self,
        controller: impl Into<String>,
        diagnostic: ControllerDiagnostic,
        output: SignalId,
    ) -> Result<(), AddControllerDiagnosticError<SignalId>> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::AddControllerDiagnostic {
                controller: controller.into(),
                diagnostic,
                output,
                response_sender,
            })
            .map_err(|_| AddControllerDiagnosticError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| AddControllerDiagnosticError::Disconnected)?
    }

    pub fn controller_parameters(
        &self,
        name: impl Into<String>,
    ) -> Result<Vec<ParameterDescriptor>, ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ControllerParameters {
                name: name.into(),
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn controller_diagnostics(
        &self,
        name: impl Into<String>,
    ) -> Result<Vec<ControllerDiagnostic>, ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ControllerDiagnostics {
                name: name.into(),
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn read_controller_parameter(
        &self,
        name: impl Into<String>,
        key: impl Into<String>,
    ) -> Result<InstrumentValue, ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ReadControllerParameter {
                name: name.into(),
                key: key.into(),
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn write_controller_parameter(
        &self,
        name: impl Into<String>,
        key: impl Into<String>,
        value: InstrumentValue,
    ) -> Result<InstrumentValue, ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::WriteControllerParameter {
                name: name.into(),
                key: key.into(),
                value,
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn configure_controller<I, K>(
        &self,
        name: impl Into<String>,
        updates: I,
    ) -> Result<(), ControllerRequestError>
    where
        I: IntoIterator<Item = (K, InstrumentValue)>,
        K: AsRef<str>,
    {
        let updates = updates
            .into_iter()
            .map(|(key, value)| (key.as_ref().to_owned(), value))
            .collect();

        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ConfigureController {
                name: name.into(),
                updates,
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn reference_kind(
        &self,
        name: impl Into<String>,
    ) -> Result<Option<ReferenceKind>, ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ReferenceKind {
                name: name.into(),
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn reference_parameters(
        &self,
        name: impl Into<String>,
    ) -> Result<Vec<ParameterDescriptor>, ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ReferenceParameters {
                name: name.into(),
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn read_reference_parameter(
        &self,
        name: impl Into<String>,
        key: impl Into<String>,
    ) -> Result<InstrumentValue, ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ReadReferenceParameter {
                name: name.into(),
                key: key.into(),
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn write_reference_parameter(
        &self,
        name: impl Into<String>,
        key: impl Into<String>,
        value: InstrumentValue,
    ) -> Result<InstrumentValue, ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::WriteReferenceParameter {
                name: name.into(),
                key: key.into(),
                value,
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn configure_reference<I, K>(
        &self,
        name: impl Into<String>,
        updates: I,
    ) -> Result<(), ControllerRequestError>
    where
        I: IntoIterator<Item = (K, InstrumentValue)>,
        K: AsRef<str>,
    {
        let updates = updates
            .into_iter()
            .map(|(key, value)| (key.as_ref().to_owned(), value))
            .collect();

        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ConfigureReference {
                name: name.into(),
                updates,
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn set_reference(
        &self,
        name: impl Into<String>,
        source: ReferenceSource,
    ) -> Result<(), ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::SetReference {
                name: name.into(),
                source,
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn set_controller_input(
        &self,
        name: impl Into<String>,
        input: SignalId,
    ) -> Result<(), ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::SetControllerInput {
                name: name.into(),
                input,
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn controller_state(
        &self,
        name: impl Into<String>,
    ) -> Result<ControlLoopState, ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ControllerState {
                name: name.into(),
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn pause_controller(&self, name: impl Into<String>) -> Result<(), ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::PauseController {
                name: name.into(),
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn resume_controller(&self, name: impl Into<String>) -> Result<(), ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ResumeController {
                name: name.into(),
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn reset_controller_integral(
        &self,
        name: impl Into<String>,
    ) -> Result<(), ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ResetControllerIntegral {
                name: name.into(),
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn reset_controller(&self, name: impl Into<String>) -> Result<(), ControllerRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ResetController {
                name: name.into(),
                response_sender,
            })
            .map_err(|_| ControllerRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ControllerRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub fn process(
        &self,
        signal_id: SignalId,
        timestamp: f64,
        value: f64,
    ) -> Result<(), ProcessingServiceDisconnected> {
        self.process_batch(vec![ProcessingInput::new(signal_id, timestamp, value)])
    }

    pub fn process_batch(
        &self,
        inputs: Vec<ProcessingInput<SignalId>>,
    ) -> Result<(), ProcessingServiceDisconnected> {
        if inputs.is_empty() {
            return Ok(());
        }

        self.command_sender
            .send(ProcessingCommand::Process(inputs))
            .map_err(|_| ProcessingServiceDisconnected)
    }

    pub fn reset_from(&self, signal_id: SignalId) -> Result<(), ProcessingServiceDisconnected> {
        self.command_sender
            .send(ProcessingCommand::ResetFrom { signal_id })
            .map_err(|_| ProcessingServiceDisconnected)
    }

    pub fn clear(&self) -> Result<(), ProcessingServiceDisconnected> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::Clear { response_sender })
            .map_err(|_| ProcessingServiceDisconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ProcessingServiceDisconnected)
    }

    pub fn controllers_affected_by_removal(
        &self,
        signal_id: SignalId,
    ) -> Result<Vec<String>, ProcessingServiceDisconnected> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ControllersAffectedByRemoval {
                signal_id,
                response_sender,
            })
            .map_err(|_| ProcessingServiceDisconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ProcessingServiceDisconnected)
    }

    pub fn controller_names(&self) -> Result<Vec<String>, ProcessingServiceDisconnected> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::ControllerNames { response_sender })
            .map_err(|_| ProcessingServiceDisconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ProcessingServiceDisconnected)
    }

    pub fn remove_from(
        &self,
        signal_id: SignalId,
    ) -> Result<Vec<SignalId>, ProcessingServiceDisconnected> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::RemoveFrom {
                signal_id,
                response_sender,
            })
            .map_err(|_| ProcessingServiceDisconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ProcessingServiceDisconnected)
    }
}
