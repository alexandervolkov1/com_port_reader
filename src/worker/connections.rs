use std::{
    collections::{BTreeMap, btree_map::Entry},
    error::Error,
    fmt,
    path::PathBuf,
};

use crate::connection::ConnectionId;

use super::{Worker, WorkerHandle, WorkerHandleError};

pub struct ConnectionWorkers {
    workers: BTreeMap<ConnectionId, Worker>,
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
        };

        result.insert(primary).expect(
            "new connection worker collection \
                 must be empty",
        );

        result
    }

    pub fn insert(&mut self, worker: Worker) -> Result<(), DuplicateConnectionWorkerError> {
        let connection_id = worker.connection_id();

        match self.workers.entry(connection_id) {
            Entry::Vacant(entry) => {
                entry.insert(worker);
                Ok(())
            }

            Entry::Occupied(_) => Err(DuplicateConnectionWorkerError { connection_id }),
        }
    }

    pub fn handle(&self, connection_id: ConnectionId) -> Option<WorkerHandle> {
        self.workers.get(&connection_id).map(Worker::handle)
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
