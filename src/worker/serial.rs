use crossbeam_channel::{Sender, bounded};

use crate::{
    acquisition::{CombinedSource, SerialCommandSource},
    data::{SeriesId, SeriesStore},
    process_recorder::ProcessRecorder,
    serial_connection::SerialConfigStore,
    signal_processing::ProcessingHandle,
};

use super::{ConnectionWorkerEvent, Worker, WorkerConfig, WorkerHandle, WorkerServices};

const CONNECTION_COMMAND_CAPACITY: usize = 32;

pub fn spawn_serial_connection_worker(
    config_store: SerialConfigStore,
    event_sender: Sender<ConnectionWorkerEvent>,
    series: SeriesStore,
    process_recorder: ProcessRecorder,
    processing: ProcessingHandle<SeriesId>,
    worker_config: WorkerConfig,
) -> Worker {
    let connection_id = config_store.connection_id();

    let (command_sender, command_receiver) = bounded(CONNECTION_COMMAND_CAPACITY);

    let handle = WorkerHandle::new(connection_id, command_sender);

    let source = CombinedSource::new(vec![Box::new(SerialCommandSource::new(config_store))]);

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
