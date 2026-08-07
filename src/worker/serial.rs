use crossbeam_channel::{Sender, bounded};

use crate::{
    acquisition::{CombinedSource, SerialCommandSource},
    data::SeriesStore,
    sample_sink::SampleSink,
    serial_connection::SerialConfigStore,
};

use super::{Worker, WorkerConfig, WorkerEvent, WorkerHandle};

const CONNECTION_COMMAND_CAPACITY: usize = 32;

pub struct SpawnedConnectionWorker {
    pub worker: Worker,
    pub handle: WorkerHandle,
}

pub fn spawn_serial_connection_worker(
    config_store: SerialConfigStore,
    event_sender: Sender<WorkerEvent>,
    series: SeriesStore,
    sink: Box<dyn SampleSink>,
    worker_config: WorkerConfig,
) -> SpawnedConnectionWorker {
    let connection_id = config_store.connection_id();

    let (command_sender, command_receiver) = bounded(CONNECTION_COMMAND_CAPACITY);

    let handle = WorkerHandle::new(connection_id, command_sender);

    let source = CombinedSource::new(vec![Box::new(SerialCommandSource::new(config_store))]);

    let worker = Worker::spawn(
        handle.clone(),
        command_receiver,
        event_sender,
        series,
        Box::new(source),
        sink,
        worker_config,
    );

    SpawnedConnectionWorker { worker, handle }
}
