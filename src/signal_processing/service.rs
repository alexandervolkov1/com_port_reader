use std::{
    error::Error,
    fmt,
    hash::Hash,
    io,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};

use super::{
    ProcessedSignal, SignalFilterDefinition, SignalProcessingError, SignalProcessingGraph,
    SignalProcessingGraphDefinitionError,
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
    thread: Option<JoinHandle<()>>,
}

impl<SignalId> SignalProcessingService<SignalId>
where
    SignalId: Copy + Eq + Hash + Send + 'static,
{
    pub fn spawn() -> io::Result<Self> {
        let (command_sender, command_receiver) = unbounded();

        let (event_sender, event_receiver) = unbounded();

        let thread = thread::Builder::new()
            .name("signal-processing".to_owned())
            .spawn(move || {
                run_signal_processing(command_receiver, event_sender);
            })?;

        Ok(Self {
            handle: SignalProcessingHandle { command_sender },
            event_receiver,
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
) where
    SignalId: Copy + Eq + Hash,
{
    let mut graph = SignalProcessingGraph::new();

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

            SignalProcessingCommand::Process(inputs) => {
                process_inputs(&mut graph, inputs, &event_sender);
            }

            SignalProcessingCommand::ResetFrom { signal_id } => {
                graph.reset_from(signal_id);
            }

            SignalProcessingCommand::RemoveFrom {
                signal_id,
                response_sender,
            } => {
                let removed = graph.remove_from(signal_id);

                let _ = response_sender.send(removed);
            }

            SignalProcessingCommand::Clear => {
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
    inputs: Vec<SignalProcessingInput<SignalId>>,
    event_sender: &Sender<SignalProcessingEvent<SignalId>>,
) where
    SignalId: Copy + Eq + Hash,
{
    let mut output_samples = Vec::new();

    for input in inputs {
        match graph.process(input.signal_id, input.timestamp, input.value) {
            Ok(mut processed) => {
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
        AddSignalFilterError, SignalProcessingEvent, SignalProcessingInput, SignalProcessingService,
    };

    use crate::signal_processing::{
        ProcessedSignal, SignalFilterDefinition, SignalFilterError,
        SignalProcessingGraphDefinitionError,
    };

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
            events.recv_timeout(Duration::from_secs(1)),
            Ok(SignalProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 0.0,
                value: 10.0,
            },])),
        );

        handle.process(1, 1.0, 20.0).unwrap();

        assert_eq!(
            events.recv_timeout(Duration::from_secs(1)),
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
            events.recv_timeout(Duration::from_secs(1)),
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

        events.recv_timeout(Duration::from_secs(1)).unwrap();

        handle.process(1, 1.0, 20.0).unwrap();

        let event = events.recv_timeout(Duration::from_secs(1)).unwrap();

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
    fn rejects_duplicate_output() {
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        assert_eq!(
            handle.add_filter(3, 2, SignalFilterDefinition::median(3).unwrap(),),
            Err(AddSignalFilterError::Definition(
                SignalProcessingGraphDefinitionError::DuplicateOutput { output: 2 },
            )),
        );
    }

    #[test]
    fn resets_filters_before_processing_new_values() {
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();
        let events = service.event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle.process(1, 10.0, 10.0).unwrap();

        events.recv_timeout(Duration::from_secs(1)).unwrap();

        handle.process(1, 11.0, 20.0).unwrap();

        events.recv_timeout(Duration::from_secs(1)).unwrap();

        handle.reset_from(1).unwrap();

        handle.process(1, 0.0, 100.0).unwrap();

        assert_eq!(
            events.recv_timeout(Duration::from_secs(1)),
            Ok(SignalProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 0.0,
                value: 100.0,
            },])),
        );
    }

    #[test]
    fn clear_removes_registered_filters() {
        let service = SignalProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();
        let events = service.event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle.clear().unwrap();
        handle.process(1, 0.0, 10.0).unwrap();

        assert!(events.recv_timeout(Duration::from_millis(50),).is_err());
    }
}
