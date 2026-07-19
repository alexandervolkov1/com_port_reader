use crossbeam_channel::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::{
    data::{Signal, SignalSeries},
    worker::Worker,
};

pub struct ControlsModel {
    worker: Worker,
    series: Arc<Mutex<Vec<SignalSeries>>>,
    command_receiver: Receiver<Signal>,
    response_sender: Sender<String>,
}

impl ControlsModel {
    pub fn new(
        series: Arc<Mutex<Vec<SignalSeries>>>,
        command_receiver: Receiver<Signal>,
        response_sender: Sender<String>,
    ) -> Self {
        Self {
            worker: Worker::new(),
            series,
            command_receiver,
            response_sender,
        }
    }

    pub fn series(&self) -> &Arc<Mutex<Vec<SignalSeries>>> {
        &self.series
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
        if let Ok(mut series) = self.series.lock() {
            series.clear();
        }
    }

    pub fn is_running(&self) -> bool {
        self.worker.is_running()
    }
}
