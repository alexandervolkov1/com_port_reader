use crossbeam_channel::Sender;

use super::{AddControllerDiagnosticError, ProcessingInput};
use crate::{
    instrument::{InstrumentValue, ParameterDescriptor},
    process_control::{
        ControlLoopDefinition, ControlLoopState, ControlOutputTarget, ControllerAccessError,
        ControllerDiagnostic, ControllerRegistryError, ReferenceKind, ReferenceSource,
    },
    signal_processing::{
        SignalFilterDefinition, SignalProcessingGraphDefinitionError,
        SignalProcessingGraphUpdateError,
    },
};

pub(super) enum ProcessingCommand<SignalId> {
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

    RemoveController {
        name: String,
        response_sender: Sender<bool>,
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
