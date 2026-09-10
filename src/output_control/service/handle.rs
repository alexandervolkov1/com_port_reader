use crossbeam_channel::{Receiver, Sender, bounded};

use super::{
    command::OutputCommand,
    error::{OutputRequestError, OutputWriteError},
};
#[cfg(test)]
use crate::{acquisition::AcquisitionError, output_control::OutputMode};
use crate::{
    acquisition::InstrumentWriteResult,
    connection::ConnectionId,
    instrument::{ConnectedParameterAddress, InstrumentValue, InstrumentWriteRequest},
    output_control::AutomaticOutputIntent,
    process_control::ControllerInstanceId,
    process_recorder::ProcessActionId,
};

pub(crate) struct OutputWriteResponse {
    receiver: Receiver<InstrumentWriteResult>,
}

impl OutputWriteResponse {
    fn new(receiver: Receiver<InstrumentWriteResult>) -> Self {
        Self { receiver }
    }

    pub(crate) fn recv(&self) -> Result<InstrumentValue, OutputWriteError> {
        self.receiver
            .recv()
            .map_err(|_| OutputWriteError::Disconnected)?
            .map_err(OutputWriteError::Instrument)
    }
}

#[derive(Clone)]
pub(crate) struct OutputHandle {
    pub(super) command_sender: Sender<OutputCommand>,
}

impl OutputHandle {
    pub(crate) fn register_controller(
        &self,
        target: ConnectedParameterAddress,
        controller: impl Into<String>,
        instance_id: ControllerInstanceId,
        safe_request: Option<InstrumentWriteRequest>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::RegisterController {
                target,
                controller: controller.into(),
                instance_id,
                safe_request,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub(crate) fn rollback_controller_registration(
        &self,
        target: ConnectedParameterAddress,
        controller: impl Into<String>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::RollbackControllerRegistration {
                target,
                controller: controller.into(),
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub(crate) fn release_controller(
        &self,
        controller: impl Into<String>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::ReleaseController {
                controller: controller.into(),
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    #[cfg(test)]
    pub(crate) fn mode(
        &self,
        target: ConnectedParameterAddress,
    ) -> Result<OutputMode, OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::Mode {
                target,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    #[cfg(test)]
    pub(crate) fn last_applied(
        &self,
        target: ConnectedParameterAddress,
    ) -> Result<Option<InstrumentValue>, OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::LastApplied {
                target,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    #[cfg(test)]
    pub(crate) fn last_write_failure(
        &self,
        target: ConnectedParameterAddress,
    ) -> Result<Option<AcquisitionError>, OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::LastWriteFailure {
                target,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub(crate) fn apply_automatic(
        &self,
        intent: AutomaticOutputIntent,
    ) -> Result<OutputWriteResponse, OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::ApplyAutomatic {
                intent,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        let result = response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?;

        result.map(OutputWriteResponse::new)
    }

    pub(crate) fn write_instrument(
        &self,
        action_id: Option<ProcessActionId>,
        connection_id: ConnectionId,
        request: InstrumentWriteRequest,
        instrument_response_sender: Sender<InstrumentWriteResult>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::WriteInstrument {
                action_id,
                connection_id,
                request,
                instrument_response_sender,
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
    }

    pub(crate) fn request_automatic(
        &self,
        controller: impl Into<String>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::RequestAutomatic {
                controller: controller.into(),
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub(crate) fn rollback_automatic_request(
        &self,
        controller: impl Into<String>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::RollbackAutomaticRequest {
                controller: controller.into(),
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?
            .map_err(Into::into)
    }

    pub(crate) fn apply_safe(
        &self,
        controller: impl Into<String>,
    ) -> Result<OutputWriteResponse, OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::ApplySafe {
                controller: controller.into(),
                response_sender,
            })
            .map_err(|_| OutputRequestError::Disconnected)?;

        let result = response_receiver
            .recv()
            .map_err(|_| OutputRequestError::Disconnected)?;

        result.map(OutputWriteResponse::new)
    }
}
