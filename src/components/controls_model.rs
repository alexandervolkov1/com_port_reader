use crossbeam_channel::{Receiver, Sender};

use crate::{
    data::SeriesStore,
    worker::{Worker, WorkerCommand},
};

pub struct ControlsModel {
    worker: Worker,
    series: SeriesStore,
}

impl ControlsModel {
    pub fn new(
        series: SeriesStore,
        command_sender: Sender<WorkerCommand>,
        command_receiver: Receiver<WorkerCommand>,
        response_sender: Sender<String>,
    ) -> Self {
        let worker = Worker::spawn(
            command_sender,
            command_receiver,
            response_sender,
            series.clone(),
        );

        Self { worker, series }
    }

    pub fn start(&self) {
        self.worker.start();
    }

    pub fn stop(&self) {
        self.worker.stop();
    }

    pub fn clear(&self) {
        self.series.clear();
    }

    pub fn is_running(&self) -> bool {
        self.worker.is_running()
    }
}
