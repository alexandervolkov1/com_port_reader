use std::{
    error::Error,
    fmt,
    hash::Hash,
    io,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};

use crate::process_control::{
    ControlOutputTarget, PidLoopDefinition, PidLoopDefinitionError, PidLoopEvent, PidLoopRegistry,
    PidLoopRegistryError,
};

use super::{
    ProcessedSignal, SignalFilterDefinition, SignalProcessingError, SignalProcessingGraph,
    SignalProcessingGraphDefinitionError, SignalProcessingGraphUpdateError,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SignalProcessingInput<SignalId> {
    pub signal_id: SignalId,
    pub timestamp: f64,
    pub value: f64,
}

impl<SignalId> SignalProcessingInput<SignalId> {
    pub const fn new(signal_id: SignalId, timestamp: f64, value: f64) -> Self {
        Self {
            signal_id,
            timestamp,
            value,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SignalProcessingEvent<SignalId> {
    Samples(Vec<ProcessedSignal<SignalId>>),
    Error(SignalProcessingError<SignalId>),
}

enum SignalProcessingCommand<SignalId> {
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

    AddPidLoop {
        definition: PidLoopDefinition<SignalId, ControlOutputTarget>,
        response_sender: Sender<Result<(), PidLoopRegistryError>>,
    },

    SetPidSetpoint {
        name: String,
        setpoint: f64,
        response_sender: Sender<Result<bool, PidLoopDefinitionError>>,
    },

    Process(Vec<SignalProcessingInput<SignalId>>),

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

pub struct SignalProcessingHandle<SignalId> {
    command_sender: Sender<SignalProcessingCommand<SignalId>>,
}

impl<SignalId> Clone for SignalProcessingHandle<SignalId> {
    fn clone(&self) -> Self {
        Self {
            command_sender: self.command_sender.clone(),
        }
    }
}

impl<SignalId> SignalProcessingHandle<SignalId> {
    pub fn add_filter(
        &self,
        input: SignalId,
        output: SignalId,
        definition: SignalFilterDefinition,
    ) -> Result<(), AddSignalFilterError<SignalId>> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(SignalProcessingCommand::AddFilter {
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
            .send(SignalProcessingCommand::ReplaceFilter {
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

    pub fn add_pid_loop(
        &self,
        definition: PidLoopDefinition<SignalId, ControlOutputTarget>,
    ) -> Result<(), AddPidLoopError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(SignalProcessingCommand::AddPidLoop {
                definition,
                response_sender,
            })
            .map_err(|_| AddPidLoopError::Disconnected)?;

        let result = response_receiver
            .recv()
            .map_err(|_| AddPidLoopError::Disconnected)?;

        result.map_err(AddPidLoopError::Definition)
    }

    pub fn set_pid_setpoint(
        &self,
        name: impl Into<String>,
        setpoint: f64,
    ) -> Result<bool, SetPidLoopSetpointError> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(SignalProcessingCommand::SetPidSetpoint {
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

    pub fn process(
        &self,
        signal_id: SignalId,
        timestamp: f64,
        value: f64,
    ) -> Result<(), SignalProcessingServiceDisconnected> {
        self.process_batch(vec![SignalProcessingInput::new(
            signal_id, timestamp, value,
        )])
    }

    pub fn process_batch(
        &self,
        inputs: Vec<SignalProcessingInput<SignalId>>,
    ) -> Result<(), SignalProcessingServiceDisconnected> {
        if inputs.is_empty() {
            return Ok(());
        }

        self.command_sender
            .send(SignalProcessingCommand::Process(inputs))
            .map_err(|_| SignalProcessingServiceDisconnected)
    }

    pub fn reset_from(
        &self,
        signal_id: SignalId,
    ) -> Result<(), SignalProcessingServiceDisconnected> {
        self.command_sender
            .send(SignalProcessingCommand::ResetFrom { signal_id })
            .map_err(|_| SignalProcessingServiceDisconnected)
    }

    pub fn clear(&self) -> Result<(), SignalProcessingServiceDisconnected> {
        self.command_sender
            .send(SignalProcessingCommand::Clear)
            .map_err(|_| SignalProcessingServiceDisconnected)
    }

    pub fn remove_from(
        &self,
        signal_id: SignalId,
    ) -> Result<Vec<SignalId>, SignalProcessingServiceDisconnected> {
        let (response_sender, response_receiver) = bounded(1);

        self.command_sender
            .send(SignalProcessingCommand::RemoveFrom {
                signal_id,
                response_sender,
            })
            .map_err(|_| SignalProcessingServiceDisconnected)?;

        response_receiver
            .recv()
            .map_err(|_| SignalProcessingServiceDisconnected)
    }
}

pub struct SignalProcessingService<SignalId> {
    handle: SignalProcessingHandle<SignalId>,
    event_receiver: Receiver<SignalProcessingEvent<SignalId>>,
    control_event_receiver: Receiver<PidLoopEvent<SignalId>>,
    thread: Option<JoinHandle<()>>,
}

impl<SignalId> SignalProcessingService<SignalId>
where
    SignalId: Copy + Eq + Hash + Send + 'static,
{
    pub fn spawn() -> io::Result<Self> {
        let (command_sender, command_receiver) = unbounded();

        let (event_sender, event_receiver) = unbounded();

        let (control_event_sender, control_event_receiver) = unbounded();

        let thread = thread::Builder::new()
            .name("signal-processing".to_owned())
            .spawn(move || {
                run_signal_processing(command_receiver, event_sender, control_event_sender);
            })?;

        Ok(Self {
            handle: SignalProcessingHandle { command_sender },

            event_receiver,

            control_event_receiver,

            thread: Some(thread),
        })
    }
}

impl<SignalId> SignalProcessingService<SignalId> {
    pub fn handle(&self) -> SignalProcessingHandle<SignalId> {
        self.handle.clone()
    }

    pub fn event_receiver(&self) -> Receiver<SignalProcessingEvent<SignalId>> {
        self.event_receiver.clone()
    }

    pub fn control_event_receiver(&self) -> Receiver<PidLoopEvent<SignalId>> {
        self.control_event_receiver.clone()
    }

    pub fn take_events(&self) -> Vec<SignalProcessingEvent<SignalId>> {
        self.event_receiver.try_iter().collect()
    }
}

impl<SignalId> Drop for SignalProcessingService<SignalId> {
    fn drop(&mut self) {
        let _ = self
            .handle
            .command_sender
            .send(SignalProcessingCommand::Shutdown);

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_signal_processing<SignalId>(
    command_receiver: Receiver<SignalProcessingCommand<SignalId>>,
    event_sender: Sender<SignalProcessingEvent<SignalId>>,
    control_event_sender: Sender<PidLoopEvent<SignalId>>,
) where
    SignalId: Copy + Eq + Hash,
{
    let mut graph = SignalProcessingGraph::new();

    let mut registry = PidLoopRegistry::new();

    while let Ok(command) = command_receiver.recv() {
        match command {
            SignalProcessingCommand::AddFilter {
                input,
                output,
                definition,
                response_sender,
            } => {
                let result = graph.add_filter(input, output, definition);

                let _ = response_sender.send(result);
            }

            SignalProcessingCommand::ReplaceFilter {
                output,
                definition,
                response_sender,
            } => {
                let result = graph.replace_filter(output, definition);

                let _ = response_sender.send(result);
            }

            SignalProcessingCommand::AddPidLoop {
                definition,
                response_sender,
            } => {
                let result = registry.add(definition);

                let _ = response_sender.send(result);
            }

            SignalProcessingCommand::SetPidSetpoint {
                name,
                setpoint,
                response_sender,
            } => {
                let result = registry.set_setpoint(&name, setpoint);

                let _ = response_sender.send(result);
            }

            SignalProcessingCommand::Process(inputs) => {
                process_inputs(
                    &mut graph,
                    &mut registry,
                    inputs,
                    &event_sender,
                    &control_event_sender,
                );
            }

            SignalProcessingCommand::ResetFrom { signal_id } => {
                graph.reset_from(signal_id);

                registry.reset_from(signal_id);
            }

            SignalProcessingCommand::RemoveFrom {
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

            SignalProcessingCommand::Clear => {
                registry.clear();
                graph.clear();
            }

            SignalProcessingCommand::Shutdown => {
                break;
            }
        }
    }
}

fn process_inputs<SignalId>(
    graph: &mut SignalProcessingGraph<SignalId>,

    registry: &mut PidLoopRegistry<SignalId>,

    inputs: Vec<SignalProcessingInput<SignalId>>,

    event_sender: &Sender<SignalProcessingEvent<SignalId>>,

    control_event_sender: &Sender<PidLoopEvent<SignalId>>,
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
                let _ = event_sender.send(SignalProcessingEvent::Error(error));
            }
        }
    }

    if !output_samples.is_empty() {
        let _ = event_sender.send(SignalProcessingEvent::Samples(output_samples));
    }
}

fn send_control_events<SignalId>(
    events: Vec<PidLoopEvent<SignalId>>,

    sender: &Sender<PidLoopEvent<SignalId>>,
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

            Self::Disconnected => formatter.write_str("Signal processing service is disconnected"),
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
pub enum AddPidLoopError {
    Definition(PidLoopRegistryError),

    Disconnected,
}

impl fmt::Display for AddPidLoopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Definition(error) => error.fmt(formatter),

            Self::Disconnected => formatter.write_str(
                "Signal processing \
                     service is disconnected",
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
                "Signal processing \
                     service is disconnected",
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
                "Signal processing service \
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
pub struct SignalProcessingServiceDisconnected;

impl fmt::Display for SignalProcessingServiceDisconnected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Signal processing service is disconnected")
    }
}

impl Error for SignalProcessingServiceDisconnected {}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        AddPidLoopError, AddSignalFilterError, ReplaceSignalFilterError, SetPidLoopSetpointError,
        SignalProcessingEvent, SignalProcessingInput, SignalProcessingService,
        SignalProcessingServiceDisconnected,
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
            ControlOutputTarget, PidGains, PidLoopDefinition, PidLoopEvent, PidLoopOutput,
            PidOutputLimits,
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
    ) -> PidLoopDefinition<u64, ControlOutputTarget> {
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

        PidLoopDefinition::new(
            name,
            input,
            target,
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap()
    }

    fn receive_pid_output(
        events: &crossbeam_channel::Receiver<PidLoopEvent<u64>>,
    ) -> PidLoopOutput<u64> {
        let event = events.recv_timeout(EVENT_TIMEOUT).unwrap();

        let PidLoopEvent::Output(output) = event else {
            panic!("expected PID output event");
        };

        output
    }

    #[test]
    fn processes_signal_in_background_thread() {
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle.process(1, 0.0, 10.0).unwrap();

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(SignalProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 0.0,
                value: 10.0,
            },])),
        );

        handle.process(1, 1.0, 20.0).unwrap();

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(SignalProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 1.0,
                value: 15.0,
            },])),
        );
    }

    #[test]
    fn processes_input_batch() {
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .process_batch(vec![
                SignalProcessingInput::new(1, 0.0, 10.0),
                SignalProcessingInput::new(1, 1.0, 20.0),
            ])
            .unwrap();

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(SignalProcessingEvent::Samples(vec![
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
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle.process(1, 1.0, 10.0).unwrap();

        events.recv_timeout(EVENT_TIMEOUT).unwrap();

        handle.process(1, 1.0, 20.0).unwrap();

        let event = events.recv_timeout(EVENT_TIMEOUT).unwrap();

        let SignalProcessingEvent::Error(error) = event else {
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
        let service = SignalProcessingService::<u64>::spawn().unwrap();

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
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle.add_pid_loop(pid_definition("heater", 1, 1)).unwrap();

        handle.process(1, 10.0, 80.0).unwrap();

        let first_control = receive_pid_output(&control_events);

        assert_eq!(first_control.output.value(), 40.0,);

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(SignalProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 10.0,
                value: 80.0,
            },])),
        );

        handle.process(1, 11.0, 60.0).unwrap();

        receive_pid_output(&control_events);

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(SignalProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 11.0,
                value: 70.0,
            },])),
        );

        handle.reset_from(1).unwrap();

        // Timestamp intentionally goes backwards.
        // Both the filter and PID must have been reset.
        handle.process(1, 0.0, 90.0).unwrap();

        let restarted_control = receive_pid_output(&control_events);

        assert_eq!(restarted_control.timestamp, 0.0,);

        assert_eq!(restarted_control.output.value(), 20.0,);

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(SignalProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 0.0,
                value: 90.0,
            },])),
        );
    }

    #[test]
    fn clear_removes_registered_processing_state() {
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle.add_pid_loop(pid_definition("heater", 1, 1)).unwrap();

        handle.clear().unwrap();

        handle.process(1, 0.0, 10.0).unwrap();

        assert!(events.recv_timeout(NO_EVENT_TIMEOUT).is_err(),);

        assert!(control_events.recv_timeout(NO_EVENT_TIMEOUT).is_err(),);
    }

    #[test]
    fn replaces_filter_in_background_service() {
        let service = SignalProcessingService::<u64>::spawn().unwrap();

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
            Ok(SignalProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 2.0,
                value: 100.0,
            },])),
        );
    }

    #[test]
    fn rejects_replacing_unknown_service_filter() {
        let service = SignalProcessingService::<u64>::spawn().unwrap();

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
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle.add_pid_loop(pid_definition("heater", 1, 1)).unwrap();

        handle.process(1, 1_000.0, 80.0).unwrap();

        let output = receive_pid_output(&control_events);

        assert_eq!(output.loop_name, "heater",);

        assert_eq!(output.input, 1,);

        assert_eq!(output.measurement, 80.0,);

        assert_eq!(output.output.value(), 40.0,);
    }

    #[test]
    fn runs_pid_for_filtered_measurement() {
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_pid_loop(pid_definition("filtered_heater", 2, 1))
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
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_pid_loop(pid_definition("raw_heater", 1, 1))
            .unwrap();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_pid_loop(pid_definition("filtered_heater", 2, 2))
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
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle.add_pid_loop(pid_definition("heater", 1, 1)).unwrap();

        assert_eq!(handle.set_pid_setpoint("heater", 90.0,), Ok(true),);

        handle.process(1, 1_000.0, 80.0).unwrap();

        let output = receive_pid_output(&control_events);

        assert_eq!(output.setpoint, 90.0,);

        assert_eq!(output.output.value(), 20.0,);
    }

    #[test]
    fn reports_missing_pid_when_setting_setpoint() {
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        assert_eq!(
            service.handle().set_pid_setpoint("missing", 90.0,),
            Ok(false),
        );
    }

    #[test]
    fn rejects_invalid_pid_setpoint() {
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle.add_pid_loop(pid_definition("heater", 1, 1)).unwrap();

        assert!(matches!(
            handle.set_pid_setpoint("heater", f64::NAN,),
            Err(SetPidLoopSetpointError::Definition(_)),
        ));
    }

    #[test]
    fn rejects_duplicate_pid_name() {
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle.add_pid_loop(pid_definition("heater", 1, 1)).unwrap();

        assert!(matches!(
            handle.add_pid_loop(pid_definition("heater", 2, 2,),),
            Err(AddPidLoopError::Definition(_)),
        ));
    }

    #[test]
    fn remove_from_removes_raw_and_dependent_pid_loops() {
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_pid_loop(pid_definition("raw_heater", 1, 1))
            .unwrap();

        handle
            .add_pid_loop(pid_definition("filtered_heater", 2, 2))
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
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_pid_loop(pid_definition("removed_heater", 2, 1))
            .unwrap();

        handle
            .add_pid_loop(pid_definition("unrelated_heater", 3, 2))
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
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        assert_eq!(service.handle().process_batch(Vec::new()), Ok(()),);
    }

    #[test]
    fn cloned_handle_reports_stopped_service() {
        let handle = {
            let service = SignalProcessingService::<u64>::spawn().unwrap();

            service.handle()
        };

        assert_eq!(
            handle.process(1, 1_000.0, 80.0,),
            Err(SignalProcessingServiceDisconnected),
        );

        assert_eq!(
            handle.add_pid_loop(pid_definition("heater", 1, 1,),),
            Err(AddPidLoopError::Disconnected),
        );
    }

    #[test]
    fn describes_service_errors() {
        assert_eq!(
            AddSignalFilterError::<u64>::Disconnected.to_string(),
            "Signal processing service is disconnected",
        );

        assert_eq!(
            AddPidLoopError::Disconnected.to_string(),
            "Signal processing service is disconnected",
        );

        assert_eq!(
            SetPidLoopSetpointError::Disconnected.to_string(),
            "Signal processing service is disconnected",
        );

        assert_eq!(
            SignalProcessingServiceDisconnected.to_string(),
            "Signal processing service is disconnected",
        );
    }
}
