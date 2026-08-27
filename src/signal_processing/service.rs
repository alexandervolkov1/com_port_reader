use std::{
    error::Error,
    fmt,
    hash::Hash,
    io,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};

use crate::process_control::{
    ControlEvent, ControlLoopDefinition, ControlOutputTarget, ControllerRegistry,
    ControllerRegistryError, PidControllerError,
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

    SetPidSetpoint {
        name: String,
        setpoint: f64,
        response_sender: Sender<Result<bool, PidControllerError>>,
    },

    Process(Vec<ProcessingInput<SignalId>>),

    ResetFrom {
        signal_id: SignalId,
    },

    Clear,

    Shutdown,

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

    pub fn set_pid_setpoint(
        &self,
        name: impl Into<String>,
        setpoint: f64,
    ) -> Result<bool, SetPidLoopSetpointError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(ProcessingCommand::SetPidSetpoint {
                name: name.into(),
                setpoint,
                response_sender,
            })
            .map_err(|_| SetPidLoopSetpointError::Disconnected)?;

        let result = response_receiver
            .recv()
            .map_err(|_| SetPidLoopSetpointError::Disconnected)?;

        result.map_err(SetPidLoopSetpointError::Controller)
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
        self.command_sender
            .send(ProcessingCommand::Clear)
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

fn run_processing<SignalId>(
    command_receiver: Receiver<ProcessingCommand<SignalId>>,
    event_sender: Sender<ProcessingEvent<SignalId>>,
    control_event_sender: Sender<ControlEvent<SignalId>>,
) where
    SignalId: Copy + Eq + Hash,
{
    let mut graph = SignalProcessingGraph::new();

    let mut registry = ControllerRegistry::new();

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

            ProcessingCommand::SetPidSetpoint {
                name,
                setpoint,
                response_sender,
            } => {
                let result = registry.set_setpoint(&name, setpoint);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::Process(inputs) => {
                process_inputs(
                    &mut graph,
                    &mut registry,
                    inputs,
                    &event_sender,
                    &control_event_sender,
                );
            }

            ProcessingCommand::ResetFrom { signal_id } => {
                graph.reset_from(signal_id);

                registry.reset_from(signal_id);
            }

            ProcessingCommand::RemoveFrom {
                signal_id,
                response_sender,
            } => {
                let removed = graph.remove_from(signal_id);

                registry.remove_from(signal_id);

                for dependent_id in removed
                    .iter()
                    .copied()
                    .filter(|removed_id| *removed_id != signal_id)
                {
                    registry.remove_from(dependent_id);
                }

                let _ = response_sender.send(removed);
            }

            ProcessingCommand::Clear => {
                registry.clear();
                graph.clear();
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

    inputs: Vec<ProcessingInput<SignalId>>,

    event_sender: &Sender<ProcessingEvent<SignalId>>,

    control_event_sender: &Sender<ControlEvent<SignalId>>,
) where
    SignalId: Copy + Eq + Hash,
{
    let mut output_samples = Vec::new();

    for input in inputs {
        send_control_events(
            registry.process(input.signal_id, input.timestamp, input.value),
            control_event_sender,
        );

        match graph.process(input.signal_id, input.timestamp, input.value) {
            Ok(mut processed) => {
                for signal in &processed {
                    send_control_events(
                        registry.process(signal.signal_id, signal.timestamp, signal.value),
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

fn send_control_events<SignalId>(
    events: Vec<ControlEvent<SignalId>>,

    sender: &Sender<ControlEvent<SignalId>>,
) {
    for event in events {
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SetPidLoopSetpointError {
    Controller(PidControllerError),
    Disconnected,
}

impl fmt::Display for SetPidLoopSetpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Controller(error) => error.fmt(formatter),

            Self::Disconnected => formatter.write_str(
                "Processing \
                     service is disconnected",
            ),
        }
    }
}

impl Error for SetPidLoopSetpointError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Controller(error) => Some(error),

            Self::Disconnected => None,
        }
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
        AddControlLoopError, AddSignalFilterError, ProcessingEvent, ProcessingInput,
        ProcessingService, ProcessingServiceDisconnected, ReplaceSignalFilterError,
        SetPidLoopSetpointError,
    };

    use crate::{
        connection::ConnectionId,
        instrument::{
            ParameterAccess, ParameterRange, ParameterValueType,
            virtual_instrument::{
                VirtualInstrumentId, VirtualParameterDescriptor, VirtualParameterId,
            },
        },
        process_control::{
            ControlEvent, ControlLoopDefinition, ControlOutput, ControlOutputTarget, PidController,
            PidGains, PidOutputLimits,
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
    fn changes_pid_setpoint() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        assert_eq!(handle.set_pid_setpoint("heater", 90.0,), Ok(true),);

        handle.process(1, 1_000.0, 80.0).unwrap();

        let output = receive_pid_output(&control_events);

        assert_eq!(output.output.setpoint(), Some(90.0),);

        assert_eq!(output.output.value(), 20.0,);
    }

    #[test]
    fn reports_missing_pid_when_setting_setpoint() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        assert_eq!(
            service.handle().set_pid_setpoint("missing", 90.0,),
            Ok(false),
        );
    }

    #[test]
    fn rejects_invalid_pid_setpoint() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        assert!(matches!(
            handle.set_pid_setpoint("heater", f64::NAN,),
            Err(SetPidLoopSetpointError::Controller(_)),
        ));
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
            SetPidLoopSetpointError::Disconnected.to_string(),
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
}
