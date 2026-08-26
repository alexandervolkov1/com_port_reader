mod filter;
mod graph;
mod service;

pub use filter::{
    MAX_FILTER_WINDOW_SIZE, SignalFilter, SignalFilterDefinition, SignalFilterDefinitionError,
    SignalFilterError, SignalFilterKind,
};

pub use graph::{
    ProcessedSignal, SignalProcessingError, SignalProcessingGraph,
    SignalProcessingGraphDefinitionError, SignalProcessingGraphUpdateError,
};

pub use service::{
    AddPidLoopError, AddSignalFilterError, ProcessingEvent, ProcessingHandle, ProcessingInput,
    ProcessingService, ProcessingServiceDisconnected, ReplaceSignalFilterError,
    SetPidLoopSetpointError,
};
