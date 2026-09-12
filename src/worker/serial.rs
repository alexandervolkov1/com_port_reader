use crossbeam_channel::{Sender, bounded};

use super::{ConnectionWorkerEvent, Worker, WorkerConfig, WorkerHandle, WorkerServices};
use crate::{
    acquisition::{AcquisitionSource, CombinedSource, SerialCommandSource},
    data::{SeriesId, SeriesStore},
    process_recorder::ProcessRecorder,
    serial_connection::SerialConfigStore,
    signal_processing::ProcessingHandle,
};

const CONNECTION_COMMAND_CAPACITY: usize = 32;

pub fn spawn_serial_connection_worker(
    config_store: SerialConfigStore,
    event_sender: Sender<ConnectionWorkerEvent>,
    series: SeriesStore,
    process_recorder: ProcessRecorder,
    processing: ProcessingHandle<SeriesId>,
    worker_config: WorkerConfig,
) -> Worker {
    spawn_serial_connection_worker_with_sources(
        config_store,
        Vec::new(),
        event_sender,
        series,
        process_recorder,
        processing,
        worker_config,
    )
}

/// Builds a serial connection worker with optional higher-priority sources.
/// Sources are tried in the provided order, followed by the serial source.
/// This keeps local virtual instruments composable with real Metakon/serial
/// devices without changing the worker or Lua command paths.
pub fn spawn_serial_connection_worker_with_sources(
    config_store: SerialConfigStore,
    mut additional_sources: Vec<Box<dyn AcquisitionSource>>,
    event_sender: Sender<ConnectionWorkerEvent>,
    series: SeriesStore,
    process_recorder: ProcessRecorder,
    processing: ProcessingHandle<SeriesId>,
    worker_config: WorkerConfig,
) -> Worker {
    let connection_id = config_store.connection_id();

    let (command_sender, command_receiver) = bounded(CONNECTION_COMMAND_CAPACITY);

    let handle = WorkerHandle::new(connection_id, command_sender);

    additional_sources.push(Box::new(SerialCommandSource::new(config_store)));
    let source = CombinedSource::new(additional_sources);

    let services = WorkerServices::new(series, process_recorder, processing);

    Worker::spawn(
        handle,
        command_receiver,
        event_sender,
        services,
        Box::new(source),
        worker_config,
    )
}
