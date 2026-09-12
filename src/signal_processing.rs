//! Signal filtering and background control-loop execution.
//!
//! The processing service owns a directed filter graph and controller registry. It receives
//! acquisition batches from workers and publishes derived samples and output intents without
//! blocking acquisition scheduling.

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
    AddControlLoopError, AddControllerDiagnosticError, AddSignalFilterError,
    ControllerRequestError, ProcessingEvent, ProcessingHandle, ProcessingInput, ProcessingService,
    ProcessingServiceDisconnected, ReplaceSignalFilterError,
};
