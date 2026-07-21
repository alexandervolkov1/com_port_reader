use crossbeam_channel::{Receiver, Sender};

use crate::{
    data::SeriesStore,
    worker::{Worker, WorkerCommand},
};

pub struct ControlsModel {
    worker: Worker,
    series: SeriesStore,
    command_receiver: Receiver<WorkerCommand>,
    response_sender: Sender<String>,
}

impl ControlsModel {
    pub fn new(
        series: SeriesStore,
        command_receiver: Receiver<WorkerCommand>,
        response_sender: Sender<String>,
    ) -> Self {
        Self {
            worker: Worker::new(),
            series,
            command_receiver,
            response_sender,
        }
    }

    pub fn start(&mut self) {
        self.worker.start(
            self.command_receiver.clone(),
            self.response_sender.clone(),
            self.series.clone(),
        );
    }

    pub fn stop(&mut self) {
        self.worker.stop();
    }

    pub fn clear(&mut self) {
        self.series.clear();
    }

    pub fn is_running(&self) -> bool {
        self.worker.is_running()
    }
}
