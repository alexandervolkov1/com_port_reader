use crossbeam_channel::{Receiver, Sender};

use super::error::OutputRequestError;
#[cfg(test)]
use crate::{
    acquisition::AcquisitionError, instrument::InstrumentValue, output_control::OutputMode,
};
use crate::{
    acquisition::InstrumentWriteResult,
    connection::ConnectionId,
    instrument::{ConnectedParameterAddress, InstrumentWriteRequest},
    output_control::{AutomaticOutputIntent, OutputArbiterError},
    process_control::ControllerInstanceId,
    process_recorder::ProcessActionId,
};

pub(super) enum OutputCommand {
    RegisterController {
        target: ConnectedParameterAddress,
        controller: String,
        instance_id: ControllerInstanceId,
        safe_request: Option<InstrumentWriteRequest>,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    RollbackControllerRegistration {
        target: ConnectedParameterAddress,
        controller: String,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    ReleaseController {
        controller: String,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    #[cfg(test)]
    Mode {
        target: ConnectedParameterAddress,
        response_sender: Sender<Result<OutputMode, OutputArbiterError>>,
    },

    #[cfg(test)]
    LastApplied {
        target: ConnectedParameterAddress,
        response_sender: Sender<Result<Option<InstrumentValue>, OutputArbiterError>>,
    },

    #[cfg(test)]
    LastWriteFailure {
        target: ConnectedParameterAddress,
        response_sender: Sender<Result<Option<AcquisitionError>, OutputArbiterError>>,
    },

    ApplyAutomatic {
        intent: AutomaticOutputIntent,
        response_sender: Sender<Result<Receiver<InstrumentWriteResult>, OutputRequestError>>,
    },

    RequestAutomatic {
        controller: String,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    RollbackAutomaticRequest {
        controller: String,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    WriteInstrument {
        action_id: Option<ProcessActionId>,
        connection_id: ConnectionId,
        request: InstrumentWriteRequest,
        instrument_response_sender: Sender<InstrumentWriteResult>,
        response_sender: Sender<Result<(), OutputRequestError>>,
    },

    ApplySafe {
        controller: String,
        response_sender: Sender<Result<Receiver<InstrumentWriteResult>, OutputRequestError>>,
    },

    Shutdown,
}
