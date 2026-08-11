use std::{
    collections::{BTreeMap, btree_map::Entry},
    error::Error,
    fmt,
    path::PathBuf,
};

use crate::connection::ConnectionId;

use super::{ConnectionRouter, Worker, WorkerHandleError};

pub struct ConnectionWorkers {
    workers: BTreeMap<ConnectionId, Worker>,
    router: ConnectionRouter,
}

impl ConnectionWorkers {
    pub fn new(primary: Worker) -> Self {
        assert_eq!(
            primary.connection_id(),
            ConnectionId::PRIMARY,
            "primary worker must use the primary \
             connection ID",
        );

        let mut result = Self {
            workers: BTreeMap::new(),
            router: ConnectionRouter::default(),
        };

        result.insert(primary).expect(
            "new connection worker collection \
                 must be empty",
        );

        result
    }

    pub fn insert(&mut self, worker: Worker) -> Result<(), DuplicateConnectionWorkerError> {
        let connection_id = worker.connection_id();

        let handle = worker.handle();

        match self.workers.entry(connection_id) {
            Entry::Vacant(entry) => {
                self.router.insert(handle);
                entry.insert(worker);

                Ok(())
            }

            Entry::Occupied(_) => Err(DuplicateConnectionWorkerError { connection_id }),
        }
    }

    pub fn router(&self) -> ConnectionRouter {
        self.router.clone()
    }

    pub fn start(&self) -> Result<(), ConnectionWorkersError> {
        self.apply_to_all(Worker::start)
    }

    pub fn stop(&self) -> Result<(), ConnectionWorkersError> {
        self.apply_to_all(Worker::stop)
    }

    pub fn clear_series(&self) -> Result<(), ConnectionWorkersError> {
        self.primary()
            .clear_series()
            .map_err(|source| ConnectionWorkersError::new(ConnectionId::PRIMARY, source))
    }

    pub fn is_running(&self) -> bool {
        self.workers.values().any(Worker::is_running)
    }

    pub fn start_csv_recording(&self, path: PathBuf) -> Result<(), ConnectionWorkersError> {
        self.primary()
            .start_csv_recording(path)
            .map_err(|source| ConnectionWorkersError::new(ConnectionId::PRIMARY, source))
    }

    pub fn stop_recording(&self) -> Result<(), ConnectionWorkersError> {
        self.primary()
            .stop_recording()
            .map_err(|source| ConnectionWorkersError::new(ConnectionId::PRIMARY, source))
    }

    pub fn is_recording(&self) -> bool {
        self.primary().is_recording()
    }

    fn primary(&self) -> &Worker {
        self.workers.get(&ConnectionId::PRIMARY).expect(
            "primary worker was inserted \
                 during construction",
        )
    }

    fn apply_to_all(
        &self,
        operation: impl Fn(&Worker) -> Result<(), WorkerHandleError>,
    ) -> Result<(), ConnectionWorkersError> {
        let mut first_error = None;

        for (&connection_id, worker) in &self.workers {
            let error = operation(worker)
                .err()
                .map(|source| ConnectionWorkersError::new(connection_id, source));

            if first_error.is_none() {
                first_error = error;
            }
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionWorkersError {
    connection_id: ConnectionId,
    source: WorkerHandleError,
}

impl ConnectionWorkersError {
    const fn new(connection_id: ConnectionId, source: WorkerHandleError) -> Self {
        Self {
            connection_id,
            source,
        }
    }
}

impl fmt::Display for ConnectionWorkersError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "connection {}: {}",
            self.connection_id, self.source,
        )
    }
}

impl Error for ConnectionWorkersError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuplicateConnectionWorkerError {
    connection_id: ConnectionId,
}

impl fmt::Display for DuplicateConnectionWorkerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "worker for connection {} already exists",
            self.connection_id,
        )
    }
}

impl Error for DuplicateConnectionWorkerError {}

#[cfg(test)]
mod tests {
    use std::{
        thread,
        time::{Duration, Instant},
    };

    use crossbeam_channel::{Sender, bounded, unbounded};

    use super::ConnectionWorkers;

    use crate::{
        acquisition::{AcquisitionError, AcquisitionSource},
        connection::ConnectionId,
        data::{NewSeries, Sample, SeriesId, SeriesMetadata, SeriesStore},
        sample_sink::NullSampleSink,
        utils::current_time_f64,
        worker::{ConnectionWorkerEvent, Worker, WorkerConfig, WorkerHandle},
    };

    struct FixedSource {
        value: f64,
    }

    impl AcquisitionSource for FixedSource {
        fn sample_series(
            &mut self,
            _series: &SeriesMetadata,
        ) -> Result<Option<Sample>, AcquisitionError> {
            Ok(Some(Sample::new(current_time_f64(), self.value)))
        }
    }

    fn spawn_test_worker(
        connection_id: ConnectionId,
        value: f64,
        series: SeriesStore,
        event_sender: Sender<ConnectionWorkerEvent>,
    ) -> Worker {
        let (command_sender, command_receiver) = bounded(32);

        let handle = WorkerHandle::new(connection_id, command_sender);

        Worker::spawn(
            handle,
            command_receiver,
            event_sender,
            series,
            Box::new(FixedSource { value }),
            Box::new(NullSampleSink::new()),
            WorkerConfig::new(Duration::from_millis(10)),
        )
    }

    fn last_value(series: &SeriesStore, series_id: SeriesId) -> Option<f64> {
        series.with(|all_series| {
            all_series
                .iter()
                .find(|series| series.id == series_id)
                .and_then(|series| series.samples.last())
                .map(|sample| sample.value)
        })
    }

    #[test]
    fn polls_connections_on_independent_workers() {
        let series = SeriesStore::new();

        let primary_series_id = series
            .add_series(
                NewSeries::named_serial_command("primary", "primary_series")
                    .with_connection(ConnectionId::PRIMARY),
            )
            .unwrap();

        let secondary_connection = ConnectionId::new(2);

        let secondary_series_id = series
            .add_series(
                NewSeries::named_serial_command("secondary", "secondary_series")
                    .with_connection(secondary_connection),
            )
            .unwrap();

        let (event_sender, _event_receiver) = unbounded();

        let primary_worker = spawn_test_worker(
            ConnectionId::PRIMARY,
            10.0,
            series.clone(),
            event_sender.clone(),
        );

        let secondary_worker =
            spawn_test_worker(secondary_connection, 20.0, series.clone(), event_sender);

        let mut workers = ConnectionWorkers::new(primary_worker);

        workers.insert(secondary_worker).unwrap();

        workers.start().unwrap();

        let deadline = Instant::now() + Duration::from_secs(1);

        while last_value(&series, primary_series_id).is_none()
            || last_value(&series, secondary_series_id).is_none()
        {
            assert!(
                Instant::now() < deadline,
                "connection workers did not produce \
                 samples before the timeout",
            );

            thread::sleep(Duration::from_millis(5));
        }

        assert_eq!(last_value(&series, primary_series_id), Some(10.0),);

        assert_eq!(last_value(&series, secondary_series_id), Some(20.0),);

        workers.stop().unwrap();
    }
}
