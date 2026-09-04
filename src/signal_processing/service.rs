use std::{
    error::Error,
    fmt,
    hash::Hash,
    io,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};

use crate::instrument::{InstrumentValue, ParameterDescriptor};
use crate::process_control::{
    ControlEvent, ControlLoopDefinition, ControlLoopState, ControlOutputTarget,
    ControllerAccessError, ControllerDiagnostic, ControllerRegistry, ControllerRegistryError,
    ReferenceKind, ReferenceSource,
};

use super::{
    ProcessedSignal, SignalFilterDefinition, SignalProcessingError, SignalProcessingGraph,
    SignalProcessingGraphDefinitionError, SignalProcessingGraphUpdateError,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProcessingInput<SignalId> {
    pub signal_id: SignalId,
    pub timestamp: f64,
    pub value: f64,
}

impl<SignalId> ProcessingInput<SignalId> {
    pub const fn new(signal_id: SignalId, timestamp: f64, value: f64) -> Self {
        Self {
            signal_id,
            timestamp,
            value,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ControllerDiagnosticBinding<SignalId> {
    controller: String,
    diagnostic: ControllerDiagnostic,
    output: SignalId,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProcessingEvent<SignalId> {
    Samples(Vec<ProcessedSignal<SignalId>>),
    Error(SignalProcessingError<SignalId>),
}

enum ProcessingCommand<SignalId> {
    AddFilter {
        input: SignalId,
        output: SignalId,
        definition: SignalFilterDefinition,
        response_sender: Sender<Result<(), SignalProcessingGraphDefinitionError<SignalId>>>,
    },

    ReplaceFilter {
        output: SignalId,
        definition: SignalFilterDefinition,
        response_sender: Sender<Result<(), SignalProcessingGraphUpdateError<SignalId>>>,
    },

    AddControlLoop {
        definition: ControlLoopDefinition<SignalId, ControlOutputTarget>,
        response_sender: Sender<Result<(), ControllerRegistryError>>,
    },

    AddControllerDiagnostic {
        controller: String,
        diagnostic: ControllerDiagnostic,
        output: SignalId,
        response_sender: Sender<Result<(), AddControllerDiagnosticError<SignalId>>>,
    },

    ControllerParameters {
        name: String,
        response_sender: Sender<Result<Vec<ParameterDescriptor>, ControllerAccessError>>,
    },

    ControllerDiagnostics {
        name: String,
        response_sender: Sender<Result<Vec<ControllerDiagnostic>, ControllerAccessError>>,
    },

    ReadControllerParameter {
        name: String,
        key: String,
        response_sender: Sender<Result<InstrumentValue, ControllerAccessError>>,
    },

    WriteControllerParameter {
        name: String,
        key: String,
        value: InstrumentValue,
        response_sender: Sender<Result<InstrumentValue, ControllerAccessError>>,
    },

    ConfigureController {
        name: String,
        updates: Vec<(String, InstrumentValue)>,
        response_sender: Sender<Result<(), ControllerAccessError>>,
    },

    ReferenceKind {
        name: String,
        response_sender: Sender<Result<Option<ReferenceKind>, ControllerAccessError>>,
    },

    ReferenceParameters {
        name: String,
        response_sender: Sender<Result<Vec<ParameterDescriptor>, ControllerAccessError>>,
    },

    ReadReferenceParameter {
        name: String,
        key: String,
        response_sender: Sender<Result<InstrumentValue, ControllerAccessError>>,
    },

    WriteReferenceParameter {
        name: String,
        key: String,
        value: InstrumentValue,
        response_sender: Sender<Result<InstrumentValue, ControllerAccessError>>,
    },

    ConfigureReference {
        name: String,
        updates: Vec<(String, InstrumentValue)>,
        response_sender: Sender<Result<(), ControllerAccessError>>,
    },

    SetReference {
        name: String,
        source: ReferenceSource,
        response_sender: Sender<Result<(), ControllerAccessError>>,
    },

    SetControllerInput {
        name: String,
        input: SignalId,
        response_sender: Sender<Result<(), ControllerAccessError>>,
    },

    ControllerState {
        name: String,
        response_sender: Sender<Result<ControlLoopState, ControllerAccessError>>,
    },

    PauseController {
        name: String,
        response_sender: Sender<Result<(), ControllerAccessError>>,
    },

    ResumeController {
        name: String,
        response_sender: Sender<Result<(), ControllerAccessError>>,
    },

    ResetControllerIntegral {
        name: String,
        response_sender: Sender<Result<(), ControllerAccessError>>,
    },

    ResetController {
        name: String,
        response_sender: Sender<Result<(), ControllerAccessError>>,
    },

    Process(Vec<ProcessingInput<SignalId>>),

    ResetFrom {
        signal_id: SignalId,
    },

    Clear {
        response_sender: Sender<()>,
    },

    Shutdown,

    ControllerNames {
        response_sender: Sender<Vec<String>>,
    },

    ControllersAffectedByRemoval {
        signal_id: SignalId,
        response_sender: Sender<Vec<String>>,
    },

    RemoveFrom {
        signal_id: SignalId,
        response_sender: Sender<Vec<SignalId>>,
    },
}

pub struct ProcessingHandle<SignalId> {
    command_sender: Sender<ProcessingCommand<SignalId>>,
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

pub struct ProcessingService<SignalId> {
    handle: ProcessingHandle<SignalId>,
    event_receiver: Receiver<ProcessingEvent<SignalId>>,
    control_event_receiver: Receiver<ControlEvent<SignalId>>,
    thread: Option<JoinHandle<()>>,
}

impl<SignalId> ProcessingService<SignalId>
where
    SignalId: Copy + Eq + Hash + Send + 'static,
{
    pub fn spawn() -> io::Result<Self> {
        let (command_sender, command_receiver) = unbounded();

        let (event_sender, event_receiver) = unbounded();

        let (control_event_sender, control_event_receiver) = unbounded();

        let thread = thread::Builder::new()
            .name("processing".to_owned())
            .spawn(move || {
                run_processing(command_receiver, event_sender, control_event_sender);
            })?;

        Ok(Self {
            handle: ProcessingHandle { command_sender },

            event_receiver,

            control_event_receiver,

            thread: Some(thread),
        })
    }
}

impl<SignalId> ProcessingService<SignalId> {
    pub fn handle(&self) -> ProcessingHandle<SignalId> {
        self.handle.clone()
    }

    pub fn event_receiver(&self) -> Receiver<ProcessingEvent<SignalId>> {
        self.event_receiver.clone()
    }

    pub fn control_event_receiver(&self) -> Receiver<ControlEvent<SignalId>> {
        self.control_event_receiver.clone()
    }

    pub fn take_events(&self) -> Vec<ProcessingEvent<SignalId>> {
        self.event_receiver.try_iter().collect()
    }
}

impl<SignalId> Drop for ProcessingService<SignalId> {
    fn drop(&mut self) {
        let _ = self.handle.command_sender.send(ProcessingCommand::Shutdown);

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn add_controller_diagnostic<SignalId>(
    graph: &SignalProcessingGraph<SignalId>,
    registry: &ControllerRegistry<SignalId>,
    bindings: &mut Vec<ControllerDiagnosticBinding<SignalId>>,
    controller: String,
    diagnostic: ControllerDiagnostic,
    output: SignalId,
) -> Result<(), AddControllerDiagnosticError<SignalId>>
where
    SignalId: Copy + Eq + Hash,
{
    registry
        .validate_diagnostic(&controller, diagnostic)
        .map_err(AddControllerDiagnosticError::Controller)?;

    if graph.contains_output(output) || bindings.iter().any(|binding| binding.output == output) {
        return Err(AddControllerDiagnosticError::DuplicateOutput { output });
    }

    bindings.push(ControllerDiagnosticBinding {
        controller,
        diagnostic,
        output,
    });

    Ok(())
}

fn controllers_affected_by_removal<SignalId>(
    graph: &SignalProcessingGraph<SignalId>,
    registry: &ControllerRegistry<SignalId>,
    signal_id: SignalId,
) -> Vec<String>
where
    SignalId: Copy + Eq + Hash,
{
    let mut affected_signals = graph.removal_set_from(signal_id);

    if !affected_signals.contains(&signal_id) {
        affected_signals.insert(0, signal_id);
    }

    let mut controllers = Vec::new();

    for affected_signal in affected_signals {
        controllers.extend(registry.names_from(affected_signal));
    }

    controllers
}

fn run_processing<SignalId>(
    command_receiver: Receiver<ProcessingCommand<SignalId>>,
    event_sender: Sender<ProcessingEvent<SignalId>>,
    control_event_sender: Sender<ControlEvent<SignalId>>,
) where
    SignalId: Copy + Eq + Hash,
{
    let mut graph = SignalProcessingGraph::new();
    let mut registry = ControllerRegistry::new();
    let mut controller_diagnostics = Vec::new();

    while let Ok(command) = command_receiver.recv() {
        match command {
            ProcessingCommand::AddFilter {
                input,
                output,
                definition,
                response_sender,
            } => {
                let result = graph.add_filter(input, output, definition);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ReplaceFilter {
                output,
                definition,
                response_sender,
            } => {
                let result = graph.replace_filter(output, definition);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::AddControlLoop {
                definition,
                response_sender,
            } => {
                let result = registry.add(definition);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::AddControllerDiagnostic {
                controller,
                diagnostic,
                output,
                response_sender,
            } => {
                let result = add_controller_diagnostic(
                    &graph,
                    &registry,
                    &mut controller_diagnostics,
                    controller,
                    diagnostic,
                    output,
                );

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ControllerParameters {
                name,
                response_sender,
            } => {
                let result = registry.parameters(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ControllerDiagnostics {
                name,
                response_sender,
            } => {
                let result = registry.diagnostics(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ReadControllerParameter {
                name,
                key,
                response_sender,
            } => {
                let result = registry.read_parameter(&name, &key);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::WriteControllerParameter {
                name,
                key,
                value,
                response_sender,
            } => {
                let result = registry.write_parameter(&name, &key, value);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ConfigureController {
                name,
                updates,
                response_sender,
            } => {
                let result = registry.configure(&name, updates);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ReferenceKind {
                name,
                response_sender,
            } => {
                let result = registry.reference_kind(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ReferenceParameters {
                name,
                response_sender,
            } => {
                let result = registry.reference_parameters(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ReadReferenceParameter {
                name,
                key,
                response_sender,
            } => {
                let result = registry.read_reference_parameter(&name, &key);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::WriteReferenceParameter {
                name,
                key,
                value,
                response_sender,
            } => {
                let result = registry.write_reference_parameter(&name, &key, value);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ConfigureReference {
                name,
                updates,
                response_sender,
            } => {
                let result = registry.configure_reference(&name, updates);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::SetReference {
                name,
                source,
                response_sender,
            } => {
                let result = registry.set_reference(&name, source);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::SetControllerInput {
                name,
                input,
                response_sender,
            } => {
                let result = registry.set_input(&name, input);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ControllerState {
                name,
                response_sender,
            } => {
                let result = registry.state(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::PauseController {
                name,
                response_sender,
            } => {
                let result = registry.pause(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ResumeController {
                name,
                response_sender,
            } => {
                let result = registry.resume(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ResetControllerIntegral {
                name,
                response_sender,
            } => {
                let result = registry.reset_integral(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ResetController {
                name,
                response_sender,
            } => {
                let result = registry.reset(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::Process(inputs) => {
                process_inputs(
                    &mut graph,
                    &mut registry,
                    &controller_diagnostics,
                    inputs,
                    &event_sender,
                    &control_event_sender,
                );
            }

            ProcessingCommand::ResetFrom { signal_id } => {
                graph.reset_from(signal_id);

                registry.reset_from(signal_id);
            }

            ProcessingCommand::ControllerNames { response_sender } => {
                let _ = response_sender.send(registry.names());
            }

            ProcessingCommand::ControllersAffectedByRemoval {
                signal_id,
                response_sender,
            } => {
                let controllers = controllers_affected_by_removal(&graph, &registry, signal_id);

                let _ = response_sender.send(controllers);
            }

            ProcessingCommand::RemoveFrom {
                signal_id,
                response_sender,
            } => {
                let mut removed = graph.remove_from(signal_id);

                controller_diagnostics.retain(|binding| binding.output != signal_id);

                let mut removed_controllers = registry.remove_from(signal_id);

                let dependent_ids = removed.clone();

                for dependent_id in dependent_ids
                    .into_iter()
                    .filter(|removed_id| *removed_id != signal_id)
                {
                    removed_controllers.extend(registry.remove_from(dependent_id));
                }

                if !removed_controllers.is_empty() {
                    controller_diagnostics.retain(|binding| {
                        if removed_controllers
                            .iter()
                            .any(|controller| controller == &binding.controller)
                        {
                            removed.push(binding.output);

                            false
                        } else {
                            true
                        }
                    });
                }

                let _ = response_sender.send(removed);
            }

            ProcessingCommand::Clear { response_sender } => {
                controller_diagnostics.clear();
                registry.clear();
                graph.clear();

                let _ = response_sender.send(());
            }

            ProcessingCommand::Shutdown => {
                break;
            }
        }
    }
}

fn process_inputs<SignalId>(
    graph: &mut SignalProcessingGraph<SignalId>,
    registry: &mut ControllerRegistry<SignalId>,
    controller_diagnostics: &[ControllerDiagnosticBinding<SignalId>],
    inputs: Vec<ProcessingInput<SignalId>>,
    event_sender: &Sender<ProcessingEvent<SignalId>>,
    control_event_sender: &Sender<ControlEvent<SignalId>>,
) where
    SignalId: Copy + Eq + Hash,
{
    let mut output_samples = Vec::new();

    for input in inputs {
        process_control_events(
            registry.process(input.signal_id, input.timestamp, input.value),
            controller_diagnostics,
            &mut output_samples,
            control_event_sender,
        );

        match graph.process(input.signal_id, input.timestamp, input.value) {
            Ok(mut processed) => {
                for signal in &processed {
                    process_control_events(
                        registry.process(signal.signal_id, signal.timestamp, signal.value),
                        controller_diagnostics,
                        &mut output_samples,
                        control_event_sender,
                    );
                }

                output_samples.append(&mut processed);
            }

            Err(error) => {
                let _ = event_sender.send(ProcessingEvent::Error(error));
            }
        }
    }

    if !output_samples.is_empty() {
        let _ = event_sender.send(ProcessingEvent::Samples(output_samples));
    }
}

fn process_control_events<SignalId>(
    events: Vec<ControlEvent<SignalId>>,
    bindings: &[ControllerDiagnosticBinding<SignalId>],
    output_samples: &mut Vec<ProcessedSignal<SignalId>>,
    sender: &Sender<ControlEvent<SignalId>>,
) where
    SignalId: Copy + Eq,
{
    for event in events {
        if let ControlEvent::Output(control_output) = &event {
            for binding in bindings {
                if binding.controller != control_output.loop_name {
                    continue;
                }

                let Some(value) = control_output.output.diagnostic(binding.diagnostic) else {
                    continue;
                };

                output_samples.push(ProcessedSignal {
                    signal_id: binding.output,
                    timestamp: control_output.timestamp,
                    value,
                });
            }
        }

        let _ = sender.send(event);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddSignalFilterError<SignalId> {
    Definition(SignalProcessingGraphDefinitionError<SignalId>),

    Disconnected,
}

impl<SignalId> fmt::Display for AddSignalFilterError<SignalId>
where
    SignalId: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Definition(error) => error.fmt(formatter),

            Self::Disconnected => formatter.write_str("Processing service is disconnected"),
        }
    }
}

impl<SignalId> Error for AddSignalFilterError<SignalId>
where
    SignalId: fmt::Debug + fmt::Display + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Definition(error) => Some(error),
            Self::Disconnected => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AddControlLoopError {
    Definition(ControllerRegistryError),

    Disconnected,
}

impl fmt::Display for AddControlLoopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Definition(error) => error.fmt(formatter),

            Self::Disconnected => formatter.write_str(
                "Processing \
                     service is disconnected",
            ),
        }
    }
}

impl Error for AddControlLoopError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Definition(error) => Some(error),

            Self::Disconnected => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AddControllerDiagnosticError<SignalId> {
    Controller(ControllerAccessError),
    DuplicateOutput { output: SignalId },
    Disconnected,
}

impl<SignalId> fmt::Display for AddControllerDiagnosticError<SignalId>
where
    SignalId: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Controller(error) => error.fmt(formatter),

            Self::DuplicateOutput { output } => {
                write!(
                    formatter,
                    "Processing output {output} \
                     is already registered",
                )
            }

            Self::Disconnected => formatter.write_str(
                "Processing service is \
                     disconnected",
            ),
        }
    }
}

impl<SignalId> Error for AddControllerDiagnosticError<SignalId>
where
    SignalId: fmt::Debug + fmt::Display + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Controller(error) => Some(error),

            Self::DuplicateOutput { .. } | Self::Disconnected => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControllerRequestError {
    Access(ControllerAccessError),

    Disconnected,
}

impl fmt::Display for ControllerRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Access(error) => error.fmt(formatter),

            Self::Disconnected => formatter.write_str(
                "Processing service is \
                     disconnected",
            ),
        }
    }
}

impl Error for ControllerRequestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Access(error) => Some(error),

            Self::Disconnected => None,
        }
    }
}

impl From<ControllerAccessError> for ControllerRequestError {
    fn from(error: ControllerAccessError) -> Self {
        Self::Access(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplaceSignalFilterError<SignalId> {
    Definition(SignalProcessingGraphUpdateError<SignalId>),

    Disconnected,
}

impl<SignalId> fmt::Display for ReplaceSignalFilterError<SignalId>
where
    SignalId: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Definition(error) => error.fmt(formatter),

            Self::Disconnected => formatter.write_str(
                "Processing service \
                     is disconnected",
            ),
        }
    }
}

impl<SignalId> Error for ReplaceSignalFilterError<SignalId>
where
    SignalId: fmt::Debug + fmt::Display + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Definition(error) => Some(error),
            Self::Disconnected => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessingServiceDisconnected;

impl fmt::Display for ProcessingServiceDisconnected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Processing service is disconnected")
    }
}

impl Error for ProcessingServiceDisconnected {}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        AddControlLoopError, AddControllerDiagnosticError, AddSignalFilterError, ControlLoopState,
        ControllerRequestError, ProcessingEvent, ProcessingInput, ProcessingService,
        ProcessingServiceDisconnected, ReplaceSignalFilterError,
    };

    use crate::{
        connection::ConnectionId,
        instrument::{
            InstrumentValue, ParameterAccess, ParameterRange, ParameterValueType,
            virtual_instrument::{
                VirtualInstrumentId, VirtualParameterDescriptor, VirtualParameterId,
            },
        },
        process_control::{
            ControlEvent, ControlLoopDefinition, ControlOutput, ControlOutputTarget,
            ControllerAccessError, ControllerDiagnostic, PidController, PidGains, PidOutputLimits,
            ReferenceKind, ReferenceSource,
        },
        signal_processing::{
            ProcessedSignal, SignalFilterDefinition, SignalFilterError,
            SignalProcessingGraphDefinitionError, SignalProcessingGraphUpdateError,
        },
    };

    const EVENT_TIMEOUT: Duration = Duration::from_secs(1);

    const NO_EVENT_TIMEOUT: Duration = Duration::from_millis(50);

    fn pid_definition(
        name: &str,
        input: u64,
        parameter: u16,
    ) -> ControlLoopDefinition<u64, ControlOutputTarget> {
        let descriptor = VirtualParameterDescriptor::new(
            VirtualParameterId::new(parameter),
            format!("power_{parameter}"),
            "Power",
            ParameterAccess::ReadWrite,
            ParameterValueType::Number,
        )
        .with_range(ParameterRange::Number {
            minimum: 0.0,
            maximum: 100.0,
        });

        let target = ControlOutputTarget::virtual_instrument(
            ConnectionId::PRIMARY,
            VirtualInstrumentId::new(1),
            &descriptor,
        )
        .unwrap();

        let controller = PidController::with_output_limits(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap()
        .into();

        ControlLoopDefinition::new(name, input, target, controller).unwrap()
    }

    fn ramp_pid_definition(
        name: &str,
        input: u64,
        parameter: u16,
    ) -> ControlLoopDefinition<u64, ControlOutputTarget> {
        pid_definition(name, input, parameter)
            .with_reference(ReferenceSource::ramp(20.0, 150.0, 10.0).unwrap())
    }

    fn receive_pid_output(
        events: &crossbeam_channel::Receiver<ControlEvent<u64>>,
    ) -> ControlOutput<u64> {
        let event = events.recv_timeout(EVENT_TIMEOUT).unwrap();

        let ControlEvent::Output(output) = event else {
            panic!("expected PID output event");
        };

        output
    }

    #[test]
    fn processes_signal_in_background_thread() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle.process(1, 0.0, 10.0).unwrap();

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 0.0,
                value: 10.0,
            },])),
        );

        handle.process(1, 1.0, 20.0).unwrap();

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 1.0,
                value: 15.0,
            },])),
        );
    }

    #[test]
    fn processes_input_batch() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .process_batch(vec![
                ProcessingInput::new(1, 0.0, 10.0),
                ProcessingInput::new(1, 1.0, 20.0),
            ])
            .unwrap();

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![
                ProcessedSignal {
                    signal_id: 2,
                    timestamp: 0.0,
                    value: 10.0,
                },
                ProcessedSignal {
                    signal_id: 2,
                    timestamp: 1.0,
                    value: 15.0,
                },
            ])),
        );
    }

    #[test]
    fn reports_filter_error_as_event() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle.process(1, 1.0, 10.0).unwrap();

        events.recv_timeout(EVENT_TIMEOUT).unwrap();

        handle.process(1, 1.0, 20.0).unwrap();

        let event = events.recv_timeout(EVENT_TIMEOUT).unwrap();

        let ProcessingEvent::Error(error) = event else {
            panic!("expected processing error event");
        };

        assert_eq!(error.output(), 2);

        assert_eq!(
            error.filter_error(),
            SignalFilterError::NonIncreasingTimestamp {
                previous: 1.0,
                current: 1.0,
            },
        );
    }

    #[test]
    fn rejects_duplicate_filter_output() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        assert_eq!(
            handle.add_filter(3, 2, SignalFilterDefinition::median(3).unwrap(),),
            Err(AddSignalFilterError::Definition(
                SignalProcessingGraphDefinitionError::DuplicateOutput { output: 2 },
            ),),
        );
    }

    #[test]
    fn reset_from_resets_filter_and_pid_state() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle.process(1, 10.0, 80.0).unwrap();

        let first_control = receive_pid_output(&control_events);

        assert_eq!(first_control.output.value(), 40.0,);

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 10.0,
                value: 80.0,
            },])),
        );

        handle.process(1, 11.0, 60.0).unwrap();

        receive_pid_output(&control_events);

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 11.0,
                value: 70.0,
            },])),
        );

        handle.reset_from(1).unwrap();

        handle.process(1, 0.0, 90.0).unwrap();

        let restarted_control = receive_pid_output(&control_events);

        assert_eq!(restarted_control.timestamp, 0.0,);

        assert_eq!(restarted_control.output.value(), 20.0,);

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 0.0,
                value: 90.0,
            },])),
        );
    }

    #[test]
    fn clear_removes_registered_processing_state() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle.clear().unwrap();

        handle.process(1, 0.0, 10.0).unwrap();

        assert!(events.recv_timeout(NO_EVENT_TIMEOUT).is_err(),);

        assert!(control_events.recv_timeout(NO_EVENT_TIMEOUT).is_err(),);
    }

    #[test]
    fn replaces_filter_in_background_service() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle.process(1, 0.0, 10.0).unwrap();

        events.recv_timeout(EVENT_TIMEOUT).unwrap();

        handle.process(1, 1.0, 20.0).unwrap();

        events.recv_timeout(EVENT_TIMEOUT).unwrap();

        handle
            .replace_filter(2, SignalFilterDefinition::median(3).unwrap())
            .unwrap();

        handle.process(1, 2.0, 100.0).unwrap();

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 2.0,
                value: 100.0,
            },])),
        );
    }

    #[test]
    fn rejects_replacing_unknown_service_filter() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        assert_eq!(
            handle.replace_filter(10, SignalFilterDefinition::median(3).unwrap(),),
            Err(ReplaceSignalFilterError::Definition(
                SignalProcessingGraphUpdateError::UnknownOutput { output: 10 },
            ),),
        );
    }

    #[test]
    fn runs_pid_for_raw_measurement() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle.process(1, 1_000.0, 80.0).unwrap();

        let output = receive_pid_output(&control_events);

        assert_eq!(output.loop_name, "heater",);

        assert_eq!(output.input, 1,);

        assert_eq!(output.measurement, 80.0,);

        assert_eq!(output.output.value(), 40.0,);
    }

    #[test]
    fn runs_pid_for_filtered_measurement() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("filtered_heater", 2, 1))
            .unwrap();

        handle.process(1, 1_000.0, 80.0).unwrap();

        let first = receive_pid_output(&control_events);

        handle.process(1, 1_001.0, 60.0).unwrap();

        let second = receive_pid_output(&control_events);

        assert_eq!(first.loop_name, "filtered_heater",);

        assert_eq!(first.input, 2,);

        assert_eq!(first.measurement, 80.0,);

        assert_eq!(first.output.value(), 40.0,);

        assert_eq!(second.loop_name, "filtered_heater",);

        assert_eq!(second.input, 2,);

        assert_eq!(second.measurement, 70.0,);

        assert_eq!(second.output.value(), 60.0,);
    }

    #[test]
    fn supports_raw_and_filtered_pid_loops_together() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(pid_definition("raw_heater", 1, 1))
            .unwrap();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("filtered_heater", 2, 2))
            .unwrap();

        handle.process(1, 1_000.0, 80.0).unwrap();

        let first = receive_pid_output(&control_events);

        let second = receive_pid_output(&control_events);

        assert_eq!(first.loop_name, "raw_heater",);

        assert_eq!(first.input, 1,);

        assert_eq!(first.output.value(), 40.0,);

        assert_eq!(second.loop_name, "filtered_heater",);

        assert_eq!(second.input, 2,);

        assert_eq!(second.output.value(), 40.0,);
    }

    #[test]
    fn changes_controller_setpoint() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        assert_eq!(
            handle
                .write_controller_parameter("heater", "setpoint", InstrumentValue::Number(90.0,),),
            Ok(InstrumentValue::Number(90.0,),),
        );

        handle.process(1, 1_000.0, 80.0).unwrap();

        let output = receive_pid_output(&control_events);

        assert_eq!(output.output.setpoint(), Some(90.0),);

        assert_eq!(output.output.value(), 20.0,);
    }

    #[test]
    fn manages_reference_through_processing_handle() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(ramp_pid_definition("heater", 1, 1))
            .unwrap();

        assert_eq!(
            handle.reference_kind("heater",),
            Ok(Some(ReferenceKind::Ramp,)),
        );

        let parameters = handle.reference_parameters("heater").unwrap();

        let keys = parameters
            .into_iter()
            .map(|parameter| parameter.key)
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["start", "target", "rate",],);

        assert_eq!(
            handle.read_reference_parameter("heater", "target",),
            Ok(InstrumentValue::Number(150.0,),),
        );

        handle.process(1, 10.0, 0.0).unwrap();

        let first = receive_pid_output(&control_events);

        assert_eq!(first.output.setpoint(), Some(20.0),);

        handle.process(1, 12.0, 0.0).unwrap();

        let second = receive_pid_output(&control_events);

        assert_eq!(second.output.setpoint(), Some(40.0),);

        assert_eq!(
            handle.write_reference_parameter("heater", "target", InstrumentValue::Number(200.0,),),
            Ok(InstrumentValue::Number(200.0,),),
        );

        assert_eq!(
            handle.read_controller_parameter("heater", "setpoint",),
            Ok(InstrumentValue::Number(40.0,),),
        );

        handle.process(1, 1_000.0, 0.0).unwrap();

        let restarted = receive_pid_output(&control_events);

        assert_eq!(restarted.output.setpoint(), Some(40.0),);

        handle.process(1, 1_001.0, 0.0).unwrap();

        let next = receive_pid_output(&control_events);

        assert_eq!(next.output.setpoint(), Some(50.0),);
    }

    #[test]
    fn rejects_duplicate_pid_name() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        assert!(matches!(
            handle.add_control_loop(pid_definition("heater", 2, 2,),),
            Err(AddControlLoopError::Definition(_)),
        ));
    }

    #[test]
    fn configures_reference_through_processing_handle() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(ramp_pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .configure_reference(
                "heater",
                [
                    ("target", InstrumentValue::Number(200.0)),
                    ("rate", InstrumentValue::Number(5.0)),
                ],
            )
            .unwrap();

        assert_eq!(
            handle.read_reference_parameter("heater", "target",),
            Ok(InstrumentValue::Number(200.0,),),
        );

        assert_eq!(
            handle.read_reference_parameter("heater", "rate",),
            Ok(InstrumentValue::Number(5.0,),),
        );
    }

    #[test]
    fn replaces_reference_through_processing_handle() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(ramp_pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .set_reference("heater", ReferenceSource::fixed(175.0).unwrap())
            .unwrap();

        assert_eq!(
            handle.reference_kind("heater",),
            Ok(Some(ReferenceKind::Fixed,)),
        );

        assert_eq!(
            handle.read_reference_parameter("heater", "value",),
            Ok(InstrumentValue::Number(175.0,),),
        );

        assert_eq!(
            handle.read_controller_parameter("heater", "setpoint",),
            Ok(InstrumentValue::Number(175.0,),),
        );
    }

    #[test]
    fn remove_from_removes_raw_and_dependent_pid_loops() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("raw_heater", 1, 1))
            .unwrap();

        handle
            .add_control_loop(pid_definition("filtered_heater", 2, 2))
            .unwrap();

        let removed = handle.remove_from(1).unwrap();

        assert!(
            removed.contains(&2),
            "filtered output must be removed with its input",
        );

        handle.process(1, 1_000.0, 80.0).unwrap();

        assert!(
            control_events.recv_timeout(NO_EVENT_TIMEOUT).is_err(),
            "PID loops for the removed branch must no longer run",
        );
    }

    #[test]
    fn remove_from_does_not_remove_unrelated_pid_loop() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("removed_heater", 2, 1))
            .unwrap();

        handle
            .add_control_loop(pid_definition("unrelated_heater", 3, 2))
            .unwrap();

        handle.remove_from(1).unwrap();

        handle.process(3, 1_000.0, 80.0).unwrap();

        let output = receive_pid_output(&control_events);

        assert_eq!(output.loop_name, "unrelated_heater",);

        assert_eq!(output.input, 3,);

        assert_eq!(output.output.value(), 40.0,);

        assert!(control_events.recv_timeout(NO_EVENT_TIMEOUT).is_err(),);
    }

    #[test]
    fn accepts_empty_batch() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        assert_eq!(service.handle().process_batch(Vec::new()), Ok(()),);
    }

    #[test]
    fn cloned_handle_reports_stopped_service() {
        let handle = {
            let service = ProcessingService::<u64>::spawn().unwrap();

            service.handle()
        };

        assert_eq!(
            handle.process(1, 1_000.0, 80.0,),
            Err(ProcessingServiceDisconnected),
        );

        assert_eq!(
            handle.add_control_loop(pid_definition("heater", 1, 1,),),
            Err(AddControlLoopError::Disconnected),
        );
    }

    #[test]
    fn describes_service_errors() {
        assert_eq!(
            AddSignalFilterError::<u64>::Disconnected.to_string(),
            "Processing service is disconnected",
        );

        assert_eq!(
            AddControlLoopError::Disconnected.to_string(),
            "Processing service is disconnected",
        );

        assert_eq!(
            ProcessingServiceDisconnected.to_string(),
            "Processing service is disconnected",
        );

        assert_eq!(
            ReplaceSignalFilterError::<u64>::Disconnected.to_string(),
            "Processing service is disconnected",
        );
    }

    #[test]
    fn reads_controller_parameter() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        assert_eq!(
            handle.read_controller_parameter("heater", "setpoint",),
            Ok(InstrumentValue::Number(100.0,),),
        );
    }

    #[test]
    fn writes_controller_parameter() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        assert_eq!(
            handle.write_controller_parameter(
                "heater",
                "setpoint",
                InstrumentValue::Number(120.0,),
            ),
            Ok(InstrumentValue::Number(120.0,),),
        );

        assert_eq!(
            handle.read_controller_parameter("heater", "setpoint",),
            Ok(InstrumentValue::Number(120.0,),),
        );
    }

    #[test]
    fn configures_controller_atomically() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .configure_controller(
                "heater",
                [
                    ("output_min", InstrumentValue::Number(10.0)),
                    ("output_max", InstrumentValue::Number(90.0)),
                ],
            )
            .unwrap();

        assert_eq!(
            handle.read_controller_parameter("heater", "output_min",),
            Ok(InstrumentValue::Number(10.0)),
        );

        assert_eq!(
            handle.read_controller_parameter("heater", "output_max",),
            Ok(InstrumentValue::Number(90.0)),
        );
    }

    #[test]
    fn reports_missing_controller_request() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        assert!(matches!(
            service
                .handle()
                .read_controller_parameter("missing", "setpoint",),
            Err(ControllerRequestError::Access(_)),
        ));
    }

    #[test]
    fn resets_controller_by_name() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .write_controller_parameter("heater", "ki", InstrumentValue::Number(1.0))
            .unwrap();

        handle.process(1, 0.0, 90.0).unwrap();

        let _ = receive_pid_output(&control_events);

        handle.process(1, 1.0, 90.0).unwrap();

        let accumulated = receive_pid_output(&control_events);

        assert!(accumulated.output.integral().unwrap() > 0.0,);

        handle.reset_controller("heater").unwrap();

        handle.process(1, 0.0, 90.0).unwrap();

        let restarted = receive_pid_output(&control_events);

        assert_eq!(restarted.output.integral(), Some(0.0),);
    }

    #[test]
    fn emits_controller_diagnostic_sample() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .add_controller_diagnostic("heater", ControllerDiagnostic::Proportional, 10)
            .unwrap();

        handle.process(1, 1_000.0, 80.0).unwrap();

        let control = receive_pid_output(&control_events);

        assert_eq!(control.output.proportional(), Some(40.0),);

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT,),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 10,
                timestamp: 1_000.0,
                value: 40.0,
            },],),),
        );
    }

    #[test]
    fn rejects_diagnostic_for_missing_controller() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        assert_eq!(
            handle.add_controller_diagnostic("missing", ControllerDiagnostic::Integral, 10,),
            Err(AddControllerDiagnosticError::Controller(
                ControllerAccessError::ControlLoopNotFound("missing".to_owned(),),
            )),
        );
    }

    #[test]
    fn removes_diagnostics_with_controller_input() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .add_controller_diagnostic("heater", ControllerDiagnostic::Integral, 10)
            .unwrap();

        assert_eq!(handle.remove_from(1), Ok(vec![10]),);
    }

    #[test]
    fn pauses_and_resumes_controller_without_losing_state() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();
        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .write_controller_parameter("heater", "ki", InstrumentValue::Number(1.0))
            .unwrap();

        handle.process(1, 0.0, 90.0).unwrap();
        let _ = receive_pid_output(&control_events);

        handle.process(1, 1.0, 90.0).unwrap();

        let accumulated = receive_pid_output(&control_events);

        assert_eq!(accumulated.output.integral(), Some(10.0),);

        handle.pause_controller("heater").unwrap();

        assert_eq!(
            handle.controller_state("heater"),
            Ok(ControlLoopState::Paused),
        );

        handle.process(1, 100.0, 90.0).unwrap();

        assert!(matches!(
            control_events.recv_timeout(NO_EVENT_TIMEOUT),
            Err(crossbeam_channel::RecvTimeoutError::Timeout),
        ));

        handle.resume_controller("heater").unwrap();

        assert_eq!(
            handle.controller_state("heater"),
            Ok(ControlLoopState::Running),
        );

        handle.process(1, 101.0, 90.0).unwrap();

        let resumed = receive_pid_output(&control_events);

        assert_eq!(resumed.output.integral(), Some(10.0),);
    }

    #[test]
    fn changes_controller_input_without_stopping_processing() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .write_controller_parameter("heater", "ki", InstrumentValue::Number(1.0))
            .unwrap();

        handle.process(1, 0.0, 90.0).unwrap();

        let _ = receive_pid_output(&control_events);

        handle.process(1, 1.0, 90.0).unwrap();

        let accumulated = receive_pid_output(&control_events);

        assert_eq!(accumulated.input, 1,);

        assert!(accumulated.output.integral().unwrap() > 0.0);

        let integral = accumulated.output.integral().unwrap();

        handle.set_controller_input("heater", 2).unwrap();

        handle.process(1, 100.0, 90.0).unwrap();

        let filtered = receive_pid_output(&control_events);

        assert_eq!(filtered.input, 2,);

        assert_eq!(filtered.output.integral(), Some(integral),);

        assert_eq!(filtered.output.derivative(), Some(0.0),);
    }

    #[test]
    fn previews_controllers_removed_with_signal_branch() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("raw_heater", 1, 1))
            .unwrap();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("filtered_heater", 2, 2))
            .unwrap();

        handle
            .add_control_loop(pid_definition("unrelated_heater", 3, 3))
            .unwrap();

        assert_eq!(
            handle.controllers_affected_by_removal(1,).unwrap(),
            vec!["raw_heater".to_owned(), "filtered_heater".to_owned(),],
        );

        /*
         * Preview must not mutate runtime.
         */
        assert_eq!(
            handle.controller_state("raw_heater",),
            Ok(ControlLoopState::Running),
        );

        assert_eq!(
            handle.controller_state("filtered_heater",),
            Ok(ControlLoopState::Running),
        );

        assert_eq!(
            handle.controller_state("unrelated_heater",),
            Ok(ControlLoopState::Running),
        );
    }

    #[test]
    fn returns_all_controller_names_without_modifying_runtime() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("first", 1, 1))
            .unwrap();

        handle
            .add_control_loop(pid_definition("second", 2, 2))
            .unwrap();

        assert_eq!(
            handle.controller_names().unwrap(),
            vec!["first".to_owned(), "second".to_owned(),],
        );

        assert_eq!(
            handle.controller_state("first",),
            Ok(ControlLoopState::Running,),
        );

        assert_eq!(
            handle.controller_state("second",),
            Ok(ControlLoopState::Running,),
        );
    }
}
