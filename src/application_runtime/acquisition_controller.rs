use std::collections::BTreeSet;

use crate::{
    connection::ConnectionId,
    process_recorder::ProcessActionId,
    worker::{ConnectionWorkers, ConnectionWorkersError},
};

pub(crate) struct AcquisitionController {
    workers: ConnectionWorkers,
}

impl AcquisitionController {
    pub fn new(workers: ConnectionWorkers) -> Self {
        Self { workers }
    }

    pub fn start(&self, action_id: Option<ProcessActionId>) -> Result<(), ConnectionWorkersError> {
        self.workers.start(action_id)
    }

    pub fn stop(&self, action_id: Option<ProcessActionId>) -> Result<(), ConnectionWorkersError> {
        self.workers.stop(action_id)
    }

    pub fn is_running(&self) -> bool {
        self.workers.is_running()
    }

    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    pub fn stopped_connection_ids(&self) -> BTreeSet<ConnectionId> {
        self.workers.stopped_connection_ids()
    }
}
