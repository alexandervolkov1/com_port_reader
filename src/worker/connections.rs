use std::{collections::BTreeMap, error::Error, fmt, path::PathBuf};

use crate::connection::ConnectionId;

use super::{Worker, WorkerHandleError};

pub struct ConnectionWorkers {
    workers: BTreeMap<ConnectionId, Worker>,
}

impl ConnectionWorkers {
    pub fn new(primary: Worker) -> Self {
        let connection_id = primary.connection_id();

        assert_eq!(
            connection_id,
            ConnectionId::PRIMARY,
            "primary worker must use the primary \
             connection ID",
        );

        let mut workers = BTreeMap::new();

        workers.insert(connection_id, primary);

        Self { workers }
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
