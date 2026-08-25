use std::{
    error::Error,
    fmt, io,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};

use super::{
    ControlOutputTarget, PidLoopDefinition, PidLoopDefinitionError, PidLoopEvent, PidLoopRegistry,
    PidLoopRegistryError,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProcessControlInput<SignalId> {
    pub signal_id: SignalId,
    pub timestamp: f64,
    pub value: f64,
}

impl<SignalId> ProcessControlInput<SignalId> {
    pub const fn new(signal_id: SignalId, timestamp: f64, value: f64) -> Self {
        Self {
            signal_id,
            timestamp,
            value,
        }
    }
}

enum ProcessControlCommand<SignalId> {
    AddLoop {
        definition: PidLoopDefinition<SignalId, ControlOutputTarget>,
        response_sender: Sender<Result<(), PidLoopRegistryError>>,
    },
    SetSetpoint {
        name: String,
        setpoint: f64,
        response_sender: Sender<Result<bool, PidLoopDefinitionError>>,
    },
    RemoveLoop {
        name: String,
        response_sender: Sender<bool>,
    },
    Process(Vec<ProcessControlInput<SignalId>>),
    ResetFrom {
        signal_id: SignalId,
    },
    RemoveFrom {
        signal_id: SignalId,
        response_sender: Sender<Vec<String>>,
    },
    Clear,
    Shutdown,
}

pub struct ProcessControlHandle<SignalId> {
    command_sender: Sender<ProcessControlCommand<SignalId>>,
}

impl<SignalId> Clone for ProcessControlHandle<SignalId> {
    fn clone(&self) -> Self {
        Self {
            command_sender: self.command_sender.clone(),
        }
    }
}

impl<SignalId> ProcessControlHandle<SignalId> {
    pub fn add_loop(
        &self,
        definition: PidLoopDefinition<SignalId, ControlOutputTarget>,
    ) -> Result<(), AddPidLoopError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessControlCommand::AddLoop {
                definition,

                response_sender,
            })
            .map_err(|_| AddPidLoopError::Disconnected)?;

        let result = response_receiver
            .recv()
            .map_err(|_| AddPidLoopError::Disconnected)?;

        result.map_err(AddPidLoopError::Definition)
    }

    pub fn set_setpoint(
        &self,
        name: impl Into<String>,
        setpoint: f64,
    ) -> Result<bool, SetPidLoopSetpointError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessControlCommand::SetSetpoint {
                name: name.into(),
                setpoint,
                response_sender,
            })
            .map_err(|_| SetPidLoopSetpointError::Disconnected)?;

        let result = response_receiver
            .recv()
            .map_err(|_| SetPidLoopSetpointError::Disconnected)?;

        result.map_err(SetPidLoopSetpointError::Definition)
    }

    pub fn remove_loop(
        &self,
        name: impl Into<String>,
    ) -> Result<bool, ProcessControlServiceDisconnected> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessControlCommand::RemoveLoop {
                name: name.into(),

                response_sender,
            })
            .map_err(|_| ProcessControlServiceDisconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ProcessControlServiceDisconnected)
    }

    pub fn process(
        &self,
        signal_id: SignalId,
        timestamp: f64,
        value: f64,
    ) -> Result<(), ProcessControlServiceDisconnected> {
        self.process_batch(vec![ProcessControlInput::new(signal_id, timestamp, value)])
    }

    pub fn process_batch(
        &self,
        inputs: Vec<ProcessControlInput<SignalId>>,
    ) -> Result<(), ProcessControlServiceDisconnected> {
        if inputs.is_empty() {
            return Ok(());
        }

        self.command_sender
            .send(ProcessControlCommand::Process(inputs))
            .map_err(|_| ProcessControlServiceDisconnected)
    }

    pub fn reset_from(&self, signal_id: SignalId) -> Result<(), ProcessControlServiceDisconnected> {
        self.command_sender
            .send(ProcessControlCommand::ResetFrom { signal_id })
            .map_err(|_| ProcessControlServiceDisconnected)
    }

    pub fn remove_from(
        &self,
        signal_id: SignalId,
    ) -> Result<Vec<String>, ProcessControlServiceDisconnected> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessControlCommand::RemoveFrom {
                signal_id,

                response_sender,
            })
            .map_err(|_| ProcessControlServiceDisconnected)?;

        response_receiver
            .recv()
            .map_err(|_| ProcessControlServiceDisconnected)
    }

    pub fn clear(&self) -> Result<(), ProcessControlServiceDisconnected> {
        self.command_sender
            .send(ProcessControlCommand::Clear)
            .map_err(|_| ProcessControlServiceDisconnected)
    }
}

pub struct ProcessControlService<SignalId> {
    handle: ProcessControlHandle<SignalId>,

    event_receiver: Receiver<PidLoopEvent<SignalId>>,

    thread: Option<JoinHandle<()>>,
}

impl<SignalId> ProcessControlService<SignalId>
where
    SignalId: Copy + Eq + Send + 'static,
{
    pub fn spawn() -> io::Result<Self> {
        let (command_sender, command_receiver) = unbounded();

        let (event_sender, event_receiver) = unbounded();

        let thread = thread::Builder::new()
            .name("process-control".to_owned())
            .spawn(move || {
                run_process_control(command_receiver, event_sender);
            })?;

        Ok(Self {
            handle: ProcessControlHandle { command_sender },

            event_receiver,

            thread: Some(thread),
        })
    }
}

impl<SignalId> ProcessControlService<SignalId> {
    pub fn handle(&self) -> ProcessControlHandle<SignalId> {
        self.handle.clone()
    }

    pub fn event_receiver(&self) -> Receiver<PidLoopEvent<SignalId>> {
        self.event_receiver.clone()
    }

    pub fn take_events(&self) -> Vec<PidLoopEvent<SignalId>> {
        self.event_receiver.try_iter().collect()
    }
}

impl<SignalId> Drop for ProcessControlService<SignalId> {
    fn drop(&mut self) {
        let _ = self
            .handle
            .command_sender
            .send(ProcessControlCommand::Shutdown);

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_process_control<SignalId>(
    command_receiver: Receiver<ProcessControlCommand<SignalId>>,

    event_sender: Sender<PidLoopEvent<SignalId>>,
) where
    SignalId: Copy + Eq,
{
    let mut registry = PidLoopRegistry::new();

    while let Ok(command) = command_receiver.recv() {
        match command {
            ProcessControlCommand::AddLoop {
                definition,

                response_sender,
            } => {
                let result = registry.add(definition);

                let _ = response_sender.send(result);
            }

            ProcessControlCommand::SetSetpoint {
                name,
                setpoint,
                response_sender,
            } => {
                let result = registry.set_setpoint(&name, setpoint);

                let _ = response_sender.send(result);
            }

            ProcessControlCommand::RemoveLoop {
                name,

                response_sender,
            } => {
                let removed = registry.remove(&name);

                let _ = response_sender.send(removed);
            }

            ProcessControlCommand::Process(inputs) => {
                process_inputs(&mut registry, inputs, &event_sender);
            }

            ProcessControlCommand::ResetFrom { signal_id } => {
                registry.reset_from(signal_id);
            }

            ProcessControlCommand::RemoveFrom {
                signal_id,

                response_sender,
            } => {
                let removed = registry.remove_from(signal_id);

                let _ = response_sender.send(removed);
            }

            ProcessControlCommand::Clear => {
                registry.clear();
            }

            ProcessControlCommand::Shutdown => {
                break;
            }
        }
    }
}

fn process_inputs<SignalId>(
    registry: &mut PidLoopRegistry<SignalId>,

    inputs: Vec<ProcessControlInput<SignalId>>,

    event_sender: &Sender<PidLoopEvent<SignalId>>,
) where
    SignalId: Copy + Eq,
{
    for input in inputs {
        let events = registry.process(input.signal_id, input.timestamp, input.value);

        for event in events {
            let _ = event_sender.send(event);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AddPidLoopError {
    Definition(PidLoopRegistryError),

    Disconnected,
}

impl fmt::Display for AddPidLoopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Definition(error) => error.fmt(formatter),

            Self::Disconnected => formatter.write_str(
                "Process control service \
                     is disconnected",
            ),
        }
    }
}

impl Error for AddPidLoopError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Definition(error) => Some(error),

            Self::Disconnected => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetPidLoopSetpointError {
    Definition(PidLoopDefinitionError),

    Disconnected,
}

impl fmt::Display for SetPidLoopSetpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Definition(error) => error.fmt(formatter),

            Self::Disconnected => formatter.write_str(
                "Process control service \
                 is disconnected",
            ),
        }
    }
}

impl Error for SetPidLoopSetpointError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Definition(error) => Some(error),

            Self::Disconnected => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessControlServiceDisconnected;

impl fmt::Display for ProcessControlServiceDisconnected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "Process control service \
             is disconnected",
        )
    }
}

impl Error for ProcessControlServiceDisconnected {}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        connection::ConnectionId,
        instrument::{
            InstrumentValue, InstrumentWriteRequest, ParameterAccess, ParameterRange,
            ParameterValueType,
            virtual_instrument::{
                VirtualInstrumentId, VirtualParameterDescriptor, VirtualParameterId,
            },
        },
        process_control::{
            ControlOutputTarget, PidGains, PidLoopDefinition, PidLoopDefinitionError, PidLoopEvent,
            PidLoopExecutionError, PidLoopRegistryError, PidOutputLimits,
        },
    };

    use super::{
        AddPidLoopError, ProcessControlInput, ProcessControlService,
        ProcessControlServiceDisconnected, SetPidLoopSetpointError,
    };

    const EVENT_TIMEOUT: Duration = Duration::from_secs(1);

    fn target(parameter: u16) -> ControlOutputTarget {
        let descriptor = VirtualParameterDescriptor::new(
            VirtualParameterId::new(parameter),
            format!("power_{parameter}",),
            "Power",
            ParameterAccess::ReadWrite,
            ParameterValueType::Number,
        )
        .with_range(ParameterRange::Number {
            minimum: 0.0,

            maximum: 100.0,
        });

        ControlOutputTarget::virtual_instrument(
            ConnectionId::new(2),
            VirtualInstrumentId::new(7),
            &descriptor,
        )
        .unwrap()
    }

    fn definition(
        name: &str,

        input: u64,

        parameter: u16,
    ) -> PidLoopDefinition<u64, ControlOutputTarget> {
        PidLoopDefinition::new(
            name,
            input,
            target(parameter),
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn processes_measurements_in_background_thread() {
        let service = ProcessControlService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle.add_loop(definition("heater", 5, 4)).unwrap();

        handle.process(5, 1_000.0, 80.0).unwrap();

        let event = events.recv_timeout(EVENT_TIMEOUT).unwrap();

        let PidLoopEvent::Output(output) = event else {
            panic!("expected PID output event",);
        };

        assert_eq!(output.loop_name, "heater",);

        assert_eq!(output.input, 5,);

        assert_eq!(output.timestamp, 1_000.0,);

        assert_eq!(output.connection_id, ConnectionId::new(2),);

        assert_eq!(output.output.value(), 40.0,);

        assert_eq!(
            output.request,
            InstrumentWriteRequest::VirtualInstrument {
                instrument: VirtualInstrumentId::new(7),

                parameter: VirtualParameterId::new(4),

                value: InstrumentValue::Number(40.0),
            },
        );
    }

    #[test]
    fn processes_measurement_batch() {
        let service = ProcessControlService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle.add_loop(definition("heater", 5, 4)).unwrap();

        handle
            .process_batch(vec![
                ProcessControlInput::new(5, 1_000.0, 80.0),
                ProcessControlInput::new(5, 1_001.0, 75.0),
            ])
            .unwrap();

        let first = events.recv_timeout(EVENT_TIMEOUT).unwrap();

        let second = events.recv_timeout(EVENT_TIMEOUT).unwrap();

        let PidLoopEvent::Output(first) = first else {
            panic!(
                "expected first \
                 PID output",
            );
        };

        let PidLoopEvent::Output(second) = second else {
            panic!(
                "expected second \
                 PID output",
            );
        };

        assert_eq!(first.output.value(), 40.0,);

        assert_eq!(second.output.value(), 50.0,);
    }

    #[test]
    fn changes_pid_loop_setpoint() {
        let service = ProcessControlService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle.add_loop(definition("heater", 5, 4)).unwrap();

        assert_eq!(handle.set_setpoint("heater", 90.0), Ok(true),);

        handle.process(5, 1_000.0, 80.0).unwrap();

        let event = events.recv_timeout(EVENT_TIMEOUT).unwrap();

        let PidLoopEvent::Output(output) = event else {
            panic!("expected PID output event");
        };

        assert_eq!(output.output.value(), 20.0,);
    }

    #[test]
    fn reports_missing_loop_when_setting_setpoint() {
        let service = ProcessControlService::<u64>::spawn().unwrap();

        assert_eq!(service.handle().set_setpoint("missing", 90.0), Ok(false),);
    }

    #[test]
    fn rejects_invalid_pid_setpoint() {
        let service = ProcessControlService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle.add_loop(definition("heater", 1, 1)).unwrap();

        assert_eq!(
            handle.set_setpoint("heater", f64::NAN),
            Err(SetPidLoopSetpointError::Definition(
                PidLoopDefinitionError::NonFiniteSetpoint,
            )),
        );
    }

    #[test]
    fn rejects_duplicate_loop() {
        let service = ProcessControlService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle.add_loop(definition("heater", 1, 1)).unwrap();

        let result = handle.add_loop(definition("heater", 2, 2));

        assert_eq!(
            result,
            Err(AddPidLoopError::Definition(
                PidLoopRegistryError::DuplicateName("heater".to_owned(),),
            ),),
        );
    }

    #[test]
    fn reports_controller_failure() {
        let service = ProcessControlService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle.add_loop(definition("heater", 1, 1)).unwrap();

        handle.process(1, f64::NAN, 80.0).unwrap();

        let event = events.recv_timeout(EVENT_TIMEOUT).unwrap();

        assert!(matches!(
            event,

            PidLoopEvent::Error(
                PidLoopExecutionError::
                    Controller {
                        loop_name,
                        ..
                    },
            ) if loop_name
                == "heater"
        ),);
    }

    #[test]
    fn removes_loop_by_name() {
        let service = ProcessControlService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle.add_loop(definition("heater", 1, 1)).unwrap();

        assert_eq!(handle.remove_loop("heater",), Ok(true),);

        assert_eq!(handle.remove_loop("heater",), Ok(false),);
    }

    #[test]
    fn removes_loops_for_deleted_signal() {
        let service = ProcessControlService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle.add_loop(definition("first", 1, 1)).unwrap();

        handle.add_loop(definition("second", 1, 2)).unwrap();

        handle.add_loop(definition("third", 2, 3)).unwrap();

        let removed = handle.remove_from(1).unwrap();

        assert_eq!(removed, vec!["first".to_owned(), "second".to_owned(),],);

        handle.process(1, 1_000.0, 80.0).unwrap();

        handle.process(2, 1_000.0, 80.0).unwrap();

        let event = events.recv_timeout(EVENT_TIMEOUT).unwrap();

        let PidLoopEvent::Output(output) = event else {
            panic!(
                "expected remaining \
                 PID loop output",
            );
        };

        assert_eq!(output.loop_name, "third",);

        assert!(events.try_recv().is_err(),);
    }

    #[test]
    fn resets_pid_state_for_input() {
        let service = ProcessControlService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle.add_loop(definition("heater", 1, 1)).unwrap();

        handle.process(1, 10.0, 80.0).unwrap();

        events.recv_timeout(EVENT_TIMEOUT).unwrap();

        handle.process(1, 11.0, 75.0).unwrap();

        events.recv_timeout(EVENT_TIMEOUT).unwrap();

        handle.reset_from(1).unwrap();

        handle.process(1, 1.0, 90.0).unwrap();

        let event = events.recv_timeout(EVENT_TIMEOUT).unwrap();

        let PidLoopEvent::Output(output) = event else {
            panic!(
                "expected PID output \
                 after reset",
            );
        };

        assert_eq!(output.timestamp, 1.0,);

        assert_eq!(output.output.value(), 20.0,);
    }

    #[test]
    fn clears_registered_loops() {
        let service = ProcessControlService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle.add_loop(definition("heater", 1, 1)).unwrap();

        handle.clear().unwrap();

        handle.process(1, 1_000.0, 80.0).unwrap();

        assert!(events.recv_timeout(Duration::from_millis(50,),).is_err(),);
    }

    #[test]
    fn accepts_empty_batch() {
        let service = ProcessControlService::<u64>::spawn().unwrap();

        let result = service.handle().process_batch(Vec::new());

        assert_eq!(result, Ok(()),);
    }

    #[test]
    fn cloned_handle_reports_stopped_service() {
        let handle = {
            let service = ProcessControlService::<u64>::spawn().unwrap();

            service.handle()
        };

        let result = handle.process(1, 1_000.0, 80.0);

        assert_eq!(result, Err(ProcessControlServiceDisconnected,),);
    }

    #[test]
    fn describes_service_errors() {
        assert_eq!(
            AddPidLoopError::Disconnected.to_string(),
            "Process control service \
             is disconnected",
        );

        assert_eq!(
            ProcessControlServiceDisconnected.to_string(),
            "Process control service \
             is disconnected",
        );
    }
}
