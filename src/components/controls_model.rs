use crossbeam_channel::{Receiver, Sender};

use crate::{
    data::SeriesStore,
    worker::{Worker, WorkerCommand, WorkerHandle},
};

pub struct ControlsModel {
    worker: Worker,
}

impl ControlsModel {
    pub fn new(
        series: SeriesStore,
        worker_handle: WorkerHandle,
        command_receiver: Receiver<WorkerCommand>,
        response_sender: Sender<String>,
    ) -> Self {
        let worker = Worker::spawn(worker_handle, command_receiver, response_sender, series);

        Self { worker }
    }

    pub fn start(&self) {
        self.worker.start();
    }

    pub fn stop(&self) {
        self.worker.stop();
    }

    pub fn clear(&self) {
        self.worker.clear_series();
    }

    pub fn is_running(&self) -> bool {
        self.worker.is_running()
    }
}
