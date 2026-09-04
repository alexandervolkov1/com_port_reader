use std::{
    error::Error,
    fmt, io,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};

use crate::instrument::ConnectedParameterAddress;

use super::{OutputArbiter, OutputArbiterError, OutputMode};

enum OutputCommand {
    RegisterController {
        target: ConnectedParameterAddress,
        controller: String,
        response_sender: Sender<Result<(), OutputArbiterError>>,
    },

    Mode {
        target: ConnectedParameterAddress,
        response_sender: Sender<Result<OutputMode, OutputArbiterError>>,
    },

    Shutdown,
}

#[derive(Clone)]
pub(crate) struct OutputHandle {
    command_sender: Sender<OutputCommand>,
}

impl OutputHandle {
    pub(crate) fn register_controller(
        &self,
        target: ConnectedParameterAddress,
        controller: impl Into<String>,
    ) -> Result<(), OutputRequestError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(OutputCommand::RegisterController {
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
}

pub(crate) struct OutputService {
    handle: OutputHandle,
    thread: Option<JoinHandle<()>>,
}

impl OutputService {
    pub(crate) fn spawn() -> io::Result<Self> {
        let (command_sender, command_receiver) = unbounded();

        let handle = OutputHandle { command_sender };

        let thread = thread::Builder::new()
            .name("output-control".to_owned())
            .spawn(move || {
                run(command_receiver);
            })?;

        Ok(Self {
            handle,
            thread: Some(thread),
        })
    }

    pub(crate) fn handle(&self) -> OutputHandle {
        self.handle.clone()
    }
}

impl Drop for OutputService {
    fn drop(&mut self) {
        let _ = self.handle.command_sender.send(OutputCommand::Shutdown);

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(command_receiver: Receiver<OutputCommand>) {
    let mut arbiter = OutputArbiter::new();

    while let Ok(command) = command_receiver.recv() {
        match command {
            OutputCommand::RegisterController {
                target,
                controller,
                response_sender,
            } => {
                let result = arbiter.register_controller(target, controller);

                let _ = response_sender.send(result);
            }

            OutputCommand::Mode {
                target,
                response_sender,
            } => {
                let result = arbiter.mode(target);

                let _ = response_sender.send(result);
            }

            OutputCommand::Shutdown => {
                break;
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OutputRequestError {
    Arbiter(OutputArbiterError),
    Disconnected,
}

impl fmt::Display for OutputRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arbiter(error) => error.fmt(formatter),

            Self::Disconnected => formatter.write_str("Output control service is disconnected"),
        }
    }
}

impl Error for OutputRequestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Arbiter(error) => Some(error),

            Self::Disconnected => None,
        }
    }
}

impl From<OutputArbiterError> for OutputRequestError {
    fn from(error: OutputArbiterError) -> Self {
        Self::Arbiter(error)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        connection::ConnectionId,
        instrument::{
            ConnectedParameterAddress, InstrumentParameterAddress,
            virtual_instrument::{VirtualInstrumentId, VirtualParameterId},
        },
    };

    use super::{OutputArbiterError, OutputMode, OutputRequestError, OutputService};

    fn target() -> ConnectedParameterAddress {
        ConnectedParameterAddress::new(
            ConnectionId::new(2),
            InstrumentParameterAddress::virtual_instrument(
                VirtualInstrumentId::new(7),
                VirtualParameterId::new(4),
            ),
        )
    }

    #[test]
    fn owns_registered_output_state() {
        let service = OutputService::spawn().unwrap();

        let handle = service.handle();

        let target = target();

        handle.register_controller(target, "heater").unwrap();

        assert_eq!(handle.mode(target), Ok(OutputMode::Automatic),);
    }

    #[test]
    fn rejects_duplicate_registration() {
        let service = OutputService::spawn().unwrap();

        let handle = service.handle();

        let target = target();

        handle.register_controller(target, "heater").unwrap();

        assert_eq!(
            handle.register_controller(target, "other",),
            Err(OutputRequestError::Arbiter(
                OutputArbiterError::AlreadyRegistered {
                    controller: "heater".to_owned(),
                },
            ),),
        );
    }

    #[test]
    fn reports_disconnected_service() {
        let handle = {
            let service = OutputService::spawn().unwrap();

            service.handle()
        };

        assert_eq!(
            handle.mode(target()),
            Err(OutputRequestError::Disconnected,),
        );
    }
}
