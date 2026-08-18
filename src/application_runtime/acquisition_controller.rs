use crate::{
    app_log::LogHandle,
    worker::{ConnectionWorkers, ConnectionWorkersError},
};

pub(crate) struct AcquisitionController {
    workers: ConnectionWorkers,
    log: LogHandle,
}

impl AcquisitionController {
    pub fn new(workers: ConnectionWorkers, log: LogHandle) -> Self {
        Self { workers, log }
    }

    pub fn start(&self) {
        self.report_worker_error("start acquisition", self.workers.start());
    }

    pub fn stop(&self) {
        self.report_worker_error("stop acquisition", self.workers.stop());
    }

    pub fn clear(&self) {
        self.report_worker_error("clear series", self.workers.clear_series());
    }

    pub fn is_running(&self) -> bool {
        self.workers.is_running()
    }

    fn report_worker_error(&self, action: &str, result: Result<(), ConnectionWorkersError>) {
        if let Err(error) = result {
            self.log.error(format!("Failed to {action}: {error}",));
        }
    }
}
